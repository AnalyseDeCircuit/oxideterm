use std::{borrow::Cow, sync::Arc, time::Duration};

use oxideterm_ssh::{HostKeyStatus, SshConfig, SshTransportClient, check_host_key_with_route};
use russh::{mac, server};
use tokio::net::TcpListener;

struct LegacyServer;

impl server::Handler for LegacyServer {
    type Error = russh::Error;

    async fn auth_none(&mut self, _: &str) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }
}

#[tokio::test]
async fn host_key_preflight_uses_the_connection_mac_policy() {
    for explicit_mac in [false, true] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let server_config = Arc::new(server::Config {
            keys: vec![key],
            preferred: russh::Preferred {
                cipher: Cow::Borrowed(&[russh::cipher::AES_128_CTR]),
                mac: Cow::Borrowed(&[mac::HMAC_SHA1]),
                ..Default::default()
            },
            ..Default::default()
        });
        let server_task = tokio::spawn(async move {
            let mut sessions = tokio::task::JoinSet::new();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let config = server_config.clone();
                sessions.spawn(async move {
                    if let Ok(session) = server::run_stream(config, stream, LegacyServer).await {
                        let _ = session.await;
                    }
                });
            }
        });
        let mut config = SshConfig::password("127.0.0.1", port, "test", "unused");
        config.legacy_ssh_compatibility = !explicit_mac;
        if explicit_mac {
            config.ssh_algorithms.mac = vec!["hmac-sha1".into()];
        }
        config.strict_host_key_checking = false;
        config.trust_host_key = Some(false);
        config.expected_host_key_fingerprint = Some(fingerprint.clone());

        let modern_status = oxideterm_ssh::check_host_key(&config.host, port, 5).await;
        assert!(
            matches!(modern_status, HostKeyStatus::Error { message } if message.contains("MAC")),
            "default preflight must not enable SHA-1 implicitly"
        );
        let status = check_host_key_with_route(
            &config.host,
            port,
            5,
            None,
            None,
            config.legacy_ssh_compatibility,
            &config.ssh_algorithms,
        )
        .await;
        let connection = tokio::time::timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config).test_connection(),
        )
        .await
        .unwrap();
        server_task.abort();
        let _ = server_task.await;
        connection.expect("the configured connection must support the server MAC");
        // A developer's known_hosts may already contain a loopback key. Both statuses
        // prove negotiation reached the actual key without bypassing trust verification.
        let captured_fingerprint = match status {
            HostKeyStatus::Unknown { fingerprint, .. } => fingerprint,
            HostKeyStatus::Changed {
                actual_fingerprint, ..
            } => actual_fingerprint,
            other => panic!("preflight did not capture the server key: {other:?}"),
        };
        assert_eq!(captured_fingerprint, fingerprint);
    }
}

struct LegacyRsaPeer;

impl server::Handler for LegacyRsaPeer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(
            if user == "fixture-user" && password == "fixture-password" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: russh::ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"legacy-rsa-ready".to_vec())
    }
}

#[tokio::test]
async fn explicit_legacy_rsa_policy_reaches_a_node_owned_password_shell() {
    use oxideterm_ssh::{ConnectionConsumer, SshConnectionRegistry};

    let key = russh::keys::PrivateKey::random(
        &mut rand10::rng(),
        russh::keys::Algorithm::Rsa { hash: None },
    )
    .unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    for kex in [russh::kex::DH_GEX_SHA1, russh::kex::DH_G1_SHA1] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_config = Arc::new(server::Config {
            keys: vec![key.clone()],
            preferred: russh::Preferred {
                key: Cow::Owned(vec![russh::keys::Algorithm::Rsa { hash: None }]),
                kex: Cow::Owned(vec![kex]),
                cipher: Cow::Borrowed(&[russh::cipher::AES_256_CTR]),
                mac: Cow::Borrowed(&[mac::HMAC_SHA256]),
                ..Default::default()
            },
            ..Default::default()
        });
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            if let Ok(session) = server::run_stream(server_config, stream, LegacyRsaPeer).await {
                let _ = session.await;
            }
        });
        let mut config = SshConfig::password("127.0.0.1", port, "fixture-user", "fixture-password");
        config.legacy_ssh_compatibility = true;
        config.expected_host_key_fingerprint = Some(fingerprint.clone());
        config.ssh_algorithms.kex = vec![kex.as_ref().to_owned()];
        config.ssh_algorithms.host_key = vec!["ssh-rsa".into()];
        config.ssh_algorithms.cipher = vec!["aes256-ctr".into()];
        config.ssh_algorithms.mac = vec!["hmac-sha2-256".into()];
        let registry = SshConnectionRegistry::default();
        let outcome = tokio::time::timeout(Duration::from_secs(20), async {
            let node = SshTransportClient::new(config)
                .connect_node_with_registry(
                    registry.clone(),
                    ConnectionConsumer::NodeRouter("legacy-rsa".into()),
                )
                .await?;
            let mut pty = SshTransportClient::connect_shell_on_existing_connection(
                registry,
                node.connection_id().to_owned(),
                ConnectionConsumer::Terminal("legacy-rsa".into()),
                80,
                24,
            )
            .await?;
            let mut output = Vec::new();
            while output.len() < b"legacy-rsa-ready".len() {
                match pty.output_rx.try_recv() {
                    Ok(chunk) => output.extend_from_slice(&chunk),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        tokio::time::sleep(Duration::from_millis(10)).await
                    }
                    Err(error) => panic!("legacy RSA shell closed: {error}"),
                }
            }
            assert_eq!(output, b"legacy-rsa-ready");
            Ok::<(), oxideterm_ssh::SshTransportError>(())
        })
        .await;
        server.abort();
        let _ = server.await;
        outcome
            .expect("legacy handshake and shell must finish")
            .unwrap();
    }
}
