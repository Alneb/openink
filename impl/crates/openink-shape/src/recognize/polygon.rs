//! Polylines and polygons: sides from the pieces, chamfers and gentle corners merged, triangles and quadrilaterals
//! specialised and idealised.

use super::{cand, centroid, lines, Cand, ANGLE_TOL, AXIS_SNAP, SPECIAL_BONUS};
use crate::fit;
use crate::geom::{line_angle_diff, wrap, Pt};
use crate::model::ShapeClass;
use crate::segment::Piece;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// A quadrilateral whose corners are all this close to square is a rectangle: hand-drawn boxes lean a corner to
/// 75–80°, and at ANGLE_TOL they came out as parallelograms and trapezoids.
const RIGHT_TOL: f32 = 18.0 * PI / 180.0;
const RIGHT_PAR_TOL: f32 = 14.0 * PI / 180.0;
/// Relative tolerance for equal lengths.
const LEN_TOL: f32 = 0.12;
/// A side shorter than this × the median side may be a rounded corner (see `polygon`).
const CHAMFER: f32 = 0.5;

/// Consecutive straight pieces whose directions agree are one side (a pen lift or a false corner).
fn merge_sides(ps: &[Piece], closed: bool) -> Vec<Vec<Pt>> {
    let mut sides: Vec<(Vec<Pt>, f32)> = Vec::new();
    for p in ps {
        match sides.last_mut() {
            Some((pts, ang)) if line_angle_diff(*ang, p.angle()) < ANGLE_TOL => {
                pts.extend_from_slice(&p.pts);
                *ang = fit_angle(pts);
            }
            _ => sides.push((p.pts.clone(), fit_angle(&p.pts))),
        }
    }
    if closed && sides.len() > 3 && line_angle_diff(sides[0].1, sides.last().unwrap().1) < ANGLE_TOL {
        let (last, _) = sides.pop().unwrap();
        sides[0].0.splice(0..0, last);
    }
    sides.into_iter().map(|(p, _)| p).collect()
}

fn fit_angle(pts: &[Pt]) -> f32 {
    let (a, b) = fit::line(pts);
    b.sub(a).angle()
}

/// Polyline vertices: side lines intersected, ends projected onto the end sides.
pub(super) fn polyline(ps: &[Piece]) -> Option<Vec<Pt>> {
    let sides = merge_sides(ps, false);
    if sides.len() < 2 {
        return None;
    }
    let ls: Vec<(Pt, Pt)> = sides.iter().map(|s| fit::line(s)).collect();
    let mut v = vec![ls[0].0];
    for w in ls.windows(2) {
        v.push(fit::intersect(w[0].0, w[0].1, w[1].0, w[1].1).unwrap_or(w[0].1.lerp(w[1].0, 0.5)));
    }
    v.push(ls.last().unwrap().1);
    Some(v)
}

/// Candidates for the closed polygon through the pieces. A rounded corner can come out as a short extra
/// side (a chamfer): while the shortest side is under `CHAMFER` × the median side, the polygon without it is
/// tried too, and the fit residual picks between them.
pub(super) fn polygon(ps: &[Piece]) -> Vec<Cand> {
    let sides = merge_sides(ps, true);
    let mut out = chamfered(sides.clone());
    // A side split at a slight bend (Atlas's hexagons: one extra corner turning 8–11°) is one side: also try the
    // polygon with its gentlest corners merged while they turn less than GENTLE_TURN.
    let mut merged = sides;
    while let Some(next) = merge_gentlest(&merged) {
        out.extend(polygon_of(&next.iter().map(|s| fit::line(s)).collect::<Vec<_>>()));
        merged = next;
    }
    out
}

/// A corner turning less than this is not a polygon corner.
const GENTLE_TURN: f32 = 20.0 * PI / 180.0;

/// The sides with the two that meet at the gentlest corner joined, if that corner turns less than GENTLE_TURN.
fn merge_gentlest(sides: &[Vec<Pt>]) -> Option<Vec<Vec<Pt>>> {
    let n = sides.len();
    if n <= 3 {
        return None;
    }
    let ang: Vec<f32> = sides.iter().map(|s| fit_angle(s)).collect();
    let (i, turn) = (0..n).map(|i| (i, line_angle_diff(ang[i], ang[(i + 1) % n]))).min_by(|a, b| a.1.total_cmp(&b.1))?;
    if turn >= GENTLE_TURN {
        return None;
    }
    let j = (i + 1) % n;
    let mut out: Vec<Vec<Pt>> = Vec::with_capacity(n - 1);
    for k in 0..n {
        if k == j {
            continue;
        }
        if k == i {
            let mut s = sides[i].clone();
            s.extend_from_slice(&sides[j]);
            out.push(s);
        } else {
            out.push(sides[k].clone());
        }
    }
    if j == 0 {
        // side i (the last) absorbed side 0: keep the order starting from the merged side
        out.rotate_right(1);
    }
    Some(out)
}

/// The polygon of `sides`, and again without its shortest side while that side is a chamfered corner (CHAMFER).
fn chamfered(mut sides: Vec<Vec<Pt>>) -> Vec<Cand> {
    let mut out = Vec::new();
    while sides.len() >= 3 {
        let ls: Vec<(Pt, Pt)> = sides.iter().map(|s| fit::line(s)).collect();
        out.extend(polygon_of(&ls));
        if sides.len() == 3 {
            break;
        }
        let lens: Vec<f32> = ls.iter().map(|(a, b)| a.dist(*b)).collect();
        let mut sorted = lens.clone();
        sorted.sort_by(f32::total_cmp);
        let (short, len) = lens.iter().copied().enumerate().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        if len >= CHAMFER * sorted[sorted.len() / 2] {
            break;
        }
        sides.remove(short);
    }
    out
}

/// The polygon whose sides lie on `ls` (consecutive lines intersected), generic and specialised.
fn polygon_of(ls: &[(Pt, Pt)]) -> Vec<Cand> {
    let n = ls.len();
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (ls[i], ls[(i + 1) % n]);
        v.push(fit::intersect(a.0, a.1, b.0, b.1).unwrap_or(a.1.lerp(b.0, 0.5)));
    }
    let general = match n {
        3 => ShapeClass::Triangle,
        4 => ShapeClass::Quadrilateral,
        _ => ShapeClass::Polygon,
    };
    let mut out = vec![cand(general, lines(&v, true), 0.0)];
    let special = match n {
        3 => triangle(&v),
        4 => quad(&v),
        _ => None,
    };
    if let Some((class, sv)) = special {
        out.push(cand(class, lines(&axis_snap(&sv), true), SPECIAL_BONUS));
    } else if n <= 4 {
        out[0].prims = lines(&axis_snap(&v), true);
    }
    out
}

/// Interior angle at each vertex.
fn angles(v: &[Pt]) -> Vec<f32> {
    let n = v.len();
    (0..n)
        .map(|i| {
            let (a, b) = (v[(i + n - 1) % n].sub(v[i]), v[(i + 1) % n].sub(v[i]));
            wrap(b.angle() - a.angle()).abs()
        })
        .collect()
}

fn side_lens(v: &[Pt]) -> Vec<f32> {
    let n = v.len();
    (0..n).map(|i| v[i].dist(v[(i + 1) % n])).collect()
}

fn near_eq(a: f32, b: f32) -> bool {
    (a - b).abs() <= LEN_TOL * a.max(b)
}

/// Most specific triangle class within tolerance, with its idealised vertices.
fn triangle(v: &[Pt]) -> Option<(ShapeClass, Vec<Pt>)> {
    let ang = angles(v);
    let l = side_lens(v);
    if ang.iter().all(|a| (a - PI / 3.0).abs() < ANGLE_TOL) {
        let c = centroid(v);
        let r = v.iter().map(|p| p.dist(c)).sum::<f32>() / 3.0;
        let a0 = v[0].sub(c).angle();
        let dir = orient(v);
        let reg = (0..3).map(|k| c.add(Pt::unit(a0 + dir * k as f32 * TAU / 3.0).scale(r))).collect();
        return Some((ShapeClass::EquilateralTriangle, reg));
    }
    let right = (0..3).find(|&i| (ang[i] - FRAC_PI_2).abs() < ANGLE_TOL);
    // Apex i is opposite side (i+1)→(i+2); the legs from i are sides i-1 and i.
    let iso = (0..3).find(|&i| near_eq(l[(i + 2) % 3], l[i]));
    match (right, iso) {
        (Some(i), Some(j)) if i == j => Some((ShapeClass::RightIsoscelesTriangle, apex_on_bisector(v, i, true))),
        (Some(i), _) => Some((ShapeClass::RightTriangle, right_apex(v, i))),
        (None, Some(i)) => Some((ShapeClass::IsoscelesTriangle, apex_on_bisector(v, i, false))),
        _ => None,
    }
}

/// Move apex `i` onto the perpendicular bisector of the opposite side (and onto Thales' circle if `right`).
fn apex_on_bisector(v: &[Pt], i: usize, right: bool) -> Vec<Pt> {
    let (b, c) = (v[(i + 1) % 3], v[(i + 2) % 3]);
    let m = b.lerp(c, 0.5);
    let nrm = c.sub(b).rotate(FRAC_PI_2).scale(1.0 / b.dist(c).max(1e-6));
    let h = if right { b.dist(c) * 0.5 } else { v[i].sub(m).dot(nrm).abs() };
    let s = if v[i].sub(m).dot(nrm) >= 0.0 { 1.0 } else { -1.0 };
    let mut out = v.to_vec();
    out[i] = m.add(nrm.scale(h * s));
    out
}

/// Move the right-angle vertex `i` radially onto the circle over the hypotenuse.
fn right_apex(v: &[Pt], i: usize) -> Vec<Pt> {
    let (b, c) = (v[(i + 1) % 3], v[(i + 2) % 3]);
    let m = b.lerp(c, 0.5);
    let d = v[i].sub(m);
    let mut out = v.to_vec();
    out[i] = m.add(d.scale(b.dist(c) * 0.5 / d.len().max(1e-6)));
    out
}

/// Most specific quadrilateral class within tolerance, with its idealised vertices.
fn quad(v: &[Pt]) -> Option<(ShapeClass, Vec<Pt>)> {
    let dir = |i: usize| v[(i + 1) % 4].sub(v[i]).angle();
    let par = |i: usize| line_angle_diff(dir(i), dir(i + 2)) < ANGLE_TOL;
    let ang = angles(v);
    let l = side_lens(v);
    // Square corners, and both pairs of sides near parallel (a trapezoid's legs are not).
    let side_par = |i: usize| line_angle_diff(dir(i), dir(i + 2)) < RIGHT_PAR_TOL;
    let right = ang.iter().all(|a| (a - FRAC_PI_2).abs() < RIGHT_TOL) && side_par(0) && side_par(1);
    let equal = (1..4).all(|i| near_eq(l[0], l[i]));
    let c = centroid(v);
    if right {
        // Rectangle from the mean side direction and averaged opposite side lengths.
        let a0 = mean_dir(dir(0), dir(2) + PI);
        let (w, h) = ((l[0] + l[2]) * 0.5, (l[1] + l[3]) * 0.5);
        let (w, h) = if equal { let s = (w + h) * 0.5; (s, s) } else { (w, h) };
        let (u, n) = (Pt::unit(a0), Pt::unit(a0).rotate(FRAC_PI_2 * orient(v)));
        let r = [u.scale(-w / 2.0).add(n.scale(-h / 2.0)), u.scale(w / 2.0).add(n.scale(-h / 2.0)),
                 u.scale(w / 2.0).add(n.scale(h / 2.0)), u.scale(-w / 2.0).add(n.scale(h / 2.0))];
        let class = if equal { ShapeClass::Square } else { ShapeClass::Rectangle };
        return Some((class, r.iter().map(|p| c.add(*p)).collect()));
    }
    if par(0) && par(1) {
        // Parallelogram: average the opposite side vectors around the centroid.
        let s0 = v[1].sub(v[0]).sub(v[3].sub(v[2])).scale(0.5);
        let s1 = v[2].sub(v[1]).sub(v[0].sub(v[3])).scale(0.5);
        let (s0, s1, class) = if equal {
            let m = (s0.len() + s1.len()) * 0.5;
            (s0.scale(m / s0.len().max(1e-6)), s1.scale(m / s1.len().max(1e-6)), ShapeClass::Rhombus)
        } else {
            (s0, s1, ShapeClass::Parallelogram)
        };
        let p0 = c.sub(s0.scale(0.5)).sub(s1.scale(0.5));
        return Some((class, vec![p0, p0.add(s0), p0.add(s0).add(s1), p0.add(s1)]));
    }
    if let Some(i) = (0..2).find(|&i| par(i)) {
        // Trapezoid: rotate the shorter of the two parallel sides about its midpoint.
        let (j, k) = if l[i] >= l[i + 2] { (i, i + 2) } else { (i + 2, i) };
        let mut out = v.to_vec();
        let m = v[k].lerp(v[(k + 1) % 4], 0.5);
        let u = Pt::unit(dir(j) + PI).scale(l[k] * 0.5);
        out[k] = m.sub(u);
        out[(k + 1) % 4] = m.add(u);
        return Some((ShapeClass::Trapezoid, out));
    }
    None
}

fn orient(v: &[Pt]) -> f32 {
    if crate::geom::area(v) >= 0.0 { 1.0 } else { -1.0 }
}

/// Mean of two directions that should agree (mod π).
fn mean_dir(a: f32, b: f32) -> f32 {
    let d = wrap(b - a);
    let d = if d.abs() > FRAC_PI_2 { wrap(d - PI.copysign(d)) } else { d };
    a + d * 0.5
}

/// Rotate the polygon about its centroid so the side nearest an axis lies on it, when within AXIS_SNAP.
fn axis_snap(v: &[Pt]) -> Vec<Pt> {
    let n = v.len();
    let best = (0..n)
        .map(|i| {
            let a = v[(i + 1) % n].sub(v[i]).angle();
            wrap((a / FRAC_PI_2).round() * FRAC_PI_2 - a)
        })
        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        .unwrap_or(0.0);
    if best.abs() >= AXIS_SNAP {
        return v.to_vec();
    }
    let c = centroid(v);
    v.iter().map(|p| p.sub(c).rotate(best).add(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    fn class_of(v: &[Pt]) -> Option<ShapeClass> {
        match v.len() {
            3 => triangle(v).map(|c| c.0),
            4 => quad(v).map(|c| c.0),
            _ => None,
        }
    }

    #[test]
    fn quads_specialise() {
        let q = |a: [(f32, f32); 4]| a.iter().map(|&(x, y)| pt(x, y)).collect::<Vec<_>>();
        assert_eq!(class_of(&q([(0.0, 0.0), (10.0, 0.3), (10.2, 6.0), (0.1, 5.8)])), Some(ShapeClass::Rectangle));
        // A hand-drawn box with one corner leaning to ~80° (and the sides ~8° off parallel) is still a rectangle.
        assert_eq!(class_of(&q([(0.0, 0.0), (100.0, 4.0), (104.0, 62.0), (-4.0, 58.0)])), Some(ShapeClass::Rectangle));
        // A trapezoid's legs are far from parallel: it stays one.
        assert_eq!(class_of(&q([(0.0, 0.0), (100.0, 0.0), (80.0, 60.0), (20.0, 60.0)])), Some(ShapeClass::Trapezoid));
        assert_eq!(class_of(&q([(0.0, 0.0), (10.0, 0.2), (9.8, 10.1), (0.2, 9.9)])), Some(ShapeClass::Square));
        assert_eq!(class_of(&q([(0.0, 0.0), (10.0, 0.0), (13.0, 6.0), (3.0, 6.0)])), Some(ShapeClass::Parallelogram));
        assert_eq!(class_of(&q([(0.0, 5.0), (5.0, 0.0), (10.0, 5.0), (5.0, 10.0)])), Some(ShapeClass::Square));
        // A rhombus needs clearly skewed corners (62° here): at 77° a hand-drawn diamond is read as a square.
        assert_eq!(class_of(&q([(0.0, 5.0), (3.0, 0.0), (6.0, 5.0), (3.0, 10.0)])), Some(ShapeClass::Rhombus));
        assert_eq!(class_of(&q([(0.0, 0.0), (12.0, 0.0), (9.0, 5.0), (3.0, 5.0)])), Some(ShapeClass::Trapezoid));
        assert_eq!(class_of(&q([(0.0, 0.0), (12.0, 2.0), (9.0, 7.0), (1.0, 4.0)])), None);
    }

    #[test]
    fn triangles_specialise() {
        let t = |a: [(f32, f32); 3]| a.iter().map(|&(x, y)| pt(x, y)).collect::<Vec<_>>();
        let h = 3f32.sqrt() * 5.0;
        assert_eq!(class_of(&t([(0.0, 0.0), (10.0, 0.0), (5.0, h)])), Some(ShapeClass::EquilateralTriangle));
        assert_eq!(class_of(&t([(0.0, 0.0), (10.0, 0.0), (0.0, 6.0)])), Some(ShapeClass::RightTriangle));
        assert_eq!(class_of(&t([(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)])), Some(ShapeClass::RightIsoscelesTriangle));
        assert_eq!(class_of(&t([(0.0, 0.0), (6.0, 0.0), (3.0, 12.0)])), Some(ShapeClass::IsoscelesTriangle));
        assert_eq!(class_of(&t([(0.0, 0.0), (10.0, 0.0), (7.0, 3.0)])), None);
    }

    #[test]
    fn idealised_rectangle_is_exact() {
        let v = [pt(0.0, 0.0), pt(10.0, 0.3), pt(10.2, 6.0), pt(0.1, 5.8)];
        let (_, r) = quad(&v).unwrap();
        for a in angles(&r) {
            assert!((a - FRAC_PI_2).abs() < 1e-4);
        }
    }
}
