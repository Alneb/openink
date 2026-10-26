//! Arrows: a shaft (straight, polyline or curved) with open (barbs) or closed (triangle) heads at either end.

use super::conic::arc;
use super::polygon::polyline;
use super::{lines, snap_line, Cand};
use crate::fit;
use crate::geom::{area, hull, path_len, perimeter_closed, wrap, BBox, Pt};
use crate::model::{Decoration, Primitive, ShapeClass};
use crate::segment::Piece;

/// A barb is a short straight piece leaving a shaft tip at 10°–80° to the shaft.
const BARB_MAX: f32 = 0.45;
const BARB_MIN: f32 = 0.05;
const TIP_NEAR: f32 = 0.25;
/// A shaft bowing more than this (sagitta / chord) is a curve.
const CURVED_BULGE: f32 = 0.07;

pub(super) fn arrow(ps: &[Piece], step: f32) -> Option<Cand> {
    let longest = ps.iter().map(Piece::len).fold(0.0, f32::max);
    if longest <= 0.0 {
        return None;
    }
    let short = |p: &Piece| !p.curved && p.len() < BARB_MAX * longest && p.len() > BARB_MIN * longest;
    // The shaft: the longest piece grown through same-stroke neighbours that are not barb-sized.
    let li = ps.iter().position(|p| p.len() == longest)?;
    let (mut a, mut b) = (li, li);
    while a > 0 && ps[a - 1].stroke == ps[li].stroke && !short(&ps[a - 1]) {
        a -= 1;
    }
    while b + 1 < ps.len() && ps[b + 1].stroke == ps[li].stroke && !short(&ps[b + 1]) {
        b += 1;
    }
    let shaft = &ps[a..=b];
    let (tail, tip) = (shaft[0].start(), shaft.last().unwrap().end());
    let into = |at: Pt, toward: Pt| toward.sub(at).angle();
    let ends = [(tail, shaft[0].pts[shaft[0].pts.len().min(4) - 1]), (tip, shaft.last().unwrap().pts[shaft.last().unwrap().pts.len().saturating_sub(4)])];
    let mut explained = Vec::new();
    // The shaft stroke's ink beyond each end: a head drawn without lifting the pen.
    let runs = [&ps[..a], &ps[b + 1..]].map(|r| -> Vec<Pt> { r.iter().filter(|p| p.stroke == ps[li].stroke).flat_map(|p| p.pts.iter().copied()).collect() });
    let heads: Vec<Decoration> = ends
        .iter()
        .zip(&runs)
        .map(|(&(end, inner), run)| {
            if let Some(head) = closed_head(ps, ps[li].stroke, end, longest) {
                explained.extend(head);
                return Decoration::TriangleHead;
            }
            if is_triangle_head(run, end, longest) {
                explained.extend(run.iter().copied());
                return Decoration::TriangleHead;
            }
            let back = into(end, inner);
            let barbs: Vec<&Piece> = ps
                .iter()
                .enumerate()
                .filter(|(k, p)| {
                    if (*k >= a && *k <= b) || !short(p) {
                        return false;
                    }
                    let (near, far) = if p.start().dist(end) <= p.end().dist(end) { (p.start(), p.end()) } else { (p.end(), p.start()) };
                    let diff = wrap(far.sub(near).angle() - back).abs();
                    near.dist(end) < TIP_NEAR * longest && diff > 10f32.to_radians() && diff < 80f32.to_radians()
                })
                .map(|(_, p)| p)
                .collect();
            explained.extend(barbs.iter().flat_map(|p| p.pts.iter().copied()));
            if barbs.is_empty() { Decoration::None } else { Decoration::ArrowHead }
        })
        .collect();
    let (h_tail, h_tip) = (heads[0], heads[1]);
    if h_tail == Decoration::None && h_tip == Decoration::None {
        return None;
    }
    let shaft_pts: Vec<Pt> = shaft.iter().flat_map(|p| p.pts.iter().copied()).collect();
    // A gentle bend reads as straight point by point, so also test how far the shaft bows from its chord.
    let curved = shaft.iter().any(|p| p.curved) || (shaft.len() == 1 && bulge(&shaft_pts) > CURVED_BULGE);
    let double = h_tail != Decoration::None && h_tip != Decoration::None;
    let mut prims;
    let class;
    if curved {
        prims = vec![arc(&shaft_pts, step)?];
        class = if double { ShapeClass::CurvedDoubleArrow } else { ShapeClass::CurvedArrow };
    } else if shaft.len() == 1 {
        prims = vec![snap_line(fit::line(&shaft[0].pts))];
        class = if double { ShapeClass::DoubleArrow } else { ShapeClass::Arrow };
    } else {
        prims = lines(&polyline(shaft)?, false);
        class = if double { ShapeClass::PolylineDoubleArrow } else { ShapeClass::PolylineArrow };
    }
    let n = prims.len();
    set_deco(&mut prims[0], true, h_tail);
    set_deco(&mut prims[n - 1], false, h_tip);
    // A verified closed head (a small triangle stroke touching the shaft end) is strong evidence on its own: the
    // classifier, trained mostly on open heads, still calls these shafts lines.
    let bonus = if h_tail == Decoration::TriangleHead || h_tip == Decoration::TriangleHead { CLOSED_HEAD_BONUS } else { 0.0 };
    Some(Cand { class, prims, bonus, explained })
}

/// A closed head is another stroke, small next to the shaft, that touches the shaft end, goes most of the
/// way round its hull and is close to a triangle (its largest inscribed triangle covers most of the hull;
/// a circle's covers 41 %, a square's 50 %).
const HEAD_MAX: f32 = 0.6;
/// Subtracted from an arrow's cost when one of its heads is a verified closed triangle (tuned on the user's arrows
/// and checked with shape-eval).
const CLOSED_HEAD_BONUS: f32 = 1.5;
const HEAD_TRIANGLE: f32 = 0.65;
const HEAD_ROUND: f32 = 0.85;

/// The head's ink, if there is one at `end`.
fn closed_head(ps: &[Piece], shaft_stroke: usize, end: Pt, longest: f32) -> Option<Vec<Pt>> {
    let mut strokes: Vec<usize> = ps.iter().map(|p| p.stroke).filter(|&s| s != shaft_stroke).collect();
    strokes.dedup();
    strokes.into_iter().find_map(|s| {
        let pts: Vec<Pt> = ps.iter().filter(|p| p.stroke == s).flat_map(|p| p.pts.iter().copied()).collect();
        let (h, d) = small_closed_triangle(&pts, longest)?;
        let touches = inside_convex(&h, end) || pts.iter().map(|p| p.dist(end)).fold(f32::MAX, f32::min) < 0.3 * d;
        touches.then_some(pts)
    })
}

/// One stroke's head ink (from `end` round and back) as a closed head: small next to the shaft, back at `end`, and
/// close to a triangle.
fn is_triangle_head(pts: &[Pt], end: Pt, longest: f32) -> bool {
    let Some((_, d)) = small_closed_triangle(pts, longest) else { return false };
    // It leaves from `end` and comes back to it (a V head ends at its second barb, away from the tip).
    pts[0].dist(end).max(pts[pts.len() - 1].dist(end)) < 0.3 * d
}

/// The test both head kinds share: ink small next to the shaft, going most of the way round its hull, whose hull is
/// close to a triangle. Returns that hull and the ink's box diagonal.
fn small_closed_triangle(pts: &[Pt], longest: f32) -> Option<(Vec<Pt>, f32)> {
    let d = BBox::of(pts).diag();
    if pts.len() < 3 || d > HEAD_MAX * longest || d < BARB_MIN * longest {
        return None;
    }
    let h = hull(pts);
    let hull_area = area(&h).abs();
    let closed = h.len() >= 3 && hull_area > 0.0 && path_len(pts) >= HEAD_ROUND * perimeter_closed(&h);
    (closed && largest_triangle(&h) >= HEAD_TRIANGLE * hull_area).then_some((h, d))
}

/// Area of the largest triangle on the vertices of a convex polygon (brute force: hulls here are small).
fn largest_triangle(h: &[Pt]) -> f32 {
    let mut best = 0.0f32;
    for i in 0..h.len() {
        for j in i + 1..h.len() {
            for k in j + 1..h.len() {
                best = best.max(h[j].sub(h[i]).cross(h[k].sub(h[i])).abs() * 0.5);
            }
        }
    }
    best
}

fn inside_convex(h: &[Pt], p: Pt) -> bool {
    let side = |i: usize| h[(i + 1) % h.len()].sub(h[i]).cross(p.sub(h[i]));
    (0..h.len()).all(|i| side(i) >= 0.0) || (0..h.len()).all(|i| side(i) <= 0.0)
}

/// Largest distance from the chord, over the chord length.
fn bulge(pts: &[Pt]) -> f32 {
    let (a, b) = (pts[0], *pts.last().unwrap());
    let chord = a.dist(b).max(1e-6);
    pts.iter().map(|p| crate::geom::seg_dist(*p, a, b)).fold(0.0, f32::max) / chord
}

fn set_deco(p: &mut Primitive, start: bool, d: Decoration) {
    match p {
        Primitive::Line { d1, d2, .. } | Primitive::Arc { d1, d2, .. } => *(if start { d1 } else { d2 }) = d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;
    use std::f32::consts::TAU;

    /// A shaft (stroke 1) ending at (100, 0) and a head stroke (stroke 0) through `head`.
    fn with_head(head: Vec<Pt>) -> Vec<Piece> {
        let shaft = (0..=50).map(|i| pt(i as f32 * 2.0, 0.0)).collect();
        vec![Piece { pts: head, stroke: 0, curved: false }, Piece { pts: shaft, stroke: 1, curved: false }]
    }

    fn path(corners: &[Pt]) -> Vec<Pt> {
        corners.windows(2).flat_map(|w| (0..10).map(move |i| w[0].lerp(w[1], i as f32 / 10.0))).chain(corners.last().copied()).collect()
    }

    #[test]
    fn a_triangle_is_a_closed_head_and_a_lollipop_is_not() {
        let tip = pt(100.0, 0.0);
        let tri = path(&[pt(80.0, -12.0), tip, pt(80.0, 12.0), pt(80.0, -12.0)]);
        assert!(closed_head(&with_head(tri), 1, tip, 100.0).is_some());
        let circle: Vec<Pt> = (0..=40).map(|i| pt(110.0, 0.0).add(Pt::unit(i as f32 / 40.0 * TAU).scale(10.0))).collect();
        assert!(closed_head(&with_head(circle), 1, tip, 100.0).is_none(), "a round end is not a head");
        let v = path(&[pt(80.0, -12.0), tip, pt(80.0, 12.0)]);
        assert!(closed_head(&with_head(v), 1, tip, 100.0).is_none(), "an open V is barbs, not a closed head");
        let far = path(&[pt(20.0, -12.0), pt(40.0, 0.0), pt(20.0, 12.0), pt(20.0, -12.0)]);
        assert!(closed_head(&with_head(far), 1, tip, 100.0).is_none(), "a triangle away from the end");
    }

    #[test]
    fn largest_inscribed_triangle() {
        let sq = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        assert!((largest_triangle(&sq) - 0.5).abs() < 1e-6);
        let tri = [pt(0.0, 0.0), pt(2.0, 0.0), pt(0.0, 2.0)];
        assert!((largest_triangle(&tri) - 2.0).abs() < 1e-6);
    }
}
