//! The knote2 pen set and its defaults.

/// One pen of knote2's toolbar. The `NEOPEN_PEN_TYPE_*` it replaces is noted on each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PenKind {
    /// Nearly constant width (BALLPOINT 8).
    Ballpoint,
    /// Pressure- and speed-shaped width (FOUNTAIN_V2 6).
    Fountain,
    /// Strong pressure response, thin hairlines to wide bellies (BRUSH 1).
    Brush,
    /// Wide, constant, translucent; self-overlaps don't darken (MARKER 3).
    Marker,
    /// Calligraphy: a flat rectangular nib at a fixed angle (SQUARE 9).
    Square,
    /// Grain-textured dabs, darker with pressure, wider with tilt (PENCIL 7).
    Pencil,
    /// Coarser, lighter grain than the pencil (CHARCOAL_V2 5).
    Charcoal,
}

/// How a pen's ink reaches the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Opaque filled convex polygons; their union is the stroke.
    Fill,
    /// Like `Fill`, but drawn opaque into a per-stroke layer that is then composited at
    /// [`MARKER_ALPHA`], as knote2's `NeoMarkerPenWrapper.drawStroke` does.
    Translucent,
    /// Grain-textured dabs `(x, y, size, angle, alpha)`, like libneo's pencil ink.
    Dabs,
}

/// Layer alpha of translucent pens (knote2 composites the marker at 128/255).
pub const MARKER_ALPHA: f32 = 128.0 / 255.0;

impl PenKind {
    pub const ALL: [PenKind; 7] = [
        PenKind::Ballpoint, PenKind::Fountain, PenKind::Brush, PenKind::Marker,
        PenKind::Square, PenKind::Pencil, PenKind::Charcoal,
    ];

    /// Wire code shared with `org.openink.stroke.PenKind`. Explicit so reordering cannot shift it.
    pub const fn code(self) -> i32 {
        match self {
            PenKind::Ballpoint => 0,
            PenKind::Fountain => 1,
            PenKind::Brush => 2,
            PenKind::Marker => 3,
            PenKind::Square => 4,
            PenKind::Pencil => 5,
            PenKind::Charcoal => 6,
        }
    }

    pub fn from_code(c: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.code() == c)
    }

    pub const fn name(self) -> &'static str {
        match self {
            PenKind::Ballpoint => "ballpoint",
            PenKind::Fountain => "fountain",
            PenKind::Brush => "brush",
            PenKind::Marker => "marker",
            PenKind::Square => "square",
            PenKind::Pencil => "pencil",
            PenKind::Charcoal => "charcoal",
        }
    }

    pub const fn output(self) -> Output {
        match self {
            PenKind::Marker => Output::Translucent,
            PenKind::Pencil | PenKind::Charcoal => Output::Dabs,
            _ => Output::Fill,
        }
    }

    /// knote2's `ShapeFactory.SHAPE_*` for this pen. Pencil and charcoal share 22 and are told
    /// apart by the shape's texture attribute.
    pub const fn knote2_shape_type(self) -> i32 {
        match self {
            PenKind::Ballpoint => 2,
            PenKind::Fountain => 5,
            PenKind::Brush => 21,
            PenKind::Marker => 15,
            PenKind::Square => 47,
            PenKind::Pencil | PenKind::Charcoal => 22,
        }
    }

    /// knote2's `PenConstant.*_DEFAULT_STROKE_WIDTH` (px on this 300 dpi panel) in mm.
    /// The square nib has no constant; 1 mm is a usable calligraphy default.
    pub const fn default_width_mm(self) -> f32 {
        const PX_TO_MM: f32 = 25.4 / 300.0;
        match self {
            PenKind::Ballpoint | PenKind::Fountain => 7.2 * PX_TO_MM,
            PenKind::Brush => 8.4 * PX_TO_MM,
            PenKind::Marker => 48.0 * PX_TO_MM,
            PenKind::Square => 1.0,
            PenKind::Pencil => 6.0 * PX_TO_MM,
            PenKind::Charcoal => 3.6 * PX_TO_MM,
        }
    }

    /// knote2's width slider range (`NotePenInfoUtils.get{Min,Max}StrokeWidth`), mm.
    pub const fn width_range_mm(self) -> (f32, f32) {
        match self {
            PenKind::Marker => (0.5, 8.0),
            _ => (0.1, 2.0),
        }
    }
}

/// Per-stroke settings. Ranges are enforced by [`PenConfig::sanitized`], which every stroke
/// applies, so an out-of-range value from Java cannot reach the geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenConfig {
    /// Nominal width in px: the stroke's diameter at full pressure (dab size for textured pens).
    pub width: f32,
    /// 0..1, knote2 default 0.3: how much pressure changes the width (fountain, brush).
    pub pressure_sensitivity: f32,
    /// 0..1, knote2 default 0.6: position/pressure smoothing strength.
    pub smooth_level: f32,
    /// 0..1, `NeoPenConfig` default 0.5: how much speed thins fountain and brush ink.
    pub velocity_sensitivity: f32,
    /// Tilt widens textured pens beyond 30° (`NeoPenConfig.tiltEnabled/tiltScale`).
    pub tilt_enabled: bool,
    /// Width factor at full tilt, `NeoPenConfig` default 3.
    pub tilt_scale: f32,
    /// `NeoPenConfig.dpi`: the speed term counts travel in 160-dpi px. The panel is 300 dpi; SurfaceFlinger's
    /// live pens say 320 (docs/SF-INSTANT-INK.md).
    pub dpi: f32,
    /// `NeoPenConfig.minWidth`, px: the thinnest a pressure-shaped line gets (knote2 0.001, SF 1).
    pub min_width: f32,
}

impl PenConfig {
    /// knote2's defaults for `kind`, at `dpi` (300 on the Note Air 5C).
    pub fn knote2_default(kind: PenKind, dpi: f32) -> Self {
        PenConfig {
            width: mm_to_px(kind.default_width_mm(), dpi),
            pressure_sensitivity: 0.3,
            smooth_level: 0.6,
            velocity_sensitivity: 0.5,
            tilt_enabled: matches!(kind.output(), Output::Dabs),
            tilt_scale: 3.0,
            dpi,
            min_width: 0.001,
        }
    }

    pub fn sanitized(self) -> Self {
        let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        PenConfig {
            width: if self.width.is_finite() { self.width.clamp(0.5, 500.0) } else { 3.0 },
            pressure_sensitivity: unit(self.pressure_sensitivity),
            smooth_level: unit(self.smooth_level),
            velocity_sensitivity: unit(self.velocity_sensitivity),
            tilt_enabled: self.tilt_enabled,
            tilt_scale: if self.tilt_scale.is_finite() { self.tilt_scale.clamp(1.0, 10.0) } else { 1.0 },
            dpi: if self.dpi.is_finite() { self.dpi.clamp(50.0, 1200.0) } else { 300.0 },
            min_width: if self.min_width.is_finite() { self.min_width.clamp(0.0, 50.0) } else { 0.001 },
        }
    }
}

pub fn mm_to_px(mm: f32, dpi: f32) -> f32 {
    mm * dpi / 25.4
}
