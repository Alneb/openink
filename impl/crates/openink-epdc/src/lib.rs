//! The open e-ink refresh engine (docs/EPDC-ENGINE.md): what Onyx's SurfaceFlinger layer
//! (EpdcManager, the update-entry policy, the schemas) decides, redesigned and host-testable.

pub mod engine;
pub mod geom;
pub mod intent;
pub mod ledger;

pub use engine::{Engine, Policy, Reason, Update};
pub use geom::{Millis, Rect};
pub use intent::Intent;
