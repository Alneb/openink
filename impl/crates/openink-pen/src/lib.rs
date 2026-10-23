//! Stylus input, replacing `libonyx_pen_touch_reader.so` (docs/ONYX-EPD-PEN.md, "Pen").
//!
//! Like the stock reader it reads evdev directly, bypassing InputFlinger for latency.
//! [`decode`] is pure (host-tested); [`device`] is the Linux/Android I/O around it.

pub mod decode;
pub mod region;
pub mod trace;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod device;

pub use decode::{AbsRange, PenAction, PenDecoder, PenEvent, RawEvent, Snapshot, Tool};
pub use region::{Rect, RegionFilter};
