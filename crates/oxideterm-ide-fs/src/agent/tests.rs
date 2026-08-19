#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_agent_symlink_directories_as_directories() {
        let node_id = NodeId::new("node-1");
        let entry = FileEntry {
            name: "current".to_string(),
            path: "/repo/current".to_string(),
            file_type: "symlink".to_string(),
            is_symlink: true,
            symlink_target: Some("/repo/releases/current".to_string()),
            target_file_type: Some("directory".to_string()),
            size: 0,
            mtime: Some(12),
            permissions: None,
            children: None,
            truncated: false,
        };

        let mapped = file_tree_entry_from_agent(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::Directory);
        assert_eq!(
            mapped.location,
            IdeLocation::remote("node-1", "/repo/current")
        );
    }

    #[test]
    fn recognizes_agent_write_conflicts() {
        assert!(is_agent_conflict_parts(-4, "File modified externally"));
        assert!(is_agent_conflict_parts(-1, "hash mismatch"));
    }

    #[test]
    fn sftp_opened_buffer_keeps_sftp_conflict_detection_when_agent_appears() {
        let sftp_version = SavedFileVersion {
            size_bytes: Some(3),
            modified_millis: Some(1000),
            etag: None,
        };
        let agent_version = SavedFileVersion {
            size_bytes: Some(3),
            modified_millis: Some(1000),
            etag: Some("hash".to_string()),
        };

        assert!(!should_write_via_agent(Some(&sftp_version)));
        assert!(should_write_via_agent(Some(&agent_version)));
        assert!(should_write_via_agent(None));
    }

    #[test]
    fn drops_agent_registry_without_tokio_reactor() {
        let registry = AgentRegistry::default();
        let (write_tx, _write_rx) = mpsc::channel::<String>(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);
        let (watch_tx, _) = broadcast::channel::<AgentWatchEvent>(1);
        let transport = AgentTransport {
            write_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            watch_tx,
            shutdown_tx,
            alive: Arc::new(AtomicBool::new(false)),
        };
        registry.register(
            "conn-1".to_string(),
            AgentSession::new(
                transport,
                SysInfoResult {
                    version: "0.12.1".to_string(),
                    compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    arch: "x86_64".to_string(),
                    os: "linux".to_string(),
                    pid: 42,
                    capabilities: Vec::new(),
                },
            ),
        );

        drop(registry);
    }

    #[test]
    fn parses_remote_agent_version_like_tauri() {
        assert_eq!(
            parse_remote_version_output("NOT_FOUND"),
            RemoteAgentInstallState::Missing
        );
        assert_eq!(
            parse_remote_version_output(&format!(
                "oxideterm-agent 0.12.1 compat {CURRENT_AGENT_COMPATIBILITY_VERSION}"
            )),
            RemoteAgentInstallState::Current
        );
        assert_eq!(
            parse_remote_version_output("oxideterm-agent 0.12.1 compat abc"),
            RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
                version: "0.12.1".to_string(),
                compatibility_version: INVALID_AGENT_COMPATIBILITY_VERSION,
            })
        );
    }

    #[test]
    fn resolves_encoded_appimage_agent_payload() {
        let temp_dir = unique_agent_test_dir("encoded-resolve");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let encoded_path = temp_dir.join("oxideterm-agent-aarch64-linux-musl.b64");
        std::fs::write(&encoded_path, "encoded").unwrap();

        let resolved = resolve_agent_binary_in_dirs(
            "oxideterm-agent-aarch64-linux-musl",
            vec![temp_dir.clone()],
        )
        .unwrap();

        assert_eq!(resolved, encoded_path);
        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn reads_encoded_appimage_agent_payload_as_binary() {
        use base64::Engine as _;

        let temp_dir = unique_agent_test_dir("encoded-read");
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let encoded_path = temp_dir.join("oxideterm-agent-x86_64-linux-musl.b64");
        let agent_bytes = b"\x7fELF bundled remote agent";
        let encoded = base64::engine::general_purpose::STANDARD.encode(agent_bytes);
        tokio::fs::write(&encoded_path, encoded).await.unwrap();

        let decoded = read_agent_binary_payload(&encoded_path).await.unwrap();

        assert_eq!(decoded, agent_bytes);
        tokio::fs::remove_dir_all(temp_dir).await.unwrap();
    }

    #[test]
    fn agent_errors_keep_remote_fs_error_classes() {
        assert_eq!(
            ide_error_from_agent_message("permission denied: /repo/secret").kind,
            IdeFileErrorKind::PermissionDenied
        );
        assert_eq!(
            ide_error_from_agent_message("ENOENT: /repo/missing").kind,
            IdeFileErrorKind::NotFound
        );
        assert_eq!(
            ide_error_from_agent_error(AgentError::ChannelClosed).kind,
            IdeFileErrorKind::Disconnected
        );
        assert_eq!(
            ide_error_from_agent_error(AgentError::Timeout(30)).kind,
            IdeFileErrorKind::Timeout
        );

        for message in [
            "permission denied: /repo/secret",
            "EACCES: cannot open /repo/secret",
            "operation not permitted: /repo/secret",
        ] {
            assert_eq!(
                ide_error_from_agent_message(message).kind,
                IdeFileErrorKind::PermissionDenied
            );
        }

        for message in [
            "path not found: /repo/missing",
            "No such file or directory: /repo/missing",
            "ENOENT: /repo/missing",
        ] {
            assert_eq!(
                ide_error_from_agent_message(message).kind,
                IdeFileErrorKind::NotFound
            );
        }
    }

    #[test]
    fn agent_error_log_labels_do_not_include_remote_payloads() {
        assert_eq!(
            agent_error_log_label(&AgentError::Rpc {
                code: -32000,
                message: "permission denied: /srv/.env".to_string(),
            }),
            "rpc"
        );
        assert_eq!(
            agent_error_log_label(&AgentError::Ssh(
                "connection failed while running ~/.oxideterm/oxideterm-agent".to_string(),
            )),
            "ssh"
        );
    }

    fn unique_agent_test_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "oxideterm-agent-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn routes_agent_watch_notifications_to_receiver() {
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (watch_tx, mut watch_rx) = broadcast::channel::<AgentWatchEvent>(1);
        let mut second_watch_rx = watch_tx.subscribe();

        handle_agent_line(
            &pending,
            &watch_tx,
            r#"{"method":"watch/event","params":{"path":"/srv/app/main.rs","kind":"modified"}}"#,
        )
        .await;

        let event = watch_rx.recv().await.unwrap();
        assert_eq!(event.path, "/srv/app/main.rs");
        assert_eq!(event.kind, "modified");
        let second_event = second_watch_rx.recv().await.unwrap();
        assert_eq!(second_event.path, "/srv/app/main.rs");
        assert_eq!(second_event.kind, "modified");
    }

    #[tokio::test]
    async fn watch_dispatcher_stops_when_subscription_owner_is_released() {
        let key = IdeWatchKey::new("node-watch", "/srv/app");
        let shared = Arc::new(IdeWatchShared::new("connection-watch".to_string()));
        let (agent_events_tx, agent_events_rx) = broadcast::channel::<AgentWatchEvent>(4);
        let mut subscription = IdeWatchSubscription {
            rx: shared.events_tx.subscribe(),
        };
        shared.start_dispatcher(key, agent_events_rx);

        agent_events_tx
            .send(AgentWatchEvent {
                path: "/srv/app/main.rs".to_string(),
                kind: "modified".to_string(),
            })
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), subscription.recv())
            .await
            .expect("watch dispatcher should deliver before cancellation")
            .expect("watch subscription should remain open");
        assert_eq!(event.path, "/srv/app/main.rs");

        shared.shutdown().await;
        assert!(
            shared
                .dispatcher_task
                .lock()
                .expect("IDE watch dispatcher task poisoned")
                .is_none()
        );
        drop(shared);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), subscription.recv())
                .await
                .expect("released watch subscription should close"),
            None
        );
    }

    #[tokio::test]
    async fn watch_stop_completes_remotely_before_owner_release() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-watch-stop");
        let config = oxideterm_ssh::SshConfig::password("watch-host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());
        let handle = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(
            handle.connection_id(),
            oxideterm_ssh::ConnectionState::Active,
        );
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (write_tx, mut write_rx) = mpsc::channel::<String>(1);
        let (watch_tx, _) = broadcast::channel::<AgentWatchEvent>(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);
        fs.registry.register(
            handle.connection_id().to_string(),
            AgentSession::new(
                AgentTransport {
                    write_tx,
                    pending: pending.clone(),
                    watch_tx: watch_tx.clone(),
                    shutdown_tx,
                    alive: Arc::new(AtomicBool::new(true)),
                },
                SysInfoResult {
                    version: "0.12.1".to_string(),
                    compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    arch: "x86_64".to_string(),
                    os: "linux".to_string(),
                    pid: 42,
                    capabilities: Vec::new(),
                },
            ),
        );
        let watch_key = IdeWatchKey::new(node_id.0.clone(), "/srv/app");
        let owned_watch_key = fs.owned_watch_key(watch_key.clone());
        let shared = Arc::new(IdeWatchShared::new(handle.connection_id().to_string()));
        let (_agent_event_tx, agent_event_rx) = broadcast::channel::<AgentWatchEvent>(1);
        shared.start_dispatcher(watch_key, agent_event_rx);
        fs.watch_subscriptions
            .insert(owned_watch_key.clone(), shared.clone());

        let response_task = tokio::spawn(async move {
            let request = write_rx.recv().await.expect("watch/stop request");
            let request: serde_json::Value =
                serde_json::from_str(&request).expect("valid agent request");
            assert_eq!(request["method"], "watch/stop");
            assert_eq!(request["params"]["path"], "/srv/app");
            let response = serde_json::json!({
                "id": request["id"],
                "result": {}
            })
            .to_string();
            handle_agent_line(&pending, &watch_tx, &response).await;
        });

        fs.stop_watch_directory(&node_id.0, "/srv/app")
            .await
            .unwrap();
        response_task.await.unwrap();
        assert!(!fs.watch_subscriptions.contains_key(&owned_watch_key));
        assert!(
            shared
                .dispatcher_task
                .lock()
                .expect("IDE watch dispatcher task poisoned")
                .is_none()
        );
        assert!(has_ide_consumer(&handle, &node_id.0));

        fs.release_ide_session_for_node(&node_id.0);
        assert!(!has_ide_consumer(&handle, &node_id.0));
    }

    #[tokio::test]
    async fn releasing_one_node_cancels_only_its_watch_dispatchers() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);
        let first_key = IdeWatchKey::new("node-watch-first", "/srv/first");
        let second_key = IdeWatchKey::new("node-watch-second", "/srv/second");
        let first_owned_key = fs.owned_watch_key(first_key.clone());
        let second_owned_key = fs.owned_watch_key(second_key.clone());
        let first_shared = Arc::new(IdeWatchShared::new("connection-first".to_string()));
        let second_shared = Arc::new(IdeWatchShared::new("connection-second".to_string()));
        let (_first_tx, first_rx) = broadcast::channel::<AgentWatchEvent>(1);
        let (second_tx, second_rx) = broadcast::channel::<AgentWatchEvent>(1);
        first_shared.start_dispatcher(first_key.clone(), first_rx);
        second_shared.start_dispatcher(second_key.clone(), second_rx);
        let mut second_subscription = IdeWatchSubscription {
            rx: second_shared.events_tx.subscribe(),
        };
        fs.watch_subscriptions
            .insert(first_owned_key.clone(), first_shared.clone());
        fs.watch_subscriptions
            .insert(second_owned_key.clone(), second_shared.clone());

        fs.release_ide_session_for_node("node-watch-first");

        assert!(!fs.watch_subscriptions.contains_key(&first_owned_key));
        assert!(fs.watch_subscriptions.contains_key(&second_owned_key));
        assert!(
            first_shared
                .dispatcher_task
                .lock()
                .expect("IDE watch dispatcher task poisoned")
                .is_none()
        );
        second_tx
            .send(AgentWatchEvent {
                path: "/srv/second/lib.rs".to_string(),
                kind: "modified".to_string(),
            })
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), second_subscription.recv())
                .await
                .expect("other node dispatcher should remain responsive")
                .is_some()
        );

        fs.release_ide_session_for_node("node-watch-second");
        assert!(!fs.watch_subscriptions.contains_key(&second_owned_key));
    }

    #[test]
    fn parses_exec_grep_output_like_tauri_search_fallback() {
        let matches = parse_grep_output(
            "./src/main.rs:12:let needle = true;\nREADME.md:2:Needle again\n",
            "needle",
            false,
        );

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, "src/main.rs");
        assert_eq!(matches[0].line, 12);
        assert_eq!(matches[0].match_start, 4);
        assert_eq!(matches[1].path, "README.md");
    }

    #[test]
    fn grep_fallback_escapes_query_and_home_cwd_like_tauri() {
        assert_eq!(regex_escape_for_basic_grep("a+b[0]"), "a\\+b\\[0\\]");
        assert_eq!(shell_cd_arg("~"), "~");
        assert_eq!(shell_cd_arg("~/my repo"), "~/'my repo'");
        assert_eq!(shell_cd_arg("/srv/my repo"), "'/srv/my repo'");
    }

    fn has_ide_consumer(handle: &SshConnectionHandle, node_id: &str) -> bool {
        ide_consumer_count(handle, node_id) > 0
    }

    fn ide_consumer_count(handle: &SshConnectionHandle, node_id: &str) -> usize {
        let session_prefix = format!("{node_id}:");
        handle
            .info()
            .consumers
            .iter()
            .filter(|consumer| {
                matches!(
                    consumer,
                    ConnectionConsumer::Ide(consumer_id)
                        if consumer_id == node_id || consumer_id.starts_with(&session_prefix)
                )
            })
            .count()
    }

    #[tokio::test]
    async fn ide_session_acquisition_registers_and_releases_ide_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide");
        let config = oxideterm_ssh::SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let handle = registry.acquire(
            config.clone(),
            oxideterm_ssh::ConnectionConsumer::NodeRouter("node-ide".to_string()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();

        assert!(has_ide_consumer(&handle, "node-ide"));

        fs.release_ide_consumer("node-ide");
        assert!(!has_ide_consumer(&handle, "node-ide"));
    }

    #[tokio::test]
    async fn released_session_rejects_late_connection_acquisition() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide-late");
        let config = oxideterm_ssh::SshConfig::password("late-host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let handle = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(
            handle.connection_id(),
            oxideterm_ssh::ConnectionState::Connecting,
        );
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let released_session = fs.ide_session_for_node(&node_id);
        let released_acquisition = tokio::spawn({
            let released_session = released_session.clone();
            async move { released_session.acquire_connection().await }
        });
        // Let the first acquisition enter NodeRouter's readiness wait before
        // invalidating its owner, then create the replacement node session.
        tokio::time::sleep(Duration::from_millis(30)).await;
        fs.release_ide_session_for_node(&node_id.0);
        let current_session = fs.ide_session_for_node(&node_id);
        let current_consumer = current_session.consumer.clone();
        let current_acquisition = tokio::spawn({
            let current_session = current_session.clone();
            async move { current_session.acquire_connection().await }
        });

        registry.mark_state(
            handle.connection_id(),
            oxideterm_ssh::ConnectionState::Active,
        );
        assert!(matches!(
            released_acquisition.await.unwrap(),
            Err(RouteError::NotConnected(_))
        ));
        current_acquisition.await.unwrap().unwrap();

        let info = handle.info();
        assert!(!info.consumers.contains(&released_session.consumer));
        assert!(info.consumers.contains(&current_consumer));
        fs.release_ide_session_for_node(&node_id.0);
        assert!(!has_ide_consumer(&handle, &node_id.0));
    }

    #[tokio::test]
    async fn same_node_owners_release_sessions_and_watches_independently() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-shared-owner");
        let config = oxideterm_ssh::SshConfig::password("shared-host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());
        let handle = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(
            handle.connection_id(),
            oxideterm_ssh::ConnectionState::Active,
        );
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let ai_owner = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        let surface_owner = ai_owner.scoped_owner();
        ai_owner.ensure_ide_session_for_node(&node_id).await.unwrap();
        surface_owner
            .ensure_ide_session_for_node(&node_id)
            .await
            .unwrap();
        assert_eq!(ide_consumer_count(&handle, &node_id.0), 2);

        let watch_key = IdeWatchKey::new(node_id.0.clone(), "/srv/app");
        let ai_watch_key = ai_owner.owned_watch_key(watch_key.clone());
        let surface_watch_key = surface_owner.owned_watch_key(watch_key.clone());
        let shared_watch = Arc::new(IdeWatchShared::new(handle.connection_id().to_string()));
        let (_ai_events_tx, ai_events_rx) = broadcast::channel::<AgentWatchEvent>(1);
        shared_watch.start_dispatcher(watch_key, ai_events_rx);
        ai_owner
            .watch_subscriptions
            .insert(ai_watch_key.clone(), shared_watch.clone());
        surface_owner
            .watch_subscriptions
            .insert(surface_watch_key.clone(), shared_watch.clone());

        surface_owner.release_ide_session_for_node(&node_id.0);

        assert_eq!(ide_consumer_count(&handle, &node_id.0), 1);
        assert!(ai_owner.watch_subscriptions.contains_key(&ai_watch_key));
        assert!(
            !surface_owner
                .watch_subscriptions
                .contains_key(&surface_watch_key)
        );
        assert!(
            shared_watch
                .dispatcher_task
                .lock()
                .expect("IDE watch dispatcher task poisoned")
                .is_some()
        );
        assert!(
            ai_owner
                .ensure_ide_session_for_node(&node_id)
                .await
                .is_ok()
        );

        ai_owner.release_ide_session_for_node(&node_id.0);
        assert_eq!(ide_consumer_count(&handle, &node_id.0), 0);
        assert!(!ai_owner.watch_subscriptions.contains_key(&ai_watch_key));
        assert!(
            shared_watch
                .dispatcher_task
                .lock()
                .expect("IDE watch dispatcher task poisoned")
                .is_none()
        );
    }

    #[tokio::test]
    async fn releasing_one_ide_session_preserves_other_node_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let first_node = NodeId::new("node-ide-first");
        let second_node = NodeId::new("node-ide-second");
        let first_config =
            oxideterm_ssh::SshConfig::password("first-host", 22, "first-user", "pw");
        let second_config =
            oxideterm_ssh::SshConfig::password("second-host", 22, "second-user", "pw");
        router.upsert_node(first_node.clone(), first_config.clone());
        router.upsert_node(second_node.clone(), second_config.clone());

        let first_handle = registry.acquire(
            first_config,
            ConnectionConsumer::NodeRouter(first_node.0.clone()),
        );
        first_handle.set_physical(Arc::new(()));
        registry.mark_state(
            first_handle.connection_id(),
            oxideterm_ssh::ConnectionState::Active,
        );
        router
            .bind_connection(&first_node, first_handle.connection_id().to_string())
            .unwrap();

        let second_handle = registry.acquire(
            second_config,
            ConnectionConsumer::NodeRouter(second_node.0.clone()),
        );
        second_handle.set_physical(Arc::new(()));
        registry.mark_state(
            second_handle.connection_id(),
            oxideterm_ssh::ConnectionState::Active,
        );
        router
            .bind_connection(&second_node, second_handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&first_node).await.unwrap();
        fs.ensure_ide_session_for_node(&second_node).await.unwrap();

        fs.release_ide_session_for_node(&first_node.0);

        assert!(!has_ide_consumer(&first_handle, &first_node.0));
        assert!(has_ide_consumer(&second_handle, &second_node.0));
    }

    #[tokio::test]
    async fn release_all_ide_consumers_completes_and_releases_registered_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide-release-all");
        let config = oxideterm_ssh::SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let handle = registry.acquire(
            config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();
        assert!(has_ide_consumer(&handle, &node_id.0));

        let (release_finished_tx, release_finished_rx) = std::sync::mpsc::sync_channel(1);
        let fs_for_release = fs.clone();
        std::thread::spawn(move || {
            fs_for_release.release_all_ide_consumers();
            let _ = release_finished_tx.send(());
        });

        // A bounded wait turns a future DashMap lock-order regression into a
        // focused test failure instead of hanging the entire test process.
        let release_timeout = Duration::from_secs(1);
        release_finished_rx
            .recv_timeout(release_timeout)
            .expect("releasing all IDE consumers must not deadlock");
        assert!(fs.ide_sessions.is_empty());
        assert!(!has_ide_consumer(&handle, &node_id.0));
    }

    #[tokio::test]
    async fn stop_watch_without_active_ide_session_does_not_acquire_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);

        fs.stop_watch_directory("node-ide", "/srv/app")
            .await
            .unwrap();

        assert!(fs.ide_sessions.is_empty());
    }

    #[tokio::test]
    async fn ide_remote_session_rebind_releases_previous_connection_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide");
        let config = oxideterm_ssh::SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let first = registry.acquire(
            config.clone(),
            oxideterm_ssh::ConnectionConsumer::NodeRouter("node-ide".to_string()),
        );
        first.set_physical(Arc::new(()));
        registry.mark_state(first.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, first.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router.clone(), NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();
        assert!(has_ide_consumer(&first, "node-ide"));

        registry.mark_state(first.connection_id(), oxideterm_ssh::ConnectionState::LinkDown);
        let second_config = oxideterm_ssh::SshConfig::password("host2", 22, "me", "pw");
        router.upsert_node(node_id.clone(), second_config.clone());
        let second = registry.acquire(
            second_config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("node-ide".to_string()),
        );
        second.set_physical(Arc::new(()));
        registry.mark_state(second.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, second.connection_id().to_string())
            .unwrap();

        fs.ensure_ide_session_for_node(&node_id).await.unwrap();
        assert!(!has_ide_consumer(&first, "node-ide"));
        assert!(has_ide_consumer(&second, "node-ide"));

        fs.release_ide_session_for_node("node-ide");
        assert!(!has_ide_consumer(&second, "node-ide"));
    }

    #[test]
    fn agent_status_is_scoped_by_node_and_connection() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let first = NodeId::new("node-a");
        let second = NodeId::new("node-b");
        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Ask);

        fs.set_status_for_node(
            &first,
            Some("conn-a"),
            AgentStatus::Ready {
                version: "1.0.0".into(),
                arch: "x86_64".into(),
                pid: 7,
            },
        );
        fs.set_status_for_node(
            &second,
            Some("conn-b"),
            AgentStatus::Failed {
                reason: "boom".into(),
            },
        );

        assert!(matches!(
            fs.status_for_node(Some("node-a")),
            AgentStatus::Ready { version, .. } if version == "1.0.0"
        ));
        assert!(matches!(
            fs.status_for_node(Some("node-b")),
            AgentStatus::Failed { reason } if reason == "boom"
        ));

        fs.set_status_for_node(&first, Some("conn-a2"), AgentStatus::SftpFallback);
        assert_eq!(
            fs.status_for_node(Some("node-a")),
            AgentStatus::SftpFallback
        );
        assert!(fs.agent_statuses.contains_key(&AgentStatusKey {
            node_id: "node-a".into(),
            connection_id: "conn-a".into(),
        }));
    }

    #[tokio::test]
    async fn ide_session_on_proxy_child_consumes_child_connection() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = oxideterm_ssh::SshConfig::password("jump", 22, "me", "pw");
        let child_config = oxideterm_ssh::SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = registry.acquire(
            parent_config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("jump".to_string()),
        );
        parent.set_physical(Arc::new(()));
        registry.mark_state(parent.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&parent_id, parent.connection_id().to_string())
            .unwrap();

        let child = registry.acquire(
            child_config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("target".to_string()),
        );
        child.set_physical(Arc::new(()));
        registry.mark_state(child.connection_id(), oxideterm_ssh::ConnectionState::Active);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent.connection_id().to_string()),
        );
        router
            .bind_connection(&child_id, child.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&child_id).await.unwrap();

        assert!(!has_ide_consumer(&parent, "target"));
        assert!(has_ide_consumer(&child, "target"));
    }

    #[tokio::test]
    async fn terminal_consumer_release_does_not_kill_ide_remote_fs() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide");
        let config = oxideterm_ssh::SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let handle = registry.acquire(
            config.clone(),
            oxideterm_ssh::ConnectionConsumer::NodeRouter("node-ide".to_string()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let terminal_consumer = ConnectionConsumer::Terminal("term-a".to_string());
        let terminal = registry.acquire(config, terminal_consumer.clone());
        assert_eq!(terminal.connection_id(), handle.connection_id());

        let fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();

        registry.release(handle.connection_id(), &terminal_consumer);
        let info = handle.info();
        assert!(!info.consumers.contains(&terminal_consumer));
        assert!(has_ide_consumer(&handle, "node-ide"));
        assert_eq!(info.state, oxideterm_ssh::ConnectionState::Active);
    }

    #[tokio::test]
    async fn parent_link_down_interrupts_child_ide_and_release_cleans_consumer() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = oxideterm_ssh::SshConfig::password("jump", 22, "me", "pw");
        let child_config = oxideterm_ssh::SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = registry.acquire(
            parent_config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("jump".to_string()),
        );
        parent.set_physical(Arc::new(()));
        registry.mark_state(parent.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&parent_id, parent.connection_id().to_string())
            .unwrap();

        let child = registry.acquire(
            child_config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("target".to_string()),
        );
        child.set_physical(Arc::new(()));
        registry.mark_state(child.connection_id(), oxideterm_ssh::ConnectionState::Active);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent.connection_id().to_string()),
        );
        router
            .bind_connection(&child_id, child.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router.clone(), NodeAgentMode::Disabled);
        fs.ensure_ide_session_for_node(&child_id).await.unwrap();
        assert!(has_ide_consumer(&child, "target"));

        registry.mark_link_down_cascade(parent.connection_id());
        assert_eq!(parent.state(), oxideterm_ssh::ConnectionState::LinkDown);
        assert_eq!(child.state(), oxideterm_ssh::ConnectionState::LinkDown);
        assert!(matches!(
            router.acquire_connection(
                &child_id,
                ConnectionConsumer::Ide("target-reopen".to_string())
            ),
            Err(RouteError::NotConnected(_))
        ));
        assert!(!child
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide("target-reopen".to_string())));

        fs.release_ide_consumer("target");
        assert!(!has_ide_consumer(&child, "target"));
    }

    #[tokio::test]
    async fn manual_disconnect_cleanup_does_not_revive_ide_session() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("node-ide");
        let config = oxideterm_ssh::SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node_id.clone(), config.clone());

        let handle = registry.acquire(
            config,
            oxideterm_ssh::ConnectionConsumer::NodeRouter("node-ide".to_string()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), oxideterm_ssh::ConnectionState::Active);
        router
            .bind_connection(&node_id, handle.connection_id().to_string())
            .unwrap();

        let fs = NodeAgentIdeFileSystem::new(router.clone(), NodeAgentMode::Ask);
        fs.ensure_ide_session_for_node(&node_id).await.unwrap();
        fs.release_ide_consumer("node-ide");
        router
            .disconnect_node_runtime(&node_id, "manual disconnect")
            .unwrap();

        fs.stop_watch_directory("node-ide", "/srv/app")
            .await
            .unwrap();

        assert!(!has_ide_consumer(&handle, "node-ide"));
        assert!(matches!(
            router.acquire_connection(&node_id, ConnectionConsumer::Ide("node-ide".to_string())),
            Err(RouteError::NotConnected(_))
        ));
    }
}
