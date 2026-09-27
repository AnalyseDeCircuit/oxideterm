use oxideterm_audit::*;
use oxideterm_ssh::{SshConfig, SshTransportClient};
use russh::{keys, server};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

struct Keys;
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![7; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}
type FileCalls = Arc<std::sync::Mutex<Vec<(u32, String)>>>;
struct Peer {
    interactive: bool,
    generation: u32,
    files: FileCalls,
    channels: std::collections::HashMap<russh::ChannelId, russh::Channel<server::Msg>>,
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(
            if !self.interactive && user == "audit-user" && password == "fixture-auth-secret" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
    }
    async fn channel_open_session(
        &mut self,
        channel: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
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
        session.channel_success(channel)
    }
    async fn subsystem_request(
        &mut self,
        channel: russh::ChannelId,
        name: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            return session.channel_failure(channel);
        }
        session.channel_success(channel)?;
        let stream = self.channels.remove(&channel).unwrap().into_stream();
        russh_sftp::server::run(
            stream,
            FilePeer {
                generation: self.generation,
                calls: self.files.clone(),
            },
        )
        .await;
        Ok(())
    }
    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _: &str,
        _: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<server::Auth, Self::Error> {
        if !self.interactive {
            return Ok(server::Auth::reject());
        }
        Ok(match response {
            None => server::Auth::Partial {
                name: "sensitive-challenge-name".into(),
                instructions: "sensitive-challenge-instructions".into(),
                prompts: vec![("sensitive-challenge-prompt".into(), false)].into(),
            },
            Some(mut responses) => {
                if responses.next().as_deref() == Some(b"fixture-otp-secret".as_slice()) {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                }
            }
        })
    }
}

struct FilePeer {
    generation: u32,
    calls: FileCalls,
}
impl russh_sftp::server::Handler for FilePeer {
    type Error = russh_sftp::protocol::StatusCode;
    fn unimplemented(&self) -> Self::Error {
        russh_sftp::protocol::StatusCode::OpUnsupported
    }
    async fn realpath(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Name, Self::Error> {
        Ok(russh_sftp::protocol::Name {
            id,
            files: vec![russh_sftp::protocol::File::dummy(if path == "." {
                "/fixture".into()
            } else {
                path
            })],
        })
    }
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        self.calls.lock().unwrap().push((self.generation, path));
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: russh_sftp::protocol::StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}

async fn start_peer(
    interactive: bool,
) -> (SshConfig, String, tokio::task::JoinHandle<()>, FileCalls) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let key = keys::PrivateKey::random(&mut rand10::rng(), keys::Algorithm::Ed25519).unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(keys::HashAlg::Sha256)
        .to_string();
    let calls: FileCalls = Default::default();
    let files = calls.clone();
    let server = tokio::spawn(async move {
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::ZERO,
            ..Default::default()
        });
        let mut connections = tokio::task::JoinSet::new();
        let mut generation = 0;
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            generation += 1;
            let config = config.clone();
            let peer = Peer {
                interactive,
                generation,
                files: files.clone(),
                channels: Default::default(),
            };
            connections.spawn(async move {
                if let Ok(connection) = server::run_stream(config, socket, peer).await {
                    let _ = connection.await;
                }
            });
        }
    });
    let mut config = SshConfig::password("127.0.0.1", port, "audit-user", "fixture-auth-secret");
    config.trust_host_key = Some(false);
    config.expected_host_key_fingerprint = Some(fingerprint.clone());
    (config, fingerprint, server, calls)
}

#[tokio::test]
async fn protocol_authentication_audit_distinguishes_success_rejection_and_host_key_denial() {
    for (password, key_matches, expected_auth) in [
        ("fixture-auth-secret", true, Some(AuditOutcome::Succeeded)),
        ("incorrect-auth-secret", true, Some(AuditOutcome::Failed)),
        ("fixture-auth-secret", false, None),
    ] {
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let (mut config, fingerprint, server, _) = start_peer(false).await;
        let port = config.port;
        config.auth = oxideterm_ssh::AuthMethod::password(password);
        config.trust_host_key = Some(false);
        config.expected_host_key_fingerprint = Some(if key_matches {
            fingerprint.clone()
        } else {
            "SHA256:not-the-server-key".into()
        });
        let target = format!("audit-user@127.0.0.1:{port}");
        let context =
            AuditContext::new(service.client(), AuditSource::User).session("ssh", &target);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config)
                .with_audit_context(Some(context.clone()))
                .test_connection(),
        )
        .await
        .unwrap();
        assert_eq!(
            result.is_ok(),
            expected_auth == Some(AuditOutcome::Succeeded)
        );
        server.abort();
        let _ = server.await;
        let page = service
            .client()
            .query(AuditQuery {
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let mut expected = if let Some(outcome) = expected_auth {
            vec![
                ("ssh_authenticate", outcome),
                ("ssh_authenticate", AuditOutcome::Started),
                ("ssh_host_key", AuditOutcome::Succeeded),
                ("ssh_host_key", AuditOutcome::Started),
            ]
        } else {
            vec![
                ("ssh_host_key", AuditOutcome::Denied),
                ("ssh_host_key", AuditOutcome::Started),
            ]
        };
        expected.insert(
            0,
            (
                "ssh_connect",
                if result.is_ok() {
                    AuditOutcome::Succeeded
                } else {
                    AuditOutcome::Failed
                },
            ),
        );
        expected.push(("ssh_connect", AuditOutcome::Started));
        let connection_operation = &page.records[0].details.operation.as_ref().unwrap().id;
        assert_eq!(
            page.records
                .iter()
                .filter(|record| record.details.operation.as_ref().unwrap().action
                    != "ssh_auth_attempt")
                .map(|record| {
                    let op = record.details.operation.as_ref().unwrap();
                    assert_eq!(op.session_id, context.session_id);
                    assert_eq!(op.source, AuditSource::User);
                    if op.action != "ssh_connect" {
                        assert_eq!(op.parent_id.as_ref(), Some(connection_operation));
                    }
                    assert_eq!(record.details.target.as_ref().unwrap().as_str(), target);
                    (op.action.as_str(), op.outcome)
                })
                .collect::<Vec<_>>(),
            expected
        );
        let auth_parent = page.records.iter().find_map(|record| {
            let op = record.details.operation.as_ref().unwrap();
            (op.action == "ssh_authenticate").then_some(op.id.as_str())
        });
        let attempts = page
            .records
            .iter()
            .rev()
            .filter_map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                if op.action != "ssh_auth_attempt" || op.phase != Some(AuditPhase::Result) {
                    return None;
                }
                assert_eq!(op.parent_id.as_deref(), auth_parent);
                let detail: serde_json::Value =
                    serde_json::from_str(record.details.detail.as_ref().unwrap()).unwrap();
                Some((
                    detail["method"].as_str().unwrap().to_string(),
                    detail["sequence"].as_u64().unwrap(),
                    op.outcome,
                ))
            })
            .collect::<Vec<_>>();
        let mut expected_attempts = if key_matches {
            vec![("none".to_string(), 1, AuditOutcome::Denied)]
        } else {
            vec![]
        };
        if let Some(outcome) = expected_auth {
            expected_attempts.push((
                "password".to_string(),
                2,
                if outcome == AuditOutcome::Succeeded {
                    outcome
                } else {
                    AuditOutcome::Denied
                },
            ));
            if outcome == AuditOutcome::Failed {
                expected_attempts.push(("password".to_string(), 3, AuditOutcome::Denied));
                expected_attempts.push((
                    "keyboard-interactive".to_string(),
                    4,
                    AuditOutcome::Denied,
                ));
            }
        }
        assert_eq!(attempts, expected_attempts);
        let serialized = serde_json::to_string(&page.records).unwrap();
        assert!(
            !serialized.contains("fixture-auth-secret")
                && !serialized.contains("incorrect-auth-secret")
        );
        assert!(serialized.contains(&fingerprint));
    }
}

struct Prompt {
    cancel: bool,
}
impl oxideterm_ssh::SshPromptHandler for Prompt {
    fn keyboard_interactive(
        &self,
        request: oxideterm_ssh::KeyboardInteractivePromptRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        oxideterm_ssh::KeyboardInteractiveResponses,
                        oxideterm_ssh::SshPromptError,
                    >,
                > + Send
                + '_,
        >,
    > {
        assert_eq!(request.prompts[0].prompt, "sensitive-challenge-prompt");
        Box::pin(async move {
            if self.cancel {
                Err(oxideterm_ssh::SshPromptError::Cancelled)
            } else {
                Ok(Zeroizing::new(vec!["fixture-otp-secret".into()]))
            }
        })
    }
}

#[tokio::test]
async fn interactive_audit_tracks_challenge_response_and_user_cancellation_without_prompt_content()
{
    for cancel in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let (mut config, _, server, _) = start_peer(true).await;
        config.auth = oxideterm_ssh::AuthMethod::KeyboardInteractive;
        let context = AuditContext::new(service.client(), AuditSource::User)
            .session("ssh", "interactive-fixture");
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config)
                .with_audit_context(Some(context))
                .with_prompt_handler(Arc::new(Prompt { cancel }))
                .test_connection(),
        )
        .await
        .unwrap();
        assert_eq!(result.is_ok(), !cancel);
        server.abort();
        let _ = server.await;
        let page = service
            .client()
            .query(AuditQuery {
                limit: 30,
                ..Default::default()
            })
            .await
            .unwrap();
        let outcomes = page
            .records
            .iter()
            .rev()
            .filter_map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                if op.phase != Some(AuditPhase::Result)
                    || !matches!(op.action.as_str(), "ssh_auth_attempt" | "ssh_auth_prompt")
                {
                    return None;
                }
                let detail: serde_json::Value =
                    serde_json::from_str(record.details.detail.as_ref().unwrap()).unwrap();
                Some((
                    op.action.as_str(),
                    detail["request"].as_str().unwrap_or("prompt").to_string(),
                    op.outcome,
                ))
            })
            .collect::<Vec<_>>();
        let mut expected = vec![
            (
                "ssh_auth_attempt",
                "authenticate".to_string(),
                AuditOutcome::Denied,
            ),
            ("ssh_auth_attempt", "start".to_string(), AuditOutcome::Sent),
            (
                "ssh_auth_prompt",
                "prompt".to_string(),
                if cancel {
                    AuditOutcome::Cancelled
                } else {
                    AuditOutcome::Sent
                },
            ),
        ];
        if !cancel {
            expected.push((
                "ssh_auth_attempt",
                "response".to_string(),
                AuditOutcome::Succeeded,
            ));
        }
        assert_eq!(outcomes, expected);
        let serialized = serde_json::to_string(&page.records).unwrap();
        for secret in [
            "sensitive-challenge-name",
            "sensitive-challenge-instructions",
            "sensitive-challenge-prompt",
            "fixture-otp-secret",
        ] {
            assert!(!serialized.contains(secret));
        }
    }
}

#[tokio::test]
async fn old_terminal_and_sftp_keep_their_physical_transport_after_registry_rebind() {
    use oxideterm_ssh::{ConnectionConsumer, SshConnectionRegistry, SshTransportCommand};
    let directory = tempfile::tempdir().unwrap();
    oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
        .unwrap()
        .set_policy(oxideterm_audit::AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let service = AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
    let registry = SshConnectionRegistry::with_audit(
        Default::default(),
        Some(AuditContext::new(service.client(), AuditSource::User)),
    );
    let (config, _, server, calls) = start_peer(false).await;
    let old_terminal = SshTransportClient::new(config.clone())
        .connect_shell_with_registry(registry.clone(), ConnectionConsumer::Terminal("old".into()))
        .await
        .unwrap();
    let files_consumer = ConnectionConsumer::Sftp("files".into());
    let connection = registry.acquire(config.clone(), files_consumer.clone());
    let router = oxideterm_ssh::NodeRouter::new(registry.clone());
    let node = oxideterm_ssh::NodeId::new("logical-node");
    router.upsert_node(node.clone(), config.clone());
    router
        .bind_connection(&node, connection.connection_id().to_string())
        .unwrap();
    let logical_session = router.audit_context(&node).unwrap().session_id.unwrap();
    let old_files = router.acquire_transfer_sftp(&node).await.unwrap();
    let old_context = old_terminal.audit_context().unwrap();
    let old_transport = old_context.transport_id.as_ref().unwrap();
    connection.clear_physical().await;
    let new_terminal = SshTransportClient::new(config.clone())
        .connect_shell_with_registry(registry.clone(), ConnectionConsumer::Terminal("new".into()))
        .await
        .unwrap();
    let new_files = router.acquire_transfer_sftp(&node).await.unwrap();
    assert_eq!(
        router.audit_context(&node).unwrap().session_id.as_deref(),
        Some(logical_session.as_str())
    );
    router.remove_runtime_subtree(&node);
    router.upsert_node(node.clone(), config);
    assert_ne!(
        router.audit_context(&node).unwrap().session_id.as_deref(),
        Some(logical_session.as_str())
    );
    let new_context = new_terminal.audit_context().unwrap();
    let new_transport = new_context.transport_id.as_ref().unwrap();
    assert_ne!(old_transport, new_transport);
    assert_eq!(old_context.session_id, new_context.session_id);
    assert_eq!(old_context.connection_id, new_context.connection_id);
    assert_eq!(
        old_terminal.audit_context().unwrap().transport_id.as_ref(),
        Some(old_transport)
    );
    old_files.mkdir("/old-channel").await.unwrap();
    new_files.mkdir("/new-channel").await.unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [(1, "/old-channel".into()), (2, "/new-channel".into())]
    );
    let page = service
        .client()
        .query(AuditQuery {
            category: Some(AuditCategory::File),
            limit: 20,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                assert_eq!(op.session_id.as_deref(), Some(logical_session.as_str()));
                assert_eq!(
                    record.details.node_id.as_ref().unwrap().as_str(),
                    "logical-node"
                );
                (
                    record.details.detail.as_ref().unwrap().as_str(),
                    op.transport_id.as_ref().unwrap().as_str(),
                    op.outcome,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "/new-channel",
                new_transport.as_str(),
                AuditOutcome::Succeeded
            ),
            (
                "/new-channel",
                new_transport.as_str(),
                AuditOutcome::Started
            ),
            (
                "/old-channel",
                old_transport.as_str(),
                AuditOutcome::Succeeded
            ),
            (
                "/old-channel",
                old_transport.as_str(),
                AuditOutcome::Started
            ),
        ]
    );
    old_terminal
        .command_tx
        .send(SshTransportCommand::Close)
        .await
        .unwrap();
    new_terminal
        .command_tx
        .send(SshTransportCommand::Close)
        .await
        .unwrap();
    drop((old_files, new_files, old_terminal, new_terminal));
    registry.release(connection.connection_id(), &files_consumer);
    server.abort();
    let _ = server.await;
}
