//! The recogniser: classifier + segmenter → fitted, specialised and beautified candidates.
//! Design and tolerances: docs/SHAPE-ENGINE.md ("Recognise").

use crate::features::{point_features, stroke_features, Ink, Norm, RESOLUTION};
use crate::fit::{self, Ellipse};
use crate::geom::{area, hull, line_angle_diff, path_len, perimeter_closed, pt, wrap, BBox, Pt};
use crate::model::{Candidate, Decoration, Family, Primitive, Recognition, ShapeClass};
use crate::nn::Models;
use crate::segment::{dp_pieces, pieces, Piece};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Families below this probability get no candidate.
const FAMILY_FLOOR: f32 = 0.03;
/// Weight of the fit residual (in resampling steps) against −ln P. Chosen with `shape-eval`
/// (docs/SHAPE-ENGINE.md, "Tuning").
pub const RESIDUAL_WEIGHT: f32 = 0.15;
/// A specialised shape that holds its tolerances beats its general form by this much.
const SPECIAL_BONUS: f32 = 0.3;
const ARC_CIRCLE_BONUS: f32 = 0.08;
/// Angle tolerance for parallel, right angle, 60° and axis snapping.
const ANGLE_TOL: f32 = 10.0 * PI / 180.0;
/// A quadrilateral whose corners are all this close to square is a rectangle: hand-drawn boxes lean a corner to
/// 75–80°, and at ANGLE_TOL they came out as parallelograms and trapezoids.
const RIGHT_TOL: f32 = 18.0 * PI / 180.0;
const RIGHT_PAR_TOL: f32 = 14.0 * PI / 180.0;
/// Within this of an axis, a shape is turned onto it (was 6°: boxes drawn 8–12° off stayed tilted).
const AXIS_SNAP: f32 = 12.0 * PI / 180.0;
/// Relative tolerance for equal lengths.
const LEN_TOL: f32 = 0.12;
/// Axis ratio above which an ellipse is a circle.
const CIRCLE_RATIO: f32 = 0.88;
/// A side shorter than this × the median side may be a rounded corner (see `polygon`).
const CHAMFER: f32 = 0.5;

pub struct Recognizer {
    pub models: Models,
    /// Added to the reject cost: higher snaps more readily.
    pub reject_margin: f32,
    /// How much a bad geometric fit counts against the classifier's confidence.
    pub residual_weight: f32,
}

/// Chosen with `shape-eval` (docs/SHAPE-ENGINE.md, "Tuning").
pub const REJECT_MARGIN: f32 = 0.0;

impl Recognizer {
    pub fn new(models: Models) -> Recognizer {
        Recognizer { models, reject_margin: REJECT_MARGIN, residual_weight: RESIDUAL_WEIGHT }
    }

    /// The recogniser with the weights shipped in the crate (`weights/shape.oisw`).
    pub fn builtin() -> Recognizer {
        static WEIGHTS: &[u8] = include_bytes!("../weights/shape.oisw");
        // The file is ours and checked by `builtin_weights_load`, so a failure here is a build defect.
        Recognizer::new(Models::from_bytes(WEIGHTS).expect("embedded weights/shape.oisw is valid"))
    }

    pub fn recognize(&self, ink: &Ink) -> Recognition {
        let Some(norm) = Norm::new(ink) else { return Recognition::Rejected };
        let probs = self.models.classifier.probabilities(&stroke_features(&norm));
        let best = (0..Family::COUNT).max_by(|&a, &b| probs[a].total_cmp(&probs[b])).unwrap();
        if Family::from_index(best) == Some(Family::ScratchOut) {
            return Recognition::ScratchOut { hull: hull(&norm.points()) };
        }
        let seg = self.models.segmenter.probabilities(&point_features(&norm));
        let ps = pieces(&norm, &seg);
        let ctx = Ctx { pts: norm.points(), step: norm.step, dp: dp_pieces(&norm), residual_weight: self.residual_weight };
        // The likely families first; if none of them yields a shape (e.g. an arrow whose head was not
        // found), every family gets a try before giving up.
        let mut cands = scored(&probs, FAMILY_FLOOR, &ps, &ctx);
        if cands.is_empty() {
            cands = scored(&probs, 0.0, &ps, &ctx);
        }
        cands.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        // Both corner sources can give the same class: keep its cheaper fit only.
        let mut seen = Vec::new();
        cands.retain(|c| {
            let new = !seen.contains(&c.class);
            seen.push(c.class);
            new
        });
        // "Not a shape" competes on cost like any candidate: the user asked for a snap, so a shape that fits
        // well wins over a doubtful reject.
        let reject_cost = -(probs[Family::Reject.index()] + 1e-6).ln() + self.reject_margin;
        if !cands.first().is_some_and(|c| c.cost < reject_cost) {
            return Recognition::Rejected;
        }
        Recognition::Shape(cands)
    }
}

fn scored(probs: &[f32], floor: f32, ps: &[Piece], ctx: &Ctx) -> Vec<Candidate> {
    let mut out = Vec::new();
    for fam in Family::ALL {
        let p = probs[fam.index()];
        if p < floor {
            continue;
        }
        for c in candidates(fam, ps, ctx) {
            let (mean, _) = fit::residual_explained(&ctx.pts, &c.prims, &c.explained, ctx.step);
            let cost = -(p + 1e-6).ln() + ctx.residual_weight * mean / ctx.step - c.bonus;
            out.push(Candidate { class: c.class, primitives: c.prims, cost });
        }
    }
    out
}

struct Ctx {
    pts: Vec<Pt>,
    step: f32,
    /// Douglas–Peucker pieces: a second corner source next to the segmenter's.
    dp: Vec<Piece>,
    residual_weight: f32,
}

struct Cand {
    class: ShapeClass,
    prims: Vec<Primitive>,
    /// Subtracted from the cost (a specialised shape that holds its tolerances).
    bonus: f32,
    /// Ink the primitives stand for without tracing it (an arrow's drawn head): counts as fitted.
    explained: Vec<Pt>,
}

fn cand(class: ShapeClass, prims: Vec<Primitive>, bonus: f32) -> Cand {
    Cand { class, prims, bonus, explained: Vec::new() }
}

fn candidates(fam: Family, ps: &[Piece], ctx: &Ctx) -> Vec<Cand> {
    match fam {
        Family::Line => vec![cand(ShapeClass::Line, vec![snap_line(fit::line(&ctx.pts))], 0.0)],
        Family::Polyline => [ps, &ctx.dp[..]]
            .iter()
            .filter_map(|p| polyline(p))
            .map(|v| cand(ShapeClass::Polyline, lines(&v, false), 0.0))
            .collect(),
        Family::Polygon => polygon(ps).into_iter().chain(polygon(&ctx.dp)).collect(),
        Family::Ellipse => ellipse(ctx),
        Family::Arc => arcs(&ctx.pts),
        Family::Arrow => arrow(ps, ctx.step).into_iter().collect(),
        Family::ScratchOut | Family::Reject => Vec::new(),
    }
}

// ---------- lines and polylines ----------

/// Snap a segment to horizontal / vertical when it is within AXIS_SNAP, keeping its midpoint and length.
fn snap_line((a, b): (Pt, Pt)) -> Primitive {
    let ang = b.sub(a).angle();
    let target = (ang / FRAC_PI_2).round() * FRAC_PI_2;
    if wrap(ang - target).abs() < AXIS_SNAP {
        let (m, h) = (a.lerp(b, 0.5), a.dist(b) * 0.5);
        let d = Pt::unit(target).scale(h);
        return Primitive::line(m.sub(d), m.add(d));
    }
    Primitive::line(a, b)
}

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
fn polyline(ps: &[Piece]) -> Option<Vec<Pt>> {
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

fn lines(v: &[Pt], closed: bool) -> Vec<Primitive> {
    let mut out: Vec<Primitive> = v.windows(2).map(|w| Primitive::line(w[0], w[1])).collect();
    if closed && v.len() > 2 {
        out.push(Primitive::line(*v.last().unwrap(), v[0]));
    }
    out
}

// ---------- polygons ----------

/// Candidates for the closed polygon through the pieces. A rounded corner can come out as a short extra
/// side (a chamfer): while the shortest side is under `CHAMFER` × the median side, the polygon without it is
/// tried too, and the fit residual picks between them.
fn polygon(ps: &[Piece]) -> Vec<Cand> {
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

fn centroid(v: &[Pt]) -> Pt {
    v.iter().fold(pt(0.0, 0.0), |s, p| s.add(*p)).scale(1.0 / v.len() as f32)
}

/// Most specific triangle class within tolerance, with its idealised vertices.
fn triangle(v: &[Pt]) -> Option<(ShapeClass, Vec<Pt>)> {
    let ang = angles(v);
    let l = side_lens(v);
    if ang.iter().all(|a| (a - PI / 3.0).abs() < ANGLE_TOL) {
        let c = centroid(v);
        let r = v.iter().map(|p| p.dist(c)).sum::<f32>() / 3.0;
        let a0 = v[0].sub(c).angle();
        let dir = if crate::geom::area(v) >= 0.0 { 1.0 } else { -1.0 };
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

// ---------- conics ----------

fn ellipse(ctx: &Ctx) -> Vec<Cand> {
    let Some(e) = fit::ellipse(&ctx.pts) else { return Vec::new() };
    let full = |e: Ellipse| Primitive::Arc { centre: e.centre, rx: e.rx, ry: e.ry, phi: e.phi, start: 0.0, sweep: TAU, d1: Decoration::None, d2: Decoration::None };
    let mut e_snap = e;
    let target = (e.phi / FRAC_PI_2).round() * FRAC_PI_2;
    if wrap(e.phi - target).abs() < AXIS_SNAP {
        e_snap.phi = target;
    }
    let mut out = vec![cand(ShapeClass::Ellipse, vec![full(e_snap)], 0.0)];
    if e.rx.min(e.ry) / e.rx.max(e.ry) > CIRCLE_RATIO {
        let r = (e.rx + e.ry) * 0.5;
        out.push(cand(ShapeClass::Circle, vec![full(Ellipse { centre: e.centre, rx: r, ry: r, phi: 0.0 })], SPECIAL_BONUS));
    }
    out
}

/// Arc candidates: an elliptic arc when its fit is sane, and a circular arc (more stable on short arcs).
/// Both are scored, so the residual decides; the circle gets a smaller bonus than closed shapes, because on
/// a short arc the ellipse always fits a little better.
fn arcs(pts: &[Pt]) -> Vec<Cand> {
    let span = crate::geom::BBox::of(pts).diag();
    let mut out = Vec::new();
    if let Some(e) = fit::ellipse(pts).filter(|e| e.rx.max(e.ry) < 2.0 * span) {
        out.push(cand(ShapeClass::ArcOfEllipse, vec![fit::arc_of(e, pts)], 0.0));
    }
    if let Some((c, r)) = fit::circle(pts).filter(|&(_, r)| r < 4.0 * span) {
        out.push(cand(ShapeClass::ArcOfCircle, vec![fit::arc_of(Ellipse { centre: c, rx: r, ry: r, phi: 0.0 }, pts)], ARC_CIRCLE_BONUS));
    }
    out
}

/// The best-fitting arc (for curved arrow shafts).
fn arc(pts: &[Pt], step: f32) -> Option<Primitive> {
    arcs(pts)
        .into_iter()
        .map(|c| (fit::residual(pts, &c.prims, step).0 - c.bonus * step, c.prims[0]))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| p)
}

// ---------- arrows ----------

/// A barb is a short straight piece leaving a shaft tip at 10°–80° to the shaft.
const BARB_MAX: f32 = 0.45;
const BARB_MIN: f32 = 0.05;
const TIP_NEAR: f32 = 0.25;
/// A shaft bowing more than this (sagitta / chord) is a curve.
const CURVED_BULGE: f32 = 0.07;

fn arrow(ps: &[Piece], step: f32) -> Option<Cand> {
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
        let d = BBox::of(&pts).diag();
        if d > HEAD_MAX * longest || d < BARB_MIN * longest {
            return None;
        }
        let h = hull(&pts);
        let (hull_area, perimeter) = (area(&h).abs(), perimeter_closed(&h));
        if h.len() < 3 || hull_area <= 0.0 || path_len(&pts) < HEAD_ROUND * perimeter {
            return None;
        }
        let touches = inside_convex(&h, end) || pts.iter().map(|p| p.dist(end)).fold(f32::MAX, f32::min) < 0.3 * d;
        (touches && largest_triangle(&h) >= HEAD_TRIANGLE * hull_area).then_some(pts)
    })
}

/// One stroke's head ink (from `end` round and back) as a closed head: small next to the shaft, back at `end`, and
/// close to a triangle.
fn is_triangle_head(pts: &[Pt], end: Pt, longest: f32) -> bool {
    let d = BBox::of(pts).diag();
    if pts.len() < 3 || d > HEAD_MAX * longest || d < BARB_MIN * longest {
        return false;
    }
    let h = hull(pts);
    let hull_area = area(&h).abs();
    // It leaves from `end` and comes back to it (a V head ends at its second barb, away from the tip).
    let back = pts[0].dist(end).max(pts[pts.len() - 1].dist(end)) < 0.3 * d;
    h.len() >= 3 && hull_area > 0.0 && back && path_len(pts) >= HEAD_ROUND * perimeter_closed(&h) && largest_triangle(&h) >= HEAD_TRIANGLE * hull_area
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

/// Resampling step for a box diagonal (exposed for tests and callers that scale tolerances).
pub fn step_for(diag: f32) -> f32 {
    diag / RESOLUTION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class_of(v: &[Pt]) -> Option<ShapeClass> {
        match v.len() {
            3 => triangle(v).map(|c| c.0),
            4 => quad(v).map(|c| c.0),
            _ => None,
        }
    }

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

    #[test]
    fn builtin_weights_load() {
        let _ = Recognizer::builtin();
    }

    /// End to end on synthetic ink: the shipped weights must keep recognising most shapes exactly.
    #[test]
    fn builtin_accuracy_floor() {
        let rec = Recognizer::builtin();
        let mut rng = crate::synth::Rng::new(77);
        let n = 300;
        let ok = (0..n)
            .filter(|_| {
                let ex = crate::synth::example(&mut rng);
                match (rec.recognize(&ex.ink), ex.class, ex.family) {
                    (Recognition::Shape(c), Some(cls), _) => c[0].class == cls,
                    (Recognition::ScratchOut { .. }, None, Family::ScratchOut) => true,
                    (Recognition::Rejected, None, Family::Reject) => true,
                    _ => false,
                }
            })
            .count();
        assert!(ok * 100 >= n * 80, "exact-class accuracy {ok}/{n} fell below 80 %");
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
