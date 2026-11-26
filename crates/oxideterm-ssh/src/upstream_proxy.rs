// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use crate::SshTransportError;
use oxideterm_network_proxy::tcp;
use socket2::{SockRef, TcpKeepalive};
pub use tcp::{UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyError, UpstreamProxyProtocol};

const SSH_TCP_KEEPALIVE_IDLE: Duration = Duration::from_secs(30);
#[cfg(any(
    target_os = "freebsd",
    target_os = "ios",
    target_os = "linux",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "windows",
))]
const SSH_TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

fn configure_ssh_tcp_keepalive(stream: &tokio::net::TcpStream) -> std::io::Result<()> {
    // Enable OS-level TCP keepalive on the SSH transport socket so NAT boxes
    // and firewalls observe periodic liveness packets even when the session is
    // quiet, and stalled sockets fail sooner than multi-hour OS defaults.
    let keepalive = TcpKeepalive::new().with_time(SSH_TCP_KEEPALIVE_IDLE);
    #[cfg(any(
        target_os = "freebsd",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "windows",
    ))]
    let keepalive = keepalive.with_interval(SSH_TCP_KEEPALIVE_INTERVAL);
    SockRef::from(stream).set_tcp_keepalive(&keepalive)
}

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
    let stream = tcp::dial_initial_tcp(host, port, timeout_secs, proxy)
        .await
        .map_err(ssh_error)?;
    if let Err(error) = configure_ssh_tcp_keepalive(&stream) {
        tracing::debug!(
            host,
            port,
            %error,
            "failed to configure OS TCP keepalive on SSH socket"
        );
    }
    Ok(stream)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dial_initial_tcp_enables_os_tcp_keepalive() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accept = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let stream = dial_initial_tcp("127.0.0.1", port, 5, None).await.unwrap();
        assert!(SockRef::from(&stream).keepalive().unwrap());

        accept.abort();
    }
}
