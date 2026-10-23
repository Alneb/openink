//! Ink normalisation and the two feature sets: one vector per ink (classifier) and one per
//! resampled point (segmenter). Our own definitions; docs/SHAPE-ENGINE.md lists them.
//! The training-data generator calls the same functions, so training and inference cannot drift.

use crate::geom::{hull, line_angle_diff, pca, perimeter_closed, pt, resample, area, wrap, BBox, Pt};
use std::f32::consts::PI;

/// One raw pen sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    /// Milliseconds, any origin; only differences are used.
    pub t: f32,
}

/// The ink to recognise: one or more strokes, in drawing order.
pub type Ink = [Vec<Sample>];

/// Points per bounding-box diagonal after resampling.
pub const RESOLUTION: f32 = 64.0;
pub const STROKE_FEATURES: usize = 40;
pub const POINT_FEATURES: usize = 10;

/// A resampled point: position, its stroke, and the interpolated time.
#[derive(Clone, Copy, Debug)]
pub struct RPoint {
    pub p: Pt,
    pub stroke: usize,
    pub t: f32,
    /// Index of the nearest raw sample (by arc length) within its stroke.
    pub src: usize,
}

/// Ink resampled to `diag / RESOLUTION` spacing. All features are built on this.
#[derive(Clone, Debug)]
pub struct Norm {
    pub pts: Vec<RPoint>,
    pub bbox: BBox,
    pub step: f32,
    pub strokes: usize,
}

impl Norm {
    /// `None` when there is too little ink to say anything (a dot).
    pub fn new(ink: &Ink) -> Option<Norm> {
        let all: Vec<Pt> = ink.iter().flatten().map(|s| pt(s.x, s.y)).collect();
        let bbox = BBox::of(&all);
        if all.len() < 2 || bbox.diag() < 4.0 {
            return None;
        }
        let step = bbox.diag() / RESOLUTION;
        let mut pts = Vec::new();
        let mut strokes = 0;
        for st in ink.iter() {
            if st.is_empty() {
                strokes += 1; // keep stroke indices aligned with the caller's
                continue;
            }
            let raw: Vec<Pt> = st.iter().map(|s| pt(s.x, s.y)).collect();
            let r = smooth(&resample(&raw, step));
            // Time per resampled point: from the raw sample nearest in arc length.
            let mut cum = vec![0.0f32; raw.len()];
            for i in 1..raw.len() {
                cum[i] = cum[i - 1] + raw[i - 1].dist(raw[i]);
            }
            let mut j = 0;
            for (k, p) in r.iter().enumerate() {
                let want = k as f32 * step;
                while j + 1 < cum.len() && cum[j + 1] < want {
                    j += 1;
                }
                let src = j.min(st.len() - 1);
                pts.push(RPoint { p: *p, stroke: strokes, t: st[src].t, src });
            }
            strokes += 1;
        }
        Some(Norm { pts, bbox, step, strokes })
    }

    pub fn points(&self) -> Vec<Pt> {
        self.pts.iter().map(|r| r.p).collect()
    }

    /// Index ranges of each stroke in `pts`.
    pub fn stroke_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut out: Vec<std::ops::Range<usize>> = Vec::new();
        for (i, r) in self.pts.iter().enumerate() {
            match out.last_mut() {
                Some(last) if self.pts[last.start].stroke == r.stroke => last.end = i + 1,
                _ => out.push(i..i + 1),
            }
        }
        out
    }
}

/// Two passes of a [1, 2, 1] / 4 filter (≈ one step of blur), ends kept: removes sample-level zigzag
/// (pen jitter, Atlas's `wobble()`) that otherwise reads as curvature, while corners stay sharp at the
/// 2-step scale the features use.
fn smooth(r: &[Pt]) -> Vec<Pt> {
    let mut a = r.to_vec();
    for _ in 0..2 {
        if a.len() < 3 {
            break;
        }
        let b = a.clone();
        for i in 1..a.len() - 1 {
            a[i] = b[i - 1].add(b[i].scale(2.0)).add(b[i + 1]).scale(0.25);
        }
    }
    a
}

/// Signed turning angle at `i` over `k` points either side, within the point's stroke.
fn turning(pts: &[RPoint], range: &std::ops::Range<usize>, i: usize, k: usize) -> f32 {
    let a = i.saturating_sub(k).max(range.start);
    let b = (i + k).min(range.end - 1);
    if a == i || b == i {
        return 0.0;
    }
    let d1 = pts[i].p.sub(pts[a].p);
    let d2 = pts[b].p.sub(pts[i].p);
    if d1.len() <= 0.0 || d2.len() <= 0.0 {
        return 0.0;
    }
    wrap(d2.angle() - d1.angle())
}

fn tanh(x: f32) -> f32 {
    x.tanh()
}

/// Per-ink feature vector for the family classifier.
pub fn stroke_features(n: &Norm) -> [f32; STROKE_FEATURES] {
    let mut f = [0.0f32; STROKE_FEATURES];
    let pts = n.points();
    let ranges = n.stroke_ranges();
    let diag = n.bbox.diag().max(1e-3);
    let (w, h) = (n.bbox.w().max(1e-3), n.bbox.h().max(1e-3));
    let len: f32 = ranges.iter().map(|r| crate::geom::path_len(&pts[r.clone()])).sum::<f32>().max(1e-3);

    f[0] = ((n.strokes as f32 - 1.0) / 3.0).min(1.0);
    f[1] = tanh((h / w).ln());
    f[2] = tanh((len / diag - 1.0) / 4.0);
    let first = pts[0];
    let last = *pts.last().unwrap();
    f[3] = first.dist(last) / len;
    // Closure: the smallest gap between any stroke end and the ink's start, over the diagonal.
    let ends: Vec<Pt> = ranges.iter().flat_map(|r| [pts[r.start], pts[r.end - 1]]).collect();
    let mut gap = f32::MAX;
    for (i, a) in ends.iter().enumerate() {
        for b in &ends[i + 1..] {
            gap = gap.min(a.dist(*b));
        }
    }
    if ends.len() == 2 {
        gap = first.dist(last);
    }
    f[4] = (gap / diag).min(1.0);
    let hl = hull(&pts);
    let ha = area(&hl).abs();
    f[5] = ha / (w * h);
    let hp = perimeter_closed(&hl).max(1e-3);
    f[6] = 4.0 * PI * ha / (hp * hp);

    // Turning statistics at a scale of 2 steps.
    let (mut total, mut net, mut corners, mut max_turn, mut reversals, mut straight) = (0.0f32, 0.0f32, 0, 0.0f32, 0, 0);
    let mut mean_abs = 0.0;
    let mut count = 0;
    for r in &ranges {
        let mut in_corner = false;
        for i in r.clone() {
            let t = turning(&n.pts, r, i, 2);
            total += t.abs() * 0.5;
            net += t * 0.5;
            mean_abs += t.abs();
            count += 1;
            if t.abs() < 0.15 {
                straight += 1;
            }
            max_turn = max_turn.max(t.abs());
            let sharp = t.abs() > PI / 4.0;
            if sharp && !in_corner {
                corners += 1;
            }
            if t.abs() > PI * 5.0 / 6.0 && !in_corner {
                reversals += 1;
            }
            in_corner = sharp;
        }
    }
    let count = count.max(1) as f32;
    f[7] = tanh(total / (2.0 * PI) / 2.0);
    f[8] = tanh(net.abs() / (2.0 * PI));
    f[9] = if total > 0.0 { net.abs() / total } else { 0.0 };
    f[10] = (corners as f32 / 8.0).min(1.0);
    f[11] = max_turn / PI;
    let (c, _, major, minor) = pca(&pts);
    f[12] = if major > 0.0 { (minor / major).sqrt() } else { 0.0 };
    f[13] = tanh(reversals as f32 / 4.0);

    // 14..22: undirected direction histogram, length-weighted, soft-binned.
    for r in &ranges {
        for i in r.start + 1..r.end {
            let d = pts[i].sub(pts[i - 1]);
            let l = d.len();
            if l <= 0.0 {
                continue;
            }
            let a = d.angle().rem_euclid(PI) / PI * 8.0;
            let b0 = a.floor() as usize % 8;
            let frac = a - a.floor();
            f[14 + b0] += l * (1.0 - frac);
            f[14 + (b0 + 1) % 8] += l * frac;
        }
    }
    for v in &mut f[14..22] {
        *v /= len;
    }

    // 22..30: radial distance histogram (distance from centroid over its mean), 30: its spread.
    let rad: Vec<f32> = pts.iter().map(|p| p.dist(c)).collect();
    let mean_r = (rad.iter().sum::<f32>() / rad.len() as f32).max(1e-3);
    for r in &rad {
        let b = ((r / mean_r) * 4.0).floor() as usize;
        f[22 + b.min(7)] += 1.0 / rad.len() as f32;
    }
    let var_r = rad.iter().map(|r| (r - mean_r) * (r - mean_r)).sum::<f32>() / rad.len() as f32;
    f[30] = var_r.sqrt() / mean_r;
    f[31] = straight as f32 / count;

    // 32/33: turning in the last / first 15 % of the ink (arrow heads, hooks).
    let k = ((pts.len() as f32) * 0.15).ceil() as usize;
    let end_turn = |idx: &mut dyn Iterator<Item = usize>| -> f32 {
        idx.map(|i| {
            let r = ranges.iter().find(|r| r.contains(&i)).unwrap();
            turning(&n.pts, r, i, 1).abs()
        })
        .sum::<f32>()
            / PI
    };
    f[32] = tanh(end_turn(&mut (pts.len().saturating_sub(k)..pts.len())) / 2.0);
    f[33] = tanh(end_turn(&mut (0..k.min(pts.len()))) / 2.0);
    f[34] = tanh(self_intersections(&pts, &ranges) as f32 / 4.0);
    f[35] = speed_variation(n);
    f[36] = longest_straight_run(n, &ranges) / len;
    f[37] = mean_abs / count / PI;
    let (cx, cy) = (n.bbox.centre().x, n.bbox.centre().y);
    let (mut xc, mut yc) = (0, 0);
    for r in &ranges {
        for i in r.start + 1..r.end {
            if (pts[i - 1].x - cx) * (pts[i].x - cx) < 0.0 {
                xc += 1;
            }
            if (pts[i - 1].y - cy) * (pts[i].y - cy) < 0.0 {
                yc += 1;
            }
        }
    }
    f[38] = tanh(xc as f32 / 8.0);
    f[39] = tanh(yc as f32 / 8.0);
    f
}

/// Proper crossings between non-adjacent segments (sampled every second segment to stay cheap).
fn self_intersections(pts: &[Pt], ranges: &[std::ops::Range<usize>]) -> usize {
    let segs: Vec<(Pt, Pt)> = ranges
        .iter()
        .flat_map(|r| (r.start + 1..r.end).step_by(2).map(move |i| (i - 1, i)))
        .map(|(a, b)| (pts[a], pts[(b + 1).min(pts.len() - 1)]))
        .collect();
    let mut n = 0;
    for i in 0..segs.len() {
        for j in i + 2..segs.len() {
            let (a, b) = segs[i];
            let (c, d) = segs[j];
            let d1 = b.sub(a).cross(c.sub(a));
            let d2 = b.sub(a).cross(d.sub(a));
            let d3 = d.sub(c).cross(a.sub(c));
            let d4 = d.sub(c).cross(b.sub(c));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                n += 1;
            }
        }
    }
    n
}

/// Coefficient of variation of pen speed (0 when timestamps are absent or flat).
fn speed_variation(n: &Norm) -> f32 {
    let mut v = Vec::new();
    for w in n.pts.windows(2) {
        if w[0].stroke == w[1].stroke {
            let dt = w[1].t - w[0].t;
            if dt > 0.0 {
                v.push(n.step / dt);
            }
        }
    }
    if v.len() < 4 {
        return 0.0;
    }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    let s = (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt();
    tanh(s / m.max(1e-6))
}

/// Length of the longest run whose direction stays within 12° of its start.
fn longest_straight_run(n: &Norm, ranges: &[std::ops::Range<usize>]) -> f32 {
    let mut best = 0.0f32;
    for r in ranges {
        let mut s = r.start;
        while s + 1 < r.end {
            let dir = n.pts[s + 1].p.sub(n.pts[s].p).angle();
            let mut e = s + 1;
            while e + 1 < r.end && line_angle_diff(n.pts[e + 1].p.sub(n.pts[e].p).angle(), dir) < 0.21 {
                e += 1;
            }
            best = best.max(n.pts[s].p.dist(n.pts[e].p));
            s = e;
        }
    }
    best
}

/// Per-point features for the segmenter, `POINT_FEATURES` per resampled point.
pub fn point_features(n: &Norm) -> Vec<[f32; POINT_FEATURES]> {
    let ranges = n.stroke_ranges();
    let pts: Vec<Pt> = n.points();
    let total = pts.len().max(2) as f32 - 1.0;
    let (c, _, _, _) = pca(&pts);
    let mean_r = (pts.iter().map(|p| p.dist(c)).sum::<f32>() / pts.len() as f32).max(1e-3);
    let diag = n.bbox.diag().max(1e-3);
    // Median speed, for a relative speed per point.
    let mut speeds: Vec<f32> = n
        .pts
        .windows(2)
        .filter(|w| w[0].stroke == w[1].stroke && w[1].t > w[0].t)
        .map(|w| n.step / (w[1].t - w[0].t))
        .collect();
    speeds.sort_by(f32::total_cmp);
    let med = speeds.get(speeds.len() / 2).copied().unwrap_or(0.0);

    let mut out = Vec::with_capacity(pts.len());
    for r in &ranges {
        for i in r.clone() {
            let mut f = [0.0f32; POINT_FEATURES];
            f[0] = turning(&n.pts, r, i, 1) / PI;
            f[1] = turning(&n.pts, r, i, 2) / PI;
            f[2] = turning(&n.pts, r, i, 4) / PI;
            f[3] = (i.saturating_sub(3).max(r.start)..=(i + 3).min(r.end - 1))
                .map(|j| turning(&n.pts, r, j, 1).abs())
                .sum::<f32>()
                / PI;
            let a = i.saturating_sub(4).max(r.start);
            let b = (i + 4).min(r.end - 1);
            let arc = crate::geom::path_len(&pts[a..=b]);
            f[4] = if arc > 0.0 { pts[a].dist(pts[b]) / arc } else { 1.0 };
            f[5] = i as f32 / total;
            let to_end = (i - r.start).min(r.end - 1 - i) as f32 * n.step;
            f[6] = tanh(to_end / diag * 4.0);
            f[7] = if med > 0.0 && i > r.start && n.pts[i].t > n.pts[i - 1].t {
                tanh((n.step / (n.pts[i].t - n.pts[i - 1].t) / med).ln())
            } else {
                0.0
            };
            f[8] = if i == r.start && r.start > 0 { 1.0 } else { 0.0 };
            f[9] = tanh(pts[i].dist(c) / mean_r - 1.0);
            out.push(f);
        }
    }
    out
}
