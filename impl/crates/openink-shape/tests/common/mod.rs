//! Atlas's stroke generator, shared by the Atlas regression tests (same as `shape_openink.rs` in Atlas).

/// The wobble Atlas adds to x (index i) and y (index i + 3).
pub fn wobble(i: usize) -> f32 {
    let i = i as f32;
    ((i * 1.7).sin() + (i * 0.37).cos()) * 0.9
}
