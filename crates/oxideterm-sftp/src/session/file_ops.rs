// A directory stays live until every direct child reports completion. This is
// the dependency edge that permits concurrent work while preserving post-order.
struct DirectoryDeleteState {
    path: String,
    parent_id: Option<u64>,
    remaining_entries: usize,
    deleted_descendants: u64,
}

enum PendingDeleteOperation {
    ListDirectory { directory_id: u64, path: String },
    InspectEntry { parent_id: u64, path: String },
    RemoveFile { parent_id: u64, path: String },
    RemoveDirectory { directory_id: u64, path: String },
}

enum CompletedDeleteOperation {
    DirectoryListed {
        directory_id: u64,
        entries: Vec<RemoteTreeEntry>,
    },
    DirectoryDiscovered {
        parent_id: u64,
        path: String,
    },
    FileRemoved {
        parent_id: u64,
    },
    DirectoryRemoved {
        directory_id: u64,
    },
}

impl SftpSession {
    pub async fn delete(&self, path: &str) -> Result<(), SftpError> {
        let audit = self.audit_operation("file_delete", path);
        let audit_result = async {
            let canonical_path = self.resolve_path(path).await?;
            let metadata = self
                .sftp
                .symlink_metadata(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
            if metadata.is_dir() && !metadata.is_symlink() {
                self.sftp
                    .remove_dir(&canonical_path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &canonical_path))
            } else {
                self.sftp
                    .remove_file(&canonical_path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &canonical_path))
            }
        }
        .await;
        audit.result(&audit_result);
        audit_result
    }

    pub async fn delete_recursive(&self, path: &str) -> Result<u64, SftpError> {
        let mut audit = self.audit_operation("file_delete_recursive", path);
        let mut child_context = self.audit.clone();
        if let Some(context) = child_context.as_mut() {
            context.parent_id = audit.id().map(str::to_owned);
        }
        let removed = std::sync::atomic::AtomicU64::new(0);
        let audit_result = async {
            let canonical_path = self.resolve_path(path).await?;
            let metadata = self
                .sftp
                .symlink_metadata(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
            if !metadata.is_dir() || metadata.is_symlink() {
                self.sftp
                    .remove_file(&canonical_path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
                removed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(1);
            }

            let plan = plan_directory_transfer(
                crate::DEFAULT_SFTP_DIRECTORY_PARALLELISM,
                self.sftp.advertised_open_handle_limit(),
            );
            self.delete_directory_tree_resolved(&canonical_path, plan.worker_count, child_context.as_ref(), &removed)
                .await
        }
        .await;
        let removed = removed.load(std::sync::atomic::Ordering::Relaxed);
        audit.summary(&format!("path={path}; removed={removed}"));
        audit.finish(
            match &audit_result {
                Ok(_) => oxideterm_audit::AuditOutcome::Succeeded,
                Err(_) if removed > 0 => oxideterm_audit::AuditOutcome::Partial,
                Err(_) => oxideterm_audit::AuditOutcome::Failed,
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
        audit_result
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        let audit = self.audit_operation("file_mkdir", path);
        let audit_result = async {
            let canonical_path = if is_absolute_remote_path(path) {
                path.to_string()
            } else {
                join_remote_path(&self.cwd, path)
            };
            self.sftp
                .create_dir(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))
        }
        .await;
        audit.result(&audit_result);
        audit_result
    }

    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), SftpError> {
        let audit = self.audit_operation("file_rename", &format!("{old_path} → {new_path}"));
        let audit_result = async {
            let old_canonical = self.resolve_path(old_path).await?;
            let new_canonical = if is_absolute_remote_path(new_path) {
                new_path.to_string()
            } else {
                let parent = old_canonical
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .filter(|parent| !parent.is_empty())
                    .unwrap_or("/");
                join_remote_path(parent, new_path)
            };
            self.sftp
                .rename(&old_canonical, &new_canonical)
                .await
                .map_err(|error| self.map_sftp_error(error, &old_canonical))
        }
        .await;
        audit.result(&audit_result);
        audit_result
    }

    async fn delete_directory_tree_resolved(
        &self,
        root_path: &str,
        parallelism: usize,
        child_context: Option<&oxideterm_audit::AuditContext>,
        removed: &std::sync::atomic::AtomicU64,
    ) -> Result<u64, SftpError> {
        let root_id = 0;
        let mut next_directory_id = root_id + 1;
        let mut directories = HashMap::from([(
            root_id,
            DirectoryDeleteState {
                path: root_path.to_string(),
                parent_id: None,
                remaining_entries: 0,
                deleted_descendants: 0,
            },
        )]);
        let mut pending = VecDeque::from([PendingDeleteOperation::ListDirectory {
            directory_id: root_id,
            path: root_path.to_string(),
        }]);
        let mut operations = stream::FuturesUnordered::new();

        loop {
            while operations.len() < parallelism
                && let Some(operation) = pending.pop_front()
            {
                operations.push(async move { self.execute_delete_operation(operation, child_context, removed).await });
            }

            let Some(completed) = operations.next().await else {
                return Err(SftpError::TransferError(
                    "Recursive delete ended before removing its root directory".to_string(),
                ));
            };
            match completed? {
                CompletedDeleteOperation::DirectoryListed {
                    directory_id,
                    entries,
                } => {
                    let entry_count = entries.len();
                    let state = directories.get_mut(&directory_id).ok_or_else(|| {
                        SftpError::TransferError(
                            "Recursive delete lost directory dependency state".to_string(),
                        )
                    })?;
                    state.remaining_entries = entry_count;
                    if entry_count == 0 {
                        pending.push_back(PendingDeleteOperation::RemoveDirectory {
                            directory_id,
                            path: state.path.clone(),
                        });
                        continue;
                    }

                    for entry in entries {
                        if entry.file_type == FileType::Directory && !entry.is_symlink {
                            let child_id = next_directory_id;
                            next_directory_id = next_directory_id.saturating_add(1);
                            directories.insert(
                                child_id,
                                DirectoryDeleteState {
                                    path: entry.path.clone(),
                                    parent_id: Some(directory_id),
                                    remaining_entries: 0,
                                    deleted_descendants: 0,
                                },
                            );
                            pending.push_back(PendingDeleteOperation::ListDirectory {
                                directory_id: child_id,
                                path: entry.path,
                            });
                        } else if entry.file_type == FileType::Unknown {
                            // Some SFTP servers omit type bits from directory entries.
                            // Inspect only those entries instead of adding an RTT for every file.
                            pending.push_back(PendingDeleteOperation::InspectEntry {
                                parent_id: directory_id,
                                path: entry.path,
                            });
                        } else {
                            pending.push_back(PendingDeleteOperation::RemoveFile {
                                parent_id: directory_id,
                                path: entry.path,
                            });
                        }
                    }
                }
                CompletedDeleteOperation::DirectoryDiscovered { parent_id, path } => {
                    let child_id = next_directory_id;
                    next_directory_id = next_directory_id.saturating_add(1);
                    directories.insert(
                        child_id,
                        DirectoryDeleteState {
                            path: path.clone(),
                            parent_id: Some(parent_id),
                            remaining_entries: 0,
                            deleted_descendants: 0,
                        },
                    );
                    pending.push_back(PendingDeleteOperation::ListDirectory {
                        directory_id: child_id,
                        path,
                    });
                }
                CompletedDeleteOperation::FileRemoved { parent_id } => {
                    complete_deleted_child(parent_id, 1, &mut directories, &mut pending)?;
                }
                CompletedDeleteOperation::DirectoryRemoved { directory_id } => {
                    let state = directories.remove(&directory_id).ok_or_else(|| {
                        SftpError::TransferError(
                            "Recursive delete lost completed directory state".to_string(),
                        )
                    })?;
                    let deleted_count = state.deleted_descendants.saturating_add(1);
                    if let Some(parent_id) = state.parent_id {
                        complete_deleted_child(
                            parent_id,
                            deleted_count,
                            &mut directories,
                            &mut pending,
                        )?;
                    } else {
                        return Ok(deleted_count);
                    }
                }
            }
        }
    }

    async fn execute_delete_operation(
        &self,
        operation: PendingDeleteOperation,
        child_context: Option<&oxideterm_audit::AuditContext>,
        removed: &std::sync::atomic::AtomicU64,
    ) -> Result<CompletedDeleteOperation, SftpError> {
        match operation {
            PendingDeleteOperation::ListDirectory { directory_id, path } => {
                let entries = self.list_tree_entries_resolved(&path).await?;
                Ok(CompletedDeleteOperation::DirectoryListed {
                    directory_id,
                    entries,
                })
            }
            PendingDeleteOperation::InspectEntry { parent_id, path } => {
                let metadata = self
                    .sftp
                    .symlink_metadata(&path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &path))?;
                if metadata.is_dir() && !metadata.is_symlink() {
                    Ok(CompletedDeleteOperation::DirectoryDiscovered { parent_id, path })
                } else {
                    let audit = oxideterm_audit::AuditOperation::in_context(child_context, oxideterm_audit::AuditCategory::File, "file_delete", Some(&path));
                    let result = self.sftp
                        .remove_file(&path)
                        .await
                        .map_err(|error| self.map_sftp_error(error, &path));
                    audit.result(&result);
                    result?;
                    removed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(CompletedDeleteOperation::FileRemoved { parent_id })
                }
            }
            PendingDeleteOperation::RemoveFile { parent_id, path } => {
                let audit = oxideterm_audit::AuditOperation::in_context(child_context, oxideterm_audit::AuditCategory::File, "file_delete", Some(&path));
                let result = self.sftp
                    .remove_file(&path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &path));
                audit.result(&result);
                result?;
                removed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(CompletedDeleteOperation::FileRemoved { parent_id })
            }
            PendingDeleteOperation::RemoveDirectory { directory_id, path } => {
                let audit = oxideterm_audit::AuditOperation::in_context(child_context, oxideterm_audit::AuditCategory::File, "file_delete", Some(&path));
                let result = self.sftp
                    .remove_dir(&path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &path));
                audit.result(&result);
                result?;
                removed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(CompletedDeleteOperation::DirectoryRemoved { directory_id })
            }
        }
    }
}

fn complete_deleted_child(
    parent_id: u64,
    deleted_count: u64,
    directories: &mut HashMap<u64, DirectoryDeleteState>,
    pending: &mut VecDeque<PendingDeleteOperation>,
) -> Result<(), SftpError> {
    let parent = directories.get_mut(&parent_id).ok_or_else(|| {
        SftpError::TransferError("Recursive delete lost parent directory state".to_string())
    })?;
    parent.remaining_entries = parent.remaining_entries.checked_sub(1).ok_or_else(|| {
        SftpError::TransferError("Recursive delete completed a child twice".to_string())
    })?;
    parent.deleted_descendants = parent.deleted_descendants.saturating_add(deleted_count);
    if parent.remaining_entries == 0 {
        pending.push_back(PendingDeleteOperation::RemoveDirectory {
            directory_id: parent_id,
            path: parent.path.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod delete_scheduler_tests {
    use super::*;

    #[test]
    fn parent_removal_waits_for_every_child_and_preserves_the_count() {
        let parent_id = 7;
        let mut directories = HashMap::from([(
            parent_id,
            DirectoryDeleteState {
                path: "/tree".to_string(),
                parent_id: None,
                remaining_entries: 2,
                deleted_descendants: 0,
            },
        )]);
        let mut pending = VecDeque::new();

        complete_deleted_child(parent_id, 1, &mut directories, &mut pending).unwrap();
        assert!(pending.is_empty());
        assert_eq!(directories[&parent_id].deleted_descendants, 1);

        complete_deleted_child(parent_id, 4, &mut directories, &mut pending).unwrap();
        assert_eq!(directories[&parent_id].deleted_descendants, 5);
        assert!(matches!(
            pending.pop_front(),
            Some(PendingDeleteOperation::RemoveDirectory {
                directory_id: 7,
                path,
            }) if path == "/tree"
        ));
    }
}
