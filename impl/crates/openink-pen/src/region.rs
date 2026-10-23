//! Limit / exclude regions (`setScreenHandWritingRegionLimit/Exclude`): ink is accepted inside
//! any limit rect (or anywhere, if there are none) and outside every exclude rect.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// Parse Onyx's flat `int[]{l, t, r, b, l, t, r, b, ...}`; a trailing partial rect is an error.
    pub fn from_flat(a: &[i32]) -> Option<Vec<Rect>> {
        if a.len() % 4 != 0 {
            return None;
        }
        Some(a.chunks_exact(4).map(|c| Rect { left: c[0], top: c[1], right: c[2], bottom: c[3] }).collect())
    }
}

#[derive(Clone, Debug, Default)]
pub struct RegionFilter {
    pub limit: Vec<Rect>,
    pub exclude: Vec<Rect>,
}

impl RegionFilter {
    pub fn accepts(&self, x: i32, y: i32) -> bool {
        (self.limit.is_empty() || self.limit.iter().any(|r| r.contains(x, y)))
            && !self.exclude.iter().any(|r| r.contains(x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_and_exclude() {
        let f = RegionFilter {
            limit: Rect::from_flat(&[0, 0, 100, 100]).unwrap(),
            exclude: Rect::from_flat(&[40, 40, 60, 60]).unwrap(),
        };
        assert!(f.accepts(10, 10));
        assert!(!f.accepts(50, 50));
        assert!(!f.accepts(150, 10));
        assert!(RegionFilter::default().accepts(-5, 9999));
        assert_eq!(Rect::from_flat(&[1, 2, 3]), None);
    }
}
