#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_agent_entries_to_core_file_kinds() {
        let node_id = NodeId::new("node-1");
        let entry = FileEntry {
            name: "src".to_string(),
            path: "/repo/src".to_string(),
            file_type: "directory".to_string(),
            is_symlink: false,
            symlink_target: None,
            target_file_type: None,
            size: 0,
            mtime: Some(12),
            permissions: None,
            children: None,
            truncated: false,
        };

        let mapped = file_tree_entry_from_agent(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::Directory);
        assert_eq!(mapped.location, IdeLocation::remote("node-1", "/repo/src"));
    }

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
        assert!(is_agent_conflict(&AgentRpcError {
            code: -4,
            message: "File modified externally".to_string(),
        }));
        assert!(is_agent_conflict(&AgentRpcError {
            code: -1,
            message: "hash mismatch".to_string(),
        }));
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
    fn maps_sftp_entries_like_tauri_file_info() {
        let node_id = NodeId::new("node-1");
        let entry = FileInfo {
            name: "main.rs".to_string(),
            path: "/repo/main.rs".to_string(),
            file_type: FileType::File,
            size: 128,
            modified: 7,
            permissions: "644".to_string(),
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        };

        let mapped = file_tree_entry_from_sftp(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::File);
        assert_eq!(mapped.version.modified_millis, Some(7000));
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
    fn agent_remote_path_matches_tauri_status_path() {
        assert_eq!(remote_agent_path(), "~/.oxideterm/oxideterm-agent");
        assert_eq!(
            shell_path_arg(&remote_agent_path()),
            "~/'.oxideterm/oxideterm-agent'"
        );
        assert_eq!(
            shell_path_arg("~/agent dir/oxide'term-agent"),
            "~/'agent dir/oxide'\\''term-agent'"
        );
        assert_eq!(
            shell_path_arg("/home/me/.oxideterm/oxideterm-agent"),
            "'/home/me/.oxideterm/oxideterm-agent'"
        );
    }

    #[test]
    fn agent_deploy_error_labels_match_tauri_bootstrap_classes() {
        assert_eq!(
            AgentError::ArchDetection("boom".into()).to_string(),
            "Architecture detection failed: boom"
        );
        assert_eq!(
            AgentError::Upload("denied".into()).to_string(),
            "Upload failed: denied"
        );
        assert_eq!(
            AgentError::ExecFailed("bad shell".into()).to_string(),
            "Command execution failed: bad shell"
        );
        assert_eq!(
            AgentError::StartFailed("missing libc".into()).to_string(),
            "Agent start failed: missing libc"
        );
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
    }

    #[test]
    fn permission_and_path_errors_map_to_tauri_ide_classes() {
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

        let info = handle.info();
        assert!(info.consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));

        fs.release_ide_consumer("node-ide");
        let info = handle.info();
        assert!(!info.consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));
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
        assert!(first.info().consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));

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
        assert!(!first.info().consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));
        assert!(second.info().consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));

        fs.close_ide_session("node-ide");
        assert!(!second.info().consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));
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

        assert!(!parent.info().consumers.contains(&ConnectionConsumer::Ide(
            "target".to_string()
        )));
        assert!(child.info().consumers.contains(&ConnectionConsumer::Ide(
            "target".to_string()
        )));
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
        assert!(info
            .consumers
            .contains(&ConnectionConsumer::Ide("node-ide".to_string())));
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
        assert!(child
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide("target".to_string())));

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
        assert!(!child
            .info()
            .consumers
            .contains(&ConnectionConsumer::Ide("target".to_string())));
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

        assert!(!handle.info().consumers.contains(&ConnectionConsumer::Ide(
            "node-ide".to_string()
        )));
        assert!(matches!(
            router.acquire_connection(&node_id, ConnectionConsumer::Ide("node-ide".to_string())),
            Err(RouteError::NotConnected(_))
        ));
    }
}
