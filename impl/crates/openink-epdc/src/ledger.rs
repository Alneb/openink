//! The ghost ledger: accumulated ghosting per screen tile, so cleanup flashes only where needed.

use crate::geom::Rect;
use crate::intent::GhostEffect;

pub struct GhostLedger {
    screen: Rect,
    tile: i32,
    cols: usize,
    rows: usize,
    cost: Vec<f32>,
}

impl GhostLedger {
    /// `tile` is the side of a square accounting cell in pixels (> 0).
    pub fn new(screen: Rect, tile: i32) -> Self {
        assert!(tile > 0, "tile size must be positive");
        let cols = ((screen.width() + tile - 1) / tile) as usize;
        let rows = ((screen.height() + tile - 1) / tile) as usize;
        Self { screen, tile, cols, rows, cost: vec![0.0; cols * rows] }
    }

    /// Tile index range `(c0..c1, r0..r1)` covered by `r`, clipped to the screen.
    fn span(&self, r: &Rect) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
        let r = r.intersect(&self.screen)?;
        let (ox, oy) = (self.screen.left(), self.screen.top());
        let c0 = ((r.left() - ox) / self.tile) as usize;
        let r0 = ((r.top() - oy) / self.tile) as usize;
        let c1 = ((r.right() - ox + self.tile - 1) / self.tile) as usize;
        let r1 = ((r.bottom() - oy + self.tile - 1) / self.tile) as usize;
        Some((c0..c1.min(self.cols), r0..r1.min(self.rows)))
    }

    pub fn apply(&mut self, r: &Rect, effect: GhostEffect) {
        let Some((cs, rs)) = self.span(r) else { return };
        for row in rs {
            for c in cs.clone() {
                let v = &mut self.cost[row * self.cols + c];
                match effect {
                    GhostEffect::Add(x) => *v += x,
                    GhostEffect::Reset => *v = 0.0,
                }
            }
        }
    }

    pub fn max(&self) -> f32 {
        self.cost.iter().copied().fold(0.0, f32::max)
    }

    /// Bounding box of every tile at or above `threshold`, clipped to the screen.
    pub fn dirty_bounds(&self, threshold: f32) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        for (i, &v) in self.cost.iter().enumerate() {
            if v < threshold {
                continue;
            }
            let (c, r) = ((i % self.cols) as i32, (i / self.cols) as i32);
            let x = self.screen.left() + c * self.tile;
            let y = self.screen.top() + r * self.tile;
            let t = Rect::new(x, y, x + self.tile, y + self.tile).and_then(|t| t.intersect(&self.screen));
            if let Some(t) = t {
                out = Some(out.map_or(t, |o| o.union(&t)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_per_tile_and_resets() {
        let screen = Rect::new(0, 0, 100, 100).unwrap();
        let mut l = GhostLedger::new(screen, 32);
        let r = Rect::new(10, 10, 40, 20).unwrap(); // tiles (0,0) and (1,0)
        for _ in 0..3 {
            l.apply(&r, GhostEffect::Add(1.0));
        }
        assert_eq!(l.max(), 3.0);
        assert_eq!(l.dirty_bounds(2.0), Rect::new(0, 0, 64, 32));
        l.apply(&screen, GhostEffect::Reset);
        assert_eq!(l.max(), 0.0);
        assert_eq!(l.dirty_bounds(0.5), None);
    }

    #[test]
    fn edge_tiles_clip_to_screen() {
        let screen = Rect::new(0, 0, 100, 100).unwrap();
        let mut l = GhostLedger::new(screen, 32); // 4x4, last tile 96..100
        l.apply(&Rect::new(97, 97, 120, 120).unwrap(), GhostEffect::Add(1.0));
        assert_eq!(l.dirty_bounds(1.0), Rect::new(96, 96, 100, 100));
    }
}
