// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    env, fmt,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum TcpProxyError {
    #[error("{0}")]
    ConnectionFailed(String),
    #[error("DNS resolution failed for {address}: {message}")]
    DnsResolution { address: String, message: String },
    #[error("TCP connection timed out")]
    Timeout,
}

mod socks5;
use socks5::{negotiate_socks5_auth, send_socks5_connect};

const SOCKS_VERSION: u8 = 0x05;
const SOCKS_METHOD_NO_AUTH: u8 = 0x00;
const SOCKS_METHOD_PASSWORD: u8 = 0x02;
const SOCKS_METHOD_NO_ACCEPTABLE: u8 = 0xff;
const SOCKS_COMMAND_CONNECT: u8 = 0x01;
const SOCKS_ATYP_IPV4: u8 = 0x01;
const SOCKS_ATYP_DOMAIN: u8 = 0x03;
const SOCKS_ATYP_IPV6: u8 = 0x04;
const SOCKS_AUTH_VERSION: u8 = 0x01;
const HTTP_CONNECT_MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpstreamProxyProtocol {
    Socks5,
    HttpConnect,
}

#[derive(Clone, PartialEq, Eq)]
pub struct UpstreamProxyConfig {
    pub protocol: UpstreamProxyProtocol,
    pub host: String,
    pub port: u16,
    pub auth: UpstreamProxyAuth,
    pub remote_dns: bool,
    pub no_proxy: String,
}

impl fmt::Debug for UpstreamProxyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamProxyConfig")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("auth", &self.auth)
            .field("remote_dns", &self.remote_dns)
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum UpstreamProxyAuth {
    None,
    Password {
        username: String,
        password: Zeroizing<String>,
    },
}

impl fmt::Debug for UpstreamProxyAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::Password { username, .. } => formatter
                .debug_struct("Password")
                .field("username", username)
                .field("password", &"[redacted secret]")
                .finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpstreamProxyError {
    HttpIo(String),
    HttpInvalidResponse,
    HttpHeaderTooLarge,
    HttpConnectRejected(u16),
    HttpInvalidProxyValue(&'static str),
    SocksIo(String),
    SocksInvalidGreeting,
    SocksRejectedAuthMethods,
    SocksUnsupportedAuthMethod(u8),
    SocksMissingCredentials,
    SocksCredentialsTooLong,
    SocksInvalidAuthReply,
    SocksAuthFailed,
    SocksTargetHostTooLong,
    SocksInvalidReply,
    SocksReplyCode(u8),
    SocksUnknownAddressType,
    SocksInvalidProxyValue(&'static str),
}

impl fmt::Display for UpstreamProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HttpIo(error) => {
                write!(formatter, "HTTP CONNECT upstream proxy I/O failed: {error}")
            }
            Self::HttpInvalidResponse => {
                formatter.write_str("HTTP CONNECT proxy returned an invalid response")
            }
            Self::HttpHeaderTooLarge => {
                formatter.write_str("HTTP CONNECT proxy response header exceeded the size limit")
            }
            Self::HttpConnectRejected(status) => {
                write!(
                    formatter,
                    "HTTP CONNECT proxy rejected the tunnel with status {status}"
                )
            }
            Self::HttpInvalidProxyValue(message) => formatter.write_str(message),
            Self::SocksIo(error) => {
                write!(formatter, "SOCKS5 upstream proxy I/O failed: {error}")
            }
            Self::SocksInvalidGreeting => {
                formatter.write_str("SOCKS5 proxy returned an invalid greeting")
            }
            Self::SocksRejectedAuthMethods => {
                formatter.write_str("SOCKS5 proxy rejected all auth methods")
            }
            Self::SocksUnsupportedAuthMethod(method) => write!(
                formatter,
                "SOCKS5 proxy selected unsupported auth method 0x{method:02x}"
            ),
            Self::SocksMissingCredentials => formatter
                .write_str("SOCKS5 proxy requested username/password auth without credentials"),
            Self::SocksCredentialsTooLong => {
                formatter.write_str("SOCKS5 username/password credentials exceed 255 bytes")
            }
            Self::SocksInvalidAuthReply => {
                formatter.write_str("SOCKS5 proxy returned an invalid auth reply")
            }
            Self::SocksAuthFailed => {
                formatter.write_str("SOCKS5 username/password authentication failed")
            }
            Self::SocksTargetHostTooLong => {
                formatter.write_str("SOCKS5 target hostname exceeds 255 bytes")
            }
            Self::SocksInvalidReply => {
                formatter.write_str("SOCKS5 proxy returned an invalid reply")
            }
            Self::SocksReplyCode(code) => {
                write!(
                    formatter,
                    "SOCKS5 proxy connect failed with reply code 0x{code:02x}"
                )
            }
            Self::SocksUnknownAddressType => {
                formatter.write_str("SOCKS5 proxy returned an unknown address type")
            }
            Self::SocksInvalidProxyValue(message) => formatter.write_str(message),
        }
    }
}

pub async fn dial_initial_tcp(
    target_host: &str,
    target_port: u16,
    timeout_secs: u64,
    upstream_proxy: Option<&UpstreamProxyConfig>,
) -> Result<TcpStream, TcpProxyError> {
    let timeout = Duration::from_secs(timeout_secs);
    let stream = match upstream_proxy {
        Some(proxy) => tokio::time::timeout(
            timeout,
            dial_via_upstream_proxy_or_direct(target_host, target_port, proxy),
        )
        .await
        .map_err(|_| TcpProxyError::Timeout)?,
        None => tokio::time::timeout(timeout, dial_direct_tcp(target_host, target_port))
            .await
            .map_err(|_| TcpProxyError::Timeout)?,
    }?;

    // Interactive protocols require latency-sensitive control packets to leave promptly.
    stream.set_nodelay(true).map_err(|error| {
        TcpProxyError::ConnectionFailed(format!("failed to enable TCP_NODELAY: {error}"))
    })?;
    Ok(stream)
}

pub async fn probe_upstream_proxy_route(
    target_host: &str,
    target_port: u16,
    timeout_secs: u64,
    upstream_proxy: &UpstreamProxyConfig,
) -> Result<(), TcpProxyError> {
    // A route probe must always open a fresh tunnel. Host-key caches belong to
    // SSH identity verification and cannot prove that the proxy is reachable.
    let stream =
        dial_initial_tcp(target_host, target_port, timeout_secs, Some(upstream_proxy)).await?;
    drop(stream);
    Ok(())
}

pub fn socks5_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, TcpProxyError> {
    upstream_proxy_from_env_values(
        env::var("OXIDETERM_SOCKS5_PROXY").ok().as_deref(),
        None,
        env::var("OXIDETERM_NO_PROXY").ok().as_deref(),
    )
}

pub fn upstream_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, TcpProxyError> {
    upstream_proxy_from_env_values(
        env::var("OXIDETERM_SOCKS5_PROXY").ok().as_deref(),
        env::var("OXIDETERM_HTTP_PROXY").ok().as_deref(),
        env::var("OXIDETERM_NO_PROXY").ok().as_deref(),
    )
}

fn upstream_proxy_from_env_values(
    socks5_value: Option<&str>,
    http_value: Option<&str>,
    no_proxy: Option<&str>,
) -> Result<Option<UpstreamProxyConfig>, TcpProxyError> {
    let mut proxy = match first_non_empty(socks5_value) {
        Some(value) => parse_socks5_proxy_value(value)?,
        None => match first_non_empty(http_value) {
            Some(value) => parse_http_proxy_value(value)?,
            None => return Ok(None),
        },
    };
    proxy.no_proxy = no_proxy.unwrap_or_default().to_string();
    Ok(Some(proxy))
}

pub fn parse_socks5_proxy_value(value: &str) -> Result<UpstreamProxyConfig, TcpProxyError> {
    let trimmed = value.trim();
    let (remote_dns, authority) = if let Some(rest) = trimmed.strip_prefix("socks5h://") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("socks5://") {
        (false, rest)
    } else {
        // OxideTerm's saved proxy default is proxy-side DNS; keep bare env
        // values aligned with that app default.
        (true, trimmed)
    };

    let authority = trim_url_tail(authority);
    let (auth, host_port) = parse_proxy_authority_auth(authority);
    let (host, port) = split_host_port(host_port)?;

    Ok(UpstreamProxyConfig {
        protocol: UpstreamProxyProtocol::Socks5,
        host,
        port,
        auth,
        remote_dns,
        no_proxy: String::new(),
    })
}

pub fn parse_http_proxy_value(value: &str) -> Result<UpstreamProxyConfig, TcpProxyError> {
    let trimmed = value.trim();
    if trimmed.starts_with("https://") {
        return proxy_error(UpstreamProxyError::HttpInvalidProxyValue(
            "HTTP CONNECT proxy value must use http://, not https://",
        ));
    }
    let authority = trimmed.strip_prefix("http://").unwrap_or(trimmed);
    let authority = trim_url_tail(authority);
    let (auth, host_port) = parse_proxy_authority_auth(authority);
    let (host, port) = split_host_port_with_protocol(
        host_port,
        "HTTP CONNECT proxy value must include host and port",
        "HTTP CONNECT proxy host is empty",
        "HTTP CONNECT proxy IPv6 host is missing ']'",
        "HTTP CONNECT proxy port is missing",
        "HTTP CONNECT proxy port is invalid",
    )?;

    Ok(UpstreamProxyConfig {
        protocol: UpstreamProxyProtocol::HttpConnect,
        host,
        port,
        auth,
        remote_dns: true,
        no_proxy: String::new(),
    })
}

async fn dial_via_upstream_proxy_or_direct(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, TcpProxyError> {
    if should_bypass_proxy(target_host, proxy.no_proxy.as_str()) {
        return dial_direct_tcp(target_host, target_port).await;
    }
    match proxy.protocol {
        UpstreamProxyProtocol::Socks5 => dial_via_socks5(target_host, target_port, proxy).await,
        UpstreamProxyProtocol::HttpConnect => {
            dial_via_http_connect(target_host, target_port, proxy).await
        }
    }
}

async fn dial_direct_tcp(target_host: &str, target_port: u16) -> Result<TcpStream, TcpProxyError> {
    let socket_addrs = resolve_socket_addrs(target_host, target_port).await?;
    TcpStream::connect(socket_addrs.as_slice())
        .await
        .map_err(|error| TcpProxyError::ConnectionFailed(error.to_string()))
}

async fn dial_via_socks5(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, TcpProxyError> {
    let proxy_addrs = resolve_socket_addrs(&proxy.host, proxy.port).await?;
    let mut stream = TcpStream::connect(proxy_addrs.as_slice())
        .await
        .map_err(|error| TcpProxyError::ConnectionFailed(error.to_string()))?;

    negotiate_socks5_auth(&mut stream, &proxy.auth).await?;
    send_socks5_connect(&mut stream, target_host, target_port, proxy.remote_dns).await?;
    Ok(stream)
}

async fn dial_via_http_connect(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, TcpProxyError> {
    let proxy_addrs = resolve_socket_addrs(&proxy.host, proxy.port).await?;
    let mut stream = TcpStream::connect(proxy_addrs.as_slice())
        .await
        .map_err(|error| TcpProxyError::ConnectionFailed(error.to_string()))?;

    send_http_connect_request(&mut stream, target_host, target_port, &proxy.auth).await?;
    read_http_connect_response(&mut stream).await?;
    Ok(stream)
}

async fn send_http_connect_request(
    stream: &mut TcpStream,
    target_host: &str,
    target_port: u16,
    auth: &UpstreamProxyAuth,
) -> Result<(), TcpProxyError> {
    let authority = http_authority(target_host, target_port);
    let mut request = Zeroizing::new(format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    ));
    if let UpstreamProxyAuth::Password { username, password } = auth {
        let credentials = Zeroizing::new(format!("{username}:{}", password.as_str()));
        let encoded = Zeroizing::new(BASE64_STANDARD.encode(credentials.as_bytes()));
        request.push_str("Proxy-Authorization: Basic ");
        request.push_str(encoded.as_str());
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(http_io_error)
}

async fn read_http_connect_response(stream: &mut TcpStream) -> Result<(), TcpProxyError> {
    let mut response = Vec::new();
    loop {
        if response.len() >= HTTP_CONNECT_MAX_HEADER_BYTES {
            return proxy_error(UpstreamProxyError::HttpHeaderTooLarge);
        }
        let byte = stream.read_u8().await.map_err(http_io_error)?;
        response.push(byte);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let header = std::str::from_utf8(&response)
        .map_err(|_| proxy_transport_error(UpstreamProxyError::HttpInvalidResponse))?;
    let status = parse_http_connect_status(header)?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        proxy_error(UpstreamProxyError::HttpConnectRejected(status))
    }
}

fn parse_http_connect_status(header: &str) -> Result<u16, TcpProxyError> {
    let Some(status) = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
    else {
        return proxy_error(UpstreamProxyError::HttpInvalidResponse);
    };
    Ok(status)
}

fn http_authority(host: &str, port: u16) -> String {
    if host.parse::<Ipv6Addr>().is_ok() {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn parse_proxy_authority_auth(authority: &str) -> (UpstreamProxyAuth, &str) {
    let Some((auth, host_port)) = authority.rsplit_once('@') else {
        return (UpstreamProxyAuth::None, authority);
    };
    let (username, password) = auth.split_once(':').unwrap_or((auth, ""));
    (
        UpstreamProxyAuth::Password {
            username: username.to_string(),
            password: Zeroizing::new(password.to_string()),
        },
        host_port,
    )
}

fn split_host_port(authority: &str) -> Result<(String, u16), TcpProxyError> {
    split_host_port_with_protocol(
        authority,
        "SOCKS5 proxy value must include host and port",
        "SOCKS5 proxy host is empty",
        "SOCKS5 proxy IPv6 host is missing ']'",
        "SOCKS5 proxy port is missing",
        "SOCKS5 proxy port is invalid",
    )
}

fn split_host_port_with_protocol(
    authority: &str,
    missing_host_port: &'static str,
    empty_host: &'static str,
    missing_ipv6_bracket: &'static str,
    missing_port: &'static str,
    invalid_port: &'static str,
) -> Result<(String, u16), TcpProxyError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((host, suffix)) = rest.split_once(']') else {
            return proxy_error(invalid_proxy_value(missing_ipv6_bracket));
        };
        let Some(port) = suffix.strip_prefix(':') else {
            return proxy_error(invalid_proxy_value(missing_port));
        };
        return Ok((host.to_string(), parse_port(port, invalid_port)?));
    }

    let Some((host, port)) = authority.rsplit_once(':') else {
        return proxy_error(invalid_proxy_value(missing_host_port));
    };
    if host.is_empty() {
        return proxy_error(invalid_proxy_value(empty_host));
    }
    Ok((host.to_string(), parse_port(port, invalid_port)?))
}

fn parse_port(port: &str, invalid_port: &'static str) -> Result<u16, TcpProxyError> {
    port.parse::<u16>()
        .map_err(|_| proxy_transport_error(invalid_proxy_value(invalid_port)))
}

fn invalid_proxy_value(message: &'static str) -> UpstreamProxyError {
    if message.starts_with("HTTP CONNECT") {
        UpstreamProxyError::HttpInvalidProxyValue(message)
    } else {
        UpstreamProxyError::SocksInvalidProxyValue(message)
    }
}

fn first_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn trim_url_tail(authority: &str) -> &str {
    authority
        .split_once(['/', '?', '#'])
        .map_or(authority, |(head, _)| head)
}

fn should_bypass_proxy(target_host: &str, no_proxy: &str) -> bool {
    let target = target_host.trim().trim_matches(['[', ']']);
    if target.is_empty() {
        return false;
    }
    let target_ip = target.parse::<IpAddr>().ok();
    let target_lower = target.to_ascii_lowercase();

    no_proxy.split(',').any(|raw_rule| {
        let rule = raw_rule.trim().trim_matches(['[', ']']);
        if rule.is_empty() {
            return false;
        }
        if rule == "*" {
            return true;
        }
        if let Some((network, prefix)) = parse_cidr_rule(rule) {
            // CIDR no_proxy rules only match literal IP targets. Do not resolve
            // hostnames locally here, because socks5h/remote DNS must preserve
            // proxy-side name resolution.
            return target_ip.is_some_and(|ip| ip_matches_cidr(ip, network, prefix));
        }
        if let Ok(rule_ip) = rule.parse::<IpAddr>() {
            return target_ip == Some(rule_ip);
        }
        let rule_lower = rule.to_ascii_lowercase();
        if let Some(suffix) = rule_lower.strip_prefix("*.") {
            return target_lower
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'));
        }
        target_lower == rule_lower
    })
}

fn parse_cidr_rule(rule: &str) -> Option<(IpAddr, u8)> {
    let (network, prefix) = rule.split_once('/')?;
    let network = network.parse::<IpAddr>().ok()?;
    let prefix = prefix.parse::<u8>().ok()?;
    match network {
        IpAddr::V4(_) if prefix <= 32 => Some((network, prefix)),
        IpAddr::V6(_) if prefix <= 128 => Some((network, prefix)),
        _ => None,
    }
}

fn ip_matches_cidr(ip: IpAddr, network: IpAddr, prefix: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let mask = cidr_mask(prefix, 32) as u32;
            (u32::from(ip) & mask) == (u32::from(network) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let mask = cidr_mask(prefix, 128);
            (u128::from(ip) & mask) == (u128::from(network) & mask)
        }
        _ => false,
    }
}

fn cidr_mask(prefix: u8, bits: u8) -> u128 {
    if prefix == 0 {
        0
    } else {
        (!0_u128) << (bits - prefix)
    }
}

async fn resolve_socket_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, TcpProxyError> {
    let addresses: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| TcpProxyError::DnsResolution {
            address: format!("{host}:{port}"),
            message: error.to_string(),
        })?
        .collect();
    if addresses.is_empty() {
        return Err(TcpProxyError::DnsResolution {
            address: format!("{host}:{port}"),
            message: "no address found".into(),
        });
    }
    Ok(addresses)
}

fn http_io_error(error: std::io::Error) -> TcpProxyError {
    proxy_transport_error(UpstreamProxyError::HttpIo(error.to_string()))
}

fn socks_io_error(error: std::io::Error) -> TcpProxyError {
    proxy_transport_error(UpstreamProxyError::SocksIo(error.to_string()))
}

fn proxy_error<T>(error: UpstreamProxyError) -> Result<T, TcpProxyError> {
    Err(proxy_transport_error(error))
}

fn proxy_transport_error(error: UpstreamProxyError) -> TcpProxyError {
    TcpProxyError::ConnectionFailed(error.to_string())
}

#[cfg(test)]
mod tests;
