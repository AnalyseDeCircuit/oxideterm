// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{io, net::Ipv6Addr, time::Duration};

use oxideterm_ssh::SshConnectionHandle;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
};

use crate::{
    BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT, ForwardRule, ForwardStats, ForwardStatus,
    ForwardingError, bridge::bridge_tcp_to_ssh_stream,
};

const SOCKS_VERSION_5: u8 = 0x05;
const SOCKS_AUTH_NONE: u8 = 0x00;
const SOCKS_CMD_CONNECT: u8 = 0x01;
const SOCKS_ATYP_IPV4: u8 = 0x01;
const SOCKS_ATYP_DOMAIN: u8 = 0x03;
const SOCKS_ATYP_IPV6: u8 = 0x04;
const SOCKS_REPLY_SUCCEEDED: u8 = 0x00;
const SOCKS_REPLY_HOST_UNREACHABLE: u8 = 0x04;
const SOCKS_REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;
const SOCKS_REPLY_ADDRESS_NOT_SUPPORTED: u8 = 0x08;
const FORWARD_STOP_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(crate) struct DynamicForward {
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl DynamicForward {
    pub(crate) async fn start(
        mut rule: ForwardRule,
        ssh_connection: SshConnectionHandle,
    ) -> Result<Self, ForwardingError> {
        validate_dynamic_rule(&rule)?;
        let listener = TcpListener::bind((rule.bind_address.as_str(), rule.bind_port)).await?;
        let bound_addr = listener.local_addr()?;
        rule.bind_address = bound_addr.ip().to_string();
        rule.bind_port = bound_addr.port();
        // Tauri keeps the rule payload supplied by the node command even
        // though SOCKS5 chooses each destination per connection.
        rule.status = ForwardStatus::Active;

        let stats = BridgeStatsRecorder::default();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task_rule = rule.clone();
        let task_stats = stats.clone();
        let task = tokio::spawn(async move {
            accept_dynamic_connections(
                listener,
                ssh_connection,
                task_rule,
                task_stats,
                shutdown_rx,
            )
            .await;
        });

        Ok(Self {
            rule,
            stats,
            shutdown_tx,
            task,
        })
    }

    pub(crate) fn rule(&self) -> ForwardRule {
        self.rule.clone()
    }

    pub(crate) fn stats(&self) -> ForwardStats {
        self.stats.snapshot()
    }

    pub(crate) async fn stop(self) -> ForwardRule {
        let _ = self.shutdown_tx.send(true);
        let _ = self
            .stats
            .active_connections()
            .wait_zero(FORWARD_STOP_GRACE_PERIOD)
            .await;
        self.task.abort();
        let mut stopped = self.rule;
        stopped.status = ForwardStatus::Stopped;
        stopped
    }
}

async fn accept_dynamic_connections(
    listener: TcpListener,
    ssh_connection: SshConnectionHandle,
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, origin_addr) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::warn!("dynamic forward {} accept failed: {error}", rule.id);
                        continue;
                    }
                };
                if let Err(error) = stream.set_nodelay(true) {
                    tracing::debug!("dynamic forward {} failed to set TCP_NODELAY: {error}", rule.id);
                }
                let connection = ssh_connection.clone();
                let connection_rule = rule.clone();
                let connection_stats = stats.clone();
                let connection_shutdown = shutdown_rx.clone();
                tokio::spawn(async move {
                    if let Err(error) = bridge_dynamic_connection(
                        stream,
                        connection,
                        connection_rule,
                        connection_stats,
                        connection_shutdown,
                        origin_addr.ip().to_string(),
                        origin_addr.port(),
                    )
                    .await
                    {
                        tracing::warn!("dynamic forward connection failed: {error}");
                    }
                });
            }
        }
    }
}

async fn bridge_dynamic_connection(
    mut stream: TcpStream,
    ssh_connection: SshConnectionHandle,
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    shutdown_rx: watch::Receiver<bool>,
    origin_host: String,
    origin_port: u16,
) -> Result<(), ForwardingError> {
    let destination = read_socks5_connect_destination(&mut stream).await?;
    let ssh_stream = match ssh_connection
        .open_direct_tcpip(
            &destination.host,
            destination.port,
            &origin_host,
            origin_port,
        )
        .await
    {
        Ok(stream) => stream,
        Err(error) => {
            // Tauri reports direct-tcpip open failures to the SOCKS5 client before
            // dropping the socket, so strict clients do not hang waiting for a reply.
            if let Err(reply_error) =
                send_socks5_failure(&mut stream, SOCKS_REPLY_HOST_UNREACHABLE).await
            {
                tracing::debug!(
                    "dynamic forward {} failed to send SOCKS5 host-unreachable reply: {reply_error}",
                    rule.id
                );
            }
            return Err(ForwardingError::from(error));
        }
    };
    send_socks5_success(&mut stream).await?;

    bridge_tcp_to_ssh_stream(
        stream,
        ssh_stream,
        stats,
        DEFAULT_FORWARD_IDLE_TIMEOUT,
        shutdown_rx,
        format!(
            "dynamic forward {} {}:{} -> {}:{}",
            rule.id, rule.bind_address, rule.bind_port, destination.host, destination.port
        ),
    )
    .await
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SocksDestination {
    host: String,
    port: u16,
}

async fn read_socks5_connect_destination(
    stream: &mut TcpStream,
) -> Result<SocksDestination, ForwardingError> {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header).await?;
    if header[0] != SOCKS_VERSION_5 {
        return Err(ForwardingError::InvalidRule(
            "only SOCKS5 is supported".to_string(),
        ));
    }

    let method_count = header[1] as usize;
    let mut methods = vec![0_u8; method_count];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&SOCKS_AUTH_NONE) {
        stream.write_all(&[SOCKS_VERSION_5, 0xff]).await?;
        return Err(ForwardingError::InvalidRule(
            "SOCKS5 client did not offer no-auth method".to_string(),
        ));
    }
    stream
        .write_all(&[SOCKS_VERSION_5, SOCKS_AUTH_NONE])
        .await?;

    let mut request = [0_u8; 4];
    stream.read_exact(&mut request).await?;
    if request[0] != SOCKS_VERSION_5 {
        return Err(ForwardingError::InvalidRule(
            "invalid SOCKS5 request".to_string(),
        ));
    }
    if request[1] != SOCKS_CMD_CONNECT {
        send_socks5_failure(stream, SOCKS_REPLY_COMMAND_NOT_SUPPORTED).await?;
        return Err(ForwardingError::UnsupportedForwardType("socks5 command"));
    }

    let host = match request[3] {
        SOCKS_ATYP_IPV4 => {
            let mut octets = [0_u8; 4];
            stream.read_exact(&mut octets).await?;
            std::net::Ipv4Addr::from(octets).to_string()
        }
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            stream.read_exact(&mut len).await?;
            let mut host = vec![0_u8; len[0] as usize];
            stream.read_exact(&mut host).await?;
            String::from_utf8_lossy(&host).to_string()
        }
        SOCKS_ATYP_IPV6 => {
            let mut octets = [0_u8; 16];
            stream.read_exact(&mut octets).await?;
            Ipv6Addr::from(octets).to_string()
        }
        _ => {
            send_socks5_failure(stream, SOCKS_REPLY_ADDRESS_NOT_SUPPORTED).await?;
            return Err(ForwardingError::InvalidRule(
                "unsupported SOCKS5 address type".to_string(),
            ));
        }
    };

    let mut port = [0_u8; 2];
    stream.read_exact(&mut port).await?;
    Ok(SocksDestination {
        host,
        port: u16::from_be_bytes(port),
    })
}

async fn send_socks5_success(stream: &mut TcpStream) -> io::Result<()> {
    stream
        .write_all(&[
            SOCKS_VERSION_5,
            SOCKS_REPLY_SUCCEEDED,
            0x00,
            SOCKS_ATYP_IPV4,
            0,
            0,
            0,
            0,
            0,
            0,
        ])
        .await
}

async fn send_socks5_failure(stream: &mut TcpStream, reply: u8) -> io::Result<()> {
    stream
        .write_all(&[
            SOCKS_VERSION_5,
            reply,
            0x00,
            SOCKS_ATYP_IPV4,
            0,
            0,
            0,
            0,
            0,
            0,
        ])
        .await
}

fn validate_dynamic_rule(rule: &ForwardRule) -> Result<(), ForwardingError> {
    if rule.bind_address.trim().is_empty() {
        return Err(ForwardingError::InvalidRule(
            "bind address is required".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn socks5_accepts_tauri_supported_address_types() {
        let destination = read_destination_from_request(&[
            SOCKS_VERSION_5,
            SOCKS_CMD_CONNECT,
            0x00,
            SOCKS_ATYP_IPV4,
            127,
            0,
            0,
            1,
            0x01,
            0xbb,
        ])
        .await;
        assert_eq!(
            destination,
            SocksDestination {
                host: "127.0.0.1".to_string(),
                port: 443,
            }
        );

        let mut domain_request = vec![
            SOCKS_VERSION_5,
            SOCKS_CMD_CONNECT,
            0x00,
            SOCKS_ATYP_DOMAIN,
            11,
        ];
        domain_request.extend_from_slice(b"example.com");
        domain_request.extend_from_slice(&80_u16.to_be_bytes());
        let destination = read_destination_from_request(&domain_request).await;
        assert_eq!(
            destination,
            SocksDestination {
                host: "example.com".to_string(),
                port: 80,
            }
        );

        let mut lossy_domain_request = vec![
            SOCKS_VERSION_5,
            SOCKS_CMD_CONNECT,
            0x00,
            SOCKS_ATYP_DOMAIN,
            4,
        ];
        lossy_domain_request.extend_from_slice(&[b'h', b'i', 0xff, b'!']);
        lossy_domain_request.extend_from_slice(&1080_u16.to_be_bytes());
        let destination = read_destination_from_request(&lossy_domain_request).await;
        assert_eq!(
            destination,
            SocksDestination {
                host: String::from_utf8_lossy(&[b'h', b'i', 0xff, b'!']).to_string(),
                port: 1080,
            }
        );

        let mut ipv6_request = vec![SOCKS_VERSION_5, SOCKS_CMD_CONNECT, 0x00, SOCKS_ATYP_IPV6];
        ipv6_request.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        ipv6_request.extend_from_slice(&8080_u16.to_be_bytes());
        let destination = read_destination_from_request(&ipv6_request).await;
        assert_eq!(
            destination,
            SocksDestination {
                host: "::1".to_string(),
                port: 8080,
            }
        );
    }

    #[tokio::test]
    async fn socks5_rejects_unsupported_command_with_tauri_reply() {
        let reply = read_failure_reply_for_request(&[
            SOCKS_VERSION_5,
            0x02,
            0x00,
            SOCKS_ATYP_IPV4,
            127,
            0,
            0,
            1,
            0x00,
            0x50,
        ])
        .await;

        assert_eq!(
            reply,
            socks5_reply(SOCKS_REPLY_COMMAND_NOT_SUPPORTED),
            "native must mirror Tauri's REP_CMD_NOT_SUPPORTED response"
        );
    }

    #[tokio::test]
    async fn socks5_rejects_unsupported_address_type_with_tauri_reply() {
        let reply =
            read_failure_reply_for_request(&[SOCKS_VERSION_5, SOCKS_CMD_CONNECT, 0x00, 0x05]).await;

        assert_eq!(
            reply,
            socks5_reply(SOCKS_REPLY_ADDRESS_NOT_SUPPORTED),
            "native must mirror Tauri's REP_ADDR_NOT_SUPPORTED response"
        );
    }

    #[tokio::test]
    async fn socks5_success_reply_matches_tauri_shape() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut server_stream, _) = listener.accept().await.unwrap();
            send_socks5_success(&mut server_stream).await.unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut reply = [0_u8; 10];
        client.read_exact(&mut reply).await.unwrap();
        server.await.unwrap();

        assert_eq!(reply, socks5_reply(SOCKS_REPLY_SUCCEEDED));
    }

    async fn read_destination_from_request(request: &[u8]) -> SocksDestination {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut server_stream, _) = listener.accept().await.unwrap();
            read_socks5_connect_destination(&mut server_stream).await
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        send_greeting(&mut client).await;
        client.write_all(request).await.unwrap();

        server.await.unwrap().unwrap()
    }

    async fn read_failure_reply_for_request(request: &[u8]) -> [u8; 10] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut server_stream, _) = listener.accept().await.unwrap();
            let _ = read_socks5_connect_destination(&mut server_stream).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        send_greeting(&mut client).await;
        client.write_all(request).await.unwrap();

        let mut reply = [0_u8; 10];
        client.read_exact(&mut reply).await.unwrap();
        server.await.unwrap();
        reply
    }

    async fn send_greeting(client: &mut TcpStream) {
        client
            .write_all(&[SOCKS_VERSION_5, 0x01, SOCKS_AUTH_NONE])
            .await
            .unwrap();

        let mut reply = [0_u8; 2];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [SOCKS_VERSION_5, SOCKS_AUTH_NONE]);
    }

    fn socks5_reply(status: u8) -> [u8; 10] {
        [
            SOCKS_VERSION_5,
            status,
            0x00,
            SOCKS_ATYP_IPV4,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
    }
}
