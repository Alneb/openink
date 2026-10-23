//! The brush: libneo's `NeoBrushPen` (pen type 1), decoded from libneo_pen.so (`FUN_00436ed8` down,
//! `FUN_00437004` move, `FUN_004371e0` up, `FUN_00437864` width, `FUN_00437bc8` pieces; docs/STROKE.md "Brush")
//! and checked against the library on the device (impl/java/neo-oracle).
//!
//! It shares nothing with the other pens' pipeline: no smoothing filter, no spline. Positions are the raw samples.
//! The width is W2·√p (W2 = twice the configured width) plus an "ink" term that grows when the pen slows down and
//! shrinks when it speeds up, and the pen pools ink while it rests within 3 px of where it touched down. Each
//! output point is the end of a round-capped line from the previous one, as wide as the point
//! (knote2's `PenUtils.drawStrokeByPointSize`).

use crate::geom;
use crate::pen::PenConfig;
use crate::stroke::{Ink, Sample};

/// The constants `BrushPenFactory` (`FUN_00434bc4`) hands the pen; their offsets in `NeoBrushPen::Impl`.
/// +0x68/+0x70: time between samples is taken as a fixed 3 (not the timestamps).
const DT: f32 = 3.0;
/// +0x78: weight of the previous sample in the speed and pressure averages.
const PREV_WEIGHT: f32 = 0.5;
/// +0x7c: pen-down width factor.
const DOWN_FACTOR: f32 = 1.0;
/// +0x80, +0x84: pooling while resting: the width grows by W2 · 0.01 · DT / 16 per sample.
const POOL_RATE: f32 = 0.01;
const POOL_DIV: f32 = 16.0;
/// +0x88, +0x8c: speed ratios below/above which the ink term grows/shrinks.
const SLOWER: f32 = 0.95;
const FASTER: f32 = 1.1;
/// +0x90, +0x94, +0x98: ink change (1 − e^−W2)·W2·0.02 + 1.2·r^0.5.
const INK_BASE: f32 = 0.02;
const INK_RATIO: f32 = 1.2;
const INK_EXP: f32 = 0.5;
/// +0xa4: pressure exponent.
const PRESSURE_EXP: f32 = 0.5;
/// Resting is moving less than this from the down point (px).
const REST_RADIUS: f32 = 3.0;
/// Widths reach at most W2 + 3; the ink term stays in [−0.8·W2, 3].
const OVER_MAX: f32 = 3.0;
const INK_MIN: f32 = -0.8;

#[derive(Clone, Copy, Debug)]
struct Pt {
    x: f32,
    y: f32,
    p: f32,
}

impl Pt {
    fn of(s: Sample) -> Pt {
        Pt { x: s.x, y: s.y, p: s.pressure.max(0.0) }
    }

    fn dist(self, o: Pt) -> f32 {
        (self.x - o.x).hypot(self.y - o.y)
    }
}

/// A brush stroke between pen-down and pen-up.
pub struct BrushStroke {
    /// Twice the configured width (`Impl+0x24`).
    w2: f32,
    min_width: f32,
    down: Pt,
    /// The last three accepted points, oldest first, and how many were accepted in all.
    pts: [Pt; 3],
    count: usize,
    /// Past the resting phase (`Impl+0xa8`).
    moving: bool,
    /// Current width (`Impl+0xb8`) and ink term (`Impl+0xbc`).
    size: f32,
    ink: f32,
    /// The previous output point: the next line starts there.
    last: (f32, f32),
}

impl BrushStroke {
    pub fn down(cfg: &PenConfig, s: Sample) -> (BrushStroke, Ink) {
        let w2 = 2.0 * cfg.width;
        let p = Pt::of(s);
        let mut b = BrushStroke {
            w2,
            min_width: cfg.min_width,
            down: p,
            pts: [p; 3],
            count: 1,
            moving: false,
            size: 0.0,
            ink: 0.0,
            last: (p.x, p.y),
        };
        b.size = b.clamp_width(DOWN_FACTOR * w2 * p.p.powf(PRESSURE_EXP));
        let mut ink = Ink::empty_fill();
        b.emit(&mut ink, p.x, p.y, b.size);
        (b, ink)
    }

    pub fn moved(&mut self, s: Sample) -> Ink {
        let mut ink = Ink::empty_fill();
        let p = Pt::of(s);
        if !self.moving {
            if p.dist(self.down) <= REST_RADIUS {
                // Resting: ink pools at the down point.
                self.size = self.clamp_width(self.size + self.w2 * POOL_RATE * DT / POOL_DIV);
                self.ink = self.clamp_ink(self.size - self.w2 * p.p.powf(PRESSURE_EXP));
                self.emit(&mut ink, self.down.x, self.down.y, self.size);
                return ink;
            }
            self.moving = true;
        }
        self.draw_pending(&mut ink);
        self.accept(p);
        ink
    }

    pub fn up(mut self, s: Sample) -> Ink {
        let mut ink = Ink::empty_fill();
        let p = Pt::of(s);
        if !self.moving {
            self.emit(&mut ink, p.x, p.y, self.size);
            return ink;
        }
        self.draw_pending(&mut ink);
        self.accept(p);
        let [a, b, c] = self.pts;
        // Ending no faster than before: taper to the pressure's own width; otherwise the usual rule.
        if c.dist(b) / b.dist(a) <= FASTER {
            let end = self.clamp_width(self.w2 * c.p.powf(PRESSURE_EXP));
            self.pieces(&mut ink, b, c, self.size, end);
            self.size = end;
        } else {
            self.widen(&mut ink, a, b, c);
        }
        ink
    }

    /// Where the stroke is now (for a pen-up without a usable point).
    pub fn last_sample(&self) -> Sample {
        let c = self.pts[2];
        Sample { x: c.x, y: c.y, pressure: c.p, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }
    }

    fn accept(&mut self, p: Pt) {
        self.pts = [self.pts[1], self.pts[2], p];
        self.count += 1;
    }

    /// The segment up to the newest accepted point, drawn when the next one arrives (`FUN_004376e0`).
    fn draw_pending(&mut self, ink: &mut Ink) {
        match self.count {
            0 | 1 => {}
            2 => {
                let c = self.pts[2];
                self.emit(ink, c.x, c.y, self.size);
            }
            _ => {
                let [a, b, c] = self.pts;
                self.widen(ink, a, b, c);
            }
        }
    }

    /// `FUN_00437864`: the width at `c` from the speed change over a → b → c and the pressures at b and c.
    fn widen(&mut self, ink: &mut Ink, a: Pt, b: Pt, c: Pt) {
        let v1 = b.dist(a) / DT;
        let v2 = c.dist(b) / DT;
        let r = if v1 > 0.0 { ((v2 + v1 * PREV_WEIGHT) / (PREV_WEIGHT + 1.0)) / v1 } else { 0.0 };
        let change = || (1.0 - (-self.w2).exp()) * self.w2 * INK_BASE + INK_RATIO * r.powf(INK_EXP);
        let delta = if r <= 0.0 {
            0.0
        } else if r < SLOWER {
            change() * 0.5
        } else if r > FASTER {
            change() / -3.0
        } else {
            0.0
        };
        self.ink = self.clamp_ink(self.ink + delta);
        let pm = (PREV_WEIGHT * b.p + c.p) / (PREV_WEIGHT + 1.0);
        let new = self.clamp_width(self.ink + self.w2 * pm.powf(PRESSURE_EXP));
        self.pieces(ink, b, c, self.size, new);
        self.size = new;
    }

    /// `FUN_00437bc8`: b → c in ⌊|Δwidth|⌋ + 1 equal pieces, the width interpolated linearly.
    fn pieces(&mut self, ink: &mut Ink, b: Pt, c: Pt, from: f32, to: f32) {
        let n = (to - from).abs().floor() as usize + 1;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            self.emit(ink, b.x + (c.x - b.x) * t, b.y + (c.y - b.y) * t, from + (to - from) * t);
        }
    }

    /// An output point: a round-capped line from the previous one, `size` wide.
    fn emit(&mut self, ink: &mut Ink, x: f32, y: f32, size: f32) {
        let r = size / 2.0;
        geom::capsule(&mut ink.data, (self.last.0, self.last.1, r), (x, y, r));
        ink.grow_by(self.last.0, self.last.1, r + 1.0);
        ink.grow_by(x, y, r + 1.0);
        self.last = (x, y);
    }

    fn clamp_width(&self, w: f32) -> f32 {
        w.min(self.w2 + OVER_MAX).max(self.min_width)
    }

    fn clamp_ink(&self, v: f32) -> f32 {
        v.min(OVER_MAX).max(INK_MIN * self.w2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::PenKind;

    fn cfg() -> PenConfig {
        PenConfig { width: 10.0, min_width: 1.0, ..PenConfig::knote2_default(PenKind::Brush, 300.0) }
    }

    fn at(x: f32, p: f32) -> Sample {
        Sample { x, y: 200.0, pressure: p, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }
    }

    /// Run a stroke and collect the widths of every output point, the way the oracle prints them.
    fn widths(xs: &[(f32, f32)]) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        let (mut b, _) = BrushStroke::down(&cfg(), at(xs[0].0, xs[0].1));
        out.push(vec![b.size]);
        for &(x, p) in &xs[1..xs.len() - 1] {
            let n = b.moved(at(x, p)).data.len();
            out.push(if n == 0 { vec![] } else { vec![b.size] });
        }
        b.up(at(xs[xs.len() - 1].0, xs[xs.len() - 1].1));
        out
    }

    /// libneo on the device: 2 px per sample, p 0.5 then 0.9 (impl/java/neo-oracle, 2026-10-18).
    #[test]
    fn matches_libneo_slow_start_and_ink() {
        let xs: Vec<(f32, f32)> = (0..16).map(|i| (100.0 + 2.0 * i as f32, if i < 6 { 0.5 } else { 0.9 })).collect();
        let w = widths(&xs);
        let close = |a: f32, b: f32| (a - b).abs() < 2e-3;
        assert!(close(w[0][0], 14.1421), "down {w:?}");
        assert!(close(w[1][0], 14.1796), "pooling while within 3 px: {w:?}");
        assert!(w[2].is_empty(), "leaving the rest radius draws nothing yet: {w:?}");
        assert!(close(w[3][0], 14.1796) && close(w[4][0], 14.8695), "slowing down adds (0.4 + 1.2·√(2/3))/2: {w:?}");
        assert!(close(w[7][0], 18.2393) && close(w[8][0], 19.7011), "{w:?}");
    }

    #[test]
    fn constant_speed_is_two_w_sqrt_p() {
        let xs: Vec<(f32, f32)> = (0..12).map(|i| (100.0 + 6.0 * i as f32, 0.5)).collect();
        let w = widths(&xs);
        assert!(w.iter().flatten().all(|&v| (v - 14.1421).abs() < 2e-3), "{w:?}");
    }
}
