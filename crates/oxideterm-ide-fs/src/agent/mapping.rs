fn remote_location(location: &IdeLocation) -> Result<(NodeId, String), IdeFileError> {
    match location {
        IdeLocation::Remote { node_id, path } => Ok((NodeId::new(node_id.clone()), path.clone())),
        IdeLocation::Local { .. } => Err(IdeFileError::new(
            IdeFileErrorKind::Unsupported,
            "Node agent IDE filesystem cannot read local locations",
        )),
    }
}

fn ide_file_data_from_agent(result: ReadFileResult) -> IdeFileData {
    IdeFileData {
        text: result.content,
        version: SavedFileVersion {
            size_bytes: Some(result.size),
            modified_millis: Some(result.mtime as i64),
            etag: Some(result.hash),
        },
    }
}

fn version_from_agent_write(result: &WriteFileResult) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: Some(result.size),
        modified_millis: Some(result.mtime as i64),
        etag: Some(result.hash.clone()),
    }
}

fn version_from_agent_stat(stat: &StatResult) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: stat.size,
        modified_millis: stat.mtime.map(|mtime| mtime as i64),
        etag: None,
    }
}

fn file_tree_entry_from_agent(node_id: &NodeId, entry: FileEntry) -> FileTreeEntry {
    let kind = match (entry.file_type.as_str(), entry.target_file_type.as_deref()) {
        ("directory" | "dir", _) => FileKind::Directory,
        ("file", _) => FileKind::File,
        ("symlink", Some("directory" | "dir")) => FileKind::Directory,
        ("symlink", _) => FileKind::Symlink,
        _ => FileKind::Other,
    };
    FileTreeEntry {
        location: IdeLocation::remote(node_id.0.clone(), entry.path),
        kind,
        name: entry.name,
        version: SavedFileVersion {
            size_bytes: Some(entry.size),
            modified_millis: entry.mtime.map(|mtime| mtime as i64),
            etag: None,
        },
    }
}

#[cfg(test)]
fn is_agent_conflict(error: &AgentRpcError) -> bool {
    is_agent_conflict_parts(error.code, &error.message)
}

fn is_agent_conflict_parts(code: i32, message: &str) -> bool {
    code == -4
        || message.contains("CONFLICT")
        || message.contains("hash mismatch")
        || message.contains("modified externally")
}

fn ide_error_from_agent_error(error: AgentError) -> IdeFileError {
    match error {
        AgentError::Rpc { code, message } if is_agent_conflict_parts(code, &message) => {
            IdeFileError::new(IdeFileErrorKind::Conflict, message)
        }
        AgentError::Rpc { message, .. } => ide_error_from_agent_message(message),
        AgentError::Timeout(timeout) => IdeFileError::new(
            IdeFileErrorKind::Timeout,
            format!("Agent RPC timeout after {timeout}s"),
        ),
        AgentError::ChannelClosed => {
            IdeFileError::new(IdeFileErrorKind::Disconnected, "Agent channel closed")
        }
        AgentError::Route(message)
        | AgentError::Ssh(message)
        | AgentError::Sftp(message)
        | AgentError::Upload(message)
        | AgentError::ExecFailed(message)
        | AgentError::StartFailed(message)
        | AgentError::Handshake(message)
        | AgentError::ArchDetection(message)
        | AgentError::LocalIo(message)
        | AgentError::Serialize(message)
        | AgentError::Deserialize(message)
        | AgentError::UnsupportedArch(message)
        | AgentError::BinaryNotFound(message) => ide_error_from_agent_message(message),
    }
}

fn agent_error_log_label(error: &AgentError) -> &'static str {
    match error {
        AgentError::Rpc { code, message } if is_agent_conflict_parts(*code, message) => "conflict",
        AgentError::Rpc { .. } => "rpc",
        AgentError::Timeout(_) => "timeout",
        AgentError::ChannelClosed => "channel_closed",
        AgentError::Route(_) => "route",
        AgentError::Ssh(_) => "ssh",
        AgentError::Sftp(_) => "sftp",
        AgentError::Upload(_) => "upload",
        AgentError::UnsupportedArch(_) => "unsupported_arch",
        AgentError::BinaryNotFound(_) => "binary_not_found",
        AgentError::LocalIo(_) => "local_io",
        AgentError::ExecFailed(_) => "exec_failed",
        AgentError::StartFailed(_) => "start_failed",
        AgentError::Handshake(_) => "handshake",
        AgentError::ArchDetection(_) => "arch_detection",
        AgentError::Serialize(_) => "serialize",
        AgentError::Deserialize(_) => "deserialize",
    }
}

fn ide_error_from_agent_message(message: impl Into<String>) -> IdeFileError {
    let message = message.into();
    IdeFileError::new(ide_kind_from_backend_class(classify_message(&message)), message)
}

fn ide_kind_from_backend_class(classification: BackendErrorClass) -> IdeFileErrorKind {
    match classification {
        BackendErrorClass::Conflict => IdeFileErrorKind::Conflict,
        BackendErrorClass::Cancelled | BackendErrorClass::Disconnected => {
            IdeFileErrorKind::Disconnected
        }
        BackendErrorClass::NotFound => IdeFileErrorKind::NotFound,
        BackendErrorClass::PermissionDenied | BackendErrorClass::Auth => {
            IdeFileErrorKind::PermissionDenied
        }
        BackendErrorClass::Timeout => IdeFileErrorKind::Timeout,
        BackendErrorClass::Unsupported | BackendErrorClass::HostKey => IdeFileErrorKind::Unsupported,
        BackendErrorClass::PortInUse | BackendErrorClass::Other => IdeFileErrorKind::Other,
    }
}

fn should_write_via_agent(expected_version: Option<&SavedFileVersion>) -> bool {
    // Tauri only uses the agent optimistic-lock path when the tab was opened
    // with an agent hash. Buffers opened through SFTP carry only mtime/size and
    // must keep SFTP's stat-before-write conflict check even if an agent later
    // becomes available. `None` is the explicit conflict-overwrite path.
    expected_version.is_none() || expected_version.and_then(|version| version.etag.as_ref()).is_some()
}

#[cfg(test)]
fn file_tree_entry_from_sftp(node_id: &NodeId, entry: FileInfo) -> FileTreeEntry {
    FileTreeEntry {
        location: IdeLocation::remote(node_id.0.clone(), entry.path),
        kind: match entry.file_type {
            FileType::File => FileKind::File,
            FileType::Directory => FileKind::Directory,
            FileType::Symlink => FileKind::Symlink,
            FileType::Unknown => FileKind::Other,
        },
        name: entry.name,
        version: SavedFileVersion {
            size_bytes: Some(entry.size),
            modified_millis: (entry.modified > 0).then_some(entry.modified * 1000),
            etag: None,
        },
    }
}
