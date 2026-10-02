use super::*;
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[test]
fn parses_supported_upstream_proxy_forms() {
    let cases = [
        (
            parse_socks5_proxy_value as fn(&str) -> Result<UpstreamProxyConfig, TcpProxyError>,
            "proxy.example.com:1080",
            UpstreamProxyProtocol::Socks5,
            "proxy.example.com",
            1080,
            true,
            UpstreamProxyAuth::None,
        ),
        (
            parse_socks5_proxy_value,
            "socks5://user:secret@[::1]:1080/path",
            UpstreamProxyProtocol::Socks5,
            "::1",
            1080,
            false,
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
        ),
        (
            parse_http_proxy_value,
            "http://user:secret@proxy.example.com:8080/path",
            UpstreamProxyProtocol::HttpConnect,
            "proxy.example.com",
            8080,
            true,
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
        ),
    ];

    for (parse, input, protocol, host, port, remote_dns, auth) in cases {
        let proxy = parse(input).unwrap();
        assert_eq!(proxy.protocol, protocol);
        assert_eq!(proxy.host, host);
        assert_eq!(proxy.port, port);
        assert_eq!(proxy.remote_dns, remote_dns);
        assert_eq!(proxy.auth, auth);
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
    let proxy = parse_socks5_proxy_value("socks5://user:hunter2@proxy.example.com:1080").unwrap();

    let debug = format!("{proxy:?}");

    assert!(debug.contains("user"));
    assert!(!debug.contains("hunter2"));
    assert!(debug.contains("redacted"));
}

#[test]
fn no_proxy_matches_literals_and_patterns_without_resolving_hostnames() {
    for (target, rule, expected) in [
        ("example.com", "example.com", true),
        ("api.internal", "*.internal", true),
        ("127.0.0.1", "127.0.0.1", true),
        ("10.2.3.4", "10.0.0.0/8", true),
        ("2001:db8::1", "2001:db8::/32", true),
        ("api.external", "*.internal", false),
        ("localhost", "127.0.0.0/8", false),
    ] {
        assert_eq!(
            should_bypass_proxy(target, rule),
            expected,
            "{target} / {rule}"
        );
    }
}

#[tokio::test]
async fn direct_tcp_enables_nodelay_for_ssh_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _accepted = listener.accept().await.unwrap();
    });

    let stream = dial_initial_tcp(&target_addr.ip().to_string(), target_addr.port(), 5, None)
        .await
        .unwrap();

    assert!(stream.nodelay().unwrap());
}

#[tokio::test]
async fn http_connect_preserves_target_authentication_and_tunnel_data() {
    for mode in [
        MockHttpConnectMode::Success,
        MockHttpConnectMode::BasicAuthSuccess,
    ] {
        let (proxy_addr, server) = spawn_http_connect_server(mode).await;
        let auth = match mode {
            MockHttpConnectMode::BasicAuthSuccess => UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("hunter2".to_string()),
            },
            _ => UpstreamProxyAuth::None,
        };
        let proxy = http_proxy(proxy_addr, auth);
        let mut stream = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap();
        assert!(stream.nodelay().unwrap());
        stream.write_all(b"ping").await.unwrap();
        let mut reply = [0; 4];
        tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut reply))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&reply, b"pong");
        if matches!(mode, MockHttpConnectMode::BasicAuthSuccess) {
            assert!(!format!("{proxy:?}").contains("hunter2"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn http_connect_rejects_status_and_invalid_headers_without_credentials() {
    for (mode, expected, authenticated) in [
        (MockHttpConnectMode::Status(407), "status 407", true),
        (MockHttpConnectMode::Status(502), "status 502", false),
        (MockHttpConnectMode::Malformed, "invalid response", false),
        (MockHttpConnectMode::OversizedHeader, "size limit", false),
    ] {
        let (proxy_addr, server) = spawn_http_connect_server(mode).await;
        let auth = if authenticated {
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            }
        } else {
            UpstreamProxyAuth::None
        };
        let proxy = http_proxy(proxy_addr, auth);
        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "expected {expected}");
        if authenticated {
            assert!(!error.contains("secret"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn socks5_connect_preserves_target_authentication_and_tunnel_data() {
    let password_mode = MockSocks5Mode::PasswordSuccess {
        username: "user",
        password: "secret",
    };
    for (target, mode) in [
        ("target.example.com", MockSocks5Mode::NoAuthSuccess),
        ("target.example.com", password_mode),
        ("127.0.0.1", MockSocks5Mode::NoAuthSuccess),
        ("::1", MockSocks5Mode::NoAuthSuccess),
    ] {
        let (proxy_addr, server) = spawn_socks5_server(mode, target).await;
        let auth = match mode {
            MockSocks5Mode::PasswordSuccess { username, password } => UpstreamProxyAuth::Password {
                username: username.into(),
                password: Zeroizing::new(password.into()),
            },
            _ => UpstreamProxyAuth::None,
        };
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth,
            remote_dns: true,
            no_proxy: String::new(),
        };
        let mut stream = dial_initial_tcp(target, 22, 5, Some(&proxy)).await.unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut reply = [0; 4];
        tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut reply))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&reply, b"pong", "{target}");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn socks5_rejections_preserve_the_failure_stage_without_credentials() {
    for (mode, auth, expected) in [
        (
            MockSocks5Mode::RejectMethods,
            UpstreamProxyAuth::None,
            "rejected all auth methods",
        ),
        (
            MockSocks5Mode::BadReplyCode,
            UpstreamProxyAuth::Password {
                username: "user".to_string(),
                password: Zeroizing::new("secret".to_string()),
            },
            "reply code 0x05",
        ),
    ] {
        let (proxy_addr, server) = spawn_socks5_server(mode, "target.example.com").await;
        let authenticated = matches!(&auth, UpstreamProxyAuth::Password { .. });
        let proxy = UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let error = dial_initial_tcp("target.example.com", 22, 5, Some(&proxy))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains(expected), "expected {expected}");
        if authenticated {
            assert!(!error.contains("secret"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn proxy_handshake_timeouts_use_transport_timeout_error() {
    for protocol in [
        UpstreamProxyProtocol::HttpConnect,
        UpstreamProxyProtocol::Socks5,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        let server_protocol = protocol;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            if server_protocol == UpstreamProxyProtocol::HttpConnect {
                let request = read_http_request_header(&mut stream).await;
                assert!(request.contains("CONNECT target.example.com:22 HTTP/1.1"));
            }
            std::future::pending::<()>().await;
        });
        let proxy = UpstreamProxyConfig {
            protocol,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        };

        let error = dial_initial_tcp("target.example.com", 22, 1, Some(&proxy))
            .await
            .unwrap_err();

        assert!(matches!(error, TcpProxyError::Timeout));
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
    }
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
    BasicAuthSuccess,
    Status(u16),
    Malformed,
    OversizedHeader,
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

async fn spawn_http_connect_server(
    mode: MockHttpConnectMode,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request_header(&mut stream).await;
        assert!(request.contains("CONNECT target.example.com:22 HTTP/1.1"));
        match mode {
            MockHttpConnectMode::Success => {
                assert!(!request.contains("Proxy-Authorization:"));
                stream
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await
                    .unwrap();
            }
            MockHttpConnectMode::BasicAuthSuccess => {
                assert!(request.contains("Proxy-Authorization: Basic dXNlcjpodW50ZXIy\r\n"));
                assert!(!request.contains("hunter2"));
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
        }
        if matches!(
            mode,
            MockHttpConnectMode::Success | MockHttpConnectMode::BasicAuthSuccess
        ) {
            let mut payload = [0; 4];
            stream.read_exact(&mut payload).await.unwrap();
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
        }
    });
    (addr, server)
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

async fn spawn_socks5_server(
    mode: MockSocks5Mode,
    target: &'static str,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
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

        let atyp = read_connect_request(&mut stream, target).await;
        let reply_code = match mode {
            MockSocks5Mode::BadReplyCode => 0x05,
            _ => 0x00,
        };
        write_success_reply(&mut stream, atyp, reply_code).await;
        if reply_code == 0 {
            let mut payload = [0; 4];
            stream.read_exact(&mut payload).await.unwrap();
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
        }
    });
    (addr, server)
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

async fn read_connect_request(stream: &mut TcpStream, expected_target: &str) -> u8 {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[0], SOCKS_VERSION);
    assert_eq!(header[1], SOCKS_COMMAND_CONNECT);
    match header[3] {
        SOCKS_ATYP_IPV4 => {
            let mut target = [0_u8; 6];
            stream.read_exact(&mut target).await.unwrap();
            assert_eq!(
                &target[..4],
                &expected_target
                    .parse::<std::net::Ipv4Addr>()
                    .unwrap()
                    .octets()
            );
            assert_eq!(&target[4..], &22_u16.to_be_bytes());
        }
        SOCKS_ATYP_IPV6 => {
            let mut target = [0_u8; 18];
            stream.read_exact(&mut target).await.unwrap();
            assert_eq!(
                &target[..16],
                &expected_target
                    .parse::<std::net::Ipv6Addr>()
                    .unwrap()
                    .octets()
            );
            assert_eq!(&target[16..], &22_u16.to_be_bytes());
        }
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            stream.read_exact(&mut len).await.unwrap();
            let mut target = vec![0_u8; len[0] as usize + 2];
            stream.read_exact(&mut target).await.unwrap();
            assert_eq!(&target[..len[0] as usize], expected_target.as_bytes());
            assert_eq!(&target[len[0] as usize..], &22_u16.to_be_bytes());
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
