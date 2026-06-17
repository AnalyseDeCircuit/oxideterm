// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Protocol-level building blocks for SSH X11 forwarding.
//!
//! This crate intentionally stops before SSH channel wiring. It owns DISPLAY
//! parsing, X11 forwarding request metadata, fake-cookie auth material, and X11
//! setup-packet auth rewriting so the future SSH runtime can stay thin.

mod allocation;
mod auth;
mod authority;
mod config;
mod display;
mod endpoint;
mod error;
mod local_auth;
mod plan;
mod policy;
mod protocol;
mod registry;
mod remote_xauth;
mod runtime;
mod runtime_strategy;
mod setup_buffer;
mod xauth;
mod xauthority;

pub use allocation::X11RemoteDisplayAllocator;
pub use auth::{X11AuthCookie, X11AuthMaterial, X11AuthProtocol};
pub use authority::X11AuthorityEnvironment;
pub use config::{X11ForwardConfig, X11SshRequest};
pub use display::{X11Display, X11DisplayTransport};
pub use endpoint::X11LocalEndpoint;
pub use error::{X11ForwardingError, X11Result};
pub use local_auth::X11LocalAuthorityResolver;
pub use plan::X11ForwardPlan;
pub use policy::{X11AuthFallbackMode, X11ForwardPolicy, X11ForwardTrust};
pub use protocol::{
    X11AuthFailureResponse, X11ByteOrder, X11SetupAuthentication, X11SetupRequest,
    build_auth_failure_response, inspect_setup_authentication, inspect_setup_request,
    required_setup_packet_len, rewrite_setup_authentication,
};
pub use registry::{X11AuthSpoofRegistry, X11SpoofedAuth};
pub use remote_xauth::X11RemoteXauthUpdate;
pub use runtime::{
    BoxedX11Stream, X11AsyncStream, X11RuntimeError, bridge_x11_stream_to_endpoint,
    connect_local_x11_endpoint,
};
pub use runtime_strategy::{X11RuntimePlatform, X11RuntimeStrategy, X11RuntimeSupport};
pub use setup_buffer::{
    X11RegisteredSetupDecision, X11RegisteredSetupRewrite, X11SetupBuffer, X11SetupBufferState,
    X11SetupDecision, X11SetupReject, X11SetupRejectReason, X11SetupRewrite,
};
pub use xauth::{
    X11AuthCommand, X11AuthEntry, X11AuthorityFile, parse_xauth_list, parse_xauth_nlist,
    select_xauth_entry,
};
pub use xauthority::{
    X11AuthorityFamily, X11AuthorityMatchContext, X11BinaryAuthorityEntry, parse_xauthority_file,
    select_xauthority_entry,
};
