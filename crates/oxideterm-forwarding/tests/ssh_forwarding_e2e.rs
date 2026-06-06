// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

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

struct TestSshServer {
    port: u16,
    host_key_fingerprint: String,
    direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
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

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let handler = ForwardingServer {
                forwards: forwards.clone(),
                direct_tcpip_opens: server_direct_tcpip_opens.clone(),
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
    }
}

#[derive(Clone)]
struct ForwardingServer {
    forwards: Arc<Mutex<HashMap<(String, u32), tokio::task::JoinHandle<()>>>>,
    direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
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
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
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
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
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
        Ok(true)
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
        if let Some(task) = self
            .forwards
            .lock()
            .await
            .remove(&(address.to_string(), port))
        {
            task.abort();
        }
        Ok(true)
    }
}
