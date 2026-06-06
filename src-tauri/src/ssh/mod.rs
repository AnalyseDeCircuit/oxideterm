// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
mod agent_forward;
mod auth;
mod client;
mod config;
pub mod connection_registry;
mod error;
mod handle_owner;
pub mod keyboard_interactive;
pub mod known_hosts;
#[cfg(all(test, unix))]
mod openssh_auth_integration_tests;
pub mod preflight;
mod proxy;
#[cfg(test)]
mod proxy_integration_tests;
mod session;
mod upstream_proxy;

pub use agent::{SshAgentClient, is_agent_available};
pub(crate) use auth::ManagedKeyResolver;
pub use client::{ClientHandler, SshClient};
pub use config::{AuthMethod, ProxyHopConfig, SshConfig};
pub use connection_registry::{
    AcquiredSftpMeta, ConnectionEntry, ConnectionInfo, ConnectionPoolConfig, ConnectionPoolStats,
    ConnectionRegistryError, ConnectionState, ConnectionSummary, RemoteEnvDetectionReason,
    SshConnectionRegistry,
};
pub use error::SshError;
pub use handle_owner::{HandleCommand, HandleController, spawn_handle_owner_task};
pub use keyboard_interactive::{
    EVENT_KBI_PROMPT, EVENT_KBI_RESULT, KbiCancelRequest, KbiError, KbiPrompt, KbiPromptEvent,
    KbiRespondRequest, KbiResultEvent,
};
pub use known_hosts::{HostKeyVerification, KnownHostsStore, get_known_hosts};
pub use preflight::{
    HostKeyCache, HostKeyStatus, accept_host_key, check_host_key,
    check_host_key_with_upstream_proxy, get_host_key_cache,
};
pub use proxy::{
    ProxyChain, ProxyConnectEndpoint, ProxyConnectEndpointKind, ProxyConnectError,
    ProxyConnectOperation, ProxyConnection, ProxyHop, connect_via_proxy,
    connect_via_proxy_for_test, connect_via_proxy_for_test_with_managed_key_resolver,
    connect_via_single_hop,
};
pub use session::{
    DEFAULT_PTY_MODES, ExtendedSessionHandle, SessionCommand, SessionHandle, SshSession,
};
pub use upstream_proxy::{
    UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyError, UpstreamProxyProtocol,
    dial_initial_tcp, parse_http_proxy_value, parse_socks5_proxy_value, socks5_proxy_from_env,
    upstream_proxy_from_env,
};
