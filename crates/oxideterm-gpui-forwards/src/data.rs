//! Non-secret values exchanged with the forwarding host. Native callers already
//! use these data types, so no conversion or copy is needed at that boundary.

#[cfg(not(target_family = "wasm"))]
pub use oxideterm_forwarding::{
    DetectedPort, ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate,
    PortDetectionSnapshot,
};
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_ssh::NodeId;

#[cfg(target_family = "wasm")]
mod portable;
#[cfg(target_family = "wasm")]
pub use portable::*;
