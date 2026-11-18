
/// Executes a v2 live read after the UI broker resolved an opaque owner
/// handle to this exact node. No target identifier crosses this boundary.
async fn read_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    node_id: NodeId,
    sftp_owner: Option<crate::workspace::ai_runtime_context::AiSftpRuntimeOwner>,
    args: &serde_json::Value,
    ide_file_system: Option<oxideterm_ide_fs::NodeAgentIdeFileSystem>,
    post_user_approval: bool,
) -> AiActionResultLite {
    let resource = args
        .get("resource")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let Some(path) = args
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return snapshot.fail(
            "Resource path is required.",
            "missing_path",
            "read_resource requires path for file or directory resources.",
            "read",
        );
    };

    if resource == "ide" {
        let Some(ide_file_system) = ide_file_system else {
            return snapshot.fail(
                "IDE capability is unavailable.",
                "runtime_capability_unavailable",
                "Rediscover the current IDE workspace before reading a file.",
                "read",
            );
        };
        if let Ok(result) = ide_file_system
            .node_agent_read_file(node_id.0.clone(), path)
            .await
        {
            let data = serde_json::json!({
                "path": path,
                "content": result.content,
                "hash": result.hash,
                "contentHash": result.hash,
                "size": result.size,
                "mtime": result.mtime,
                "encoding": result.encoding,
                "source": "ide-surface-agent",
            });
            return snapshot.ok(
                format!("Read IDE file {path}."),
                truncate_for_model(
                    data.get("content")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    12_000,
                ),
                data,
                "read",
            );
        }
        return snapshot.fail(
            "IDE file read failed.",
            "resource_disconnected",
            "The current IDE file owner is unavailable. Rediscover it before retrying.",
            "read",
        );
    }

    let Some(sftp_owner) = sftp_owner else {
        return snapshot.fail(
            "SFTP capability is unavailable.",
            "runtime_capability_unavailable",
            "Rediscover the current SFTP session before reading.",
            "read",
        );
    };
    let shared = match services
        .node_router
        .acquire_existing_sftp_generation(
            &sftp_owner.node_id,
            &sftp_owner.connection_id,
            sftp_owner.session_generation,
        )
        .await
    {
        Ok(shared) => shared,
        Err(_) => {
            return snapshot.fail(
                "SFTP session changed before the read.",
                if post_user_approval {
                    "runtime_state_changed_after_approval"
                } else {
                    "runtime_owner_replaced"
                },
                ai_runtime_validation_recovery_message(post_user_approval),
                "read",
            );
        }
    };
    let result = {
        let sftp = shared.lock().await;
        if matches!(resource, "directory" | "sftp") {
            sftp.list_dir(
                path,
                Some(oxideterm_sftp::ListFilter {
                    show_hidden: true,
                    pattern: None,
                    sort: oxideterm_sftp::SortOrder::Name,
                }),
            )
            .await
            .map(|entries| serde_json::json!(entries))
        } else {
            sftp.preview(path)
                .await
                .map(|preview| serde_json::json!(preview))
        }
    };
    match result {
        Ok(data) => {
            let output = truncate_for_model(
                serde_json::to_string_pretty(&data).unwrap_or_default(),
                12_000,
            );
            snapshot.ok(
                if matches!(resource, "directory" | "sftp") {
                    format!(
                        "Listed {} entries.",
                        data.as_array().map(Vec::len).unwrap_or(0)
                    )
                } else {
                    format!("Read remote file preview {path}.")
                },
                output,
                data,
                "read",
            )
        }
        Err(error) if error.is_channel_recoverable() => {
            // Rebuild may prepare a future owner, but this authorized call
            // must fail instead of transparently switching generations.
            let _ = services
                .node_router
                .invalidate_and_reacquire_sftp(&sftp_owner.node_id)
                .await;
            snapshot.fail(
                "SFTP session changed during the read.",
                if post_user_approval {
                    "runtime_state_changed_after_approval"
                } else {
                    "resource_disconnected"
                },
                ai_runtime_validation_recovery_message(post_user_approval),
                "read",
            )
        }
        Err(error) => snapshot.fail(
            "Resource read failed.",
            "resource_read_failed",
            error.to_string(),
            "read",
        ),
    }
}

/// Executes a v2 live write after validating the exact SFTP or IDE owner.
async fn write_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    node_id: NodeId,
    sftp_owner: Option<crate::workspace::ai_runtime_context::AiSftpRuntimeOwner>,
    args: &serde_json::Value,
    ide_file_system: Option<oxideterm_ide_fs::NodeAgentIdeFileSystem>,
    post_user_approval: bool,
) -> AiActionResultLite {
    let resource = args
        .get("resource")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let Some(path) = args
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return snapshot.fail(
            "Path and content are required.",
            "missing_file_write_args",
            "write_resource(file) requires path and content.",
            "write",
        );
    };
    let Some(content) = args.get("content").and_then(serde_json::Value::as_str) else {
        return snapshot.fail(
            "Path and content are required.",
            "missing_file_write_args",
            "write_resource(file) requires path and content.",
            "write",
        );
    };
    if args
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return snapshot
            .ok(
                format!("Dry-run file write {path}."),
                "Dry-run only; file was not changed.",
                serde_json::Value::Null,
                "write",
            )
            .with_verified(false);
    }
    let expected_hash = args
        .get("expected_hash")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());

    if resource == "ide" {
        let Some(ide_file_system) = ide_file_system else {
            return snapshot.fail(
                "IDE capability is unavailable.",
                "runtime_capability_unavailable",
                "Rediscover the current IDE workspace before writing a file.",
                "write",
            );
        };
        if let Ok(result) = ide_file_system
            .node_agent_write_file(node_id.0.clone(), path, content, expected_hash)
            .await
        {
            let data = serde_json::json!({
                "path": path,
                "size": result.size,
                "mtime": result.mtime,
                "hash": result.hash,
                "contentHash": result.hash,
                "atomicWrite": result.atomic,
                "source": "ide-surface-agent",
            });
            return snapshot.ok(
                format!("Wrote IDE file {path}."),
                serde_json::to_string_pretty(&data).unwrap_or_else(|_| format!("{path} written.")),
                data,
                "write",
            );
        }
        return snapshot.fail(
            "IDE file write failed.",
            "resource_disconnected",
            "The current IDE file owner is unavailable. Rediscover it before retrying.",
            "write",
        );
    }

    let Some(sftp_owner) = sftp_owner else {
        return snapshot.fail(
            "SFTP capability is unavailable.",
            "runtime_capability_unavailable",
            "Rediscover the current SFTP session before writing.",
            "write",
        );
    };
    let write_result = write_remote_file(
        services,
        &sftp_owner,
        path,
        content,
        expected_hash,
    )
    .await;
    if matches!(
        &write_result,
        Err(AiRemoteFileWriteError::Sftp(error)) if error.is_channel_recoverable()
    ) {
        // The failed generation is invalidated, but this call never reuses
        // its approval against the replacement channel.
        let _ = services
            .node_router
            .invalidate_and_reacquire_sftp(&sftp_owner.node_id)
            .await;
        return snapshot.fail(
            "SFTP session changed during the write.",
            if post_user_approval {
                "runtime_state_changed_after_approval"
            } else {
                "resource_disconnected"
            },
            ai_runtime_validation_recovery_message(post_user_approval),
            "write",
        );
    }
    match write_result {
        Ok(data) => snapshot.ok(
            format!("Wrote remote file {path}."),
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| format!("{path} written.")),
            data,
            "write",
        ),
        Err(AiRemoteFileWriteError::OwnerReplaced) => snapshot.fail(
            "SFTP session changed before the write.",
            if post_user_approval {
                "runtime_state_changed_after_approval"
            } else {
                "runtime_owner_replaced"
            },
            ai_runtime_validation_recovery_message(post_user_approval),
            "write",
        ),
        Err(AiRemoteFileWriteError::ExpectedHashMismatch) => snapshot.fail(
            "Remote file changed before writing.",
            "expected_hash_mismatch",
            "File changed before writing. Read the current file before retrying.",
            "write",
        ),
        Err(AiRemoteFileWriteError::ExpectedFileMissing { path }) => snapshot.fail(
            "Cannot verify write precondition.",
            "expected_file_missing",
            format!("Cannot verify write precondition for {path}: file does not exist."),
            "write",
        ),
        Err(AiRemoteFileWriteError::ExistingFileNotText { path }) => snapshot.fail(
            "Cannot verify existing file.",
            "existing_file_not_text",
            format!("Cannot safely verify existing file {path}: it is not valid UTF-8 text."),
            "write",
        ),
        Err(AiRemoteFileWriteError::Sftp(error)) => snapshot.fail(
            "Remote file write failed.",
            "remote_file_write_failed",
            error.to_string(),
            "write",
        ),
    }
}

/// Starts a v2 SFTP transfer from an already validated concrete session.
async fn transfer_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    sftp_owner: crate::workspace::ai_runtime_context::AiSftpRuntimeOwner,
    args: &serde_json::Value,
    post_user_approval: bool,
) -> AiActionResultLite {
    let direction = args
        .get("direction")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !matches!(direction, "upload" | "download") {
        return snapshot.fail(
            "Transfer direction is required.",
            "missing_transfer_direction",
            "direction must be upload or download.",
            "write",
        );
    }
    let Some(source_path) = args
        .get("source_path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return snapshot.fail(
            "Transfer paths are required.",
            "missing_transfer_path",
            "transfer_resource requires source_path and destination_path.",
            "write",
        );
    };
    let Some(destination_path) = args
        .get("destination_path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return snapshot.fail(
            "Transfer paths are required.",
            "missing_transfer_path",
            "transfer_resource requires source_path and destination_path.",
            "write",
        );
    };
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let is_directory = ai_transfer_path_looks_directory(source_path)
        || ai_transfer_path_looks_directory(destination_path);
    let owner_session = match services
        .node_router
        .acquire_existing_sftp_generation(
            &sftp_owner.node_id,
            &sftp_owner.connection_id,
            sftp_owner.session_generation,
        )
        .await
    {
        Ok(session) => session,
        Err(_) => {
            return snapshot.fail(
                "SFTP session changed before transfer start.",
                if post_user_approval {
                    "runtime_state_changed_after_approval"
                } else {
                    "runtime_owner_replaced"
                },
                ai_runtime_validation_recovery_message(post_user_approval),
                "write",
            );
        }
    };
    match run_sftp_transfer(
        services,
        &sftp_owner,
        owner_session,
        direction,
        source_path,
        destination_path,
        &transfer_id,
        is_directory,
    )
    .await
    {
        Ok(data) => snapshot.ok(
            if is_directory {
                format!("Started {direction} directory transfer.")
            } else {
                format!("Completed {direction} transfer.")
            },
            serde_json::to_string_pretty(&data)
                .unwrap_or_else(|_| format!("transfer_id={transfer_id}")),
            data,
            "write",
        ),
        Err(AiSftpTransferError::OwnerReplaced) => snapshot.fail(
            "SFTP session changed before transfer start.",
            if post_user_approval {
                "runtime_state_changed_after_approval"
            } else {
                "runtime_owner_replaced"
            },
            ai_runtime_validation_recovery_message(post_user_approval),
            "write",
        ),
        Err(AiSftpTransferError::Operation(error)) => snapshot.fail(
            "SFTP transfer failed.",
            "sftp_transfer_failed",
            error,
            "write",
        ),
    }
}

async fn write_remote_file(
    services: &AiLiveToolServices,
    owner: &crate::workspace::ai_runtime_context::AiSftpRuntimeOwner,
    path: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AiRemoteFileWriteError> {
    // File content may contain credentials even when it is not itself a
    // credential field, so the backend copy is wiped after the write.
    let bytes = zeroize::Zeroizing::new(content.as_bytes().to_vec());
    let shared = services
        .node_router
        .acquire_existing_sftp_generation(
            &owner.node_id,
            &owner.connection_id,
            owner.session_generation,
        )
        .await
        .map_err(|_| AiRemoteFileWriteError::OwnerReplaced)?;
    let sftp = shared.lock().await;
    if let Some(expected) = expected_hash {
        let current_bytes = sftp
            .read_file_bytes(path)
            .await
            .map_err(|error| match error {
                oxideterm_ssh::SftpError::FileNotFound(_) => {
                    AiRemoteFileWriteError::ExpectedFileMissing {
                        path: path.to_string(),
                    }
                }
                other => AiRemoteFileWriteError::Sftp(other),
            })?;
        let current_content = String::from_utf8(current_bytes).map_err(|_| {
            AiRemoteFileWriteError::ExistingFileNotText {
                path: path.to_string(),
            }
        })?;
        let current = ai_hash_text_content(&current_content, "utf-8");
        if current != expected {
            return Err(AiRemoteFileWriteError::ExpectedHashMismatch);
        }
    }
    let write = sftp
        .write_content(path, &bytes)
        .await
        .map_err(AiRemoteFileWriteError::Sftp)?;
    let info = sftp
        .stat(path)
        .await
        .map_err(AiRemoteFileWriteError::Sftp)?;
    let hash = ai_hash_text_content(content, "utf-8");
    Ok(serde_json::json!({
        "path": info.path,
        "size": info.size,
        "mtime": info.modified,
        "hash": hash,
        "contentHash": hash,
        "atomicWrite": write.atomic_write,
    }))
}

async fn run_sftp_transfer(
    services: &AiLiveToolServices,
    owner: &crate::workspace::ai_runtime_context::AiSftpRuntimeOwner,
    owner_session: std::sync::Arc<tokio::sync::Mutex<oxideterm_ssh::SftpSession>>,
    direction: &str,
    source_path: &str,
    destination_path: &str,
    transfer_id: &str,
    is_directory: bool,
) -> Result<serde_json::Value, AiSftpTransferError> {
    if is_directory {
        return start_sftp_directory_transfer(
            services,
            owner,
            owner_session,
            direction,
            source_path,
            destination_path,
            transfer_id,
        )
        .await;
    }
    // The exact shared session validated by the capability is retained for
    // the transfer start; a reconnect cannot silently substitute a channel.
    let sftp = owner_session.lock().await;
    let manager = Some(services.sftp_transfer_manager.clone());
    let item_count = match (direction, is_directory) {
        ("upload", false) => {
            let bytes = sftp
                .upload_file(source_path, destination_path, transfer_id, None, manager)
                .await
                .map_err(|error| AiSftpTransferError::Operation(error.to_string()))?;
            serde_json::json!({ "bytes": bytes })
        }
        ("download", false) => {
            let bytes = sftp
                .download_file(source_path, destination_path, transfer_id, None, manager)
                .await
                .map_err(|error| AiSftpTransferError::Operation(error.to_string()))?;
            serde_json::json!({ "bytes": bytes })
        }
        _ => {
            return Err(AiSftpTransferError::Operation(
                "direction must be upload or download.".to_string(),
            ));
        }
    };
    Ok(serde_json::json!({
        "transferId": transfer_id,
        "direction": direction,
        "sourcePath": source_path,
        "destinationPath": destination_path,
        "directory": is_directory,
        "result": item_count,
    }))
}

async fn start_sftp_directory_transfer(
    services: &AiLiveToolServices,
    owner: &crate::workspace::ai_runtime_context::AiSftpRuntimeOwner,
    owner_session: std::sync::Arc<tokio::sync::Mutex<oxideterm_ssh::SftpSession>>,
    direction: &str,
    source_path: &str,
    destination_path: &str,
    transfer_id: &str,
) -> Result<serde_json::Value, AiSftpTransferError> {
    let (local_path, remote_path, direction_enum) = match direction {
        "upload" => (
            source_path,
            destination_path,
            BackgroundTransferDirection::Upload,
        ),
        "download" => (
            destination_path,
            source_path,
            BackgroundTransferDirection::Download,
        ),
        _ => {
            return Err(AiSftpTransferError::Operation(
                "direction must be upload or download.".to_string(),
            ));
        }
    };
    let resolved = services
        .node_router
        .resolve_connection(&owner.node_id)
        .await
        .map_err(|_| AiSftpTransferError::OwnerReplaced)?;
    if resolved.connection_id != owner.connection_id {
        return Err(AiSftpTransferError::OwnerReplaced);
    }
    let tar_capabilities = services
        .sftp_transfer_manager
        .tar_capabilities(&resolved.connection_id, &resolved.handle)
        .await;
    let strategy = if tar_capabilities.supports_tar {
        TransferStrategy::DirectoryTar
    } else {
        TransferStrategy::DirectoryRecursive
    };
    let snapshot = BackgroundTransferSnapshot::new(
        transfer_id.to_string(),
        owner.node_id.0.clone(),
        ai_transfer_name(local_path, remote_path),
        local_path.to_string(),
        remote_path.to_string(),
        direction_enum,
        BackgroundTransferKind::Directory,
        strategy.clone(),
        0,
        0,
    );
    let transfer_audit = services
        .node_router
        .audit_context(&owner.node_id)
        .and_then(|node| {
            resolved
                .handle
                .audit_context()
                .map(|runtime| runtime.with_request(&node))
        })
        .or_else(|| {
            resolved
                .handle
                .audit_context()
                .map(|runtime| runtime.for_request())
        });
    services
        .sftp_transfer_manager
        .register_background_transfer(snapshot.clone(), transfer_audit.as_ref());

    let manager = services.sftp_transfer_manager.clone();
    let runtime = services.backend_runtime.clone();
    let connection_handle = resolved.handle;
    let transfer_id_for_task = transfer_id.to_string();
    let direction_for_task = direction.to_string();
    let local_path_for_task = local_path.to_string();
    let remote_path_for_task = remote_path.to_string();
    let strategy_for_task = strategy.clone();
    // Tauri's node_sftp_start_directory_transfer returns after registering
    // the background transfer; keep the native task on the app backend
    // runtime so it outlives the current AI tool round.
    runtime.spawn(async move {
            let result = async {
                let _permit = manager.acquire_permit().await;
                let control = manager.register(&transfer_id_for_task);
                let _guard = SftpTransferGuard::new(Some(&manager), transfer_id_for_task.clone());
                if control.is_cancelled() {
                    return Err("Transfer cancelled".to_string());
                }
                manager.mark_background_transfer_active(&transfer_id_for_task);
                manager.update_background_transfer_strategy(
                    &transfer_id_for_task,
                    strategy_for_task.clone(),
                );

                if strategy_for_task == TransferStrategy::DirectoryTar
                    && tar_capabilities.supports_tar
                {
                    let profile = match direction_for_task.as_str() {
                        "upload" => profile_local_directory(std::path::Path::new(
                            &local_path_for_task,
                        ))
                        .await
                        .ok(),
                        "download" => {
                            let sftp = owner_session.lock().await;
                            match sftp
                                .profile_remote_directory(
                                    &remote_path_for_task,
                                    &transfer_id_for_task,
                                    &Some(manager.clone()),
                                )
                                .await
                            {
                                Ok(profile) => Some(profile),
                                Err(error) if error.is_transfer_control() => {
                                    return Err(error.to_string());
                                }
                                Err(_) => None,
                            }
                        }
                        _ => unreachable!(),
                    };
                    manager
                        .check_control(&transfer_id_for_task)
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(profile) = profile.filter(|profile| profile.prefers_tar()) {
                        if direction_for_task == "upload" {
                            let sftp = owner_session.lock().await;
                            for prefix in ai_remote_directory_prefixes(&remote_path_for_task) {
                                let _ = sftp.mkdir(&prefix).await;
                            }
                        }
                        let compression =
                            profile.recommended_compression(tar_capabilities.compression);
                        let tar_result = match direction_for_task.as_str() {
                            "upload" => tar_upload_directory(
                                &connection_handle,
                                &local_path_for_task,
                                &remote_path_for_task,
                                &transfer_id_for_task,
                                None,
                                Some(manager.clone()),
                                TarTransferOptions {
                                    profile,
                                    compression,
                                },
                            )
                            .await,
                            "download" => tar_download_directory(
                                &connection_handle,
                                &remote_path_for_task,
                                &local_path_for_task,
                                &transfer_id_for_task,
                                None,
                                Some(manager.clone()),
                                TarTransferOptions {
                                    profile,
                                    compression,
                                },
                            )
                            .await,
                            _ => unreachable!(),
                        };
                        match tar_result {
                            Ok(result) => {
                                manager.record_background_transfer_stream_bytes(
                                    &transfer_id_for_task,
                                    result.stream_bytes,
                                );
                                return Ok((
                                    result.item_count,
                                    TransferStrategy::DirectoryTar,
                                    false,
                                ));
                            }
                            Err(error) if !error.is_transfer_control() =>
                            {
                                manager.update_background_transfer_strategy(
                                    &transfer_id_for_task,
                                    TransferStrategy::DirectoryRecursive,
                                );
                                let sftp = owner_session.lock().await;
                                let fallback = match direction_for_task.as_str() {
                                    "upload" => {
                                        sftp.upload_dir(
                                            &local_path_for_task,
                                            &remote_path_for_task,
                                            &transfer_id_for_task,
                                            None,
                                            Some(manager.clone()),
                                        )
                                        .await
                                    }
                                    "download" => {
                                        sftp.download_dir(
                                            &remote_path_for_task,
                                            &local_path_for_task,
                                            &transfer_id_for_task,
                                            None,
                                            Some(manager.clone()),
                                        )
                                        .await
                                    }
                                    _ => unreachable!(),
                                };
                                return fallback
                                    .map(|count| {
                                        (count, TransferStrategy::DirectoryRecursive, true)
                                    })
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar directory transfer failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    });
                            }
                            Err(error) => return Err(error.to_string()),
                        }
                    }
                }

                manager.update_background_transfer_strategy(
                    &transfer_id_for_task,
                    TransferStrategy::DirectoryRecursive,
                );
                let sftp = owner_session.lock().await;
                match direction_for_task.as_str() {
                    "upload" => {
                        sftp.upload_dir(
                            &local_path_for_task,
                            &remote_path_for_task,
                            &transfer_id_for_task,
                            None,
                            Some(manager.clone()),
                        )
                        .await
                    }
                    "download" => {
                        sftp.download_dir(
                            &remote_path_for_task,
                            &local_path_for_task,
                            &transfer_id_for_task,
                            None,
                            Some(manager.clone()),
                        )
                        .await
                    }
                    _ => unreachable!(),
                }
                .map(|count| (count, TransferStrategy::DirectoryRecursive, false))
                .map_err(|error| error.to_string())
            }
            .await;

            match result {
                Ok((item_count, _, _)) => {
                    manager.finish_background_transfer(
                        &transfer_id_for_task,
                        BackgroundTransferState::Completed,
                        None,
                        Some(item_count),
                    );
                }
                Err(error) => {
                    let state = if error.to_ascii_lowercase().contains("cancel") {
                        BackgroundTransferState::Cancelled
                    } else {
                        BackgroundTransferState::Error
                    };
                    manager.finish_background_transfer(
                        &transfer_id_for_task,
                        state,
                        Some(error),
                        None,
                    );
                }
            }
        });

    Ok(serde_json::json!({
        "transferId": transfer_id,
        "strategy": strategy,
        "transfer": snapshot,
    }))
}

/// Application surfaces are durable navigation destinations, not live tab identities.
pub(in crate::workspace) fn ai_app_surface_stable_resources() -> Vec<oxideterm_ai::StableResourceRef>
{
    const SURFACES: &[(&str, &str)] = &[
        ("settings", "Settings"),
        ("connection_manager", "Connection manager"),
        ("connection_pool", "Connection pool"),
        ("connection_monitor", "Connection monitor"),
        ("sftp", "SFTP"),
        ("ide", "IDE"),
        ("file_manager", "File manager"),
        ("local_terminal", "Local terminal"),
        ("terminal", "Terminal"),
    ];

    SURFACES
        .iter()
        .filter_map(|(id, label)| {
            oxideterm_ai::StableResourceRef::new(
                oxideterm_ai::StableResourceKind::AppSurface,
                (*id).to_string(),
                Some((*label).to_string()),
            )
            .ok()
        })
        .collect()
}

pub(in crate::workspace) fn ai_transfer_path_looks_directory(path: &str) -> bool {
    // Tauri uses /[\\/]$/ so both POSIX and Windows-style trailing separators
    // select directory transfer semantics.
    path.ends_with('/') || path.ends_with('\\')
}
