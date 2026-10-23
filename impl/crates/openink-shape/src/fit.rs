//! Geometric fits: total-least-squares lines, algebraic circles, direct ellipses
//! (Fitzgibbon's constraint, solved with Halíř–Flusser's 3×3 reduction), and residuals.

use crate::geom::{pca, pt, wrap, Pt};
use crate::model::{Decoration, Primitive};
use std::f32::consts::PI;

/// Best line through the points, clipped to their extent along it.
pub fn line(pts: &[Pt]) -> (Pt, Pt) {
    let (c, a, _, _) = pca(pts);
    let d = Pt::unit(a);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for p in pts {
        let t = p.sub(c).dot(d);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let (p1, p2) = (c.add(d.scale(lo)), c.add(d.scale(hi)));
    // Keep the drawing direction.
    if pts.first().map_or(0.0, |f| f.dist(p1)) > pts.first().map_or(0.0, |f| f.dist(p2)) {
        (p2, p1)
    } else {
        (p1, p2)
    }
}

/// Intersection of lines `a1→a2` and `b1→b2`; `None` when nearly parallel.
pub fn intersect(a1: Pt, a2: Pt, b1: Pt, b2: Pt) -> Option<Pt> {
    let r = a2.sub(a1);
    let s = b2.sub(b1);
    let den = r.cross(s);
    if den.abs() < 1e-6 * r.len() * s.len() {
        return None;
    }
    let t = b1.sub(a1).cross(s) / den;
    Some(a1.add(r.scale(t)))
}

/// Algebraic (Kåsa) circle: centre and radius.
pub fn circle(pts: &[Pt]) -> Option<(Pt, f32)> {
    let n = pts.len() as f64;
    if n < 3.0 {
        return None;
    }
    let (mx, my) = (pts.iter().map(|p| p.x as f64).sum::<f64>() / n, pts.iter().map(|p| p.y as f64).sum::<f64>() / n);
    let (mut suu, mut svv, mut suv, mut suuu, mut svvv, mut suvv, mut svuu) = (0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let (u, v) = (p.x as f64 - mx, p.y as f64 - my);
        suu += u * u;
        svv += v * v;
        suv += u * v;
        suuu += u * u * u;
        svvv += v * v * v;
        suvv += u * v * v;
        svuu += v * u * u;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-9 {
        return None;
    }
    let (bu, bv) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
    let uc = (bu * svv - bv * suv) / det;
    let vc = (bv * suu - bu * suv) / det;
    let r = (uc * uc + vc * vc + (suu + svv) / n).sqrt();
    Some((pt((uc + mx) as f32, (vc + my) as f32), r as f32))
}

/// An ellipse: centre, semi-axes (rx ≥ ry) and rotation of the rx axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipse {
    pub centre: Pt,
    pub rx: f32,
    pub ry: f32,
    pub phi: f32,
}

impl Ellipse {
    /// Parametric angle of `p` (its eccentric anomaly).
    pub fn param(&self, p: Pt) -> f32 {
        let q = p.sub(self.centre).rotate(-self.phi);
        (q.y / self.ry.max(1e-6)).atan2(q.x / self.rx.max(1e-6))
    }
}

/// Direct least-squares ellipse fit. Points are centred and scaled first for conditioning.
pub fn ellipse(pts: &[Pt]) -> Option<Ellipse> {
    if pts.len() < 6 {
        return None;
    }
    let n = pts.len() as f64;
    let (mx, my) = (pts.iter().map(|p| p.x as f64).sum::<f64>() / n, pts.iter().map(|p| p.y as f64).sum::<f64>() / n);
    let s = pts.iter().map(|p| ((p.x as f64 - mx).powi(2) + (p.y as f64 - my).powi(2)).sqrt()).sum::<f64>() / n;
    if s <= 0.0 {
        return None;
    }
    // Scatter matrices of D1 = [x², xy, y²] and D2 = [x, y, 1].
    let mut s1 = [[0.0f64; 3]; 3];
    let mut s2 = [[0.0f64; 3]; 3];
    let mut s3 = [[0.0f64; 3]; 3];
    for p in pts {
        let (x, y) = ((p.x as f64 - mx) / s, (p.y as f64 - my) / s);
        let d1 = [x * x, x * y, y * y];
        let d2 = [x, y, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                s1[i][j] += d1[i] * d1[j];
                s2[i][j] += d1[i] * d2[j];
                s3[i][j] += d2[i] * d2[j];
            }
        }
    }
    let s3i = inv3(s3)?;
    let t = mul3(neg3(s3i), transpose3(s2)); // a2 = T a1
    let m0 = add3(s1, mul3(s2, t));
    // Premultiply by C1⁻¹ for C1 = [[0,0,2],[0,-1,0],[2,0,0]].
    let m = [
        [m0[2][0] / 2.0, m0[2][1] / 2.0, m0[2][2] / 2.0],
        [-m0[1][0], -m0[1][1], -m0[1][2]],
        [m0[0][0] / 2.0, m0[0][1] / 2.0, m0[0][2] / 2.0],
    ];
    let mut best: Option<[f64; 3]> = None;
    for lam in eig3_real(m) {
        let v = null3(sub3(m, lam))?;
        if 4.0 * v[0] * v[2] - v[1] * v[1] > 0.0 {
            best = Some(v);
        }
    }
    let a1 = best?;
    let a2 = [
        t[0][0] * a1[0] + t[0][1] * a1[1] + t[0][2] * a1[2],
        t[1][0] * a1[0] + t[1][1] * a1[1] + t[1][2] * a1[2],
        t[2][0] * a1[0] + t[2][1] * a1[1] + t[2][2] * a1[2],
    ];
    let e = conic_to_ellipse(a1[0], a1[1], a1[2], a2[0], a2[1], a2[2])?;
    Some(Ellipse {
        centre: pt((e.0 * s + mx) as f32, (e.1 * s + my) as f32),
        rx: (e.2 * s) as f32,
        ry: (e.3 * s) as f32,
        phi: e.4 as f32,
    })
}

/// Conic `A x² + B xy + C y² + D x + E y + F = 0` → (cx, cy, rx, ry, phi), rx ≥ ry.
fn conic_to_ellipse(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Option<(f64, f64, f64, f64, f64)> {
    let den = b * b - 4.0 * a * c;
    if den >= 0.0 {
        return None;
    }
    let cx = (2.0 * c * d - b * e) / den;
    let cy = (2.0 * a * e - b * d) / den;
    // Value at the centre, then the quadratic form's eigenvalues give the axes.
    let f0 = a * cx * cx + b * cx * cy + c * cy * cy + d * cx + e * cy + f;
    let tr = a + c;
    let disc = ((a - c) * (a - c) + b * b).sqrt();
    let (l1, l2) = ((tr + disc) / 2.0, (tr - disc) / 2.0);
    let (r1, r2) = ((-f0 / l1), (-f0 / l2));
    if r1 <= 0.0 || r2 <= 0.0 {
        return None;
    }
    // Axis of eigenvalue l2 (the smaller one) is the major axis.
    let phi = 0.5 * b.atan2(a - c) + std::f64::consts::FRAC_PI_2;
    let (rx, ry) = (r2.sqrt(), r1.sqrt());
    Some((cx, cy, rx, ry, phi))
}

type M3 = [[f64; 3]; 3];

fn mul3(a: M3, b: M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}
fn add3(a: M3, b: M3) -> M3 {
    let mut r = a;
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] += b[i][j];
        }
    }
    r
}
fn neg3(a: M3) -> M3 {
    a.map(|row| row.map(|v| -v))
}
fn sub3(a: M3, lam: f64) -> M3 {
    let mut r = a;
    for (i, row) in r.iter_mut().enumerate() {
        row[i] -= lam;
    }
    r
}
fn transpose3(a: M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[j][i];
        }
    }
    r
}
fn det3(a: M3) -> f64 {
    a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
}
#[allow(clippy::needless_range_loop)] // cofactor indices are clearer than iterators here
fn inv3(a: M3) -> Option<M3> {
    let d = det3(a);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let (i1, i2) = ((j + 1) % 3, (j + 2) % 3);
            let (j1, j2) = ((i + 1) % 3, (i + 2) % 3);
            r[i][j] = (a[i1][j1] * a[i2][j2] - a[i1][j2] * a[i2][j1]) / d;
        }
    }
    Some(r)
}

/// Real eigenvalues of a 3×3 matrix (roots of its characteristic cubic).
fn eig3_real(m: M3) -> Vec<f64> {
    let tr = m[0][0] + m[1][1] + m[2][2];
    let c2 = m[0][0] * m[1][1] - m[0][1] * m[1][0] + m[0][0] * m[2][2] - m[0][2] * m[2][0] + m[1][1] * m[2][2]
        - m[1][2] * m[2][1];
    let det = det3(m);
    // λ³ − tr λ² + c2 λ − det = 0 → depressed cubic t³ + p t + q with λ = t + tr/3.
    let p = c2 - tr * tr / 3.0;
    let q = -2.0 * tr.powi(3) / 27.0 + tr * c2 / 3.0 - det;
    let shift = tr / 3.0;
    let disc = (q / 2.0).powi(2) + (p / 3.0).powi(3);
    if disc > 0.0 {
        let sq = disc.sqrt();
        vec![(-q / 2.0 + sq).cbrt() + (-q / 2.0 - sq).cbrt() + shift]
    } else {
        let r = (-p / 3.0).max(0.0).sqrt();
        let phi = if r > 0.0 { (-q / (2.0 * r.powi(3))).clamp(-1.0, 1.0).acos() } else { 0.0 };
        (0..3).map(|k| 2.0 * r * ((phi + 2.0 * std::f64::consts::PI * k as f64) / 3.0).cos() + shift).collect()
    }
}

/// A unit null vector of a (near-)singular 3×3 matrix: the largest cross product of two rows.
fn null3(a: M3) -> Option<[f64; 3]> {
    let cross = |u: [f64; 3], v: [f64; 3]| [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let cands = [cross(a[0], a[1]), cross(a[0], a[2]), cross(a[1], a[2])];
    let best = cands.into_iter().max_by(|x, y| norm3(*x).total_cmp(&norm3(*y)))?;
    let n = norm3(best);
    (n > 1e-15).then(|| best.map(|v| v / n))
}
fn norm3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// The elliptic arc through `pts` in drawing order (start angle and signed sweep).
pub fn arc_of(e: Ellipse, pts: &[Pt]) -> Primitive {
    let mut prev = e.param(pts[0]);
    let start = prev;
    let mut sweep = 0.0;
    for p in &pts[1..] {
        let a = e.param(*p);
        sweep += wrap(a - prev);
        prev = a;
    }
    Primitive::Arc {
        centre: e.centre,
        rx: e.rx,
        ry: e.ry,
        phi: e.phi,
        start,
        sweep: sweep.clamp(-2.0 * PI, 2.0 * PI),
        d1: Decoration::None,
        d2: Decoration::None,
    }
}

/// Mean and max distance from the ink to the primitives, and from the primitives to the ink
/// (both ways, so a shape that leaves part of the ink unexplained, or adds lines nobody drew, scores badly).
pub fn residual(ink: &[Pt], prims: &[Primitive], step: f32) -> (f32, f32) {
    residual_explained(ink, prims, &[], step)
}

/// As [`residual`], with `explained` ink (e.g. an arrow's drawn head) added to the model as is.
pub fn residual_explained(ink: &[Pt], prims: &[Primitive], explained: &[Pt], step: f32) -> (f32, f32) {
    let mut model: Vec<Pt> = prims.iter().flat_map(|p| p.sample(step)).collect();
    model.extend_from_slice(explained);
    if model.is_empty() || ink.is_empty() {
        return (f32::MAX, f32::MAX);
    }
    let nearest = |p: &Pt, set: &[Pt]| set.iter().map(|q| p.dist(*q)).fold(f32::MAX, f32::min);
    let (mut sum, mut max) = (0.0f32, 0.0f32);
    for p in ink {
        let d = nearest(p, &model);
        sum += d;
        max = max.max(d);
    }
    let mut back = 0.0f32;
    let stride = (model.len() / 200).max(1);
    let mut cnt = 0;
    for q in model.iter().step_by(stride) {
        let d = nearest(q, ink);
        back += d;
        max = max.max(d);
        cnt += 1;
    }
    ((sum / ink.len() as f32 + back / cnt as f32) * 0.5, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_recovers_parameters() {
        let truth = Ellipse { centre: pt(300.0, 200.0), rx: 120.0, ry: 50.0, phi: 0.4 };
        let pts: Vec<Pt> = (0..90)
            .map(|i| {
                let t = i as f32 / 90.0 * 2.0 * PI;
                pt(truth.rx * t.cos(), truth.ry * t.sin()).rotate(truth.phi).add(truth.centre)
            })
            .collect();
        let e = ellipse(&pts).expect("fit");
        assert!(e.centre.dist(truth.centre) < 0.5, "{e:?}");
        assert!((e.rx - 120.0).abs() < 0.5 && (e.ry - 50.0).abs() < 0.5, "{e:?}");
        assert!(crate::geom::line_angle_diff(e.phi, truth.phi) < 0.01, "{e:?}");
    }

    #[test]
    fn ellipse_from_partial_arc() {
        let pts: Vec<Pt> = (0..60).map(|i| Pt::unit(i as f32 / 60.0 * PI).scale(100.0)).collect();
        let e = ellipse(&pts).expect("fit");
        assert!(e.centre.len() < 2.0 && (e.rx - 100.0).abs() < 2.0, "{e:?}");
    }

    #[test]
    fn circle_fit() {
        let pts: Vec<Pt> = (0..40).map(|i| Pt::unit(i as f32 * 0.1).scale(30.0).add(pt(5.0, -7.0))).collect();
        let (c, r) = circle(&pts).unwrap();
        assert!(c.dist(pt(5.0, -7.0)) < 1e-2 && (r - 30.0).abs() < 1e-2);
    }

    #[test]
    fn intersection() {
        let p = intersect(pt(0.0, 0.0), pt(1.0, 0.0), pt(5.0, -1.0), pt(5.0, 1.0)).unwrap();
        assert!(p.dist(pt(5.0, 0.0)) < 1e-5);
        assert!(intersect(pt(0.0, 0.0), pt(1.0, 0.0), pt(0.0, 1.0), pt(1.0, 1.0)).is_none());
    }
}
