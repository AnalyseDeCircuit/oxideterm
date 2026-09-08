// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::SshTransportError;
use oxideterm_network_proxy::tcp;
pub use tcp::{UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyError, UpstreamProxyProtocol};

fn ssh_error(error: tcp::TcpProxyError) -> SshTransportError {
    match error {
        tcp::TcpProxyError::ConnectionFailed(message) => {
            SshTransportError::ConnectionFailed(message)
        }
        tcp::TcpProxyError::DnsResolution { address, message } => {
            SshTransportError::DnsResolution { address, message }
        }
        tcp::TcpProxyError::Timeout => SshTransportError::Timeout,
    }
}

pub async fn dial_initial_tcp(
    host: &str,
    port: u16,
    timeout_secs: u64,
    proxy: Option<&UpstreamProxyConfig>,
) -> Result<tokio::net::TcpStream, SshTransportError> {
    tcp::dial_initial_tcp(host, port, timeout_secs, proxy)
        .await
        .map_err(ssh_error)
}
pub async fn probe_upstream_proxy_route(
    host: &str,
    port: u16,
    timeout_secs: u64,
    proxy: &UpstreamProxyConfig,
) -> Result<(), SshTransportError> {
    tcp::probe_upstream_proxy_route(host, port, timeout_secs, proxy)
        .await
        .map_err(ssh_error)
}
pub fn socks5_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
    tcp::socks5_proxy_from_env().map_err(ssh_error)
}
pub fn upstream_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
    tcp::upstream_proxy_from_env().map_err(ssh_error)
}
pub fn parse_socks5_proxy_value(value: &str) -> Result<UpstreamProxyConfig, SshTransportError> {
    tcp::parse_socks5_proxy_value(value).map_err(ssh_error)
}
pub fn parse_http_proxy_value(value: &str) -> Result<UpstreamProxyConfig, SshTransportError> {
    tcp::parse_http_proxy_value(value).map_err(ssh_error)
}
