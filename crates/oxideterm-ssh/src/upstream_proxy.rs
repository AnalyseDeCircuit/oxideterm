// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    env, fmt,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

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
    HttpConnectUnsupported,
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
            Self::HttpConnectUnsupported => {
                formatter.write_str("HTTP CONNECT upstream proxy is not implemented yet")
            }
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
            dial_via_upstream_proxy(target_host, target_port, proxy),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?,
        None => {
            let socket_addr = resolve_socket_addr(target_host, target_port)?;
            tokio::time::timeout(timeout, TcpStream::connect(socket_addr))
                .await
                .map_err(|_| SshTransportError::Timeout)?
                .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))
        }
    }
}

pub fn socks5_proxy_from_env() -> Result<Option<UpstreamProxyConfig>, SshTransportError> {
    let Ok(value) = env::var("OXIDETERM_SOCKS5_PROXY") else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut proxy = parse_socks5_proxy_value(trimmed)?;
    proxy.no_proxy = env::var("OXIDETERM_NO_PROXY").unwrap_or_default();
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

async fn dial_via_upstream_proxy(
    target_host: &str,
    target_port: u16,
    proxy: &UpstreamProxyConfig,
) -> Result<TcpStream, SshTransportError> {
    match proxy.protocol {
        UpstreamProxyProtocol::Socks5 => dial_via_socks5(target_host, target_port, proxy).await,
        UpstreamProxyProtocol::HttpConnect => {
            proxy_error(UpstreamProxyError::HttpConnectUnsupported)
        }
    }
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
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((host, suffix)) = rest.split_once(']') else {
            return proxy_error(UpstreamProxyError::SocksInvalidProxyValue(
                "SOCKS5 proxy IPv6 host is missing ']'",
            ));
        };
        let Some(port) = suffix.strip_prefix(':') else {
            return proxy_error(UpstreamProxyError::SocksInvalidProxyValue(
                "SOCKS5 proxy port is missing",
            ));
        };
        return Ok((host.to_string(), parse_port(port)?));
    }

    let Some((host, port)) = authority.rsplit_once(':') else {
        return proxy_error(UpstreamProxyError::SocksInvalidProxyValue(
            "SOCKS5 proxy value must include host and port",
        ));
    };
    if host.is_empty() {
        return proxy_error(UpstreamProxyError::SocksInvalidProxyValue(
            "SOCKS5 proxy host is empty",
        ));
    }
    Ok((host.to_string(), parse_port(port)?))
}

fn parse_port(port: &str) -> Result<u16, SshTransportError> {
    port.parse::<u16>().map_err(|_| {
        proxy_transport_error(UpstreamProxyError::SocksInvalidProxyValue(
            "SOCKS5 proxy port is invalid",
        ))
    })
}

fn trim_url_tail(authority: &str) -> &str {
    authority
        .split_once(['/', '?', '#'])
        .map_or(authority, |(head, _)| head)
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
    fn debug_redacts_socks5_password() {
        let proxy =
            parse_socks5_proxy_value("socks5://user:hunter2@proxy.example.com:1080").unwrap();

        let debug = format!("{proxy:?}");

        assert!(debug.contains("user"));
        assert!(!debug.contains("hunter2"));
        assert!(debug.contains("redacted"));
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
