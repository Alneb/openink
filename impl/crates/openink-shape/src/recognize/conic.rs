//! Ellipses, circles and arcs.

use super::{cand, Cand, Ctx, AXIS_SNAP, SPECIAL_BONUS};
use crate::fit::{self, Ellipse};
use crate::geom::{wrap, Pt};
use crate::model::{Decoration, Primitive, ShapeClass};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Axis ratio above which an ellipse is a circle.
const CIRCLE_RATIO: f32 = 0.88;
const ARC_CIRCLE_BONUS: f32 = 0.08;

pub(super) fn ellipse(ctx: &Ctx) -> Vec<Cand> {
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
pub(super) fn arcs(pts: &[Pt]) -> Vec<Cand> {
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
pub(super) fn arc(pts: &[Pt], step: f32) -> Option<Primitive> {
    arcs(pts)
        .into_iter()
        .map(|c| (fit::residual(pts, &c.prims, step).0 - c.bonus * step, c.prims[0]))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| p)
}
