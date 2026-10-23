//! SurfaceFlinger's live (instant) ink on a stock Boox, as decoded in docs/SF-INSTANT-INK.md: which painter a
//! style selects, the pen config each painter builds at pen-down, its pressure mapping and its region gate.
//! [`live_pen`] gives both halves of a pen choice: what to send SF, and the config that renders the settled
//! stroke the way SF drew it, so the line does not change when it settles.

use crate::pen::{PenConfig, PenKind};

/// `SET_STROKE_STYLE` values: an index into `MemoryPainter`'s painter list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfStyle {
    /// `PainterBase`: a constant-width Skia polyline, no pressure (knote2's ballpoint and pencil).
    Plain,
    FountainV2,
    Marker,
    NeoBrush,
    Charcoal,
    Dash,
    CharcoalV2,
    Square,
    SoftEraser,
}

impl SfStyle {
    pub const fn code(self) -> i32 {
        match self {
            SfStyle::Plain => 0,
            SfStyle::FountainV2 => 1,
            SfStyle::Marker => 2,
            SfStyle::NeoBrush => 3,
            SfStyle::Charcoal => 4,
            SfStyle::Dash => 5,
            SfStyle::CharcoalV2 => 6,
            SfStyle::Square => 7,
            SfStyle::SoftEraser => 8,
        }
    }

    /// The painter knote2 picks for this pen (`ShapeFactory.getStrokeStyle`).
    pub const fn for_pen(kind: PenKind) -> SfStyle {
        match kind {
            PenKind::Ballpoint | PenKind::Pencil => SfStyle::Plain,
            PenKind::Fountain => SfStyle::FountainV2,
            PenKind::Marker => SfStyle::Marker,
            PenKind::Brush => SfStyle::NeoBrush,
            PenKind::Charcoal => SfStyle::CharcoalV2,
            PenKind::Square => SfStyle::Square,
        }
    }
}

/// libneo settings every SF painter hard-codes (`NeoPenConfig` built in each painter's `moveTo`).
pub const SF_DPI: f32 = 320.0;
pub const SF_MIN_WIDTH: f32 = 1.0;
const SF_VELOCITY_SENSITIVITY: f32 = 0.5;
/// What the Marker, NeoBrush and Square painters fix (FountainV2's two are its stroke parameters, default
/// 0.3 and 0.2).
const SF_FIXED_PRESSURE_SENSITIVITY: f32 = 0.375;
const SF_SMOOTH_LEVEL: f32 = 0.2;
/// FountainV2 draws `strokeWidth + 3`, Square `2 · strokeWidth`.
const FOUNTAIN_EXTRA: f32 = 3.0;
const SF_MIN_STROKE_WIDTH: f32 = 0.5;

/// One pen choice, both halves: what SF is told, and how to render the settled stroke to match it.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePen {
    pub style: SfStyle,
    /// `SET_STROKE_WIDTH` (also the region gate's width).
    pub sf_width: f32,
    /// `SET_STROKE_PARAMETERS` for `style`; empty = leave SF's values alone.
    pub sf_params: Vec<f32>,
    /// Render the settled stroke with this config (same samples): it is the pen SF built.
    pub settled: PenConfig,
    /// The settled render can only approximate SF's live ink (the pencil: SF draws a plain line).
    pub exact: bool,
}

/// What to send SF for `kind` at the width/feel in `want`, and the settled config that matches SF's drawing.
pub fn live_pen(kind: PenKind, want: &PenConfig) -> LivePen {
    let want = want.sanitized();
    let style = SfStyle::for_pen(kind);
    let sf = |width, pressure_sensitivity, smooth_level| PenConfig {
        width,
        pressure_sensitivity,
        smooth_level,
        velocity_sensitivity: SF_VELOCITY_SENSITIVITY,
        tilt_enabled: false,
        tilt_scale: 3.0,
        dpi: SF_DPI,
        min_width: SF_MIN_WIDTH,
    };
    match style {
        SfStyle::FountainV2 => {
            let sf_width = (want.width - FOUNTAIN_EXTRA).max(SF_MIN_STROKE_WIDTH);
            LivePen {
                style,
                sf_width,
                sf_params: vec![want.pressure_sensitivity, want.smooth_level],
                settled: sf(sf_width + FOUNTAIN_EXTRA, want.pressure_sensitivity, want.smooth_level),
                exact: true,
            }
        }
        SfStyle::Marker | SfStyle::NeoBrush => LivePen {
            style,
            sf_width: want.width,
            sf_params: Vec::new(),
            settled: sf(want.width, SF_FIXED_PRESSURE_SENSITIVITY, SF_SMOOTH_LEVEL),
            exact: true,
        },
        SfStyle::Square => {
            let sf_width = (want.width / 2.0).max(SF_MIN_STROKE_WIDTH);
            LivePen {
                style,
                sf_width,
                // shape 2, ratio 10, angle 45 (SF's defaults), and the smoothing level.
                sf_params: vec![2.0, 10.0, 45.0, want.smooth_level],
                settled: PenConfig { min_width: 0.001, ..sf(sf_width * 2.0, SF_FIXED_PRESSURE_SENSITIVITY, want.smooth_level) },
                exact: true,
            }
        }
        SfStyle::CharcoalV2 | SfStyle::Charcoal => LivePen {
            style,
            sf_width: want.width,
            sf_params: vec![if want.tilt_enabled { 1.0 } else { 0.0 }, want.tilt_scale],
            // The Charcoal painter's own config block (dpi, smoothing) is not decoded yet: keep the app's.
            settled: PenConfig { min_width: SF_MIN_WIDTH, ..want },
            exact: false,
        },
        // A constant-width polyline through the raw points: the settled ballpoint is constant width too, with
        // no smoothing so it follows the same points. The pencil's grain cannot match a plain line.
        _ => LivePen {
            style,
            sf_width: want.width,
            sf_params: Vec::new(),
            settled: PenConfig { smooth_level: 0.0, ..want },
            exact: kind == PenKind::Ballpoint,
        },
    }
}

/// `PenManager`'s pressure: `raw / max`, except for pen model 0x153 (filtered, then a curve). This device's pen
/// reports 0x152, i.e. the linear case, the same value as `MotionEvent.getPressure()`.
#[derive(Clone, Debug, Default)]
pub struct SfPressure {
    filtered: Option<i32>,
}

pub const PEN_MODEL_CURVED: u16 = 0x153;

impl SfPressure {
    /// Pen-down of model 0x152 resets the filter (`onEvent`).
    pub fn reset(&mut self) {
        self.filtered = None;
    }

    pub fn map(&mut self, raw: i32, max: f32, model: u16) -> f32 {
        if model != PEN_MODEL_CURVED {
            return raw as f32 / max;
        }
        // Integer filter as in PenManager::moveTo: p = (prev / 2 + raw) / 1.5, prev starting at raw.
        let prev = self.filtered.unwrap_or(raw);
        let p = ((prev as f64 / 2.0 + raw as f64) / 1.5) as i32;
        self.filtered = Some(p);
        let n = p as f32 / max;
        if n < 0.15 {
            n * 4.0
        } else {
            (((n as f64 - 0.135).powf(0.2) + 0.17).min(1.0)) as f32
        }
    }
}

/// An axis-aligned rect in screen px, `[left, right] × [top, bottom]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// `PenManager::inValidRegion`: which pen positions SF draws. The square of side `width` around the point must
/// lie inside a limit rect and touch no exclude rect. With `single` (region mode 1) the first limit rect a stroke
/// enters is latched until pen-up. A rejected point ends SF's instant stroke; the next accepted one starts a new
/// one, while the app's MotionEvents keep the whole stroke.
#[derive(Clone, Debug, Default)]
pub struct RegionGate {
    pub limits: Vec<Rect>,
    pub excludes: Vec<Rect>,
    pub width: f32,
    pub single: bool,
    latched: Option<usize>,
}

impl RegionGate {
    pub fn new(limits: Vec<Rect>, excludes: Vec<Rect>, width: f32, single: bool) -> Self {
        RegionGate { limits, excludes, width, single, latched: None }
    }

    /// Pen-up clears the latch (`onEvent` with value −1 resets PenManager+0x2044).
    pub fn pen_up(&mut self) {
        self.latched = None;
    }

    pub fn admits(&mut self, x: f32, y: f32) -> bool {
        let h = self.width * 0.5;
        let Some(i) = self.limits.iter().position(|r| r.left <= x - h && x + h <= r.right && r.top <= y - h && y + h <= r.bottom) else {
            return false;
        };
        if self.single {
            match self.latched {
                None => self.latched = Some(i),
                Some(l) if l != i => return false,
                _ => {}
            }
        }
        // Touching counts as overlapping (the decompiled compare is inclusive on both sides).
        !self.excludes.iter().any(|r| r.left <= x + h && x - h <= r.right && r.top <= y + h && y - h <= r.bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fountain_live_pen_is_sf_fountain_v2() {
        let want = PenConfig::knote2_default(PenKind::Fountain, 300.0);
        let p = live_pen(PenKind::Fountain, &want);
        assert_eq!(p.style.code(), 1);
        assert!((p.sf_width + 3.0 - want.width).abs() < 1e-5, "SF draws strokeWidth + 3");
        assert_eq!((p.settled.dpi, p.settled.min_width), (320.0, 1.0));
        assert_eq!(p.sf_params, vec![0.3, 0.6]);
        assert!(p.exact);
    }

    #[test]
    fn square_sends_half_width() {
        let want = PenConfig { width: 12.0, ..PenConfig::knote2_default(PenKind::Square, 300.0) };
        let p = live_pen(PenKind::Square, &want);
        assert_eq!((p.style.code(), p.sf_width, p.settled.width), (7, 6.0, 12.0));
    }

    #[test]
    fn linear_pressure_unless_model_0x153() {
        let mut m = SfPressure::default();
        assert_eq!(m.map(2048, 4096.0, 0x152), 0.5);
        m.reset();
        let low = m.map(200, 4096.0, PEN_MODEL_CURVED);
        assert!((low - 200.0 / 4096.0 * 4.0).abs() < 1e-4, "{low}");
        let mut m = SfPressure::default();
        let high = m.map(4096, 4096.0, PEN_MODEL_CURVED);
        assert_eq!(high, 1.0);
    }

    #[test]
    fn region_gate_needs_the_whole_nib_inside() {
        let r = Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 };
        let mut g = RegionGate::new(vec![r], vec![Rect { left: 40.0, top: 40.0, right: 60.0, bottom: 60.0 }], 10.0, false);
        assert!(g.admits(50.0, 20.0));
        assert!(!g.admits(3.0, 50.0), "nib crosses the limit edge");
        assert!(!g.admits(50.0, 36.0), "nib touches the exclude rect");
        assert!(g.admits(50.0, 34.0));
    }

    #[test]
    fn single_region_mode_latches_the_first_rect() {
        let a = Rect { left: 0.0, top: 0.0, right: 50.0, bottom: 50.0 };
        let b = Rect { left: 50.0, top: 0.0, right: 100.0, bottom: 50.0 };
        let mut g = RegionGate::new(vec![a, b], vec![], 2.0, true);
        assert!(g.admits(10.0, 10.0));
        assert!(!g.admits(80.0, 10.0), "latched to the first rect");
        g.pen_up();
        assert!(g.admits(80.0, 10.0));
    }
}
