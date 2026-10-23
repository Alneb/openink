//! Split normalised ink into straight / curved pieces at the segmenter's corners.

use crate::features::Norm;
use crate::geom::{path_len, seg_dist, Pt};
use crate::nn::{PointLabel, POINT_LABELS};

/// A run of resampled points between two corners (or a stroke end), drawn without lifting the pen.
#[derive(Clone, Debug)]
pub struct Piece {
    pub pts: Vec<Pt>,
    pub stroke: usize,
    pub curved: bool,
}

impl Piece {
    pub fn start(&self) -> Pt {
        self.pts[0]
    }
    pub fn end(&self) -> Pt {
        *self.pts.last().unwrap()
    }
    pub fn len(&self) -> f32 {
        path_len(&self.pts)
    }
    pub fn is_empty(&self) -> bool {
        self.pts.len() < 2
    }
    /// Direction from start to end.
    pub fn angle(&self) -> f32 {
        self.end().sub(self.start()).angle()
    }
}

/// A corner needs this probability; a piece needs this many points.
const CORNER_P: f32 = 0.5;
const MIN_POINTS: usize = 3;
/// End pieces shorter than this fraction of the diagonal are pen hooks and are dropped.
const HOOK: f32 = 0.06;

/// Pieces in drawing order. `probs` is the segmenter output, one per `norm.pts`.
pub fn pieces(norm: &Norm, probs: &[[f32; POINT_LABELS]]) -> Vec<Piece> {
    let corner = PointLabel::Corner as usize;
    let mut out = Vec::new();
    for range in norm.stroke_ranges() {
        // One corner per run of high corner probability: its most probable point.
        let mut cuts = vec![range.start];
        let mut i = range.start;
        while i < range.end {
            if probs[i][corner] < CORNER_P {
                i += 1;
                continue;
            }
            let run_start = i;
            while i < range.end && probs[i][corner] >= CORNER_P {
                i += 1;
            }
            let best = (run_start..i).max_by(|&a, &b| probs[a][corner].total_cmp(&probs[b][corner])).unwrap();
            if best - cuts.last().unwrap() >= MIN_POINTS && range.end - 1 - best >= MIN_POINTS {
                cuts.push(best);
            }
        }
        cuts.push(range.end - 1);
        let mut stroke_pieces: Vec<Piece> = cuts
            .windows(2)
            .filter(|w| w[1] > w[0])
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let (straight, curve): (f32, f32) = (a..=b).fold((0.0, 0.0), |(s, c), k| (s + probs[k][0], c + probs[k][1]));
                Piece {
                    pts: norm.pts[a..=b].iter().map(|r| r.p).collect(),
                    stroke: norm.pts[a].stroke,
                    curved: curve > straight,
                }
            })
            .collect();
        drop_hooks(&mut stroke_pieces, norm.bbox.diag() * HOOK);
        out.extend(stroke_pieces);
    }
    out.retain(|p| !p.is_empty());
    out
}

fn drop_hooks(ps: &mut Vec<Piece>, min_len: f32) {
    if ps.len() > 1 && ps.last().unwrap().len() < min_len {
        ps.pop();
    }
    if ps.len() > 1 && ps[0].len() < min_len {
        ps.remove(0);
    }
}

/// Douglas–Peucker splits as a plain geometric second opinion (per stroke, tolerance `DP_TOL` of the
/// diagonal), for when the segmenter misses a corner. Pieces are straight by construction.
pub fn dp_pieces(norm: &Norm) -> Vec<Piece> {
    let tol = norm.bbox.diag() * DP_TOL;
    let mut out = Vec::new();
    for range in norm.stroke_ranges() {
        let pts: Vec<Pt> = norm.pts[range.clone()].iter().map(|r| r.p).collect();
        if pts.len() < 2 {
            continue;
        }
        let mut keep = vec![0, pts.len() - 1];
        dp(&pts, 0, pts.len() - 1, tol, &mut keep);
        keep.sort_unstable();
        let mut stroke_pieces: Vec<Piece> = keep
            .windows(2)
            .filter(|w| w[1] > w[0])
            .map(|w| Piece { pts: pts[w[0]..=w[1]].to_vec(), stroke: norm.pts[range.start].stroke, curved: false })
            .collect();
        drop_hooks(&mut stroke_pieces, norm.bbox.diag() * HOOK);
        out.extend(stroke_pieces);
    }
    out.retain(|p| !p.is_empty());
    out
}

/// Douglas–Peucker tolerance as a fraction of the bbox diagonal.
const DP_TOL: f32 = 0.035;

fn dp(pts: &[Pt], a: usize, b: usize, tol: f32, keep: &mut Vec<usize>) {
    if b <= a + 1 {
        return;
    }
    // A closed stroke has a == b positions: measure distance to the start point instead of a segment.
    let (far, d) = (a + 1..b)
        .map(|i| (i, if pts[a].dist(pts[b]) < 1e-6 { pts[i].dist(pts[a]) } else { seg_dist(pts[i], pts[a], pts[b]) }))
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .unwrap();
    if d > tol {
        keep.push(far);
        dp(pts, a, far, tol, keep);
        dp(pts, far, b, tol, keep);
    }
}
