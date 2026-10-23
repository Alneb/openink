//! Convex outline pieces. Every polygon is emitted counter-clockwise (in y-down screen space:
//! clockwise on screen), so a renderer may union them with nonzero winding or per-polygon max.

use std::f32::consts::{PI, TAU};

/// Flat polygon list: `[n, x0, y0, ..., x(n-1), y(n-1)]` repeated.
pub type Polygons = Vec<f32>;

/// Tessellation: segments per full circle of radius `r`, about one per 1.5 px of arc.
fn arc_segments(r: f32) -> usize {
    ((TAU * r / 1.5).ceil() as usize).clamp(8, 96)
}

fn push_poly(out: &mut Polygons, pts: &[(f32, f32)]) {
    out.push(pts.len() as f32);
    for &(x, y) in pts {
        out.push(x);
        out.push(y);
    }
}

fn arc(pts: &mut Vec<(f32, f32)>, cx: f32, cy: f32, r: f32, from: f32, to: f32) {
    let n = ((arc_segments(r) as f32 * (to - from).abs() / TAU).ceil() as usize).max(1);
    for i in 0..=n {
        let a = from + (to - from) * i as f32 / n as f32;
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
}

pub fn circle(out: &mut Polygons, cx: f32, cy: f32, r: f32) {
    let mut pts = Vec::new();
    arc(&mut pts, cx, cy, r, 0.0, TAU);
    pts.pop(); // the closing point repeats the first
    push_poly(out, &pts);
}

/// The convex hull of two discs: a round-capped segment whose width changes from `2 r0` to `2 r1`.
pub fn capsule(out: &mut Polygons, (x0, y0, r0): (f32, f32, f32), (x1, y1, r1): (f32, f32, f32)) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let d = dx.hypot(dy);
    if d <= (r0 - r1).abs() + 1e-4 {
        // One disc contains the other.
        let (x, y, r) = if r0 >= r1 { (x0, y0, r0) } else { (x1, y1, r1) };
        return circle(out, x, y, r);
    }
    let theta = dy.atan2(dx);
    // Angle between the axis and the radius to the outer tangent point.
    let phi = ((r0 - r1) / d).clamp(-1.0, 1.0).acos();
    let mut pts = Vec::new();
    arc(&mut pts, x1, y1, r1, theta - phi, theta + phi); // front cap at the end disc
    arc(&mut pts, x0, y0, r0, theta + phi, theta + TAU - phi); // back cap at the start disc
    push_poly(out, &pts);
}

/// libneo's outline piece: the convex hull of two regular octagons with vertices on the radius at 0°, 45°, …
/// (axis-aligned, not turned with the stroke), as its fountain/ballpoint/marker ink shows (impl/java/neo-oracle).
pub fn octagon_capsule(out: &mut Polygons, (x0, y0, r0): (f32, f32, f32), (x1, y1, r1): (f32, f32, f32)) {
    let mut p = Vec::with_capacity(16);
    for (x, y, r) in [(x0, y0, r0), (x1, y1, r1)] {
        for k in 0..8 {
            let a = k as f32 * TAU / 8.0;
            p.push((x + r * a.cos(), y + r * a.sin()));
        }
    }
    push_poly(out, &convex_hull(p));
}

/// Andrew's monotone chain; returns the hull counter-clockwise in y-up terms (see module doc).
pub fn convex_hull(mut p: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    p.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: (f32, f32), a: (f32, f32), b: (f32, f32)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut h: Vec<(f32, f32)> = Vec::with_capacity(p.len() * 2);
    for pass in 0..2 {
        let start = h.len();
        let iter: Box<dyn Iterator<Item = &(f32, f32)>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in iter {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop(); // the last point starts the other chain
    }
    h
}

/// A flat nib of length `len` and thickness `len / ratio` at `angle` (radians), swept from
/// `(x0, y0)` to `(x1, y1)`: the hull of the nib rectangle at both ends.
pub fn nib_sweep(out: &mut Polygons, (x0, y0, len0): (f32, f32, f32), (x1, y1, len1): (f32, f32, f32), angle: f32, ratio: f32) {
    let (s, c) = angle.sin_cos();
    let corners = |x: f32, y: f32, len: f32| {
        let (ax, ay) = (c * len / 2.0, s * len / 2.0);
        let t = len / ratio / 2.0;
        let (bx, by) = (-s * t, c * t);
        [(x + ax + bx, y + ay + by), (x + ax - bx, y + ay - by), (x - ax + bx, y - ay + by), (x - ax - bx, y - ay - by)]
    };
    let mut p = corners(x0, y0, len0).to_vec();
    p.extend(corners(x1, y1, len1));
    push_poly(out, &convex_hull(p));
}

/// Signed area (shoelace) of one polygon given as `[x0, y0, ...]`: positive = the module's orientation.
pub fn signed_area(xy: &[f32]) -> f32 {
    let n = xy.len() / 2;
    (0..n).map(|i| {
        let j = (i + 1) % n;
        xy[2 * i] * xy[2 * j + 1] - xy[2 * j] * xy[2 * i + 1]
    }).sum::<f32>() / 2.0
}

/// Iterate a flat polygon list as `&[x0, y0, ...]` slices.
pub fn polygons(flat: &[f32]) -> impl Iterator<Item = &[f32]> {
    let mut i = 0;
    std::iter::from_fn(move || {
        let n = *flat.get(i)? as usize;
        let poly = flat.get(i + 1..i + 1 + 2 * n)?;
        i += 1 + 2 * n;
        Some(poly)
    })
}

pub const DEG: f32 = PI / 180.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn only(flat: &[f32]) -> &[f32] {
        let mut it = polygons(flat);
        let p = it.next().unwrap();
        assert!(it.next().is_none());
        p
    }

    #[test]
    fn capsule_area_matches_formula_and_orientation() {
        let mut out = Vec::new();
        capsule(&mut out, (0.0, 0.0, 5.0), (100.0, 0.0, 5.0));
        let a = signed_area(only(&out));
        let exact = 100.0 * 10.0 + PI * 25.0;
        assert!(a > 0.0, "orientation");
        assert!((a - exact).abs() / exact < 0.01, "area {a} vs {exact}");
    }

    #[test]
    fn tapered_and_contained_capsules() {
        let mut out = Vec::new();
        capsule(&mut out, (0.0, 0.0, 2.0), (10.0, 0.0, 6.0));
        assert!(signed_area(only(&out)) > 0.0);
        out.clear();
        capsule(&mut out, (0.0, 0.0, 10.0), (1.0, 0.0, 2.0)); // second inside first
        let a = signed_area(only(&out));
        assert!((a - PI * 100.0).abs() < 2.0, "{a}");
    }

    #[test]
    fn nib_sweep_is_a_hull() {
        let mut out = Vec::new();
        nib_sweep(&mut out, (0.0, 0.0, 10.0), (20.0, 0.0, 10.0), 45.0 * DEG, 10.0);
        let p = only(&out);
        assert!(signed_area(p) > 0.0);
        assert!(p.len() / 2 >= 4 && p.len() / 2 <= 8);
    }
}
