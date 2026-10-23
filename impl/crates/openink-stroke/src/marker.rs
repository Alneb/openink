//! The marker: libneo's `NeoMarkerPen` (pen type 3), decoded from libneo_pen.so (`FUN_00438030` down,
//! `FUN_00438110` move, `FUN_00438224` up; docs/STROKE.md "Marker") and checked against the library on the device.
//!
//! No smoothing and no spline: each raw sample is joined to the previous one by a round-capped line whose width
//! steps with the sample's pressure (W·0.8 up to p 0.3, W·0.9 up to 0.6, else W). knote2 draws it translucent.

use crate::geom;
use crate::pen::{Output, PenConfig};
use crate::stroke::{Ink, Sample};

/// The pressure steps and their width factors.
const LIGHT: (f32, f32) = (0.3, 0.8);
const MEDIUM: (f32, f32) = (0.6, 0.9);

pub struct MarkerStroke {
    width: f32,
    last: (f32, f32),
}

impl MarkerStroke {
    pub fn down(cfg: &PenConfig, s: Sample) -> (MarkerStroke, Ink) {
        let mut m = MarkerStroke { width: cfg.width, last: (s.x, s.y) };
        let mut ink = Ink::empty(Output::Translucent);
        m.line(&mut ink, s);
        (m, ink)
    }

    /// A move or the up point: the same rule.
    pub fn to(&mut self, s: Sample) -> Ink {
        let mut ink = Ink::empty(Output::Translucent);
        self.line(&mut ink, s);
        ink
    }

    pub fn last_sample(&self) -> Sample {
        Sample { x: self.last.0, y: self.last.1, pressure: 0.0, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }
    }

    fn line(&mut self, ink: &mut Ink, s: Sample) {
        let r = self.width_at(s.pressure) / 2.0;
        geom::capsule(&mut ink.data, (self.last.0, self.last.1, r), (s.x, s.y, r));
        ink.grow_by(self.last.0, self.last.1, r + 1.0);
        ink.grow_by(s.x, s.y, r + 1.0);
        self.last = (s.x, s.y);
    }

    fn width_at(&self, p: f32) -> f32 {
        self.width
            * if p <= LIGHT.0 {
                LIGHT.1
            } else if p <= MEDIUM.0 {
                MEDIUM.1
            } else {
                1.0
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::PenKind;

    /// libneo on the device at W 10: 8 at p ≤ 0.3, 9 at ≤ 0.6, 10 above.
    #[test]
    fn width_steps_with_pressure() {
        let (m, _) = MarkerStroke::down(&PenConfig { width: 10.0, ..PenConfig::knote2_default(PenKind::Marker, 300.0) }, Sample { x: 0.0, y: 0.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 });
        assert_eq!([0.1, 0.3, 0.31, 0.6, 0.7, 1.0].map(|p| m.width_at(p)), [8.0, 8.0, 9.0, 9.0, 10.0, 10.0]);
    }
}
