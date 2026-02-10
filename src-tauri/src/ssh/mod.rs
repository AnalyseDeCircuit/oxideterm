//! SSH module - handles SSH connections and sessions
//!
//! This module provides the core SSH functionality using russh library.
//!
//! # Features
//! - Direct SSH connections
//! - ProxyJump (jump host) support for HPC environments
//! - Port forwarding (local, remote, dynamic)
//! - SSH config file parsing
//! - Host key verification via ~/.ssh/known_hosts
//! - Connection pool with idle timeout (see `connection_registry`)
//! - Keyboard-Interactive authentication (2FA) support

mod agent;
mod client;
mod config;
pub mod connection_registry;
mod error;
mod handle_owner;
pub mod keyboard_interactive;
pub mod known_hosts;
pub mod preflight;
mod proxy;
mod session;

pub use agent::{is_agent_available, SshAgentClient};
pub use client::{ClientHandler, SshClient};
pub use config::{AuthMethod, ProxyHopConfig, SshConfig};
pub use connection_registry::{
    ConnectionEntry, ConnectionInfo, ConnectionPoolConfig, ConnectionPoolStats,
    ConnectionRegistryError, ConnectionState, SshConnectionRegistry,
};
pub use error::SshError;
pub use handle_owner::{spawn_handle_owner_task, HandleCommand, HandleController};
pub use keyboard_interactive::{
    KbiCancelRequest, KbiError, KbiPrompt, KbiPromptEvent, KbiRespondRequest, KbiResultEvent,
    EVENT_KBI_PROMPT, EVENT_KBI_RESULT,
};
pub use known_hosts::{get_known_hosts, HostKeyVerification, KnownHostsStore};
pub use preflight::{
    accept_host_key, check_host_key, get_host_key_cache, HostKeyCache, HostKeyStatus,
};
pub use proxy::{connect_via_proxy, connect_via_single_hop, ProxyChain, ProxyConnection, ProxyHop};
pub use session::{ExtendedSessionHandle, SessionCommand, SessionHandle, SshSession};
