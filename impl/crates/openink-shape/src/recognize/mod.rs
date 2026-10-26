//! The recogniser: classifier + segmenter → fitted, specialised and beautified candidates.
//! Design and tolerances: docs/SHAPE-ENGINE.md ("Recognise").

mod arrow;
mod conic;
mod polygon;

use crate::features::{point_features, stroke_features, Ink, Norm, RESOLUTION};
use crate::fit;
use crate::geom::{hull, pt, wrap, Pt};
use crate::model::{Candidate, Family, Primitive, Recognition, ShapeClass};
use crate::nn::Models;
use crate::segment::{dp_pieces, pieces, Piece};
use std::f32::consts::{FRAC_PI_2, PI};

/// Families below this probability get no candidate.
const FAMILY_FLOOR: f32 = 0.03;
/// Weight of the fit residual (in resampling steps) against −ln P. Chosen with `shape-eval`
/// (docs/SHAPE-ENGINE.md, "Tuning").
pub const RESIDUAL_WEIGHT: f32 = 0.15;
/// A specialised shape that holds its tolerances beats its general form by this much.
pub(super) const SPECIAL_BONUS: f32 = 0.3;
/// Angle tolerance for parallel, right angle, 60° and axis snapping.
pub(super) const ANGLE_TOL: f32 = 10.0 * PI / 180.0;
/// Within this of an axis, a shape is turned onto it (was 6°: boxes drawn 8–12° off stayed tilted).
pub(super) const AXIS_SNAP: f32 = 12.0 * PI / 180.0;

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
        static WEIGHTS: &[u8] = include_bytes!("../../weights/shape.oisw");
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

pub(super) struct Ctx {
    pub pts: Vec<Pt>,
    pub step: f32,
    /// Douglas–Peucker pieces: a second corner source next to the segmenter's.
    pub dp: Vec<Piece>,
    pub residual_weight: f32,
}

pub(super) struct Cand {
    pub class: ShapeClass,
    pub prims: Vec<Primitive>,
    /// Subtracted from the cost (a specialised shape that holds its tolerances).
    pub bonus: f32,
    /// Ink the primitives stand for without tracing it (an arrow's drawn head): counts as fitted.
    pub explained: Vec<Pt>,
}

pub(super) fn cand(class: ShapeClass, prims: Vec<Primitive>, bonus: f32) -> Cand {
    Cand { class, prims, bonus, explained: Vec::new() }
}

fn candidates(fam: Family, ps: &[Piece], ctx: &Ctx) -> Vec<Cand> {
    match fam {
        Family::Line => vec![cand(ShapeClass::Line, vec![snap_line(fit::line(&ctx.pts))], 0.0)],
        Family::Polyline => [ps, &ctx.dp[..]]
            .iter()
            .filter_map(|p| polygon::polyline(p))
            .map(|v| cand(ShapeClass::Polyline, lines(&v, false), 0.0))
            .collect(),
        Family::Polygon => polygon::polygon(ps).into_iter().chain(polygon::polygon(&ctx.dp)).collect(),
        Family::Ellipse => conic::ellipse(ctx),
        Family::Arc => conic::arcs(&ctx.pts),
        Family::Arrow => arrow::arrow(ps, ctx.step).into_iter().collect(),
        Family::ScratchOut | Family::Reject => Vec::new(),
    }
}

// ---------- shared by the shape families ----------

/// Snap a segment to horizontal / vertical when it is within AXIS_SNAP, keeping its midpoint and length.
pub(super) fn snap_line((a, b): (Pt, Pt)) -> Primitive {
    let ang = b.sub(a).angle();
    let target = (ang / FRAC_PI_2).round() * FRAC_PI_2;
    if wrap(ang - target).abs() < AXIS_SNAP {
        let (m, h) = (a.lerp(b, 0.5), a.dist(b) * 0.5);
        let d = Pt::unit(target).scale(h);
        return Primitive::line(m.sub(d), m.add(d));
    }
    Primitive::line(a, b)
}

pub(super) fn lines(v: &[Pt], closed: bool) -> Vec<Primitive> {
    let mut out: Vec<Primitive> = v.windows(2).map(|w| Primitive::line(w[0], w[1])).collect();
    if closed && v.len() > 2 {
        out.push(Primitive::line(*v.last().unwrap(), v[0]));
    }
    out
}

pub(super) fn centroid(v: &[Pt]) -> Pt {
    v.iter().fold(pt(0.0, 0.0), |s, p| s.add(*p)).scale(1.0 / v.len() as f32)
}

/// Resampling step for a box diagonal (exposed for tests and callers that scale tolerances).
pub fn step_for(diag: f32) -> f32 {
    diag / RESOLUTION
}


#[cfg(test)]
mod tests {
    use super::*;

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
}
