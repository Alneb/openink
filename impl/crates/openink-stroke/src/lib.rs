//! Stroke rendering geometry, replacing `libneo_pen.so` (docs/STROKE.md).
//!
//! The stock library's internals are hidden, so this is a new design that keeps its contract:
//! the same pen set and defaults (from knote2's `NeoPenConfig` and `PenConstant`), incremental
//! down/move/up calls, and the same two output shapes: filled outlines, or textured dabs of
//! `(x, y, size, angle, alpha)`. Pure computation: host-tested, no Android types.

pub mod brush;
pub mod filter;
pub mod geom;
pub mod marker;
pub mod pen;
pub mod raster;
pub mod sf;
pub mod stroke;

pub use pen::{Output, PenConfig, PenKind};
pub use stroke::{ActiveStroke, Ink, Sample, Stroke};
