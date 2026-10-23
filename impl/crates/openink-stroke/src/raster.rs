//! A small software rasterizer for host previews and tests, and the dab grain masks that the
//! Android painter also uses (so the texture is ours, not Onyx's `pencil` drawable).
//! Values are ink coverage: 0 = paper, 1 = full ink.

use crate::geom;
use crate::pen::{Output, MARKER_ALPHA};
use crate::stroke::Ink;

const SUB_ROWS: usize = 4;

pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
    /// Grain masks by edge length, as the Android painter caches them.
    masks: std::collections::HashMap<usize, Vec<u8>>,
}

impl Gray {
    pub fn new(w: usize, h: usize) -> Self {
        Gray { w, h, px: vec![0.0; w * h], masks: Default::default() }
    }

    /// Draw one pen's ink. Translucent ink goes through a stroke layer, as on the device.
    pub fn draw(&mut self, ink: &Ink) {
        match ink.output {
            Output::Fill => self.fill_union(&ink.data),
            Output::Translucent => self.composite_translucent(ink),
            Output::Dabs => {
                for d in ink.data.chunks_exact(5) {
                    self.stamp(d[0], d[1], d[2], d[3], d[4]);
                }
            }
        }
    }

    /// The stroke's own layer, composited once at the marker alpha. The layer covers only the
    /// ink's bounds (shifted by whole pixels): same coverage as a full-size layer up to float
    /// rounding (12 of 28.9 M preview pixels differ, by 1/255), without a full-image allocation.
    fn composite_translucent(&mut self, ink: &Ink) {
        let Some([l, t, r, b]) = ink.bounds else { return };
        let (ox, oy) = (l.floor().max(0.0) as usize, t.floor().max(0.0) as usize);
        let (ex, ey) = ((r.ceil() as usize + 1).min(self.w), (b.ceil() as usize + 1).min(self.h));
        if ox >= ex || oy >= ey {
            return;
        }
        let mut shifted = ink.data.clone();
        let mut i = 0;
        while i < shifted.len() {
            let n = shifted[i] as usize;
            for k in 0..n {
                shifted[i + 1 + 2 * k] -= ox as f32;
                shifted[i + 2 + 2 * k] -= oy as f32;
            }
            i += 1 + 2 * n;
        }
        let mut layer = Gray::new(ex - ox, ey - oy);
        layer.fill_union(&shifted);
        for (row, src) in layer.px.chunks_exact(layer.w).enumerate() {
            let dst = &mut self.px[(oy + row) * self.w + ox..(oy + row) * self.w + ex];
            for (d, l) in dst.iter_mut().zip(src) {
                *d += (1.0 - *d) * l * MARKER_ALPHA;
            }
        }
    }

    /// Union of convex polygons: per-pixel max of each one's coverage.
    fn fill_union(&mut self, polys: &[f32]) {
        for p in geom::polygons(polys) {
            self.fill_convex(p);
        }
    }

    fn fill_convex(&mut self, xy: &[f32]) {
        let n = xy.len() / 2;
        if n < 3 {
            return;
        }
        let ys = xy.iter().skip(1).step_by(2);
        let (y0, y1) = ys.fold((f32::MAX, f32::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        let (r0, r1) = ((y0.floor().max(0.0)) as usize, (y1.ceil().min(self.h as f32)) as usize);
        let mut row = vec![0f32; self.w + 1];
        for r in r0..r1 {
            row.iter_mut().for_each(|v| *v = 0.0);
            let (mut lo, mut hi) = (usize::MAX, 0);
            for k in 0..SUB_ROWS {
                let yc = r as f32 + (k as f32 + 0.5) / SUB_ROWS as f32;
                let (mut a, mut b) = (f32::MAX, f32::MIN);
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (xa, ya, xb, yb) = (xy[2 * i], xy[2 * i + 1], xy[2 * j], xy[2 * j + 1]);
                    if (ya <= yc) != (yb <= yc) {
                        let x = xa + (yc - ya) / (yb - ya) * (xb - xa);
                        a = a.min(x);
                        b = b.max(x);
                    }
                }
                if a >= b {
                    continue;
                }
                let (a, b) = (a.max(0.0), b.min(self.w as f32));
                let (ca, cb) = (a.floor() as usize, (b.ceil() as usize).min(self.w));
                lo = lo.min(ca);
                hi = hi.max(cb);
                for (c, cell) in row.iter_mut().enumerate().take(cb).skip(ca) {
                    let cov = (b.min(c as f32 + 1.0) - a.max(c as f32)).max(0.0);
                    *cell += cov / SUB_ROWS as f32;
                }
            }
            let hi = hi.min(self.w);
            if lo < hi {
                let dst = &mut self.px[r * self.w + lo..r * self.w + hi];
                for (d, v) in dst.iter_mut().zip(&row[lo..hi]) {
                    *d = d.max(v.min(1.0));
                }
            }
        }
    }

    fn stamp(&mut self, x: f32, y: f32, size: f32, angle: f32, alpha: f32) {
        let m = mask_size(size);
        let mask = std::mem::take(self.masks.entry(m).or_insert_with(|| grain_mask(m)));
        let reach = size * 0.75 + 1.0;
        let (s, c) = (-angle).sin_cos();
        let half = m as f32 / 2.0;
        let rows = ((y - reach).floor().max(0.0) as usize)..((y + reach).ceil().min(self.h as f32) as usize);
        for r in rows {
            for col in ((x - reach).floor().max(0.0) as usize)..((x + reach).ceil().min(self.w as f32) as usize) {
                let (dx, dy) = (col as f32 + 0.5 - x, r as f32 + 0.5 - y);
                let (u, v) = (c * dx - s * dy + half, s * dx + c * dy + half);
                if u < 0.0 || v < 0.0 || u >= m as f32 || v >= m as f32 {
                    continue;
                }
                let a = mask[v as usize * m + u as usize] as f32 / 255.0 * alpha;
                let d = &mut self.px[r * self.w + col];
                *d += (1.0 - *d) * a;
            }
        }
        self.masks.insert(m, mask); // taken above so `self.px` could be borrowed mutably
    }

    /// Binary PGM, ink black on white paper.
    pub fn to_pgm(&self) -> Vec<u8> {
        let mut out = format!("P5\n{} {}\n255\n", self.w, self.h).into_bytes();
        out.extend(self.px.iter().map(|v| (255.0 * (1.0 - v.clamp(0.0, 1.0))).round() as u8));
        out
    }
}

/// The mask edge length for a dab of `size` px (the Android painter caches one per length).
pub fn mask_size(size: f32) -> usize {
    (size.round() as usize).clamp(2, 256)
}

/// A `m × m` alpha mask: a soft disc broken up by paper-scale grain (cells of about 1.5 px,
/// independent of the dab size, as real graphite catches on the paper's tooth).
pub fn grain_mask(m: usize) -> Vec<u8> {
    let r = m as f32 / 2.0;
    let mut out = Vec::with_capacity(m * m);
    for v in 0..m {
        for u in 0..m {
            let (dx, dy) = (u as f32 + 0.5 - r, v as f32 + 0.5 - r);
            let edge = (r - dx.hypot(dy)).clamp(0.0, 1.0); // 1 px soft rim
            let g = value_noise(u as f32 / 1.5, v as f32 / 1.5, m as u32);
            let grain = 0.25 + 0.75 * smoothstep(0.2, 0.8, g);
            out.push((255.0 * edge * grain).round() as u8);
        }
    }
    out
}

/// A charcoal stroke as one alpha bitmap: every dab stamped the way libneo's charcoal V2 draws it (docs/STROKE.md,
/// "Charcoal V2 drawing"): an S×S shape, opaque inside an inner ellipse of 0.6·S and fading to the outer circle,
/// times a paper grain anchored in screen space, then **dithered to on/off pixels** (on with probability
/// alpha/255). The probability comes from a hash of the pixel and the dab, so a stroke stays a pure function of
/// its samples. `dabs` are `x, y, size, angle, alpha` (Ink::data of a dab pen). Returns
/// (left, top, width, height, alpha bytes 0/255), empty when there are no dabs.
pub fn charcoal_bitmap(dabs: &[f32]) -> (i32, i32, usize, usize, Vec<u8>) {
    let tiles: Vec<(i32, i32, usize, f32)> = dabs
        .chunks_exact(5)
        .filter(|d| d.iter().all(|v| v.is_finite()) && d[2] >= 1.0)
        .map(|d| {
            let s = (d[2] as usize).clamp(2, 256);
            ((d[0] - (s / 2) as f32) as i32, (d[1] - (s / 2) as f32) as i32, s, d[4].clamp(0.0, 1.0))
        })
        .collect();
    if tiles.is_empty() {
        return (0, 0, 0, 0, Vec::new());
    }
    let left = tiles.iter().map(|t| t.0).min().unwrap();
    let top = tiles.iter().map(|t| t.1).min().unwrap();
    let w = (tiles.iter().map(|t| t.0 + t.2 as i32).max().unwrap() - left) as usize;
    let h = (tiles.iter().map(|t| t.1 + t.2 as i32).max().unwrap() - top) as usize;
    let mut out = vec![0u8; w * h];
    for (k, &(l, t, s, alpha)) in tiles.iter().enumerate() {
        let half = s as f32 * 0.5;
        let (outer, inner) = (2.0 / s as f32, 2.0 / (s as f32 * 0.6));
        for v in 0..s {
            for u in 0..s {
                let (dx, dy) = (u as f32 - half, v as f32 - half);
                // libneo's CircleMask (FUN_0044e96c): 0 inside the inner ellipse, 255 outside the outer one.
                let f1 = (outer * dx).hypot(outer * dy);
                if f1 > 1.0 {
                    continue;
                }
                let f2 = (inner * dx).hypot(inner * dy);
                let mask = if f2 < 1.0 { 0.0 } else { f1 * 255.0 * (f2 - 1.0) / (f2 - f1) };
                let shape = 1.0 - mask / 255.0;
                let (px, py) = (l + u as i32, t + v as i32);
                let g = value_noise(px as f32 / 1.5, py as f32 / 1.5, 0x5eed);
                let texture = (0.25 + 0.75 * smoothstep(0.2, 0.8, g)) * alpha;
                let a = shape * texture;
                let on = a >= 1.0 || dither(px, py, k as u32) <= (a * 10000.0) as u32;
                if on {
                    out[(py - top) as usize * w + (px - left) as usize] = 255;
                }
            }
        }
    }
    (left, top, w, h, out)
}

/// 1..=10000 from a pixel and a dab index (libneo draws `rand` in [1, 10000]; a hash keeps strokes reproducible).
fn dither(x: i32, y: i32, dab: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ dab.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h % 10000 + 1
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bilinear value noise in 0..1, deterministic per `seed`.
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let at = |i: i32, j: i32| {
        let mut h = (i as u32).wrapping_mul(0x9e37_79b1) ^ (j as u32).wrapping_mul(0x85eb_ca77) ^ seed.wrapping_mul(0xc2b2_ae3d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        (h & 0xffff) as f32 / 65535.0
    };
    let top = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
    let bot = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
    top + (bot - top) * fy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::{PenConfig, PenKind};
    use crate::stroke::{Sample, Stroke};

    #[test]
    fn charcoal_is_binary_and_dense_in_the_middle() {
        let (l, t, w, h, px) = charcoal_bitmap(&[50.0, 50.0, 20.0, 0.0, 1.0]);
        assert_eq!((l, t, w, h), (40, 40, 20, 20));
        assert!(px.iter().all(|&v| v == 0 || v == 255), "on/off pixels only, as libneo's");
        let on = |r0: f32, r1: f32| {
            let (mut n, mut k) = (0, 0);
            for v in 0..h {
                for u in 0..w {
                    let r = (u as f32 - 10.0).hypot(v as f32 - 10.0);
                    if r >= r0 && r < r1 {
                        n += 1;
                        k += usize::from(px[v * w + u] == 255);
                    }
                }
            }
            k as f32 / n as f32
        };
        assert!(on(0.0, 5.0) > on(7.0, 10.0), "denser inside the 0.6 ellipse than at the rim");
        assert_eq!(charcoal_bitmap(&[50.0, 50.0, 20.0, 0.0, 1.0]).4, px, "deterministic");
    }

    #[test]
    fn a_filled_square_covers_exactly_its_pixels() {
        let mut g = Gray::new(10, 10);
        g.fill_union(&[4.0, 2.0, 2.0, 6.0, 2.0, 6.0, 6.0, 2.0, 6.0]);
        let total: f32 = g.px.iter().sum();
        assert!((total - 16.0).abs() < 1e-3, "{total}");
        assert_eq!(g.px[3 * 10 + 3], 1.0);
        assert_eq!(g.px[10 + 3], 0.0);
    }

    #[test]
    fn marker_overlap_does_not_darken() {
        let s = |x: f32| Sample { x, y: 50.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: x };
        // Out and back over the same line: one stroke, so one layer.
        let pts: Vec<Sample> = (0..60).map(|i| s(20.0 + i as f32)).chain((0..60).rev().map(|i| s(20.0 + i as f32))).collect();
        let ink = Stroke::new(PenKind::Marker, PenConfig::knote2_default(PenKind::Marker, 300.0)).render(&pts);
        let mut g = Gray::new(120, 100);
        g.draw(&ink);
        let max = g.px.iter().cloned().fold(0.0, f32::max);
        assert!((max - MARKER_ALPHA).abs() < 1e-3, "{max}");
    }

    #[test]
    fn grain_mask_is_a_textured_disc() {
        let m = grain_mask(20);
        assert_eq!(m.len(), 400);
        assert_eq!(m[0], 0, "corner outside the disc");
        let centre: Vec<u8> = (8..12).flat_map(|v| (8..12).map(move |u| (u, v))).map(|(u, v)| m[v * 20 + u]).collect();
        let (lo, hi) = (centre.iter().min().unwrap(), centre.iter().max().unwrap());
        assert!(*hi > 100 && hi - lo > 20, "grain range {lo}..{hi}");
    }
}
