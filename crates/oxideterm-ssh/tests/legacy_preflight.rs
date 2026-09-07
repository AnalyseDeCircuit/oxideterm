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
