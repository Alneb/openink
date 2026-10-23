//! Onyx's private SurfaceFlinger requests as typed data (docs/OPEN-STACK.md, L1/L3).
//! Pure and host-testable; `tests/spec.rs` checks every request against impl/spec/sf_protocol.json.

pub mod request;

pub use request::{Arg, Pid, Reply, Request, ScreenRect};
