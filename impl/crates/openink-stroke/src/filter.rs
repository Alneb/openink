//! Input smoothing: libneo's `neo::pen::SmoothFilter`, decoded from libneo_pen.so and checked against the
//! library itself on the device (impl/java/neo-oracle; docs/STROKE.md "Smoothing"). An FIR low-pass over the
//! last N samples, N = ⌊50 · smoothLevel⌋ (2 below 3, at most 50), with weights resampled from a fixed 15-tap
//! kernel. The history starts filled with the first sample; at pen-up the up point is pushed
//! max(4, ⌊4N/15⌋) times, so the stroke runs out to it.

use crate::stroke::Sample;

/// libneo's kernel (the static table `_INIT_2` builds, oldest tap first); it sums to 1.
const KERNEL: [f32; 15] = [
    -6.0e-6, -0.000139, -0.000185, 0.000414, 0.002357, 0.003357, -0.003135, -0.023928, -0.042909, -0.017858,
    0.096525, 0.254692, 0.347072, 0.268810, 0.114933,
];
const MAX_TAPS: usize = 50;
const TAPS_PER_LEVEL: f32 = 50.0;

/// The window for `smooth_level` and its weights (oldest first, summing to 1), as `SmoothFilter::Impl` builds them.
pub fn weights(smooth_level: f32) -> Vec<f32> {
    let n = (smooth_level.clamp(0.0, 1.0) * TAPS_PER_LEVEL) as usize;
    let n = if n < 3 { 2 } else { n.min(MAX_TAPS) };
    let m = KERNEL.len();
    let w: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 * (m - 1) as f32 / (n - 1) as f32;
            let (k, f) = (t as usize, t.fract());
            (m as f32 / n as f32) * ((1.0 - f) * KERNEL[k] + f * KERNEL[(k + 1).min(m - 1)])
        })
        .collect();
    let sum: f32 = w.iter().sum();
    w.into_iter().map(|v| v / sum).collect()
}

pub struct Smoother {
    w: Vec<f32>,
    hist: Vec<Sample>,
}

impl Smoother {
    pub fn new(smooth_level: f32) -> Self {
        Smoother { w: weights(smooth_level), hist: Vec::new() }
    }

    /// Feed a raw sample; returns the filtered one. Non-finite samples are dropped: one NaN in the history would
    /// make every later point NaN.
    pub fn push(&mut self, s: Sample) -> Option<Sample> {
        if !s.is_finite() {
            return None;
        }
        if self.hist.is_empty() {
            self.hist = vec![s; self.w.len()];
        } else {
            self.hist.remove(0);
            self.hist.push(s);
        }
        let mut out = Sample { t_ms: s.t_ms, ..zero() };
        for (w, h) in self.w.iter().zip(&self.hist) {
            out.x += w * h.x;
            out.y += w * h.y;
            out.pressure += w * h.pressure;
            out.tilt_x += w * h.tilt_x;
            out.tilt_y += w * h.tilt_y;
        }
        Some(out)
    }

    /// Pen-up: the up point pushed max(4, ⌊4N/15⌋) times; every filtered point on the way out.
    pub fn finish(&mut self, s: Sample) -> Vec<Sample> {
        if !s.is_finite() {
            return Vec::new();
        }
        let flush = (self.w.len() * 4 / 15).max(4);
        (0..flush).filter_map(|_| self.push(s)).collect()
    }
}

fn zero() -> Sample {
    Sample { x: 0.0, y: 0.0, pressure: 0.0, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32) -> Sample {
        Sample { x, y: 200.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }
    }

    #[test]
    fn the_kernel_is_the_window_at_level_0_3() {
        let w = weights(0.3);
        assert_eq!(w.len(), 15);
        for (a, b) in w.iter().zip(KERNEL) {
            assert!((a - b).abs() < 1e-6);
        }
        assert_eq!(weights(0.0).len(), 2);
        assert_eq!(weights(1.0).len(), 50);
    }

    /// Values libneo itself returned on the device for a straight line x = 100 + 4i at smoothLevel 0.6.
    #[test]
    fn matches_libneo_on_a_ramp() {
        let mut f = Smoother::new(0.6);
        let want = [100.0, 100.2287, 100.8342, 101.9641, 103.699, 106.1139, 109.1436, 112.6994, 116.6426, 120.8214];
        for (i, w) in want.iter().enumerate() {
            let got = f.push(at(100.0 + 4.0 * i as f32)).unwrap().x;
            assert!((got - w).abs() < 2e-3, "node {i}: {got} vs libneo {w}");
        }
    }

    #[test]
    fn pen_up_runs_out_to_the_up_point() {
        let mut f = Smoother::new(0.6);
        for i in 0..20 {
            f.push(at(100.0 + 4.0 * i as f32));
        }
        let tail = f.finish(at(180.0));
        assert_eq!(tail.len(), 8, "max(4, 4·30/15)");
    }
}
