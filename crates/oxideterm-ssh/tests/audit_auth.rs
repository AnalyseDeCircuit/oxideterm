use oxideterm_audit::*;
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionState, NodeId, NodeReadiness, NodeRouter, NodeStateEvent,
    SshConfig, SshConnectionRegistry, SshTransportClient, SshTransportError,
};
#[path = "support/peer.rs"]
mod peer;
use peer::{SftpPeerEvent, start_peer, start_peer_with_sftp_events};
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
#[tokio::test]
async fn authentication_rejection_releases_the_pool_without_an_idle_retirement() {
    let (config, _, server, _) = start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    registry.set_idle_timeout(Some(Duration::ZERO));
    let router = NodeRouter::new(registry.clone());
    let node = NodeId::new("saved-node");
    let owner = ConnectionConsumer::NodeRouter(node.0.clone());
    let mut rejected = config.clone();
    rejected.auth = oxideterm_ssh::AuthMethod::password("incorrect-auth-secret");
    router.upsert_node(node.clone(), rejected.clone());
    let connection = registry.acquire(rejected.clone(), owner.clone());
    let failed_id = connection.connection_id().to_owned();
    router.bind_connection(&node, &failed_id).unwrap();
    let (sender, events) = std::sync::mpsc::channel();
    router.emitter().subscribe(sender);

    let result = SshTransportClient::new(rejected)
        .connect_existing_node_with_registry(registry.clone(), owner.clone(), connection)
        .await;
    assert!(matches!(
        result,
        Err(SshTransportError::AuthenticationFailed(_))
    ));
    assert!(registry.get(&failed_id).is_none());
    let states: Vec<_> = events
        .try_iter()
        .filter_map(|event| match event {
            NodeStateEvent::ConnectionStateChanged { node_id, state, .. } if node_id == node.0 => {
                Some(state)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states.last(), Some(&NodeReadiness::Error));

    let retried = SshTransportClient::new(config)
        .connect_node_with_registry(registry, owner)
        .await
        .unwrap();
    router
        .bind_connection(&node, retried.connection_id())
        .unwrap();
    assert_ne!(retried.connection_id(), failed_id);
    assert_eq!(retried.state(), ConnectionState::Active);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn shared_child_handshake_preserves_the_jump_lease_across_cancellation_and_rejection() {
    use std::future::Future;
    use std::task::{Context, Waker};

    let (jump_config, _, jump_server, _) = start_peer(false).await;
    let (target, fingerprint, target_server, writes) = start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    let jump_owner = ConnectionConsumer::NodeRouter("jump".into());
    let jump = SshTransportClient::new(jump_config)
        .connect_node_with_registry(registry.clone(), jump_owner.clone())
        .await
        .unwrap();
    let open = |label: &str, config: SshConfig| {
        let child_owner = ConnectionConsumer::NodeRouter(label.to_owned());
        let parent_lease = ConnectionConsumer::NodeRouter(format!("ancestor-{label}"));
        let child =
            registry.acquire_via_parent(config.clone(), child_owner.clone(), jump.connection_id());
        let parent = registry
            .acquire_consumer_for_connection(jump.connection_id(), parent_lease.clone())
            .unwrap();
        SshTransportClient::new(config).connect_child_node_via_parent_with_registry(
            registry.clone(),
            child_owner,
            child,
            parent,
            parent_lease,
        )
    };
    let mut first = Box::pin(open("first", target.clone()));
    // Poll both attempts before allowing the local peers to service the first handshake.
    let mut cx = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    {
        let mut cancelled = Box::pin(open("cancelled", target.clone()));
        assert!(cancelled.as_mut().poll(&mut cx).is_pending());
    }
    let (first, alias) = tokio::try_join!(first, open("alias", target.clone())).unwrap();
    assert_eq!(first.connection_id(), alias.connection_id());

    let mut rejected = target.clone();
    rejected.username = "rejected-user".into();
    assert!(matches!(
        open("rejected", rejected).await,
        Err(SshTransportError::AuthenticationFailed(_))
    ));
    let mut changed_key = target.clone();
    changed_key.expected_host_key_fingerprint = Some("SHA256:different-host".into());
    assert!(matches!(
        open("changed-key", changed_key).await,
        Err(SshTransportError::HostKeyChanged { expected_fingerprint, actual_fingerprint, .. })
            if expected_fingerprint == "SHA256:different-host" && actual_fingerprint == fingerprint
    ));
    registry.release(jump.connection_id(), &jump_owner);
    assert_eq!(
        jump.info().ref_count,
        1,
        "only the shared child still needs the jump"
    );
    alias
        .acquire_transfer_sftp()
        .await
        .unwrap()
        .mkdir("/after-rejection")
        .await
        .unwrap();
    // Two denied handshakes between the shared connection and the direct login
    // must leave the original channel on the first physical connection.
    let direct = SshTransportClient::new(target)
        .connect_node_with_registry(
            registry.clone(),
            ConnectionConsumer::NodeRouter("direct".into()),
        )
        .await
        .unwrap();
    direct
        .acquire_transfer_sftp()
        .await
        .unwrap()
        .mkdir("/direct")
        .await
        .unwrap();
    assert_eq!(
        *writes.lock().unwrap(),
        [(1, "/after-rejection".into()), (4, "/direct".into())]
    );

    registry.retire_connection(first.connection_id()).unwrap();
    assert!(jump.info().consumers.is_empty());
    jump_server.abort();
    target_server.abort();
    let _ = tokio::join!(jump_server, target_server);
}

#[tokio::test]
async fn shared_socket_close_gets_a_new_id_before_the_old_failure_is_reported() {
    use oxideterm_ssh::ConnectionTransportStatus;

    let (config, _, server, writes) = start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    let router = NodeRouter::new(registry.clone());
    let node = NodeId::new("reconnect-before-heartbeat");
    let owner = ConnectionConsumer::NodeRouter(node.0.clone());
    router.upsert_node(node.clone(), config.clone());
    let old = SshTransportClient::new(config.clone())
        .connect_node_with_registry(registry.clone(), owner.clone())
        .await
        .unwrap();
    router.bind_connection(&node, old.connection_id()).unwrap();
    let pending_consumer = ConnectionConsumer::Terminal("pending".into());
    let pending = registry.acquire(config.clone(), pending_consumer.clone());
    assert_eq!(pending.connection_id(), old.connection_id());
    let _ = old
        .run_command("close-fixture-transport", Duration::from_secs(2), 64)
        .await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while old.transport_status().await != ConnectionTransportStatus::Closed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(old.state(), ConnectionState::Active);
    assert!(
        router
            .bind_established_connection(&node, old.connection_id())
            .is_err()
    );

    let replacement = SshTransportClient::new(config.clone())
        .connect_node_with_registry(registry.clone(), owner)
        .await
        .unwrap();
    assert_ne!(replacement.connection_id(), old.connection_id());
    let _handoff = router
        .bind_established_connection(&node, replacement.connection_id())
        .unwrap();
    registry
        .mark_transport_lost_cascade(old.connection_id(), "late terminal failure")
        .await;
    assert!(matches!(
        SshTransportClient::new(config)
            .connect_existing_node_with_registry(registry.clone(), pending_consumer, pending)
            .await,
        Err(SshTransportError::ConnectionFailed(_))
    ));
    assert!(registry.get(old.connection_id()).is_none());
    let current = router.resolve_connection_now(&node).unwrap();
    assert_eq!(current.connection_id, replacement.connection_id());
    current
        .handle
        .acquire_transfer_sftp()
        .await
        .unwrap()
        .mkdir("/after-late-failure")
        .await
        .unwrap();
    assert_eq!(*writes.lock().unwrap(), [(2, "/after-late-failure".into())]);
    let _ = replacement
        .run_command("close-fixture-transport", Duration::from_secs(2), 64)
        .await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while replacement.transport_status().await != ConnectionTransportStatus::Closed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (sender, events) = std::sync::mpsc::channel();
    router.emitter().subscribe(sender);
    assert!(router.resolve_connection_now(&node).is_err());
    assert!(events.try_iter().any(|event| matches!(event, NodeStateEvent::ConnectionStatusChanged { connection_id, status, .. } if connection_id == replacement.connection_id() && status == "link_down")));

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn shared_link_down_reconnect_preserves_the_old_transport_until_its_last_release() {
    let (config, _, server, writes) = start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    registry.set_idle_timeout(None);
    let owner = ConnectionConsumer::NodeRouter("existing".into());
    let old = SshTransportClient::new(config.clone())
        .connect_node_with_registry(registry.clone(), owner.clone())
        .await
        .unwrap();
    let existing_channel = old.acquire_transfer_sftp().await.unwrap();
    let pending_owner = ConnectionConsumer::NodeRouter("pending".into());
    let pending = registry.acquire(config.clone(), pending_owner.clone());
    registry.mark_state_without_event(old.connection_id(), ConnectionState::LinkDown);
    assert!(matches!(
        SshTransportClient::new(config.clone())
            .connect_existing_node_with_registry(registry.clone(), pending_owner, pending)
            .await,
        Err(SshTransportError::ConnectionFailed(_))
    ));
    let restored = SshTransportClient::new(config.clone())
        .connect_node_with_registry(
            registry.clone(),
            ConnectionConsumer::NodeRouter("reconnecting".into()),
        )
        .await
        .unwrap();
    assert_eq!(restored.state(), ConnectionState::Active);
    existing_channel.mkdir("/still-open").await.unwrap();
    drop(existing_channel);
    registry.release(old.connection_id(), &owner);
    assert!(registry.get(old.connection_id()).is_none());
    let peer = SshTransportClient::new(config)
        .connect_node_with_registry(
            registry.clone(),
            ConnectionConsumer::NodeRouter("new-peer".into()),
        )
        .await
        .unwrap();
    assert_eq!(peer.connection_id(), restored.connection_id());
    peer.acquire_transfer_sftp()
        .await
        .unwrap()
        .mkdir("/reconnected")
        .await
        .unwrap();
    assert_eq!(
        *writes.lock().unwrap(),
        [(1, "/still-open".into()), (2, "/reconnected".into())]
    );
    server.abort();
    let _ = server.await;
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

#[derive(Default)]
struct PasswordPrompt {
    remember: bool,
    answer: Option<&'static str>,
    interactive_answer: Option<&'static str>,
    saved: Arc<std::sync::Mutex<Vec<(String, u16, String, Zeroizing<String>)>>>,
    completed: std::sync::Mutex<Vec<bool>>,
}

impl oxideterm_ssh::SshPromptHandler for PasswordPrompt {
    fn authentication_completed(&self, configured_credentials_confirmed: bool) {
        self.completed
            .lock()
            .unwrap()
            .push(configured_credentials_confirmed);
    }
    fn keyboard_interactive(
        &self,
        _: oxideterm_ssh::KeyboardInteractivePromptRequest,
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
        Box::pin(async move {
            self.interactive_answer
                .map(|answer| Zeroizing::new(vec![answer.into()]))
                .ok_or(oxideterm_ssh::SshPromptError::Cancelled)
        })
    }

    fn password(
        &self,
        prompt: oxideterm_ssh::SshPasswordPrompt,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        oxideterm_ssh::SshPasswordResponse,
                        oxideterm_ssh::SshPromptError,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let answer = self
                .answer
                .ok_or(oxideterm_ssh::SshPromptError::Cancelled)?;
            let saved = self.saved.clone();
            let on_authenticated: Option<Box<dyn FnOnce(Zeroizing<String>) + Send>> =
                self.remember.then(|| {
                    Box::new(move |password| {
                        saved.lock().unwrap().push((
                            prompt.host,
                            prompt.port,
                            prompt.username,
                            password,
                        ));
                    }) as Box<dyn FnOnce(Zeroizing<String>) + Send>
                });
            Ok(oxideterm_ssh::SshPasswordResponse {
                password: Zeroizing::new(answer.into()),
                on_authenticated,
            })
        })
    }
}

#[tokio::test]
async fn configured_credentials_are_confirmed_only_when_the_server_accepts_them() {
    for (password, username, authenticated, expected_receipts) in [
        ("fixture-auth-secret", "audit-user", true, vec![true]),
        ("incorrect-auth-secret", "audit-user", false, vec![]),
        (
            "fixture-auth-secret",
            "password-fallback-user",
            true,
            vec![true],
        ),
        (
            "incorrect-auth-secret",
            "otp-fallback-user",
            true,
            vec![false],
        ),
    ] {
        let (mut config, _, server, _) = start_peer(username != "audit-user").await;
        config.username = username.into();
        config.auth = oxideterm_ssh::AuthMethod::password(password);
        let prompt = Arc::new(PasswordPrompt {
            interactive_answer: Some("fixture-otp-secret"),
            ..Default::default()
        });
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config)
                .with_prompt_handler(prompt.clone())
                .test_connection(),
        )
        .await
        .unwrap();
        assert_eq!(result.is_ok(), authenticated, "username={username}");
        assert_eq!(
            *prompt.completed.lock().unwrap(),
            expected_receipts,
            "username={username}"
        );
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn prompted_password_is_saved_only_after_successful_authentication_and_explicit_consent() {
    for (answer, remember, authenticated, username) in [
        (Some("fixture-auth-secret"), true, true, "audit-user"),
        (Some("fixture-auth-secret"), false, true, "audit-user"),
        (Some("incorrect-auth-secret"), true, false, "audit-user"),
        (None, true, false, "audit-user"),
        (
            Some("fixture-auth-secret"),
            true,
            true,
            "password-fallback-user",
        ),
        (
            Some("incorrect-auth-secret"),
            true,
            true,
            "otp-fallback-user",
        ),
    ] {
        let interactive = username != "audit-user";
        let (mut config, _, server, _) = start_peer(interactive).await;
        config.username = username.into();
        config.auth = oxideterm_ssh::AuthMethod::password_prompt();
        let expected_endpoint = (config.host.clone(), config.port, config.username.clone());
        let prompt = Arc::new(PasswordPrompt {
            answer,
            remember,
            interactive_answer: interactive.then_some("fixture-otp-secret"),
            ..Default::default()
        });
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config)
                .with_prompt_handler(prompt.clone())
                .test_connection(),
        )
        .await
        .unwrap();
        assert_eq!(result.is_ok(), authenticated);
        let expected = if authenticated && remember && answer == Some("fixture-auth-secret") {
            vec![(
                expected_endpoint.0,
                expected_endpoint.1,
                expected_endpoint.2,
                Zeroizing::new("fixture-auth-secret".into()),
            )]
        } else {
            vec![]
        };
        assert_eq!(*prompt.saved.lock().unwrap(), expected);
        server.abort();
        let _ = server.await;
    }
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
async fn shared_sftp_waiters_recover_after_cancellation_and_invalidation() {
    use std::task::{Context, Waker};

    async fn next_home(
        events: &mut tokio::sync::mpsc::UnboundedReceiver<SftpPeerEvent>,
    ) -> tokio::sync::oneshot::Sender<String> {
        let Some(SftpPeerEvent::ResolveHome(reply)) = events.recv().await else {
            panic!("SSH peer disconnected before opening SFTP");
        };
        reply
    }

    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let (config, _, server, _) = start_peer_with_sftp_events(false, Some(tx)).await;
    let registry = SshConnectionRegistry::default();
    let owner = SshTransportClient::new(config)
        .connect_dedicated_consumer_with_registry(
            registry.clone(),
            ConnectionConsumer::Sftp("page".into()),
            None,
        )
        .await
        .unwrap();
    let scenario = async {
        let handle = owner.handle();
        let open = || {
            let handle = handle.clone();
            tokio::spawn(async move { handle.acquire_sftp_with_meta().await.unwrap() })
        };
        let first = open();
        let first_reply = next_home(&mut events).await;
        let mut waiter = Box::pin(handle.acquire_sftp_with_meta());
        assert!(
            waiter
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        first.abort();
        assert!(matches!(first.await, Err(error) if error.is_cancelled()));
        let _ = first_reply.send("/abandoned".into());
        let ((), recovered) = tokio::join!(
            async {
                next_home(&mut events)
                    .await
                    .send("/after-cancel".into())
                    .unwrap()
            },
            waiter
        );
        let recovered = recovered.unwrap();
        assert_eq!(recovered.cwd.as_deref(), Some("/after-cancel"));
        let previous_generation = recovered.generation;
        drop(recovered);

        handle.clear_sftp().await;
        let stale = open();
        let late_reply = next_home(&mut events).await;
        assert!(handle.invalidate_sftp().await);
        let replacement = open();
        let new_reply = next_home(&mut events).await;
        let _ = late_reply.send("/obsolete".into());
        new_reply.send("/current".into()).unwrap();
        let (stale, replacement) = tokio::try_join!(stale, replacement).unwrap();
        assert_eq!(stale.cwd.as_deref(), Some("/current"));
        assert_eq!(replacement.cwd.as_deref(), Some("/current"));
        assert_ne!(stale.was_new, replacement.was_new);
        assert!(Arc::ptr_eq(&stale.session, &replacement.session));
        assert!(
            handle
                .acquire_existing_sftp_generation(previous_generation)
                .await
                .is_none()
        );
        drop((stale, replacement));
        drop(owner);
        assert_eq!(registry.monitor_stats().total_sftp_sessions, 0);
        assert!(matches!(
            events.recv().await,
            Some(SftpPeerEvent::Disconnected)
        ));
    };
    tokio::time::timeout(Duration::from_secs(10), scenario)
        .await
        .expect("SFTP initialization or final SSH disconnect stalled");
    server.abort();
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
    let router = oxideterm_ssh::NodeRouter::new(registry.clone());
    let node = oxideterm_ssh::NodeId::new("logical-node");
    router.upsert_node(node.clone(), config.clone());
    let logical_session = router.audit_context(&node).unwrap().session_id.unwrap();
    let node_owner = ConnectionConsumer::NodeRouter(node.0.clone());
    let open_terminal = async |label: &str| {
        let handle = registry.acquire(config.clone(), node_owner.clone());
        let connection_id = handle.connection_id().to_owned();
        let mut context = router.audit_context(&node).unwrap();
        context.connection_id = Some(Zeroizing::new(connection_id.clone()));
        context
            .scope(
                SshTransportClient::new(config.clone()).establish_node_transport(
                    &registry,
                    &node_owner,
                    &handle,
                    None,
                ),
            )
            .await
            .unwrap();
        // The node binds each authenticated transport before terminal and SFTP consumers attach.
        let _handoff = router
            .bind_established_connection(&node, &connection_id)
            .unwrap();
        let terminal = SshTransportClient::connect_shell_on_existing_connection(
            registry.clone(),
            connection_id,
            ConnectionConsumer::Terminal(label.into()),
            80,
            24,
        )
        .await
        .unwrap();
        (handle, terminal)
    };
    let (old_connection, old_terminal) = open_terminal("old").await;
    let old_files = router.acquire_transfer_sftp(&node).await.unwrap();
    let old_context = old_terminal.audit_context().unwrap();
    let old_transport = old_context.transport_id.as_ref().unwrap();
    old_connection.clear_physical().await;
    let (new_connection, new_terminal) = open_terminal("new").await;
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
    assert_eq!(
        old_context.session_id.as_deref(),
        Some(logical_session.as_str())
    );
    assert_eq!(
        new_context.session_id.as_deref(),
        Some(logical_session.as_str())
    );
    assert_ne!(
        old_connection.connection_id(),
        new_connection.connection_id()
    );
    assert_eq!(
        old_context.connection_id.as_deref().map(String::as_str),
        Some(old_connection.connection_id())
    );
    assert_eq!(
        new_context.connection_id.as_deref().map(String::as_str),
        Some(new_connection.connection_id())
    );
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
    registry.release(new_connection.connection_id(), &node_owner);
    server.abort();
    let _ = server.await;
}
