//! Portable core of the open e-ink + stylus stack (docs/OPEN-STACK.md). No OS dependencies:
//! everything here is checked on the host against `impl/spec/sf_protocol.json`.

pub mod mode;
pub mod pen;
mod sf_codes;

pub use mode::{ModeError, ModeFlags, UpdateMode, Waveform};
pub use pen::{PenCommand, PenCtrlState, RegionMode, StrokeStyle};
pub use sf_codes::Code;

/// Binder interface token every Onyx SF transaction starts with.
pub const INTERFACE_TOKEN: &str = "android.ui.ISurfaceComposer";
/// Service name the framework looks up.
pub const SF_SERVICE: &str = "SurfaceFlinger";
