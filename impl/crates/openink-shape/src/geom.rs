//! Plane geometry on f32 points: vectors, angles, arc-length resampling, hulls.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

pub const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

// Named methods rather than operator traits: the vector maths reads as a chain (a.sub(b).dot(c)).
#[allow(clippy::should_implement_trait)]
impl Pt {
    pub fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
    pub fn scale(self, k: f32) -> Pt {
        pt(self.x * k, self.y * k)
    }
    pub fn dot(self, o: Pt) -> f32 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Pt) -> f32 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: Pt) -> f32 {
        self.sub(o).len()
    }
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }
    pub fn lerp(self, o: Pt, t: f32) -> Pt {
        self.add(o.sub(self).scale(t))
    }
    pub fn rotate(self, a: f32) -> Pt {
        let (s, c) = a.sin_cos();
        pt(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn unit(a: f32) -> Pt {
        let (s, c) = a.sin_cos();
        pt(c, s)
    }
}

/// Wrap an angle to (-π, π].
pub fn wrap(a: f32) -> f32 {
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// Undirected angle between two directions, in [0, π/2].
pub fn line_angle_diff(a: f32, b: f32) -> f32 {
    let d = wrap(a - b).abs();
    d.min(PI - d)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub min: Pt,
    pub max: Pt,
}

impl BBox {
    pub fn of(pts: &[Pt]) -> BBox {
        let mut b = BBox { min: pt(f32::MAX, f32::MAX), max: pt(f32::MIN, f32::MIN) };
        for p in pts {
            b.min = pt(b.min.x.min(p.x), b.min.y.min(p.y));
            b.max = pt(b.max.x.max(p.x), b.max.y.max(p.y));
        }
        b
    }
    pub fn w(&self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }
    pub fn h(&self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }
    pub fn diag(&self) -> f32 {
        self.w().hypot(self.h())
    }
    pub fn centre(&self) -> Pt {
        self.min.lerp(self.max, 0.5)
    }
}

pub fn path_len(pts: &[Pt]) -> f32 {
    pts.windows(2).map(|w| w[0].dist(w[1])).sum()
}

/// Points spaced `step` apart along the polyline (first and last point kept). Removes the
/// sampling-rate dependence of everything downstream.
pub fn resample(pts: &[Pt], step: f32) -> Vec<Pt> {
    let mut out = Vec::new();
    let Some(&first) = pts.first() else { return out };
    out.push(first);
    if step <= 0.0 {
        return out;
    }
    let mut carry = 0.0; // distance travelled since the last emitted point
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = a.dist(b);
        if seg <= 0.0 {
            continue;
        }
        let mut t = step - carry;
        while t <= seg {
            out.push(a.lerp(b, t / seg));
            t += step;
        }
        carry = seg - (t - step);
    }
    let last = *pts.last().unwrap();
    if out.last().map_or(true, |p| p.dist(last) > step * 0.25) {
        out.push(last);
    }
    out
}

/// Distance from `p` to segment `ab`.
pub fn seg_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b.sub(a);
    let l2 = ab.dot(ab);
    if l2 <= 0.0 {
        return p.dist(a);
    }
    let t = (p.sub(a).dot(ab) / l2).clamp(0.0, 1.0);
    p.dist(a.add(ab.scale(t)))
}

/// Convex hull (Andrew's monotone chain), counter-clockwise, no repeated first point.
pub fn hull(pts: &[Pt]) -> Vec<Pt> {
    let mut p: Vec<Pt> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut h: Vec<Pt> = Vec::with_capacity(p.len() * 2);
    for pass in 0..2 {
        let start = h.len();
        let iter: Box<dyn Iterator<Item = &Pt>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in iter {
            while h.len() >= start + 2 && h[h.len() - 2].sub(h[h.len() - 1]).cross(q.sub(h[h.len() - 1])) >= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Signed area of a closed polygon (positive = counter-clockwise in y-up axes).
pub fn area(poly: &[Pt]) -> f32 {
    let n = poly.len();
    (0..n).map(|i| poly[i].cross(poly[(i + 1) % n])).sum::<f32>() * 0.5
}

pub fn perimeter_closed(poly: &[Pt]) -> f32 {
    let n = poly.len();
    (0..n).map(|i| poly[i].dist(poly[(i + 1) % n])).sum()
}

/// Mean and principal axes of a point cloud: (centre, major-axis angle, var major, var minor).
pub fn pca(pts: &[Pt]) -> (Pt, f32, f32, f32) {
    let n = pts.len().max(1) as f32;
    let c = pts.iter().fold(pt(0.0, 0.0), |s, p| s.add(*p)).scale(1.0 / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in pts {
        let d = p.sub(c);
        sxx += d.x * d.x;
        syy += d.y * d.y;
        sxy += d.x * d.y;
    }
    let (sxx, syy, sxy) = (sxx / n, syy / n, sxy / n);
    let tr = sxx + syy;
    let disc = ((sxx - syy) * (sxx - syy) * 0.25 + sxy * sxy).sqrt();
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    (c, angle, tr * 0.5 + disc, (tr * 0.5 - disc).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_spacing() {
        let r = resample(&[pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)], 1.0);
        assert_eq!(r.len(), 21);
        for w in r.windows(2) {
            assert!((w[0].dist(w[1]) - 1.0).abs() < 1e-3 || w[0].dist(w[1]) < 1.0);
        }
    }

    #[test]
    fn hull_and_area() {
        let sq = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0), pt(0.5, 0.5)];
        let h = hull(&sq);
        assert_eq!(h.len(), 4);
        assert!((area(&h).abs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pca_axis() {
        let pts: Vec<Pt> = (0..100).map(|i| pt(i as f32, 0.5 * i as f32)).collect();
        let (_, a, _, minor) = pca(&pts);
        assert!((a - 0.5f32.atan()).abs() < 1e-3);
        assert!(minor < 1e-2);
    }
}
