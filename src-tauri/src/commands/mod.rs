//! Tauri Commands module
//!
//! This module contains all Tauri commands exposed to the frontend.

pub mod ai_chat;
pub mod archive;
pub mod config;
mod connect_v2;
pub mod forwarding;
pub mod health;
pub mod ide;
pub mod kbi;
#[cfg(feature = "local-terminal")]
pub mod local;
pub mod network;

// WSL Graphics commands — the actual module lives at src/graphics/
// We re-export from there. On non-Windows platforms, stub commands return Err.
pub mod node_forwarding;
pub mod node_sftp;
pub mod oxide_export;
pub mod oxide_import;
pub mod plugin;
pub mod plugin_registry;
pub mod plugin_server;
pub mod scroll;
pub mod session_tree;
pub mod sftp;
pub mod ssh;

pub use ai_chat::*;
pub use archive::*;
pub use connect_v2::*;
pub use forwarding::*;
pub use health::*;
pub use ide::*;
pub use kbi::*;
#[cfg(feature = "local-terminal")]
pub use local::*;
pub use network::*;
pub use node_forwarding::*;
pub use node_sftp::*;
pub use plugin::*;
pub use plugin_registry::*;
pub use plugin_server::*;
pub use scroll::*;
pub use session_tree::*;
pub use sftp::*;
pub use ssh::*;
