//! Port Forwarding Module
//!
//! Provides local, remote, and dynamic port forwarding for SSH connections.
//! Designed for HPC/supercomputing workflows (Jupyter, TensorBoard, etc.)

mod dynamic;
mod events;
mod local;
pub mod manager;
pub mod remote;

pub use dynamic::{start_dynamic_forward, DynamicForward, DynamicForwardHandle};
pub use events::{ForwardEvent, ForwardEventEmitter};
pub use local::{start_local_forward, LocalForward, LocalForwardHandle};
pub use manager::{
    ForwardRule, ForwardRuleUpdate, ForwardStats, ForwardStatus, ForwardType, ForwardingManager,
};
pub use remote::{start_remote_forward, RemoteForward, RemoteForwardHandle, RemoteForwardRegistry};
