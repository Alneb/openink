//! Inference building blocks shared by openink's trained models (shapes, handwriting).
//!
//! Weight file (`.oisw`, our own format, little-endian): `"OISW", version:u32 = 1, count:u32`, then
//! `count` tensors of `name_len:u16, name, ndim:u8, dims:u32×ndim, f32 × Π dims`.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeightsError {
    BadMagic,
    Truncated,
    Missing(String),
    Shape(String),
}

impl std::fmt::Display for WeightsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WeightsError::BadMagic => write!(f, "not an OISW weight file"),
            WeightsError::Truncated => write!(f, "weight file truncated"),
            WeightsError::Missing(n) => write!(f, "tensor {n} missing"),
            WeightsError::Shape(n) => write!(f, "tensor {n} has the wrong shape"),
        }
    }
}

impl std::error::Error for WeightsError {}

#[derive(Clone, Debug)]
struct Tensor {
    dims: Vec<usize>,
    data: Vec<f32>,
}

/// The named tensors of one `.oisw` file.
pub struct Store(HashMap<String, Tensor>);

impl Store {
    pub fn from_bytes(bytes: &[u8]) -> Result<Store, WeightsError> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], WeightsError> {
            let s = bytes.get(at..at + n).ok_or(WeightsError::Truncated)?;
            at += n;
            Ok(s)
        };
        if take(4)? != b"OISW" {
            return Err(WeightsError::BadMagic);
        }
        let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let _version = u32le(take(4)?);
        let count = u32le(take(4)?);
        let mut out = HashMap::new();
        for _ in 0..count {
            let nl = take(2)?;
            let nl = u16::from_le_bytes([nl[0], nl[1]]) as usize;
            let name = String::from_utf8_lossy(take(nl)?).into_owned();
            let ndim = take(1)?[0] as usize;
            let mut dims = Vec::with_capacity(ndim);
            for _ in 0..ndim {
                dims.push(u32le(take(4)?) as usize);
            }
            let n: usize = dims.iter().product();
            let raw = take(n * 4)?;
            let data = raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            out.insert(name, Tensor { dims, data });
        }
        Ok(Store(out))
    }

    /// The tensor's data, checked against the expected shape.
    pub fn get(&self, name: &str, dims: &[usize]) -> Result<Vec<f32>, WeightsError> {
        let t = self.0.get(name).ok_or_else(|| WeightsError::Missing(name.into()))?;
        if t.dims != dims {
            return Err(WeightsError::Shape(name.into()));
        }
        Ok(t.data.clone())
    }

    pub fn dims(&self, name: &str) -> Result<Vec<usize>, WeightsError> {
        self.0.get(name).map(|t| t.dims.clone()).ok_or_else(|| WeightsError::Missing(name.into()))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

/// `y = W x + b`, `W` row-major `[out, in]`.
#[derive(Clone, Debug)]
pub struct Dense {
    w: Vec<f32>,
    b: Vec<f32>,
    pub inp: usize,
    pub out: usize,
}

impl Dense {
    /// Loads `{name}.w` `[out, in]` and `{name}.b` `[out]`.
    pub fn load(s: &Store, name: &str) -> Result<Dense, WeightsError> {
        let d = s.dims(&format!("{name}.w"))?;
        if d.len() != 2 {
            return Err(WeightsError::Shape(name.into()));
        }
        let (out, inp) = (d[0], d[1]);
        Ok(Dense { w: s.get(&format!("{name}.w"), &[out, inp])?, b: s.get(&format!("{name}.b"), &[out])?, inp, out })
    }

    pub fn apply(&self, x: &[f32], y: &mut [f32]) {
        for (o, yo) in y.iter_mut().enumerate().take(self.out) {
            *yo = self.b[o] + dot(&self.w[o * self.inp..(o + 1) * self.inp], x);
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn softmax(v: &mut [f32]) {
    let m = v.iter().cloned().fold(f32::MIN, f32::max);
    let mut s = 0.0;
    for x in v.iter_mut() {
        *x = (*x - m).exp();
        s += *x;
    }
    for x in v.iter_mut() {
        *x /= s;
    }
}

/// `ln softmax`, stable.
pub fn log_softmax(v: &mut [f32]) {
    let m = v.iter().cloned().fold(f32::MIN, f32::max);
    let lse = m + v.iter().map(|x| (x - m).exp()).sum::<f32>().ln();
    v.iter_mut().for_each(|x| *x -= lse);
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Input standardisation stored with each model (`{name}.mu`, `{name}.sd`).
#[derive(Clone, Debug)]
pub struct Scaler {
    mu: Vec<f32>,
    sd: Vec<f32>,
}

impl Scaler {
    pub fn load(s: &Store, name: &str, n: usize) -> Result<Scaler, WeightsError> {
        Ok(Scaler { mu: s.get(&format!("{name}.mu"), &[n])?, sd: s.get(&format!("{name}.sd"), &[n])? })
    }
    pub fn apply(&self, x: &[f32]) -> Vec<f32> {
        x.iter().zip(&self.mu).zip(&self.sd).map(|((v, m), s)| (v - m) / s.max(1e-6)).collect()
    }
}

/// Where a GRU's weights came from: the gate order and whether the recurrent bias exists differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GruLayout {
    /// Our numpy trainer (train_shape.py): gates z, r, n; one bias; `n = tanh(Wn x + bn + r ⊙ Un h)`.
    Zrn,
    /// torch `nn.GRU`: gates r, z, n; biases `b` (input) and `c` (recurrent);
    /// `n = tanh(Wn x + bn + r ⊙ (Un h + cn))`.
    Torch,
}

#[derive(Clone, Debug)]
pub struct Gru {
    w: Vec<f32>, // [3H, I]
    u: Vec<f32>, // [3H, H]
    b: Vec<f32>, // [3H]
    c: Vec<f32>, // [3H], zero for Zrn
    layout: GruLayout,
    pub h: usize,
    pub i: usize,
}

impl Gru {
    /// Loads `{name}.w`, `.u`, `.b` (and `.c` for `Torch`); the hidden size comes from `.u`.
    pub fn load(s: &Store, name: &str, i: usize, layout: GruLayout) -> Result<Gru, WeightsError> {
        let d = s.dims(&format!("{name}.u"))?;
        if d.len() != 2 {
            return Err(WeightsError::Shape(format!("{name}.u")));
        }
        let h = d[1];
        let c = match layout {
            GruLayout::Zrn => vec![0.0; 3 * h],
            GruLayout::Torch => s.get(&format!("{name}.c"), &[3 * h])?,
        };
        Ok(Gru {
            w: s.get(&format!("{name}.w"), &[3 * h, i])?,
            u: s.get(&format!("{name}.u"), &[3 * h, h])?,
            b: s.get(&format!("{name}.b"), &[3 * h])?,
            c,
            layout,
            h,
            i,
        })
    }

    /// Hidden states for the sequence, in input order (`rev` runs the backward direction).
    pub fn run(&self, xs: &[Vec<f32>], rev: bool) -> Vec<Vec<f32>> {
        let (h, i) = (self.h, self.i);
        // Row offsets of the update (z) and reset (r) gates; the candidate (n) is always last.
        let (zo, ro) = match self.layout {
            GruLayout::Zrn => (0, h),
            GruLayout::Torch => (h, 0),
        };
        let mut state = vec![0.0f32; h];
        let mut out = vec![Vec::new(); xs.len()];
        let mut wx = vec![0.0f32; 3 * h];
        let mut uh = vec![0.0f32; 3 * h];
        let order: Box<dyn Iterator<Item = usize>> = if rev { Box::new((0..xs.len()).rev()) } else { Box::new(0..xs.len()) };
        for t in order {
            for r in 0..3 * h {
                wx[r] = self.b[r] + dot(&self.w[r * i..(r + 1) * i], &xs[t]);
                uh[r] = self.c[r] + dot(&self.u[r * h..(r + 1) * h], &state);
            }
            for k in 0..h {
                let z = sigmoid(wx[zo + k] + uh[zo + k]);
                let r = sigmoid(wx[ro + k] + uh[ro + k]);
                let n = (wx[2 * h + k] + r * uh[2 * h + k]).tanh();
                state[k] = (1.0 - z) * n + z * state[k];
            }
            out[t] = state.clone();
        }
        out
    }
}

/// Forward and backward GRUs over the same input; outputs concatenated `[fwd, bwd]` per step.
#[derive(Clone, Debug)]
pub struct BiGru {
    pub fwd: Gru,
    pub bwd: Gru,
}

impl BiGru {
    pub fn run(&self, xs: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let f = self.fwd.run(xs, false);
        let b = self.bwd.run(xs, true);
        f.into_iter().zip(b).map(|(mut a, b)| {
            a.extend(b);
            a
        }).collect()
    }
    pub fn out_dim(&self) -> usize {
        self.fwd.h + self.bwd.h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(tensors: &[(&str, &[usize], &[f32])]) -> Vec<u8> {
        let mut v = b"OISW".to_vec();
        v.extend(1u32.to_le_bytes());
        v.extend((tensors.len() as u32).to_le_bytes());
        for (n, d, x) in tensors {
            v.extend((n.len() as u16).to_le_bytes());
            v.extend(n.as_bytes());
            v.push(d.len() as u8);
            d.iter().for_each(|k| v.extend((*k as u32).to_le_bytes()));
            x.iter().for_each(|f| v.extend(f.to_le_bytes()));
        }
        v
    }

    #[test]
    fn parses_and_checks_shapes() {
        let s = Store::from_bytes(&file(&[("d.w", &[1, 2], &[1.0, 2.0]), ("d.b", &[1], &[0.5])])).unwrap();
        let d = Dense::load(&s, "d").unwrap();
        let mut y = [0.0];
        d.apply(&[3.0, 4.0], &mut y);
        assert_eq!(y[0], 11.5);
        assert_eq!(s.get("d.b", &[2]), Err(WeightsError::Shape("d.b".into())));
        assert!(matches!(Store::from_bytes(b"NOPE"), Err(WeightsError::BadMagic)));
    }

    /// One torch-layout step by hand: h = 1, i = 1, h0 = 0.
    #[test]
    fn torch_gru_step_matches_formula() {
        let (wr, wz, wn) = (0.5f32, -0.3f32, 0.8f32);
        let (cr, cz, cn) = (0.1f32, 0.2f32, -0.4f32);
        let s = Store::from_bytes(&file(&[
            ("g.w", &[3, 1], &[wr, wz, wn]),
            ("g.u", &[3, 1], &[0.7, 0.9, 1.1]),
            ("g.b", &[3], &[0.0, 0.0, 0.0]),
            ("g.c", &[3], &[cr, cz, cn]),
        ]))
        .unwrap();
        let g = Gru::load(&s, "g", 1, GruLayout::Torch).unwrap();
        let x = 2.0f32;
        let r = sigmoid(wr * x + cr);
        let z = sigmoid(wz * x + cz);
        let n = (wn * x + r * cn).tanh();
        let want = (1.0 - z) * n;
        let got = g.run(&[vec![x]], false)[0][0];
        assert!((got - want).abs() < 1e-6, "{got} vs {want}");
    }
}
