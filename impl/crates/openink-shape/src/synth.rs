//! Synthetic training ink: ideal shapes of every class, drawn with a simple model of a hand
//! (corner rounding, low-frequency wobble, jitter, hooks, unclosed or overlapping ends, pen lifts,
//! slowing at corners). Each raw sample carries its segmenter label.
//! Deterministic for a given seed, so datasets and tests are reproducible.

use crate::features::Sample;
use crate::geom::{pt, Pt};
use crate::model::{Family, ShapeClass};
use crate::nn::PointLabel;
use std::f32::consts::PI;

/// SplitMix64: small, fast, good enough for data generation.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
    pub fn int(&mut self, lo: usize, hi_inclusive: usize) -> usize {
        lo + (self.next_u64() % (hi_inclusive - lo + 1) as u64) as usize
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    pub fn normal(&mut self) -> f32 {
        let (u, v) = (self.f().max(1e-7), self.f());
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}

/// A piece of an ideal path.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line(Pt, Pt),
    /// Elliptic arc: centre, rx, ry, rotation, start, signed sweep.
    Arc(Pt, f32, f32, f32, f32, f32),
}

impl Piece {
    fn at(&self, u: f32) -> Pt {
        match *self {
            Piece::Line(a, b) => a.lerp(b, u),
            Piece::Arc(c, rx, ry, phi, a0, sw) => {
                let t = a0 + sw * u;
                pt(rx * t.cos(), ry * t.sin()).rotate(phi).add(c)
            }
        }
    }
    fn curved(&self) -> bool {
        matches!(self, Piece::Arc(..))
    }
    fn approx_len(&self) -> f32 {
        (0..32).map(|i| self.at(i as f32 / 32.0).dist(self.at((i + 1) as f32 / 32.0))).sum()
    }
}

/// One stroke of the ideal drawing: pieces drawn without lifting the pen.
type IdealStroke = Vec<Piece>;

/// A generated example.
#[derive(Clone, Debug)]
pub struct Example {
    pub ink: Vec<Vec<Sample>>,
    /// One label per raw sample, parallel to `ink`.
    pub labels: Vec<Vec<PointLabel>>,
    pub class: Option<ShapeClass>,
    pub family: Family,
}

/// Classes the generator can draw, with relative frequency.
const MENU: &[(Gen, f32)] = &[
    (Gen::Line, 1.2),
    (Gen::Polyline, 1.0),
    (Gen::Zigzag, 0.4),
    (Gen::Triangle, 1.0),
    (Gen::Quad, 1.4),
    (Gen::RegularPolygon, 0.6),
    (Gen::Ellipse, 1.4),
    (Gen::Arc, 0.9),
    (Gen::Arrow, 1.4),
    (Gen::ScratchOut, 0.7),
    (Gen::Scribble, 1.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gen {
    Line,
    Polyline,
    Zigzag,
    Triangle,
    Quad,
    RegularPolygon,
    Ellipse,
    Arc,
    Arrow,
    ScratchOut,
    Scribble,
}

pub fn example(rng: &mut Rng) -> Example {
    let total: f32 = MENU.iter().map(|m| m.1).sum();
    let mut x = rng.f() * total;
    let mut pick = MENU[0].0;
    for (g, w) in MENU {
        if x < *w {
            pick = *g;
            break;
        }
        x -= w;
    }
    draw(pick, rng)
}

/// Angle with a bias towards the screen axes (people draw boxes and lines mostly level).
fn level_angle(rng: &mut Rng, p_axis: f32, jitter: f32) -> f32 {
    if rng.chance(p_axis) {
        rng.int(0, 3) as f32 * PI / 2.0 + rng.normal() * jitter
    } else {
        rng.range(-PI, PI)
    }
}

fn polygon_strokes(rng: &mut Rng, verts: &[Pt]) -> Vec<IdealStroke> {
    let n = verts.len();
    let start = if rng.chance(0.7) { 0 } else { rng.int(0, n - 1) };
    let rev = rng.chance(0.4);
    let order: Vec<Pt> = (0..=n)
        .map(|i| {
            let k = if rev { (start + n - i % n) % n } else { (start + i) % n };
            verts[k]
        })
        .collect();
    let lines: Vec<Piece> = order.windows(2).map(|w| Piece::Line(w[0], w[1])).collect();
    // Sometimes lift the pen at a corner (a box in two strokes, a triangle in three).
    if rng.chance(0.2) {
        let cut = rng.int(1, n - 1);
        vec![lines[..cut].to_vec(), lines[cut..].to_vec()]
    } else if rng.chance(0.05) {
        lines.into_iter().map(|l| vec![l]).collect()
    } else {
        vec![lines]
    }
}

fn draw(g: Gen, rng: &mut Rng) -> Example {
    let size = rng.range(60.0, 1100.0);
    let c = pt(rng.range(200.0, 1400.0), rng.range(200.0, 2000.0));
    let (strokes, class): (Vec<IdealStroke>, Option<ShapeClass>) = match g {
        Gen::Line => {
            let a = level_angle(rng, 0.45, 0.04);
            let d = Pt::unit(a).scale(size * 0.5);
            (vec![vec![Piece::Line(c.sub(d), c.add(d))]], Some(ShapeClass::Line))
        }
        Gen::Polyline | Gen::Zigzag => {
            let n = if g == Gen::Zigzag { rng.int(3, 7) } else { rng.int(2, 4) };
            let mut a = level_angle(rng, 0.3, 0.05);
            let mut p = c;
            let mut pts = vec![p];
            for i in 0..n {
                let l = size * rng.range(0.2, 0.6);
                p = p.add(Pt::unit(a).scale(l));
                pts.push(p);
                let turn = if g == Gen::Zigzag { rng.range(2.0, 2.7) * if i % 2 == 0 { 1.0 } else { -1.0 } } else {
                    rng.range(0.5, 2.4) * if rng.chance(0.5) { 1.0 } else { -1.0 }
                };
                a += turn;
            }
            (vec![pts.windows(2).map(|w| Piece::Line(w[0], w[1])).collect()], Some(ShapeClass::Polyline))
        }
        Gen::Triangle => {
            let (verts, class) = triangle(rng, size);
            let rot = level_angle(rng, 0.5, 0.05);
            let verts: Vec<Pt> = verts.iter().map(|v| v.rotate(rot).add(c)).collect();
            (polygon_strokes(rng, &verts), Some(class))
        }
        Gen::Quad => {
            let (verts, class) = quad(rng, size);
            let rot = level_angle(rng, 0.6, 0.04);
            let verts: Vec<Pt> = verts.iter().map(|v| v.rotate(rot).add(c)).collect();
            (polygon_strokes(rng, &verts), Some(class))
        }
        Gen::RegularPolygon => {
            let n = [5, 6, 6, 8][rng.int(0, 3)];
            let rot = rng.range(0.0, PI);
            let verts: Vec<Pt> = (0..n)
                .map(|i| Pt::unit(rot + 2.0 * PI * i as f32 / n as f32).scale(size * 0.5 * rng.range(0.93, 1.07)).add(c))
                .collect();
            (polygon_strokes(rng, &verts), Some(ShapeClass::Polygon))
        }
        Gen::Ellipse => {
            let circle = rng.chance(0.5);
            let rx = size * 0.5;
            let ry = if circle { rx * rng.range(0.92, 1.0) } else { rx * rng.range(0.25, 0.8) };
            let phi = level_angle(rng, 0.6, 0.05);
            let a0 = rng.range(-PI, PI);
            let dir = if rng.chance(0.6) { -1.0 } else { 1.0 };
            let sweep = dir * 2.0 * PI * rng.range(0.93, 1.12);
            (vec![vec![Piece::Arc(c, rx, ry, phi, a0, sweep)]], Some(if circle { ShapeClass::Circle } else { ShapeClass::Ellipse }))
        }
        Gen::Arc => {
            let circle = rng.chance(0.6);
            let rx = size * 0.5;
            let ry = if circle { rx } else { rx * rng.range(0.35, 0.8) };
            let sweep = rng.range(0.6, 1.6) * PI * if rng.chance(0.5) { 1.0 } else { -1.0 };
            let arc = Piece::Arc(c, rx, ry, rng.range(-PI, PI), rng.range(-PI, PI), sweep);
            (vec![vec![arc]], Some(if circle { ShapeClass::ArcOfCircle } else { ShapeClass::ArcOfEllipse }))
        }
        Gen::Arrow => arrow(rng, c, size),
        Gen::ScratchOut => {
            let n = rng.int(4, 12);
            let a = level_angle(rng, 0.5, 0.2);
            let w = size * rng.range(0.4, 1.0);
            let adv = size * rng.range(0.04, 0.15);
            let mut pts = Vec::new();
            for i in 0..=n {
                let side = if i % 2 == 0 { -0.5 } else { 0.5 };
                pts.push(pt(side * w + rng.normal() * w * 0.05, i as f32 * adv).rotate(a).add(c));
            }
            (vec![pts.windows(2).map(|w| Piece::Line(w[0], w[1])).collect()], None)
        }
        Gen::Scribble => (vec![scribble(rng, c, size)], None),
    };
    let family = match (g, class) {
        (Gen::ScratchOut, _) => Family::ScratchOut,
        (Gen::Scribble, _) => Family::Reject,
        (_, Some(cl)) => cl.family(),
        _ => Family::Reject,
    };
    // A polygon's stroke ends are its vertices, so they are corners too.
    let ends_are_corners = matches!(g, Gen::Triangle | Gen::Quad | Gen::RegularPolygon);
    let (ink, labels) = humanise(rng, &strokes, size, ends_are_corners);
    Example { ink, labels, class, family }
}

fn triangle(rng: &mut Rng, size: f32) -> (Vec<Pt>, ShapeClass) {
    let s = size * 0.5;
    match rng.int(0, 4) {
        0 => (vec![pt(0.0, -s), Pt::unit(PI / 6.0).scale(s), Pt::unit(5.0 * PI / 6.0).scale(s)], ShapeClass::EquilateralTriangle),
        1 => {
            // Legs at least 1.3× apart, so it is not right-isosceles.
            let a = s * rng.range(0.7, 1.6);
            let b = a * rng.range(1.3, 2.0).powf(if rng.chance(0.5) { 1.0 } else { -1.0 });
            (vec![pt(0.0, 0.0), pt(a, 0.0), pt(0.0, b)], ShapeClass::RightTriangle)
        }
        2 => (vec![pt(0.0, 0.0), pt(s * 1.4, 0.0), pt(0.0, s * 1.4)], ShapeClass::RightIsoscelesTriangle),
        3 => {
            // Equilateral at h ≈ 1.04 s: stay at least 0.3 s away from it.
            let h = s * if rng.chance(0.4) { rng.range(0.5, 0.74) } else { rng.range(1.34, 2.0) };
            (vec![pt(0.0, -h), pt(s * 0.6, 0.0), pt(-s * 0.6, 0.0)], ShapeClass::IsoscelesTriangle)
        }
        _ => {
            // Scalene: keep every angle above ~25° so it reads as a triangle at all.
            loop {
                let v: Vec<Pt> = (0..3).map(|_| pt(rng.range(-s, s), rng.range(-s, s))).collect();
                // ...and clearly away from every special class, so the label is honest: no angle within 18° of
                // a right angle, no two sides within 20 %.
                let ok = (0..3).all(|i| {
                    let (a, b, c) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                    let ang = (b.sub(a).angle() - c.sub(a).angle()).abs();
                    let ang = ang.min(2.0 * PI - ang);
                    let (l1, l2) = (b.dist(a), c.dist(b));
                    ang > 0.45 && (ang - PI / 2.0).abs() > 0.31 && l1 > s * 0.5 && (l1 - l2).abs() > 0.2 * l1.max(l2)
                });
                if ok {
                    return (v, ShapeClass::Triangle);
                }
            }
        }
    }
}

fn quad(rng: &mut Rng, size: f32) -> (Vec<Pt>, ShapeClass) {
    let w = size * 0.5;
    // Below 0.8 so a rectangle or rhombus is never really a square.
    let h = w * rng.range(0.35, 0.8);
    match rng.int(0, 5) {
        0 => ([pt(-w, -w), pt(w, -w), pt(w, w), pt(-w, w)].iter().map(|p| p.scale(0.75)).collect(), ShapeClass::Square),
        // Rectangles go thinner (to ~7:1): long label boxes, Atlas's 200×40 case.
        1 | 2 => {
            let h = w * rng.range(0.15, 0.8);
            (vec![pt(-w, -h), pt(w, -h), pt(w, h), pt(-w, h)], ShapeClass::Rectangle)
        }
        3 => (vec![pt(0.0, -h), pt(w, 0.0), pt(0.0, h), pt(-w, 0.0)], ShapeClass::Rhombus),
        4 => {
            let k = w * rng.range(0.2, 0.6);
            (vec![pt(-w + k, -h), pt(w + k, -h), pt(w - k, h), pt(-w - k, h)], ShapeClass::Parallelogram)
        }
        _ => {
            let k = w * rng.range(0.2, 0.6);
            (vec![pt(-w + k, -h), pt(w - k * rng.range(0.3, 1.0), -h), pt(w, h), pt(-w, h)], ShapeClass::Trapezoid)
        }
    }
}

fn arrow(rng: &mut Rng, c: Pt, size: f32) -> (Vec<IdealStroke>, Option<ShapeClass>) {
    let a = level_angle(rng, 0.4, 0.05);
    let half = Pt::unit(a).scale(size * 0.5);
    let kind = rng.int(0, 2);
    let mut shaft: Vec<Piece> = match kind {
        0 => vec![Piece::Line(c.sub(half), c.add(half))],
        1 => {
            let mid = c.add(Pt::unit(a + PI / 2.0).scale(size * rng.range(0.2, 0.5)));
            vec![Piece::Line(c.sub(half), mid), Piece::Line(mid, c.add(half))]
        }
        _ => {
            let sweep = rng.range(0.5, 1.2) * PI * if rng.chance(0.5) { 1.0 } else { -1.0 };
            let r = size * 0.5;
            vec![Piece::Arc(c, r, r, 0.0, a, sweep)]
        }
    };
    let double = rng.chance(0.3);
    // Closed heads (a triangle in its own stroke, often drawn before the shaft) are bigger and wider,
    // as in the user's notes.
    let closed = rng.chance(0.35);
    let head_len = size * if closed { rng.range(0.12, 0.3) } else { rng.range(0.08, 0.22) };
    let spread = if closed { rng.range(0.4, 0.85) } else { rng.range(0.35, 0.75) };
    let tangent = |p: &Piece, end: bool| -> (Pt, f32) {
        let (u0, u1) = if end { (0.98, 1.0) } else { (0.02, 0.0) };
        let (q0, q1) = (p.at(u0), p.at(u1));
        (q1, q1.sub(q0).angle())
    };
    let head = |rng: &mut Rng, tip: Pt, dir: f32| -> Vec<Piece> {
        let b1 = tip.add(Pt::unit(dir + PI - spread).scale(head_len * rng.range(0.85, 1.15)));
        let b2 = tip.add(Pt::unit(dir + PI + spread).scale(head_len * rng.range(0.85, 1.15)));
        if !closed {
            return vec![Piece::Line(b1, tip), Piece::Line(tip, b2)];
        }
        let mut tri = vec![Piece::Line(b1, tip), Piece::Line(tip, b2), Piece::Line(b2, b1)];
        tri.rotate_left(rng.int(0, 2));
        if rng.chance(0.3) {
            tri.push(tri[0]); // gone over once more
        }
        tri
    };
    let (tip, dir) = tangent(shaft.last().unwrap(), true);
    let mut strokes = Vec::new();
    let h = head(rng, tip, dir);
    if closed {
        strokes.push(shaft.clone());
        strokes.insert(if rng.chance(0.5) { 0 } else { 1 }, h);
    } else if rng.chance(0.55) {
        // Same stroke: shaft to the tip, out along one barb, back to the tip, out along the other.
        let Piece::Line(b1, _) = h[0] else { unreachable!() };
        shaft.push(Piece::Line(tip, b1));
        shaft.push(Piece::Line(b1, tip));
        shaft.push(h[1]);
        strokes.push(shaft.clone());
    } else {
        strokes.push(shaft.clone());
        strokes.push(h);
    }
    if double {
        let (tail, back) = tangent(&shaft[0], false);
        strokes.push(head(rng, tail, back));
    }
    let class = match (kind, double) {
        (0, false) => ShapeClass::Arrow,
        (0, true) => ShapeClass::DoubleArrow,
        (1, false) => ShapeClass::PolylineArrow,
        (1, true) => ShapeClass::PolylineDoubleArrow,
        (_, false) => ShapeClass::CurvedArrow,
        (_, true) => ShapeClass::CurvedDoubleArrow,
    };
    (strokes, Some(class))
}

/// A smooth random walk with loops: writing-like ink that must not snap.
fn scribble(rng: &mut Rng, c: Pt, size: f32) -> IdealStroke {
    let n = rng.int(4, 14);
    let mut pts = vec![c];
    let mut a = rng.range(-PI, PI);
    let mut p = c;
    for _ in 0..n {
        a += rng.normal() * 1.4;
        p = p.add(Pt::unit(a).scale(size * rng.range(0.08, 0.3)));
        pts.push(p);
    }
    // Catmull-Rom-ish: arcs between midpoints, approximated by short lines through a smoothed path.
    let mut out = Vec::new();
    for w in pts.windows(3) {
        let (m0, m1) = (w[0].lerp(w[1], 0.5), w[1].lerp(w[2], 0.5));
        let steps = 6;
        let mut prev = m0;
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let q = m0.lerp(w[1], t).lerp(w[1].lerp(m1, t), t);
            out.push(Piece::Line(prev, q));
            prev = q;
        }
    }
    out
}

/// Draw ideal strokes with a hand model; returns raw samples and their labels.
fn humanise(rng: &mut Rng, strokes: &[IdealStroke], size: f32, ends_are_corners: bool) -> (Vec<Vec<Sample>>, Vec<Vec<PointLabel>>) {
    let diag = size.max(10.0);
    let corner_zone = diag * 2.0 / crate::features::RESOLUTION;
    let smooth = diag * rng.range(0.0, 0.025);
    let wobble_amp = diag * rng.range(0.002, 0.02);
    let jitter = rng.range(0.1, 0.8);
    let vmax = rng.range(0.6, 3.5); // px per ms
    let dt = rng.range(3.0, 8.0); // ms between samples
    // Speed kept at corners: most people brake (0.25), some draw at an almost even pace (Atlas's case).
    let corner_speed = if rng.chance(0.35) { rng.range(0.7, 1.0) } else { 0.25 };
    let mut t = 0.0f32;
    let mut inks = Vec::new();
    let mut labels = Vec::new();
    for st in strokes {
        // 1. Dense ideal path (1 px), with arc positions of corners and per-point curvature flag.
        let mut path: Vec<Pt> = Vec::new();
        let mut curved: Vec<bool> = Vec::new();
        let mut corners: Vec<f32> = Vec::new();
        let mut s_acc = 0.0f32;
        for (i, p) in st.iter().enumerate() {
            if i > 0 {
                corners.push(s_acc);
            }
            let n = (p.approx_len().ceil() as usize).max(2);
            for k in 0..n {
                let q = p.at(k as f32 / n as f32);
                if let Some(last) = path.last() {
                    s_acc += q.dist(*last);
                }
                path.push(q);
                curved.push(p.curved());
            }
        }
        let endp = st.last().unwrap().at(1.0);
        s_acc += path.last().map_or(0.0, |l| l.dist(endp));
        path.push(endp);
        curved.push(st.last().unwrap().curved());
        let total = s_acc.max(1.0);
        let ends = if ends_are_corners { vec![0.0, total] } else { Vec::new() };
        // Turns between consecutive pieces that are too gentle are not corners.
        corners.retain(|&s| {
            let i = arc_index(&path, s);
            let (a, b) = (i.saturating_sub(4), (i + 4).min(path.len() - 1));
            let d1 = path[i].sub(path[a]);
            let d2 = path[b].sub(path[i]);
            d1.len() > 0.0 && d2.len() > 0.0 && crate::geom::wrap(d2.angle() - d1.angle()).abs() > 0.5
        });
        corners.extend(ends);
        // 2. Smooth (rounds corners), add wobble along the normal and hooks at the ends.
        let win = (smooth as usize).max(1);
        let sm: Vec<Pt> = (0..path.len())
            .map(|i| {
                let (a, b) = (i.saturating_sub(win), (i + win).min(path.len() - 1));
                path[a..=b].iter().fold(pt(0.0, 0.0), |s, p| s.add(*p)).scale(1.0 / (b - a + 1) as f32)
            })
            .collect();
        let waves: Vec<(f32, f32, f32)> = (0..3).map(|_| (rng.range(0.5, 4.0), rng.range(0.0, 2.0 * PI), rng.normal())).collect();
        let mut s = 0.0f32;
        let mut noisy = Vec::with_capacity(sm.len());
        for i in 0..sm.len() {
            if i > 0 {
                s += path[i].dist(path[i - 1]);
            }
            let tan = sm[(i + 1).min(sm.len() - 1)].sub(sm[i.saturating_sub(1)]);
            let nrm = if tan.len() > 0.0 { pt(-tan.y, tan.x).scale(1.0 / tan.len()) } else { pt(0.0, 0.0) };
            let off: f32 = waves.iter().map(|(f, ph, a)| a * (2.0 * PI * f * s / total + ph).sin()).sum::<f32>() * wobble_amp;
            noisy.push(sm[i].add(nrm.scale(off)));
        }
        if rng.chance(0.25) && noisy.len() > 4 {
            let hook = Pt::unit(rng.range(-PI, PI)).scale(diag * rng.range(0.01, 0.04));
            for (k, q) in noisy.iter_mut().take(6).enumerate() {
                *q = q.add(hook.scale(1.0 - k as f32 / 6.0));
            }
        }
        // 3. Sample in time: slower near corners and ends.
        let mut cum = vec![0.0f32; noisy.len()];
        for i in 1..noisy.len() {
            cum[i] = cum[i - 1] + noisy[i].dist(noisy[i - 1]);
        }
        let len = *cum.last().unwrap();
        let mut samples = Vec::new();
        let mut lab = Vec::new();
        let mut pos = 0.0f32;
        let mut j = 0;
        loop {
            while j + 1 < cum.len() && cum[j + 1] < pos {
                j += 1;
            }
            let q = if j + 1 < cum.len() && cum[j + 1] > cum[j] {
                noisy[j].lerp(noisy[j + 1], (pos - cum[j]) / (cum[j + 1] - cum[j]))
            } else {
                noisy[j]
            };
            let sp = pos / len.max(1e-3) * total; // arc position on the ideal path
            let near = corners.iter().map(|c| (c - sp).abs()).fold(f32::MAX, f32::min);
            samples.push(Sample { x: q.x + rng.normal() * jitter, y: q.y + rng.normal() * jitter, t });
            lab.push(if near < corner_zone {
                PointLabel::Corner
            } else if curved[arc_index(&path, sp)] {
                PointLabel::Curve
            } else {
                PointLabel::Straight
            });
            if pos >= len {
                break;
            }
            let slow = (near / (diag * 0.08)).min(1.0).min((pos.min(len - pos) / (diag * 0.05)).min(1.0));
            let v = vmax * (corner_speed + (1.0 - corner_speed) * slow);
            pos = (pos + v * dt * rng.range(0.8, 1.2)).min(len);
            t += dt;
        }
        t += rng.range(80.0, 400.0); // pen lift
        inks.push(samples);
        labels.push(lab);
    }
    (inks, labels)
}

/// Index of the dense ideal point nearest arc position `s`.
fn arc_index(path: &[Pt], s: f32) -> usize {
    let mut acc = 0.0;
    for i in 1..path.len() {
        acc += path[i].dist(path[i - 1]);
        if acc >= s {
            return i;
        }
    }
    path.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_labelled() {
        let a = example(&mut Rng::new(7));
        let b = example(&mut Rng::new(7));
        assert_eq!(a.ink, b.ink);
        let mut rng = Rng::new(1);
        for _ in 0..300 {
            let e = example(&mut rng);
            assert_eq!(e.ink.len(), e.labels.len());
            for (s, l) in e.ink.iter().zip(&e.labels) {
                assert_eq!(s.len(), l.len());
                assert!(s.len() >= 2);
            }
        }
    }

    #[test]
    fn polygons_have_corners() {
        let mut rng = Rng::new(3);
        let mut seen = 0;
        for _ in 0..400 {
            let e = example(&mut rng);
            if e.family == Family::Polygon {
                assert!(e.labels.iter().flatten().any(|l| *l == PointLabel::Corner), "{:?}", e.class);
                seen += 1;
            }
        }
        assert!(seen > 20);
    }
}
