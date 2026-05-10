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
        let transport = AgentTransport {
            write_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
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
}
