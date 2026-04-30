// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri Commands module
//!
//! This module contains all Tauri commands exposed to the frontend.

pub mod agent_history;
pub mod ai_chat;
pub mod ai_http;
pub mod appearance;
pub mod archive;
pub mod cli;
pub mod config;
mod connect_v2;
pub mod forwarding;
pub mod health;
pub mod ide;
pub mod kbi;
#[cfg(feature = "local-terminal")]
pub mod local;
pub mod mcp;
pub mod network;

// WSL Graphics commands — the actual module lives at src/graphics/
// We re-export from there. On non-Windows platforms, stub commands return Err.
pub mod node_agent;
pub mod node_forwarding;
pub mod node_sftp;
pub mod oxide_export;
pub mod oxide_import;
pub mod plugin;
pub mod plugin_registry;
pub mod plugin_server;
pub mod portable;
pub mod quick_commands;
pub mod rag;
pub mod scroll;
pub mod session_tree;
pub mod sftp;
pub mod ssh;
pub mod startup;
pub mod trzsz;

pub use agent_history::*;
pub use ai_chat::*;
pub use ai_http::*;
pub use appearance::*;
pub use archive::*;
pub use cli::*;
pub use connect_v2::*;
pub use forwarding::*;
pub use health::*;
pub use ide::*;
pub use kbi::*;
#[cfg(feature = "local-terminal")]
pub use local::*;
pub use mcp::*;
pub use network::*;
pub use node_agent::*;
pub use node_forwarding::*;
pub use node_sftp::*;
pub use plugin::*;
pub use plugin_registry::*;
pub use plugin_server::*;
pub use portable::*;
pub use quick_commands::*;
pub use rag::*;
pub use scroll::*;
pub use session_tree::*;
pub use sftp::*;
pub use ssh::*;
pub use startup::*;
pub use trzsz::*;
