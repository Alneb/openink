//! Flat `float[]` encoding of a `Recognition`, shared with org.openink.shape.ShapeRecognizer (Java).
//!
//! `[status]` then:
//! - status 0 Rejected: nothing more.
//! - status 1 ScratchOut: `n, x0, y0, … x(n-1), y(n-1)` (hull).
//! - status 2 Shape: `count`, then per candidate (best first): `class, knote2Type, cost, prims`, then per
//!   primitive `kind` and its fields:
//!   kind 0 Line `x1 y1 x2 y2 d1 d2`; kind 1 Arc `cx cy rx ry phi start sweep d1 d2`.
//!
//! Decorations: 0 none, 1 open arrow head (two barbs), 2 closed triangle head. Codes are explicit so that reordering an enum cannot shift them.

use crate::model::{Decoration, Primitive, Recognition, ShapeClass};

pub const STATUS_REJECTED: f32 = 0.0;
pub const STATUS_SCRATCH_OUT: f32 = 1.0;
pub const STATUS_SHAPE: f32 = 2.0;
/// Candidates sent across: the best and a few alternatives (for a "not this one" UI).
pub const MAX_CANDIDATES: usize = 3;

pub const fn class_code(c: ShapeClass) -> i32 {
    use ShapeClass::*;
    match c {
        Line => 0,
        Polyline => 1,
        Polygon => 2,
        Triangle => 3,
        IsoscelesTriangle => 4,
        RightTriangle => 5,
        RightIsoscelesTriangle => 6,
        EquilateralTriangle => 7,
        Quadrilateral => 8,
        Trapezoid => 9,
        Parallelogram => 10,
        Rhombus => 11,
        Rectangle => 12,
        Square => 13,
        Ellipse => 14,
        Circle => 15,
        ArcOfEllipse => 16,
        ArcOfCircle => 17,
        Arrow => 18,
        DoubleArrow => 19,
        PolylineArrow => 20,
        PolylineDoubleArrow => 21,
        CurvedArrow => 22,
        CurvedDoubleArrow => 23,
        ScratchOut => 24,
    }
}

const fn deco(d: Decoration) -> f32 {
    match d {
        Decoration::None => 0.0,
        Decoration::ArrowHead => 1.0,
        Decoration::TriangleHead => 2.0,
    }
}

pub fn encode(r: &Recognition) -> Vec<f32> {
    match r {
        Recognition::Rejected => vec![STATUS_REJECTED],
        Recognition::ScratchOut { hull } => {
            let mut v = vec![STATUS_SCRATCH_OUT, hull.len() as f32];
            v.extend(hull.iter().flat_map(|p| [p.x, p.y]));
            v
        }
        Recognition::Shape(cands) => {
            let cands = &cands[..cands.len().min(MAX_CANDIDATES)];
            let mut v = vec![STATUS_SHAPE, cands.len() as f32];
            for c in cands {
                v.extend([class_code(c.class) as f32, c.class.knote2_type() as f32, c.cost, c.primitives.len() as f32]);
                for p in &c.primitives {
                    match *p {
                        Primitive::Line { p1, p2, d1, d2 } => v.extend([0.0, p1.x, p1.y, p2.x, p2.y, deco(d1), deco(d2)]),
                        Primitive::Arc { centre, rx, ry, phi, start, sweep, d1, d2 } => {
                            v.extend([1.0, centre.x, centre.y, rx, ry, phi, start, sweep, deco(d1), deco(d2)])
                        }
                    }
                }
            }
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;
    use crate::model::Candidate;

    #[test]
    fn layout() {
        let r = Recognition::Shape(vec![Candidate {
            class: ShapeClass::Arrow,
            primitives: vec![Primitive::Line { p1: pt(1.0, 2.0), p2: pt(3.0, 4.0), d1: Decoration::None, d2: Decoration::ArrowHead }],
            cost: 0.5,
        }]);
        assert_eq!(encode(&r), vec![2.0, 1.0, 18.0, 28.0, 0.5, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 1.0]);
        assert_eq!(encode(&Recognition::Rejected), vec![0.0]);
        assert_eq!(encode(&Recognition::ScratchOut { hull: vec![pt(5.0, 6.0)] }), vec![1.0, 1.0, 5.0, 6.0]);
    }

    #[test]
    fn class_codes_are_distinct() {
        use ShapeClass::*;
        let all = [Line, Polyline, Polygon, Triangle, IsoscelesTriangle, RightTriangle, RightIsoscelesTriangle,
            EquilateralTriangle, Quadrilateral, Trapezoid, Parallelogram, Rhombus, Rectangle, Square, Ellipse, Circle,
            ArcOfEllipse, ArcOfCircle, Arrow, DoubleArrow, PolylineArrow, PolylineDoubleArrow, CurvedArrow,
            CurvedDoubleArrow, ScratchOut];
        let mut codes: Vec<i32> = all.iter().map(|&c| class_code(c)).collect();
        codes.sort();
        assert_eq!(codes, (0..25).collect::<Vec<_>>());
    }
}
