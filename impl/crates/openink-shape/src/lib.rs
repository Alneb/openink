//! Open shape recogniser and beautifier, replacing MyScript Shape in knote2 (docs/SHAPE-ENGINE.md).
//!
//! Same pipeline shape as MyScript (docs/SHAPE-RE.md), with our own features, fits and trained
//! weights: a whole-ink classifier picks the family, a per-point segmenter splits the ink into
//! straight runs, curves and corners, geometric fits build each candidate, and the most specific
//! class that fits within tolerance wins. Pure computation, no Android types.

pub mod features;
pub mod fit;
pub mod geom;
pub mod model;
pub mod nn;
pub mod recognize;
pub mod segment;
pub mod synth;
pub mod wire;
