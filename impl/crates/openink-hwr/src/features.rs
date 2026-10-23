//! Per-point pen features. Reference implementation: `features()` in docs/scripts/hwr_synth.py; the
//! parity test (tests/parity.rs) checks this against fixtures the Python wrote.

/// dx, dy, pen_up, y, cos, sin, turn.
pub const POINT_FEATURES: usize = 7;
/// Resampling step as a fraction of the word scale.
pub const STEP: f64 = 0.1;

/// A pen trace point in any consistent unit (view px, page px...).
pub type Point = [f32; 2];

/// Writing slant as dx/dy: length-weighted over the mostly vertical segments (|dy| > |dx|), signs
/// folded so up- and down-strokes agree; 0 when there are none; clamped to ±1 (45°).
pub fn slant(strokes: &[Vec<Point>]) -> f64 {
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for s in strokes {
        for w in s.windows(2) {
            let (dx, dy) = ((w[1][0] - w[0][0]) as f64, (w[1][1] - w[0][1]) as f64);
            if dy.abs() > dx.abs() {
                let (dx, dy) = (dx * dy.signum(), dy.abs());
                let len = dx.hypot(dy);
                num += len * dx / dy;
                den += len;
            }
        }
    }
    if den > 0.0 { (num / den).clamp(-1.0, 1.0) } else { 0.0 }
}

/// Features per resampled point. Lengths are in units of the word scale (bbox height, floored at a
/// tenth of the width), so the input's unit does not matter. Empty strokes are ignored. `deslant`
/// first shears the ink upright (`x -= slant · y`); the weight file says which the model was trained on.
pub fn features(strokes: &[Vec<Point>], deslant: bool) -> Vec<[f32; POINT_FEATURES]> {
    let mut strokes: Vec<Vec<Point>> = strokes.iter().filter(|s| !s.is_empty()).cloned().collect();
    if strokes.is_empty() {
        return Vec::new();
    }
    if deslant {
        let k = slant(&strokes);
        for p in strokes.iter_mut().flatten() {
            p[0] = (p[0] as f64 - k * p[1] as f64) as f32;
        }
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in strokes.iter().flat_map(|s| s.iter()) {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k] as f64);
            hi[k] = hi[k].max(p[k] as f64);
        }
    }
    let scale = (hi[1] - lo[1]).max(0.1 * (hi[0] - lo[0])).max(1e-6);
    let yc = (lo[1] + hi[1]) / 2.0;
    let mut pts: Vec<[f64; 2]> = Vec::new();
    let mut up: Vec<f64> = Vec::new();
    for s in strokes {
        let norm: Vec<[f64; 2]> = s.iter().map(|p| [(p[0] as f64 - lo[0]) / scale, (p[1] as f64 - yc) / scale]).collect();
        let r = resample(&norm, STEP);
        up.push(1.0);
        up.extend(std::iter::repeat(0.0).take(r.len() - 1));
        pts.extend(r);
    }
    let n = pts.len();
    let dir = |i: usize| -> [f64; 2] {
        let j = (i + 1).min(n - 1);
        let d = [pts[j][0] - pts[i][0], pts[j][1] - pts[i][1]];
        let l = d[0].hypot(d[1]);
        if l > 1e-9 { [d[0] / l, d[1] / l] } else { [0.0, 0.0] }
    };
    (0..n)
        .map(|i| {
            let prev = pts[i.saturating_sub(1)];
            let (dx, dy) = (pts[i][0] - prev[0], pts[i][1] - prev[1]);
            let d = dir(i);
            let pd = dir(i.saturating_sub(1));
            [
                dx.clamp(-5.0, 5.0) as f32,
                dy.clamp(-5.0, 5.0) as f32,
                up[i] as f32,
                pts[i][1] as f32,
                d[0] as f32,
                d[1] as f32,
                (d[0] * pd[0] + d[1] * pd[1]) as f32,
            ]
        })
        .collect()
}

/// Points every `step` along the path (numpy `interp` over `linspace`); endpoints kept.
fn resample(a: &[[f64; 2]], step: f64) -> Vec<[f64; 2]> {
    if a.len() < 2 {
        return a[..1].to_vec();
    }
    let mut cum = vec![0.0];
    for w in a.windows(2) {
        cum.push(cum.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
    }
    let total = *cum.last().unwrap();
    if total < 1e-9 {
        return a[..1].to_vec();
    }
    let n = round_half_even(total / step).max(1.0) as usize;
    let mut out = Vec::with_capacity(n + 1);
    let mut seg = 0;
    for k in 0..=n {
        let t = total * k as f64 / n as f64;
        // np.interp picks the last interval whose start is <= t.
        while seg + 2 < cum.len() && cum[seg + 1] <= t {
            seg += 1;
        }
        let (c0, c1) = (cum[seg], cum[seg + 1]);
        let f = if c1 > c0 { ((t - c0) / (c1 - c0)).min(1.0) } else { 0.0 };
        out.push([a[seg][0] + (a[seg + 1][0] - a[seg][0]) * f, a[seg][1] + (a[seg + 1][1] - a[seg][1]) * f]);
    }
    out
}

/// Python's `round` (ties to even), so point counts match the trainer exactly.
fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - x.signum()
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_line_features() {
        // Width 10, height 0 -> scale 1 (a tenth of the width); 10 steps of 0.1 ... resampled length 10.
        let f = features(&[vec![[0.0, 0.0], [10.0, 0.0]]], false);
        assert_eq!(f.len(), 101);
        assert_eq!(f[0][2], 1.0);
        assert!((f[1][0] - 0.1).abs() < 1e-6 && f[1][1] == 0.0);
        assert_eq!(f[50][4], 1.0);
        assert_eq!(f[100][4], 0.0); // last point has no next direction
    }

    #[test]
    fn pen_lift_marks_stroke_start() {
        let f = features(&[vec![[0.0, 0.0], [0.0, 1.0]], vec![[1.0, 0.0], [1.0, 1.0]]], false);
        let starts: Vec<usize> = f.iter().enumerate().filter(|(_, p)| p[2] == 1.0).map(|(i, _)| i).collect();
        assert_eq!(starts, vec![0, 11]);
        assert!((f[11][0] - 1.0).abs() < 1e-6 && (f[11][1] + 1.0).abs() < 1e-6); // the jump back up
    }

    #[test]
    fn deslant_makes_slanted_strokes_vertical() {
        let st = vec![vec![[0.0, 0.0], [-3.0, 10.0]], vec![[5.0, 0.0], [2.0, 10.0]]];
        assert!((slant(&st) + 0.3).abs() < 1e-9);
        let f = features(&st, true);
        assert!(f[1][0].abs() < 1e-6 && f[1][4].abs() < 1e-6); // no x motion along the down-stroke
    }

    #[test]
    fn ties_round_to_even() {
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(2.4), 2.0);
    }
}
