// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use oxideterm_forwarding::{ForwardRule, ForwardingManager};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionPoolConfig, SshConfig, SshConnectionHandle,
    SshConnectionRegistry, SshTransportClient,
};
use rand10::{rand_core::UnwrapErr, rngs::SysRng};
use russh::{
    Channel, ChannelId,
    keys::{Algorithm, HashAlg, PrivateKey},
    server::{self, Msg, Session},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_forward_moves_bytes_through_real_ssh_server() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-local", handle);
    let rule = manager
        .create_forward(ForwardRule::local(
            "127.0.0.1",
            0,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();

    assert_eq!(
        roundtrip(("127.0.0.1", rule.bind_port), b"local").await,
        b"local".to_vec()
    );

    let opens = ssh.direct_tcpip_opens.lock().await;
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].originator_address, "127.0.0.1");
    assert_eq!(opens[0].originator_port, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_forward_stop_releases_listener_before_returning() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-local-stop", handle);
    let rule = manager
        .create_forward(ForwardRule::local(
            "127.0.0.1",
            0,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();

    manager.stop_forward(&rule.id).await.unwrap();
    let rebound_listener = TcpListener::bind(("127.0.0.1", rule.bind_port))
        .await
        .unwrap();
    drop(rebound_listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dynamic_forward_moves_socks5_bytes_through_real_ssh_server() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-dynamic", handle);
    let rule = manager
        .create_forward(ForwardRule::dynamic("127.0.0.1", 0))
        .await
        .unwrap();

    let mut stream = TcpStream::connect(("127.0.0.1", rule.bind_port))
        .await
        .unwrap();
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method = [0_u8; 2];
    stream.read_exact(&mut method).await.unwrap();
    assert_eq!(method, [0x05, 0x00]);

    let mut request = vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1];
    request.extend_from_slice(&echo_addr.port().to_be_bytes());
    stream.write_all(&request).await.unwrap();
    let mut response = [0_u8; 10];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);

    stream.write_all(b"dynamic").await.unwrap();
    let mut buf = [0_u8; 7];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"dynamic");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dynamic_forward_stop_closes_incomplete_handshake_and_releases_listener() {
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-dynamic-stop", handle);
    let rule = manager
        .create_forward(ForwardRule::dynamic("127.0.0.1", 0))
        .await
        .unwrap();
    let mut incomplete_client = TcpStream::connect(("127.0.0.1", rule.bind_port))
        .await
        .unwrap();

    manager.stop_forward(&rule.id).await.unwrap();

    assert_stream_closed(&mut incomplete_client).await;
    let rebound_listener = TcpListener::bind(("127.0.0.1", rule.bind_port))
        .await
        .unwrap();
    drop(rebound_listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_forward_moves_bytes_through_real_ssh_server() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-remote", handle);
    let rule = manager
        .create_forward(ForwardRule::remote(
            "127.0.0.1",
            0,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();

    assert_eq!(
        roundtrip(("127.0.0.1", rule.bind_port), b"remote").await,
        b"remote".to_vec()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_forward_health_check_uses_local_target() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-remote-health", handle);
    let rule = manager
        .create_forward_with_health_check(
            ForwardRule::remote("127.0.0.1", 0, echo_addr.ip().to_string(), echo_addr.port()),
            true,
        )
        .await
        .unwrap();

    assert!(ssh.direct_tcpip_opens.lock().await.is_empty());
    manager.stop_forward(&rule.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_remote_forward_stops_and_releases_requested_port() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-explicit-remote", handle);
    let requested_port = reserve_tcp_port().await;
    let rule = manager
        .create_forward(ForwardRule::remote(
            "127.0.0.1",
            requested_port,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();

    assert_eq!(rule.bind_port, requested_port);
    assert_eq!(
        roundtrip(("127.0.0.1", requested_port), b"explicit").await,
        b"explicit".to_vec()
    );

    let stopped_rule = manager.stop_forward(&rule.id).await.unwrap();
    assert_eq!(stopped_rule.bind_port, requested_port);
    let rebound_listener = TcpListener::bind(("127.0.0.1", requested_port))
        .await
        .unwrap();
    drop(rebound_listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_forward_stop_closes_existing_bridge() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-remote-active-stop", handle);
    let rule = manager
        .create_forward(ForwardRule::remote(
            "127.0.0.1",
            0,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();
    let mut active_client = TcpStream::connect(("127.0.0.1", rule.bind_port))
        .await
        .unwrap();
    active_client.write_all(b"active").await.unwrap();
    let mut echoed = [0_u8; 6];
    active_client.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, b"active");

    manager.stop_forward(&rule.id).await.unwrap();

    assert_stream_closed(&mut active_client).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_remote_cancel_keeps_rule_active() {
    let echo_addr = start_echo_service().await;
    let ssh = start_forwarding_ssh_server().await;
    let handle = connect_test_client(&ssh).await;
    let manager = ForwardingManager::new("session-remote-cancel-rejected", handle);
    let rule = manager
        .create_forward(ForwardRule::remote(
            "127.0.0.1",
            0,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        ))
        .await
        .unwrap();
    ssh.reject_remote_cancel.store(true, Ordering::SeqCst);

    assert!(manager.stop_forward(&rule.id).await.is_err());
    let retained = manager
        .list_forwards()
        .into_iter()
        .find(|candidate| candidate.id == rule.id)
        .unwrap();
    assert_eq!(retained.status, oxideterm_forwarding::ForwardStatus::Active);
    assert_eq!(
        roundtrip(("127.0.0.1", rule.bind_port), b"retained").await,
        b"retained".to_vec()
    );

    ssh.reject_remote_cancel.store(false, Ordering::SeqCst);
    manager.stop_forward(&rule.id).await.unwrap();
}

async fn connect_test_client(ssh: &TestSshServer) -> SshConnectionHandle {
    let mut config = SshConfig::password("127.0.0.1", ssh.port, "tester", "password");
    config.timeout_secs = 5;
    config.expected_host_key_fingerprint = Some(ssh.host_key_fingerprint.clone());
    config.trust_host_key = Some(false);
    let registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
    let pty = SshTransportClient::new(config)
        .connect_shell_with_registry(
            registry,
            ConnectionConsumer::Terminal("forward-e2e".to_string()),
        )
        .await
        .unwrap();
    pty.ssh_connection_handle().unwrap()
}

async fn roundtrip(addr: (&str, u16), payload: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; payload.len()];
    stream.read_exact(&mut buf).await.unwrap();
    buf
}

async fn assert_stream_closed(stream: &mut TcpStream) {
    let mut buffer = [0_u8; 1];
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .expect("forwarded connection remained open after stop");
    match result {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        other => panic!("expected a closed forwarded connection, got {other:?}"),
    }
}

async fn start_echo_service() -> SocketAddr {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.split();
                let _ = tokio::io::copy(&mut reader, &mut writer).await;
            });
        }
    });
    addr
}

async fn reserve_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    listener.local_addr().unwrap().port()
}

struct TestSshServer {
    port: u16,
    host_key_fingerprint: String,
    direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
    reject_remote_cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectTcpipOpen {
    originator_address: String,
    originator_port: u32,
}

async fn start_forwarding_ssh_server() -> TestSshServer {
    let host_key = PrivateKey::random(&mut UnwrapErr(SysRng), Algorithm::Ed25519).unwrap();
    let host_key_fingerprint = host_key
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: std::time::Duration::ZERO,
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        keys: vec![host_key],
        ..Default::default()
    });
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let forwards = Arc::new(Mutex::new(HashMap::new()));
    let direct_tcpip_opens = Arc::new(Mutex::new(Vec::new()));
    let server_direct_tcpip_opens = direct_tcpip_opens.clone();
    let reject_remote_cancel = Arc::new(AtomicBool::new(false));
    let server_reject_remote_cancel = reject_remote_cancel.clone();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let handler = ForwardingServer {
                forwards: forwards.clone(),
                direct_tcpip_opens: server_direct_tcpip_opens.clone(),
                reject_remote_cancel: server_reject_remote_cancel.clone(),
            };
            let config = config.clone();
            tokio::spawn(async move {
                let _ = server::run_stream(config, stream, handler).await;
            });
        }
    });

    TestSshServer {
        port,
        host_key_fingerprint,
        direct_tcpip_opens,
        reject_remote_cancel,
    }
}

#[derive(Clone)]
struct ForwardingServer {
    forwards: Arc<Mutex<HashMap<(String, u32), tokio::task::JoinHandle<()>>>>,
    direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
    reject_remote_cancel: Arc<AtomicBool>,
}

impl server::Handler for ForwardingServer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _user: &str,
        _password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.direct_tcpip_opens.lock().await.push(DirectTcpipOpen {
            originator_address: originator_address.to_string(),
            originator_port,
        });
        let target = format!("{host_to_connect}:{port_to_connect}");
        tokio::spawn(async move {
            let Ok(mut target) = TcpStream::connect(target).await else {
                return;
            };
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
        reply.accept().await;
        Ok(())
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let listener = TcpListener::bind((address, *port as u16)).await?;
        *port = listener.local_addr()?.port() as u32;
        let key = (address.to_string(), *port);
        let handle = session.handle();
        let connected_address = address.to_string();
        let connected_port = *port;
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut inbound, origin)) = listener.accept().await else {
                    break;
                };
                let Ok(channel) = handle
                    .channel_open_forwarded_tcpip(
                        connected_address.clone(),
                        connected_port,
                        origin.ip().to_string(),
                        origin.port() as u32,
                    )
                    .await
                else {
                    break;
                };
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut stream).await;
                });
            }
        });
        self.forwards.lock().await.insert(key, task);
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if self.reject_remote_cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if let Some(task) = self
            .forwards
            .lock()
            .await
            .remove(&(address.to_string(), port))
        {
            task.abort();
            // Release the test listener before acknowledging cancellation so
            // the client can prove that stopping the forward frees the port.
            let _ = task.await;
        }
        Ok(true)
    }
}
