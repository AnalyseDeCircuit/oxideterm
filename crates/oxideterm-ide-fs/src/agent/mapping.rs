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

fn map_agent_error(error: AgentError) -> IdeFileError {
    let message = error.to_string();
    let kind = match &error {
        AgentError::Timeout(_) => IdeFileErrorKind::Timeout,
        AgentError::Rpc { code: -2, .. } => IdeFileErrorKind::NotFound,
        AgentError::Rpc { code: -3, .. } => IdeFileErrorKind::PermissionDenied,
        AgentError::Rpc { message, .. } if message.contains("CONFLICT") => {
            IdeFileErrorKind::Conflict
        }
        AgentError::ChannelClosed | AgentError::Ssh(_) | AgentError::Route(_) => {
            IdeFileErrorKind::Disconnected
        }
        AgentError::UnsupportedArch(_) | AgentError::BinaryNotFound(_) => {
            IdeFileErrorKind::Unsupported
        }
        AgentError::Serialize(_)
        | AgentError::Deserialize(_)
        | AgentError::Rpc { .. }
        | AgentError::LocalIo(_)
        | AgentError::Sftp(_)
        | AgentError::Handshake(_) => IdeFileErrorKind::Other,
    };
    IdeFileError::new(kind, message)
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

