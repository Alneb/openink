//! The recogniser's vocabulary: families (what the classifier decides), classes (the most specific
//! shape that fits) and the output primitives. Same class set as MyScript's shape knowledge
//! (docs/SHAPE-RE.md), so everything knote2 can draw has a target.

use crate::geom::Pt;

/// What the whole-stroke classifier decides. The index order is the network's output order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    Line = 0,
    Polyline = 1,
    Polygon = 2,
    Ellipse = 3,
    Arc = 4,
    Arrow = 5,
    ScratchOut = 6,
    /// Writing, doodles: anything that must not snap.
    Reject = 7,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Line,
        Family::Polyline,
        Family::Polygon,
        Family::Ellipse,
        Family::Arc,
        Family::Arrow,
        Family::ScratchOut,
        Family::Reject,
    ];
    pub const COUNT: usize = 8;
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(i: usize) -> Option<Family> {
        Self::ALL.get(i).copied()
    }
}

/// The most specific recognised shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeClass {
    Line,
    Polyline,
    Polygon,
    Triangle,
    IsoscelesTriangle,
    RightTriangle,
    RightIsoscelesTriangle,
    EquilateralTriangle,
    Quadrilateral,
    Trapezoid,
    Parallelogram,
    Rhombus,
    Rectangle,
    Square,
    Ellipse,
    Circle,
    ArcOfEllipse,
    ArcOfCircle,
    Arrow,
    DoubleArrow,
    PolylineArrow,
    PolylineDoubleArrow,
    CurvedArrow,
    CurvedDoubleArrow,
    ScratchOut,
}

impl ShapeClass {
    /// knote2's `ShapeFactory` type for the shape it would store, where one exists. The rest are
    /// stored as `SHAPE_UNIVERSAL` (40), knote2's vertex-list shape (seen in the user's notes).
    pub fn knote2_type(self) -> i32 {
        use ShapeClass::*;
        match self {
            Circle | Ellipse => 0,
            Rectangle | Square => 1,
            Line => 7,
            Triangle | IsoscelesTriangle | EquilateralTriangle => 8,
            RightIsoscelesTriangle => 10,
            RightTriangle => 12,
            Polygon => 17,
            Rhombus => 18,
            Trapezoid => 26,
            Arrow => 28,
            Polyline => 31,
            ArcOfEllipse | ArcOfCircle => 39,
            Quadrilateral | Parallelogram | DoubleArrow | PolylineArrow | PolylineDoubleArrow | CurvedArrow
            | CurvedDoubleArrow => 40,
            ScratchOut => -1,
        }
    }

    pub fn family(self) -> Family {
        use ShapeClass::*;
        match self {
            Line => Family::Line,
            Polyline => Family::Polyline,
            Polygon | Triangle | IsoscelesTriangle | RightTriangle | RightIsoscelesTriangle | EquilateralTriangle
            | Quadrilateral | Trapezoid | Parallelogram | Rhombus | Rectangle | Square => Family::Polygon,
            Ellipse | Circle => Family::Ellipse,
            ArcOfEllipse | ArcOfCircle => Family::Arc,
            Arrow | DoubleArrow | PolylineArrow | PolylineDoubleArrow | CurvedArrow | CurvedDoubleArrow => Family::Arrow,
            ScratchOut => Family::ScratchOut,
        }
    }
}

/// Line or arc end ornament.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoration {
    None,
    /// Open head: two barbs.
    ArrowHead,
    /// Closed (triangle) head.
    TriangleHead,
}

/// Output primitives: every shape is a set of straight segments and elliptic arcs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Line { p1: Pt, p2: Pt, d1: Decoration, d2: Decoration },
    /// Points at `centre + R(phi)·(rx·cos t, ry·sin t)` for t in start..start+sweep (radians).
    Arc { centre: Pt, rx: f32, ry: f32, phi: f32, start: f32, sweep: f32, d1: Decoration, d2: Decoration },
}

impl Primitive {
    pub fn line(p1: Pt, p2: Pt) -> Primitive {
        Primitive::Line { p1, p2, d1: Decoration::None, d2: Decoration::None }
    }

    /// Points along the primitive, roughly `step` apart, for drawing and for fit residuals.
    pub fn sample(&self, step: f32) -> Vec<Pt> {
        match *self {
            Primitive::Line { p1, p2, .. } => {
                let n = ((p1.dist(p2) / step.max(0.1)).ceil() as usize).max(1);
                (0..=n).map(|i| p1.lerp(p2, i as f32 / n as f32)).collect()
            }
            Primitive::Arc { centre, rx, ry, phi, start, sweep, .. } => {
                let n = ((sweep.abs() * rx.max(ry) / step.max(0.1)).ceil() as usize).clamp(2, 4096);
                (0..=n)
                    .map(|i| {
                        let t = start + sweep * i as f32 / n as f32;
                        Pt { x: rx * t.cos(), y: ry * t.sin() }.rotate(phi).add(centre)
                    })
                    .collect()
            }
        }
    }
}

/// One scored interpretation of the ink. Lower cost is better.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub class: ShapeClass,
    pub primitives: Vec<Primitive>,
    /// −ln P(family) + fit residual − specialisation bonus.
    pub cost: f32,
}

/// The recogniser's answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Recognition {
    /// Best candidate first; never empty.
    Shape(Vec<Candidate>),
    /// A scratch-out gesture over this ink region: the caller erases what it covers.
    ScratchOut { hull: Vec<Pt> },
    /// Not a shape (writing, doodles): leave the ink alone.
    Rejected,
}
