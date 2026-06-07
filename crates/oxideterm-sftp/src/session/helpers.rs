fn classify_list_entry_file_type(
    entry_file_type: FileType,
    target_file_type: Option<FileType>,
) -> FileType {
    match entry_file_type {
        FileType::Symlink => match target_file_type {
            Some(FileType::Directory) => FileType::Directory,
            _ => FileType::Symlink,
        },
        other => other,
    }
}

fn file_type_from_attrs(metadata: &FileAttributes) -> FileType {
    if metadata.is_dir() {
        FileType::Directory
    } else if metadata.is_symlink() {
        FileType::Symlink
    } else if metadata.is_regular() {
        FileType::File
    } else {
        FileType::Unknown
    }
}

fn sort_entries(entries: &mut [FileInfo], order: SortOrder) {
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type == FileType::Directory;
        let b_is_dir = b.file_type == FileType::Directory;
        if a_is_dir != b_is_dir {
            return b_is_dir.cmp(&a_is_dir);
        }
        match order {
            SortOrder::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortOrder::NameDesc => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
            SortOrder::Size => a.size.cmp(&b.size),
            SortOrder::SizeDesc => b.size.cmp(&a.size),
            SortOrder::Modified => a.modified.cmp(&b.modified),
            SortOrder::ModifiedDesc => b.modified.cmp(&a.modified),
            SortOrder::Type => a.name.cmp(&b.name),
            SortOrder::TypeDesc => b.name.cmp(&a.name),
        }
    });
}

fn swap_path(canonical_path: &str) -> String {
    if let Some(slash_pos) = canonical_path.rfind('/') {
        let dir = &canonical_path[..=slash_pos];
        let name = &canonical_path[slash_pos + 1..];
        format!("{dir}.{name}.oxswp")
    } else {
        format!(".{canonical_path}.oxswp")
    }
}

async fn throttle_transfer(
    transferred: u64,
    started: Instant,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) {
    let Some(manager) = transfer_manager else {
        return;
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let expected = transferred as f64 / limit as f64;
    if expected > elapsed {
        tokio::time::sleep(std::time::Duration::from_secs_f64(expected - elapsed)).await;
    }
}

async fn check_transfer_control(
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    transfer_id: &str,
) -> Result<(), SftpError> {
    if let Some(manager) = transfer_manager {
        manager.check_control(transfer_id).await?;
    }
    Ok(())
}

async fn send_transfer_progress(
    progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    direction: TransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    started: Instant,
    state: TransferState,
    error: Option<String>,
) {
    let Some(tx) = progress_tx else {
        return;
    };
    let elapsed = started.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        (transferred_bytes as f64 / elapsed) as u64
    } else {
        0
    };
    let eta_seconds = if speed > 0 && total_bytes > transferred_bytes {
        Some(((total_bytes - transferred_bytes) as f64 / speed as f64) as u64)
    } else {
        None
    };
    let progress = TransferProgress {
        id: transfer_id.to_string(),
        remote_path: remote_path.to_string(),
        local_path: local_path.to_string(),
        direction,
        state,
        total_bytes,
        transferred_bytes,
        speed,
        eta_seconds,
        error,
    };

    if state == TransferState::InProgress {
        // Intermediate progress is lossy by design; the data plane must not wait
        // for a slow UI or persistence consumer while SFTP requests can keep flowing.
        let _ = tx.try_send(progress);
        return;
    }

    let _ = tx.send(progress).await;
}

fn is_missing_file_error_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
}
