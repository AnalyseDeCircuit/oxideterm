// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    env, fmt,
    net::{IpAddr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use zeroize::Zeroizing;

use crate::SshTransportError;

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
) -> Result<TcpStream, SshTransportError> {
    let timeout = Duration::from_secs(timeout_secs);
    match upstream_proxy {
        Some(proxy) => tokio::time::timeout(
            timeout,
            dial_via_upstream_proxy_or_direct(target_host, target_port, proxy),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?,
        None => tokio::time::timeout(timeout, dial_direct_tcp(target_host, target_port))
            .await
            .map_err(|_| SshTransportError::Timeout)?,
    }
}

pub fn socks5_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
    upstream_proxy_from_env_values(
        env::var("OXIDETERM_SOCKS5_PROXY").ok().as_deref(),
        None,
        env::var("OXIDETERM_NO_PROXY").ok().as_deref(),
    )
}

pub fn upstream_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
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
) -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
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

pub fn parse_socks5_proxy_value(value: &str) -> Result<UpstreamProxyConfig, SshTransportError> {
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

pub fn parse_http_proxy_value(value: &str) -> Result<UpstreamProxyConfig, SshTransportError> {
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
) -> Result<TcpStream, SshTransportError> {
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

async fn dial_direct_tcp(
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, SshTransportError> {
    let socket_addr = resolve_socket_addr(target_host, target_port)?;
    TcpStream::connect(socket_addr)
        .await
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))
}

async fn dial_via_socks5(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, SshTransportError> {
    let proxy_addr = resolve_socket_addr(&proxy.host, proxy.port)?;
    let mut stream = TcpStream::connect(proxy_addr)
        .await
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

    negotiate_socks5_auth(&mut stream, &proxy.auth).await?;
    send_socks5_connect(&mut stream, target_host, target_port, proxy.remote_dns).await?;
    Ok(stream)
}

async fn dial_via_http_connect(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, SshTransportError> {
    let proxy_addr = resolve_socket_addr(&proxy.host, proxy.port)?;
    let mut stream = TcpStream::connect(proxy_addr)
        .await
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

    send_http_connect_request(&mut stream, target_host, target_port, &proxy.auth).await?;
    read_http_connect_response(&mut stream).await?;
    Ok(stream)
}

async fn send_http_connect_request(
    stream: &mut TcpStream,
    target_host: &str,
    target_port: u16,
    auth: &UpstreamProxyAuth,
) -> Result<(), SshTransportError> {
    let authority = http_authority(target_host, target_port);
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    );
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

async fn read_http_connect_response(stream: &mut TcpStream) -> Result<(), SshTransportError> {
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

fn parse_http_connect_status(header: &str) -> Result<u16, SshTransportError> {
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

async fn negotiate_socks5_auth(
    stream: &mut TcpStream,
    auth: &UpstreamProxyAuth,
) -> Result<(), SshTransportError> {
    match auth {
        UpstreamProxyAuth::None => {
            stream
                .write_all(&[SOCKS_VERSION, 1, SOCKS_METHOD_NO_AUTH])
                .await
                .map_err(socks_io_error)?;
        }
        UpstreamProxyAuth::Password { .. } => {
            stream
                .write_all(&[
                    SOCKS_VERSION,
                    2,
                    SOCKS_METHOD_NO_AUTH,
                    SOCKS_METHOD_PASSWORD,
                ])
                .await
                .map_err(socks_io_error)?;
        }
    }

    let mut response = [0_u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(socks_io_error)?;
    if response[0] != SOCKS_VERSION {
        return proxy_error(UpstreamProxyError::SocksInvalidGreeting);
    }

    match response[1] {
        SOCKS_METHOD_NO_AUTH => Ok(()),
        SOCKS_METHOD_PASSWORD => authenticate_socks5_password(stream, auth).await,
        SOCKS_METHOD_NO_ACCEPTABLE => proxy_error(UpstreamProxyError::SocksRejectedAuthMethods),
        method => proxy_error(UpstreamProxyError::SocksUnsupportedAuthMethod(method)),
    }
}

async fn authenticate_socks5_password(
    stream: &mut TcpStream,
    auth: &UpstreamProxyAuth,
) -> Result<(), SshTransportError> {
    let UpstreamProxyAuth::Password { username, password } = auth else {
        return proxy_error(UpstreamProxyError::SocksMissingCredentials);
    };
    let username = username.as_bytes();
    let password = password.as_bytes();
    if username.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
        return proxy_error(UpstreamProxyError::SocksCredentialsTooLong);
    }

    let mut request = Vec::with_capacity(3 + username.len() + password.len());
    request.push(SOCKS_AUTH_VERSION);
    request.push(username.len() as u8);
    request.extend_from_slice(username);
    request.push(password.len() as u8);
    request.extend_from_slice(password);
    stream.write_all(&request).await.map_err(socks_io_error)?;

    let mut response = [0_u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(socks_io_error)?;
    if response[0] != SOCKS_AUTH_VERSION {
        return proxy_error(UpstreamProxyError::SocksInvalidAuthReply);
    }
    if response[1] != 0 {
        return proxy_error(UpstreamProxyError::SocksAuthFailed);
    }
    Ok(())
}

async fn send_socks5_connect(
    stream: &mut TcpStream,
    target_host: &str,
    target_port: u16,
    remote_dns: bool,
) -> Result<(), SshTransportError> {
    let mut request = Vec::new();
    request.extend_from_slice(&[SOCKS_VERSION, SOCKS_COMMAND_CONNECT, 0x00]);
    append_socks5_target(&mut request, target_host, target_port, remote_dns)?;
    stream.write_all(&request).await.map_err(socks_io_error)?;

    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(socks_io_error)?;
    if header[0] != SOCKS_VERSION || header[2] != 0x00 {
        return proxy_error(UpstreamProxyError::SocksInvalidReply);
    }
    if header[1] != 0x00 {
        return proxy_error(UpstreamProxyError::SocksReplyCode(header[1]));
    }

    drain_socks5_bind_address(stream, header[3]).await
}

fn append_socks5_target(
    request: &mut Vec<u8>,
    target_host: &str,
    target_port: u16,
    remote_dns: bool,
) -> Result<(), SshTransportError> {
    if let Ok(ip) = target_host.parse::<IpAddr>() {
        append_socks5_ip(request, ip);
    } else if remote_dns {
        let host = target_host.as_bytes();
        if host.len() > u8::MAX as usize {
            return proxy_error(UpstreamProxyError::SocksTargetHostTooLong);
        }
        request.push(SOCKS_ATYP_DOMAIN);
        request.push(host.len() as u8);
        request.extend_from_slice(host);
    } else {
        append_socks5_ip(request, resolve_socket_addr(target_host, target_port)?.ip());
    }
    request.extend_from_slice(&target_port.to_be_bytes());
    Ok(())
}

fn append_socks5_ip(request: &mut Vec<u8>, ip: IpAddr) {
    match ip {
        IpAddr::V4(ip) => {
            request.push(SOCKS_ATYP_IPV4);
            request.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(SOCKS_ATYP_IPV6);
            request.extend_from_slice(&ip.octets());
        }
    }
}

async fn drain_socks5_bind_address(
    stream: &mut TcpStream,
    atyp: u8,
) -> Result<(), SshTransportError> {
    let address_len = match atyp {
        SOCKS_ATYP_IPV4 => 4,
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            stream.read_exact(&mut len).await.map_err(socks_io_error)?;
            len[0] as usize
        }
        SOCKS_ATYP_IPV6 => 16,
        _ => return proxy_error(UpstreamProxyError::SocksUnknownAddressType),
    };
    let mut sink = vec![0_u8; address_len + 2];
    stream.read_exact(&mut sink).await.map_err(socks_io_error)?;
    Ok(())
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

fn split_host_port(authority: &str) -> Result<(String, u16), SshTransportError> {
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
) -> Result<(String, u16), SshTransportError> {
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

fn parse_port(port: &str, invalid_port: &'static str) -> Result<u16, SshTransportError> {
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

fn resolve_socket_addr(host: &str, port: u16) -> Result<SocketAddr, SshTransportError> {
    let addr = format!("{host}:{port}");
    addr.to_socket_addrs()
        .map_err(|error| SshTransportError::DnsResolution {
            address: addr.clone(),
            message: error.to_string(),
        })?
        .next()
        .ok_or_else(|| SshTransportError::DnsResolution {
            address: addr,
            message: "no address found".to_string(),
        })
}

fn http_io_error(error: std::io::Error) -> SshTransportError {
    proxy_transport_error(UpstreamProxyError::HttpIo(error.to_string()))
}

fn socks_io_error(error: std::io::Error) -> SshTransportError {
    proxy_transport_error(UpstreamProxyError::SocksIo(error.to_string()))
}

fn proxy_error<T>(error: UpstreamProxyError) -> Result<T, SshTransportError> {
    Err(proxy_transport_error(error))
}

fn proxy_transport_error(error: UpstreamProxyError) -> SshTransportError {
    SshTransportError::ConnectionFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncWriteExt, net::TcpListener};

    #[test]
    fn parses_bare_socks5_env_value_with_proxy_dns_default() {
        let proxy = parse_socks5_proxy_value("proxy.example.com:1080").unwrap();

        assert_eq!(proxy.protocol, UpstreamProxyProtocol::Socks5);
        assert_eq!(proxy.host, "proxy.example.com");
        assert_eq!(proxy.port, 1080);
        assert!(proxy.remote_dns);
        assert_eq!(proxy.auth, UpstreamProxyAuth::None);
    }

    #[test]
    fn parses_socks5_url_with_local_dns_semantics() {
        let proxy = parse_socks5_proxy_value("socks5://user:secret@[::1]:1080/path").unwrap();

        assert_eq!(proxy.host, "::1");
        assert_eq!(proxy.port, 1080);
        assert!(!proxy.remote_dns);
        match proxy.auth {
            UpstreamProxyAuth::Password { username, password } => {
                assert_eq!(username, "user");
                assert_eq!(&*password, "secret");
            }
            UpstreamProxyAuth::None => panic!("expected password auth"),
        }
    }

    #[test]
    fn parses_http_proxy_url_for_connect_env() {
        let proxy = parse_http_proxy_value("http://user:secret@proxy.example.com:8080/path")
            .expect("http proxy");

        assert_eq!(proxy.protocol, UpstreamProxyProtocol::HttpConnect);
        assert_eq!(proxy.host, "proxy.example.com");
        assert_eq!(proxy.port, 8080);
        assert!(proxy.remote_dns);
        match proxy.auth {
            UpstreamProxyAuth::Password { username, password } => {
                assert_eq!(username, "user");
                assert_eq!(&*password, "secret");
            }
            UpstreamProxyAuth::None => panic!("expected password auth"),
        }
    }

    #[test]
    fn upstream_proxy_env_prefers_socks5_then_http_and_applies_no_proxy() {
        let proxy = upstream_proxy_from_env_values(
            Some("socks5h://socks.example.com:1080"),
            Some("http://http.example.com:8080"),
            Some("localhost,*.internal"),
        )
        .unwrap()
        .expect("proxy");

        assert_eq!(proxy.protocol, UpstreamProxyProtocol::Socks5);
        assert_eq!(proxy.host, "socks.example.com");
        assert_eq!(proxy.no_proxy, "localhost,*.internal");

        let proxy = upstream_proxy_from_env_values(
            Some(" "),
            Some("http://http.example.com:8080"),
            Some("localhost"),
        )
        .unwrap()
        .expect("proxy");

        assert_eq!(proxy.protocol, UpstreamProxyProtocol::HttpConnect);
        assert_eq!(proxy.host, "http.example.com");
        assert_eq!(proxy.no_proxy, "localhost");
    }

    #[test]
    fn debug_redacts_socks5_password() {
        let proxy =
            parse_socks5_proxy_value("socks5://user:hunter2@proxy.example.com:1080").unwrap();

        let debug = format!("{proxy:?}");

        assert!(debug.contains("user"));
        assert!(!debug.contains("hunter2"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn no_proxy_matches_exact_wildcard_literal_ip_and_cidr() {
        assert!(should_bypass_proxy("example.com", "example.com"));
        assert!(should_bypass_proxy("api.internal", "*.internal"));
        assert!(should_bypass_proxy("127.0.0.1", "127.0.0.1"));
        assert!(should_bypass_proxy("10.2.3.4", "10.0.0.0/8"));
        assert!(should_bypass_proxy("2001:db8::1", "2001:db8::/32"));
        assert!(!should_bypass_proxy("api.external", "*.internal"));
    }

    #[test]
    fn no_proxy_cidr_does_not_resolve_hostname_for_remote_dns() {
        assert!(!should_bypass_proxy("localhost", "127.0.0.0/8"));
    }

    #[tokio::test]
    async fn http_connect_success_connects_to_target() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::Success).await;
        let proxy = http_proxy(proxy_addr, UpstreamProxyAuth::None);

        let mut stream = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap();

        stream.write_all(b"ping").await.unwrap();
    }

    #[tokio::test]
    async fn http_connect_basic_auth_is_sent_and_redacted() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::BasicAuthSuccess {
            username: "user",
            password: "hunter2",
        })
        .await;
        let proxy = http_proxy(
            proxy_addr,
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("hunter2".to_string()),
            },
        );

        let mut stream = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap();

        stream.write_all(b"ping").await.unwrap();
        assert!(!format!("{proxy:?}").contains("hunter2"));
    }

    #[tokio::test]
    async fn http_connect_rejected_status_is_reported_without_credentials() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::Status(407)).await;
        let proxy = http_proxy(
            proxy_addr,
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
        );

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("status 407"));
        assert!(!error.contains("secret"));
    }

    #[tokio::test]
    async fn http_connect_non_200_status_is_reported() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::Status(502)).await;
        let proxy = http_proxy(proxy_addr, UpstreamProxyAuth::None);

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("status 502"));
    }

    #[tokio::test]
    async fn http_connect_malformed_response_is_rejected() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::Malformed).await;
        let proxy = http_proxy(proxy_addr, UpstreamProxyAuth::None);

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid response"));
    }

    #[tokio::test]
    async fn http_connect_oversized_header_is_rejected() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::OversizedHeader).await;
        let proxy = http_proxy(proxy_addr, UpstreamProxyAuth::None);

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("size limit"));
    }

    #[tokio::test]
    async fn http_connect_header_timeout_uses_transport_timeout_error() {
        let proxy_addr = spawn_http_connect_server(MockHttpConnectMode::SlowHeader).await;
        let proxy = http_proxy(proxy_addr, UpstreamProxyAuth::None);

        let error = dial_initial_tcp("target.example.com", 22, 1, Some(&proxy))
            .await
            .unwrap_err();

        assert!(matches!(error, SshTransportError::Timeout));
    }

    #[tokio::test]
    async fn socks5_no_auth_connects_to_domain_target() {
        let proxy_addr = spawn_socks5_server(MockSocks5Mode::NoAuthSuccess).await;
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let mut stream = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap();

        stream.write_all(b"ping").await.unwrap();
    }

    #[tokio::test]
    async fn socks5_username_password_connects() {
        let proxy_addr = spawn_socks5_server(MockSocks5Mode::PasswordSuccess {
            username: "user",
            password: "secret",
        })
        .await;
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
            remote_dns: true,
            no_proxy: String::new(),
        };

        let mut stream = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap();

        stream.write_all(b"ping").await.unwrap();
    }

    #[tokio::test]
    async fn socks5_rejected_method_is_redacted_error() {
        let proxy_addr = spawn_socks5_server(MockSocks5Mode::RejectMethods).await;
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("rejected all auth methods"));
    }

    #[tokio::test]
    async fn socks5_bad_reply_code_is_reported_without_credentials() {
        let proxy_addr = spawn_socks5_server(MockSocks5Mode::BadReplyCode).await;
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
            remote_dns: true,
            no_proxy: String::new(),
        };

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("reply code 0x05"));
        assert!(!error.contains("secret"));
    }

    #[tokio::test]
    async fn socks5_supports_ipv4_and_ipv6_targets() {
        let proxy_addr = spawn_socks5_server(MockSocks5Mode::NoAuthSuccess).await;
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let _ipv4 = dial_initial_tcp("127.0.0.1", 22, 5, Some(&proxy))
            .await
            .unwrap();

        let proxy_addr = spawn_socks5_server(MockSocks5Mode::NoAuthSuccess).await;
        let proxy = UpstreamProxyConfig {
            port: proxy_addr.port(),
            ..proxy
        };
        let _ipv6 = dial_initial_tcp("::1", 22, 5, Some(&proxy)).await.unwrap();
    }

    #[tokio::test]
    async fn socks5_handshake_timeout_uses_transport_timeout_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(3)).await;
        });
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let error = dial_initial_tcp("target.example.com", 22, 1, Some(&proxy))
            .await
            .unwrap_err();

        assert!(matches!(error, SshTransportError::Timeout));
    }

    #[derive(Clone, Copy)]
    enum MockSocks5Mode {
        NoAuthSuccess,
        PasswordSuccess {
            username: &'static str,
            password: &'static str,
        },
        RejectMethods,
        BadReplyCode,
    }

    #[derive(Clone, Copy)]
    enum MockHttpConnectMode {
        Success,
        BasicAuthSuccess {
            username: &'static str,
            password: &'static str,
        },
        Status(u16),
        Malformed,
        OversizedHeader,
        SlowHeader,
    }

    fn http_proxy(proxy_addr: SocketAddr, auth: UpstreamProxyAuth) -> UpstreamProxyConfig {
        UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::HttpConnect,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth,
            remote_dns: true,
            no_proxy: String::new(),
        }
    }

    async fn spawn_http_connect_server(mode: MockHttpConnectMode) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_header(&mut stream).await;
            assert!(request.contains("CONNECT target.example.com:22 HTTP/1.1"));
            match mode {
                MockHttpConnectMode::Success => {
                    stream
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await
                        .unwrap();
                }
                MockHttpConnectMode::BasicAuthSuccess { username, password } => {
                    let expected = BASE64_STANDARD.encode(format!("{username}:{password}"));
                    assert!(request.contains(&format!("Proxy-Authorization: Basic {expected}")));
                    assert!(!request.contains(password));
                    stream
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await
                        .unwrap();
                }
                MockHttpConnectMode::Status(status) => {
                    stream
                        .write_all(format!("HTTP/1.1 {status} Proxy Error\r\n\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                MockHttpConnectMode::Malformed => {
                    stream.write_all(b"not-http\r\n\r\n").await.unwrap();
                }
                MockHttpConnectMode::OversizedHeader => {
                    stream
                        .write_all(&vec![b'a'; HTTP_CONNECT_MAX_HEADER_BYTES + 1])
                        .await
                        .unwrap();
                }
                MockHttpConnectMode::SlowHeader => {
                    // Keep the socket open long enough for the outer proxy dial timeout to fire.
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        });
        addr
    }

    async fn read_http_request_header(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        loop {
            let byte = stream.read_u8().await.unwrap();
            request.push(byte);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).unwrap()
    }

    async fn spawn_socks5_server(mode: MockSocks5Mode) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut greeting = [0_u8; 2];
            stream.read_exact(&mut greeting).await.unwrap();
            let mut methods = vec![0_u8; greeting[1] as usize];
            stream.read_exact(&mut methods).await.unwrap();

            match mode {
                MockSocks5Mode::RejectMethods => {
                    stream
                        .write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_ACCEPTABLE])
                        .await
                        .unwrap();
                    return;
                }
                MockSocks5Mode::NoAuthSuccess | MockSocks5Mode::BadReplyCode => {
                    assert!(methods.contains(&SOCKS_METHOD_NO_AUTH));
                    stream
                        .write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_AUTH])
                        .await
                        .unwrap();
                }
                MockSocks5Mode::PasswordSuccess { username, password } => {
                    assert!(methods.contains(&SOCKS_METHOD_PASSWORD));
                    stream
                        .write_all(&[SOCKS_VERSION, SOCKS_METHOD_PASSWORD])
                        .await
                        .unwrap();
                    assert_password_auth(&mut stream, username, password).await;
                }
            }

            let atyp = read_connect_request(&mut stream).await;
            let reply_code = match mode {
                MockSocks5Mode::BadReplyCode => 0x05,
                _ => 0x00,
            };
            write_success_reply(&mut stream, atyp, reply_code).await;
        });
        addr
    }

    async fn assert_password_auth(stream: &mut TcpStream, username: &str, password: &str) {
        let mut header = [0_u8; 2];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], SOCKS_AUTH_VERSION);
        let mut username_bytes = vec![0_u8; header[1] as usize];
        stream.read_exact(&mut username_bytes).await.unwrap();
        let mut password_len = [0_u8; 1];
        stream.read_exact(&mut password_len).await.unwrap();
        let mut password_bytes = vec![0_u8; password_len[0] as usize];
        stream.read_exact(&mut password_bytes).await.unwrap();
        assert_eq!(username_bytes, username.as_bytes());
        assert_eq!(password_bytes, password.as_bytes());
        stream.write_all(&[SOCKS_AUTH_VERSION, 0x00]).await.unwrap();
    }

    async fn read_connect_request(stream: &mut TcpStream) -> u8 {
        let mut header = [0_u8; 4];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], SOCKS_VERSION);
        assert_eq!(header[1], SOCKS_COMMAND_CONNECT);
        match header[3] {
            SOCKS_ATYP_IPV4 => {
                let mut target = [0_u8; 6];
                stream.read_exact(&mut target).await.unwrap();
            }
            SOCKS_ATYP_IPV6 => {
                let mut target = [0_u8; 18];
                stream.read_exact(&mut target).await.unwrap();
            }
            SOCKS_ATYP_DOMAIN => {
                let mut len = [0_u8; 1];
                stream.read_exact(&mut len).await.unwrap();
                let mut target = vec![0_u8; len[0] as usize + 2];
                stream.read_exact(&mut target).await.unwrap();
            }
            other => panic!("unexpected address type {other}"),
        }
        header[3]
    }

    async fn write_success_reply(stream: &mut TcpStream, atyp: u8, reply_code: u8) {
        let mut reply = vec![SOCKS_VERSION, reply_code, 0x00, atyp];
        match atyp {
            SOCKS_ATYP_IPV4 => reply.extend_from_slice(&[127, 0, 0, 1]),
            SOCKS_ATYP_IPV6 => reply.extend_from_slice(&[0_u8; 16]),
            SOCKS_ATYP_DOMAIN => {
                reply.push(9);
                reply.extend_from_slice(b"localhost");
            }
            _ => unreachable!(),
        }
        reply.extend_from_slice(&0_u16.to_be_bytes());
        stream.write_all(&reply).await.unwrap();
    }
}
