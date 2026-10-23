//! Screen geometry and time, as newtypes so pixels, tiles and milliseconds cannot be mixed up.

/// Monotonic milliseconds (the caller's clock, e.g. `SystemClock.uptimeMillis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Millis(pub u64);

impl Millis {
    pub const fn plus(self, ms: u64) -> Self {
        Self(self.0.saturating_add(ms))
    }
    /// Time since `earlier`, 0 if the clock is behind it.
    pub const fn since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// A screen rectangle in pixels, half-open: `left <= x < right`. Never empty once constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    /// `None` for an empty or inverted rectangle: an empty refresh is a caller bug, not a no-op.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Option<Self> {
        if left < right && top < bottom {
            Some(Self { left, top, right, bottom })
        } else {
            None
        }
    }

    pub const fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
        Self::new(x, y, x.saturating_add(w), y.saturating_add(h))
    }

    pub const fn left(&self) -> i32 {
        self.left
    }
    pub const fn top(&self) -> i32 {
        self.top
    }
    pub const fn right(&self) -> i32 {
        self.right
    }
    pub const fn bottom(&self) -> i32 {
        self.bottom
    }
    pub const fn width(&self) -> i32 {
        self.right - self.left
    }
    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub const fn area(&self) -> i64 {
        self.width() as i64 * self.height() as i64
    }

    pub fn union(&self, o: &Self) -> Self {
        Self { left: self.left.min(o.left), top: self.top.min(o.top), right: self.right.max(o.right), bottom: self.bottom.max(o.bottom) }
    }

    pub fn intersect(&self, o: &Self) -> Option<Self> {
        Self::new(self.left.max(o.left), self.top.max(o.top), self.right.min(o.right), self.bottom.min(o.bottom))
    }

    pub fn contains(&self, o: &Self) -> bool {
        self.left <= o.left && self.top <= o.top && self.right >= o.right && self.bottom >= o.bottom
    }

    /// Overlapping, or closer than `gap` pixels on both axes.
    pub fn near(&self, o: &Self, gap: i32) -> bool {
        self.left - gap < o.right && o.left - gap < self.right && self.top - gap < o.bottom && o.top - gap < self.bottom
    }

    /// Grown by `px` on every side (for ink: half the stroke width).
    pub fn inflate(&self, px: i32) -> Self {
        Self { left: self.left - px, top: self.top - px, right: self.right + px, bottom: self.bottom + px }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rects_are_rejected() {
        assert!(Rect::new(0, 0, 0, 10).is_none());
        assert!(Rect::new(5, 0, 4, 10).is_none());
        assert!(Rect::from_xywh(0, 0, 1, 1).is_some());
    }

    #[test]
    fn near_and_intersect() {
        let a = Rect::new(0, 0, 10, 10).unwrap();
        let b = Rect::new(14, 0, 20, 10).unwrap();
        assert!(!a.near(&b, 4) && a.near(&b, 5));
        assert!(a.intersect(&b).is_none());
        assert_eq!(a.union(&b), Rect::new(0, 0, 20, 10).unwrap());
    }
}
