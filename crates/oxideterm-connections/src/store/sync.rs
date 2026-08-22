enum SavedConnectionsStoreFileCheckpoint {
    Missing,
    Present(Vec<u8>),
}

/// Opaque rollback state for the complete connection store.
///
/// The checkpoint owns every `ConnectionStoreData` field and the exact
/// persisted bytes, but its debug representation never exposes either value.
/// It deliberately does not read or copy keychain secrets, so operations that
/// create new keychain entries must separately track those entries for cleanup.
#[must_use = "connection store checkpoints should be restored or deliberately discarded"]
pub struct ConnectionStoreCheckpoint {
    store_path: PathBuf,
    original_data: ConnectionStoreData,
    original_storage_format: ConnectionStoreStorageFormat,
    original_file: SavedConnectionsStoreFileCheckpoint,
}

impl fmt::Debug for ConnectionStoreCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionStoreCheckpoint")
            .field("store_path", &self.store_path)
            .field("contents", &"[redacted complete connection store checkpoint]")
            .finish()
    }
}

/// Opaque rollback state for a prepared saved-connections sync operation.
///
/// Dropping this handle does not perform I/O and therefore does not roll back
/// prepared data. It also never deletes stale keychain entries. Callers must
/// explicitly restore the handle or commit it into a retryable cleanup handle.
#[must_use = "prepared sync changes must be committed or rolled back"]
pub struct PreparedSavedConnectionsSync {
    checkpoint: ConnectionStoreCheckpoint,
    outcome: ApplySavedConnectionsSyncOutcome,
    pending_keychain_ids: Vec<String>,
    pending_privilege_keychain_ids: Vec<String>,
}

impl PreparedSavedConnectionsSync {
    pub fn outcome(&self) -> &ApplySavedConnectionsSyncOutcome {
        &self.outcome
    }
}

impl fmt::Debug for PreparedSavedConnectionsSync {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSavedConnectionsSync")
            .field("store_path", &self.checkpoint.store_path)
            .field("outcome", &self.outcome)
            .field("pending_keychain_entries", &self.pending_keychain_ids.len())
            .field(
                "pending_privilege_keychain_entries",
                &self.pending_privilege_keychain_ids.len(),
            )
            .field("checkpoint", &"[redacted connection store checkpoint]")
            .finish()
    }
}

/// Retryable cleanup handle created only after prepared data is committed.
///
/// Failed keychain deletions remain pending so callers can retry without
/// rolling back already committed connection data.
#[must_use = "committed sync cleanup should be finalized"]
pub struct SavedConnectionsSyncCleanup {
    store_path: PathBuf,
    outcome: ApplySavedConnectionsSyncOutcome,
    pending_keychain_ids: Vec<String>,
    pending_privilege_keychain_ids: Vec<String>,
}

impl SavedConnectionsSyncCleanup {
    pub fn outcome(&self) -> &ApplySavedConnectionsSyncOutcome {
        &self.outcome
    }

    pub fn pending_keychain_entries(&self) -> usize {
        self.pending_keychain_ids.len() + self.pending_privilege_keychain_ids.len()
    }
}

impl fmt::Debug for SavedConnectionsSyncCleanup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedConnectionsSyncCleanup")
            .field("store_path", &self.store_path)
            .field("outcome", &self.outcome)
            .field("pending_keychain_entries", &self.pending_keychain_ids.len())
            .field(
                "pending_privilege_keychain_entries",
                &self.pending_privilege_keychain_ids.len(),
            )
            .finish()
    }
}

impl ConnectionStore {
    pub fn create_checkpoint(&self) -> Result<ConnectionStoreCheckpoint> {
        Ok(ConnectionStoreCheckpoint {
            store_path: self.path.clone(),
            original_data: self.data.clone(),
            original_storage_format: self.storage_format,
            original_file: self.capture_connection_store_file()?,
        })
    }

    pub fn restore_checkpoint(&mut self, checkpoint: &ConnectionStoreCheckpoint) -> Result<()> {
        self.ensure_saved_connections_sync_store(&checkpoint.store_path)?;

        // Restore disk first. If the write fails, the current in-memory model
        // remains aligned with the current file and restoration can retry.
        match &checkpoint.original_file {
            SavedConnectionsStoreFileCheckpoint::Missing => {
                if self.path.exists() {
                    durable_remove(&self.path).with_context(|| {
                        format!("failed to remove current store {}", self.path.display())
                    })?;
                }
            }
            SavedConnectionsStoreFileCheckpoint::Present(bytes) => {
                atomic_write_file(&self.path, bytes).with_context(|| {
                    format!("failed to restore connection store {}", self.path.display())
                })?;
            }
        }

        // This clone restores connections, groups, all profile types,
        // tombstones, recent entries, managed-key metadata, and local privilege
        // metadata as one in-memory state transition.
        self.data = checkpoint.original_data.clone();
        self.storage_format = checkpoint.original_storage_format;
        Ok(())
    }

    pub fn export_saved_connections_snapshot(&self) -> Result<SavedConnectionsSyncSnapshot> {
        build_saved_connections_sync_snapshot(&self.data)
    }

    pub fn export_serial_profiles_snapshot(&self) -> Result<SerialProfilesSyncSnapshot> {
        build_serial_profiles_sync_snapshot(&self.data)
    }

    pub fn export_telnet_profiles_snapshot(&self) -> Result<TelnetProfilesSyncSnapshot> {
        build_telnet_profiles_sync_snapshot(&self.data)
    }

    pub fn export_mosh_profiles_snapshot(&self) -> Result<MoshProfilesSyncSnapshot> {
        build_mosh_profiles_sync_snapshot(&self.data)
    }

    pub fn export_standalone_sftp_profiles_snapshot(
        &self,
    ) -> Result<StandaloneSftpProfilesSyncSnapshot> {
        build_standalone_sftp_profiles_sync_snapshot(&self.data)
    }

    pub fn export_remote_desktop_profiles_snapshot(
        &self,
    ) -> Result<RemoteDesktopProfilesSyncSnapshot> {
        build_remote_desktop_profiles_sync_snapshot(&self.data)
    }

    pub fn local_sync_metadata(&self) -> Result<LocalSyncMetadata> {
        let snapshot = self.export_saved_connections_snapshot()?;
        let saved_connections_updated_at = snapshot
            .records
            .iter()
            .map(|record| record.updated_at.clone())
            .max()
            .unwrap_or_else(|| snapshot.exported_at.clone());

        Ok(LocalSyncMetadata {
            saved_connections_revision: snapshot.revision,
            saved_connections_updated_at,
        })
    }

    pub fn apply_saved_connections_snapshot(
        &mut self,
        snapshot: SavedConnectionsSyncSnapshot,
        strategy: SavedConnectionsConflictStrategy,
    ) -> Result<ApplySavedConnectionsSyncOutcome> {
        let prepared = self.prepare_saved_connections_snapshot(snapshot, strategy)?;
        let mut cleanup = self.commit_prepared_saved_connections_snapshot(prepared)?;
        let outcome = cleanup.outcome().clone();

        // Preserve the legacy one-shot API's best-effort cleanup semantics.
        // Transaction coordinators should use the explicit prepare/rollback/
        // commit/finalize API so failed cleanup remains available for retry.
        let _ = self.finalize_saved_connections_sync_cleanup(&mut cleanup);
        Ok(outcome)
    }

    pub fn prepare_saved_connections_snapshot(
        &mut self,
        snapshot: SavedConnectionsSyncSnapshot,
        strategy: SavedConnectionsConflictStrategy,
    ) -> Result<PreparedSavedConnectionsSync> {
        let checkpoint = self.create_checkpoint()?;
        let mut result = ApplySavedConnectionsSyncSnapshotResult::default();
        let mut deleted_connection_ids = Vec::new();
        let mut keychain_ids_to_delete = Vec::new();
        let mut privilege_keychain_ids_to_delete = Vec::new();

        let apply_result = (|| {
            let existing_by_id: HashMap<String, SavedConnection> = self
                .data
                .connections
                .iter()
                .cloned()
                .map(|connection| (connection.id.clone(), connection))
                .collect();
            let tombstones_by_id: HashMap<String, DeletedConnectionTombstone> =
                active_connection_tombstones(&self.data.connection_tombstones)
                    .into_iter()
                    .map(|tombstone| (tombstone.id.clone(), tombstone))
                    .collect();

            for record in snapshot.records {
                let record_updated_at = parse_connection_sync_timestamp(
                    &record.updated_at,
                    "saved connection sync updated_at",
                )?;

                if record.deleted {
                    if existing_by_id.get(&record.id).is_some_and(|existing| {
                        connection_sync_updated_at(existing) > record_updated_at
                    }) {
                        result.skipped += 1;
                        result.conflicts += 1;
                        continue;
                    }

                    if let Some(removed) =
                        self.remove_connection_with_tombstone_at(&record.id, record_updated_at)
                    {
                        deleted_connection_ids.push(removed.id.clone());
                        keychain_ids_to_delete.extend(collect_connection_keychain_ids(&removed));
                        privilege_keychain_ids_to_delete
                            .extend(collect_privilege_keychain_ids(&removed));
                        result.applied += 1;
                    } else if self.upsert_connection_tombstone(record.id.clone(), record_updated_at)
                    {
                        result.applied += 1;
                    } else {
                        result.skipped += 1;
                    }
                    continue;
                }

                let Some(payload) = record.payload else {
                    result.skipped += 1;
                    result.conflicts += 1;
                    continue;
                };

                if tombstones_by_id
                    .get(&record.id)
                    .is_some_and(|tombstone| tombstone.deleted_at >= record_updated_at)
                {
                    result.skipped += 1;
                    result.conflicts += 1;
                    continue;
                }

                let existing_by_id = self.get(&record.id).cloned();
                let existing_by_name = if existing_by_id.is_none() {
                    self.data
                        .connections
                        .iter()
                        .find(|candidate| {
                            candidate.name == payload.name && candidate.id != record.id
                        })
                        .cloned()
                } else {
                    None
                };

                if existing_by_id.is_none()
                    && existing_by_name.is_some()
                    && strategy == SavedConnectionsConflictStrategy::Skip
                {
                    result.skipped += 1;
                    result.conflicts += 1;
                    continue;
                }

                if let Some(existing_same_name) = existing_by_name.as_ref() {
                    if let Some(removed) =
                        self.remove_connection_without_tombstone(&existing_same_name.id)
                    {
                        deleted_connection_ids.push(removed.id);
                    }
                }

                let baseline = existing_by_id.as_ref().or(existing_by_name.as_ref());
                let next_connection = build_saved_connection_from_sync_payload(
                    &payload,
                    record.options.as_ref(),
                    record_updated_at,
                    baseline,
                    baseline.is_some() && strategy.preserves_local_auth(),
                )?;

                if let Some(existing) = baseline {
                    let existing_keychain_ids: HashSet<String> =
                        collect_connection_keychain_ids(existing)
                            .into_iter()
                            .collect();
                    let next_keychain_ids: HashSet<String> =
                        collect_connection_keychain_ids(&next_connection)
                            .into_iter()
                            .collect();
                    keychain_ids_to_delete.extend(
                        existing_keychain_ids
                            .difference(&next_keychain_ids)
                            .cloned(),
                    );
                }

                if let Some(group) = next_connection.group.clone() {
                    self.ensure_group(group)?;
                }
                self.add_connection(next_connection);
                result.applied += 1;
            }
            self.normalize();
            if result.applied > 0 {
                self.save()?;
            }
            Ok::<(), anyhow::Error>(())
        })();

        if let Err(error) = apply_result {
            self.restore_checkpoint(&checkpoint)
                .context("failed to restore saved connections after sync preparation failed")?;
            return Err(error.context("failed to prepare saved connections sync"));
        }

        keychain_ids_to_delete.sort();
        keychain_ids_to_delete.dedup();
        privilege_keychain_ids_to_delete.sort();
        privilege_keychain_ids_to_delete.dedup();

        Ok(PreparedSavedConnectionsSync {
            checkpoint,
            outcome: ApplySavedConnectionsSyncOutcome {
                result,
                deleted_connection_ids,
            },
            pending_keychain_ids: keychain_ids_to_delete,
            pending_privilege_keychain_ids: privilege_keychain_ids_to_delete,
        })
    }

    pub fn rollback_prepared_saved_connections_snapshot(
        &mut self,
        prepared: &PreparedSavedConnectionsSync,
    ) -> Result<()> {
        self.restore_checkpoint(&prepared.checkpoint)
    }

    pub fn commit_prepared_saved_connections_snapshot(
        &self,
        prepared: PreparedSavedConnectionsSync,
    ) -> Result<SavedConnectionsSyncCleanup> {
        self.ensure_saved_connections_sync_store(&prepared.checkpoint.store_path)?;
        Ok(SavedConnectionsSyncCleanup {
            store_path: prepared.checkpoint.store_path,
            outcome: prepared.outcome,
            pending_keychain_ids: prepared.pending_keychain_ids,
            pending_privilege_keychain_ids: prepared.pending_privilege_keychain_ids,
        })
    }

    pub fn finalize_saved_connections_sync_cleanup(
        &mut self,
        cleanup: &mut SavedConnectionsSyncCleanup,
    ) -> Result<()> {
        self.ensure_saved_connections_sync_store(&cleanup.store_path)?;
        let mut pending_keychain_ids = std::mem::take(&mut cleanup.pending_keychain_ids);
        pending_keychain_ids.append(&mut self.data.pending_keychain_cleanup);
        pending_keychain_ids.sort();
        pending_keychain_ids.dedup();
        let mut failed_keychain_ids = Vec::new();

        for keychain_id in pending_keychain_ids {
            if self.keychain.delete(&keychain_id).is_err() {
                failed_keychain_ids.push(keychain_id);
            }
        }

        let mut pending_privilege_keychain_ids =
            std::mem::take(&mut cleanup.pending_privilege_keychain_ids);
        pending_privilege_keychain_ids.append(&mut self.data.pending_privilege_keychain_cleanup);
        pending_privilege_keychain_ids.sort();
        pending_privilege_keychain_ids.dedup();
        let mut failed_privilege_keychain_ids = Vec::new();
        for keychain_id in pending_privilege_keychain_ids {
            if self.privilege_keychain.delete(&keychain_id).is_err() {
                failed_privilege_keychain_ids.push(keychain_id);
            }
        }

        self.data.pending_keychain_cleanup = failed_keychain_ids.clone();
        self.data.pending_privilege_keychain_cleanup = failed_privilege_keychain_ids.clone();
        cleanup.pending_keychain_ids = failed_keychain_ids;
        cleanup.pending_privilege_keychain_ids = failed_privilege_keychain_ids;
        self.save()
            .context("failed to persist connection keychain cleanup state")?;
        if cleanup.pending_keychain_ids.is_empty()
            && cleanup.pending_privilege_keychain_ids.is_empty()
        {
            Ok(())
        } else {
            bail!(
                "failed to delete {} stale connection keychain entries",
                cleanup.pending_keychain_entries()
            )
        }
    }

    fn capture_connection_store_file(&self) -> Result<SavedConnectionsStoreFileCheckpoint> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(SavedConnectionsStoreFileCheckpoint::Present(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SavedConnectionsStoreFileCheckpoint::Missing)
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to checkpoint {}", self.path.display()))
            }
        }
    }

    fn ensure_saved_connections_sync_store(&self, expected_path: &Path) -> Result<()> {
        if self.path == expected_path {
            Ok(())
        } else {
            bail!("saved connections sync handle belongs to a different store")
        }
    }

    pub fn apply_serial_profiles_snapshot(
        &mut self,
        snapshot: SerialProfilesSyncSnapshot,
    ) -> Result<usize> {
        // Validate the complete batch before mutating store data so a bad late
        // record cannot leave earlier profiles applied in memory.
        for profile in &snapshot.records {
            profile.validate()?;
        }
        let mut applied = 0usize;
        for profile in snapshot.records {
            if let Some(existing) = self
                .data
                .serial_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.serial_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

    pub fn apply_telnet_profiles_snapshot(
        &mut self,
        snapshot: TelnetProfilesSyncSnapshot,
    ) -> Result<usize> {
        // Validate the complete batch before changing the shared connection store.
        for profile in &snapshot.records {
            profile.validate()?;
        }
        let mut applied = 0usize;
        for profile in snapshot.records {
            if let Some(existing) = self
                .data
                .telnet_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.telnet_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

    pub fn apply_remote_desktop_profiles_snapshot(
        &mut self,
        snapshot: RemoteDesktopProfilesSyncSnapshot,
    ) -> Result<usize> {
        // Validate the complete asset batch before changing the shared store file.
        for profile in &snapshot.records {
            profile.validate()?;
        }
        let mut applied = 0usize;
        for mut profile in snapshot.records {
            // Protected-store references are device-local and cannot be imported as credentials.
            profile.credential_ref = None;
            if let Some(existing) = self
                .data
                .remote_desktop_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    // Updating portable metadata must not disconnect a valid local credential.
                    profile.credential_ref = existing.credential_ref.clone();
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.remote_desktop_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

    pub fn apply_mosh_profiles_snapshot(
        &mut self,
        snapshot: MoshProfilesSyncSnapshot,
    ) -> Result<usize> {
        // Mosh snapshots carry portable metadata only; local keychain references stay local.
        for profile in &snapshot.records {
            profile.validate()?;
        }
        let mut applied = 0usize;
        for mut profile in snapshot.records {
            if let Some(existing) = self
                .data
                .mosh_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    preserve_local_auth_secret(&mut profile.auth, &existing.auth);
                    preserve_proxy_chain_local_secrets(
                        &mut profile.proxy_chain,
                        &existing.proxy_chain,
                    );
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.mosh_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

    pub fn apply_standalone_sftp_profiles_snapshot(
        &mut self,
        snapshot: StandaloneSftpProfilesSyncSnapshot,
    ) -> Result<usize> {
        // Treat every incoming snapshot as portable metadata, even when it came from a .oxide file.
        let mut applied = 0usize;
        for mut profile in snapshot.records {
            if profile.transfer_mode == StandaloneSftpTransferMode::LocalRemote {
                profile.secondary_endpoint = None;
            }
            make_standalone_sftp_profile_portable(&mut profile);
            profile.validate()?;
            if let Some(existing) = self
                .data
                .standalone_sftp_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    preserve_standalone_sftp_local_secrets(&mut profile, existing);
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.standalone_sftp_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

}

fn build_saved_connection_from_sync_payload(
    payload: &ConnectionInfo,
    synced_options: Option<&ConnectionOptions>,
    record_updated_at: DateTime<Utc>,
    existing: Option<&SavedConnection>,
    preserve_auth: bool,
) -> Result<SavedConnection> {
    let mut auth = saved_auth_from_connection_info(payload);
    if preserve_auth && let Some(existing) = existing {
        preserve_local_auth_secret(&mut auth, &existing.auth);
    }
    let proxy_chain = build_synced_proxy_chain(&payload.proxy_chain, existing, preserve_auth);
    for hop in &proxy_chain {
        hop.ssh_algorithms.validate()?;
    }
    let options = synced_options
        .cloned()
        .unwrap_or_else(|| ConnectionOptions {
            // Older snapshots exposed only these option fields through
            // ConnectionInfo, so retain that wire-compatible fallback.
            agent_forwarding: payload.agent_forwarding,
            legacy_ssh_compatibility: payload.legacy_ssh_compatibility,
            ssh_algorithms: payload.ssh_algorithms.clone(),
            post_connect_command: payload.post_connect_command.clone(),
            ..Default::default()
        });
    options.ssh_algorithms.validate()?;

    Ok(SavedConnection {
        id: payload.id.clone(),
        version: CONFIG_VERSION,
        name: non_empty(payload.name.trim(), "Connection name")?.to_string(),
        group: normalize_optional_group_name(payload.group.as_deref())?,
        notes: payload.notes.clone(),
        host: non_empty(payload.host.trim(), "Host")?.to_string(),
        port: payload.port.max(1),
        username: non_empty(payload.username.trim(), "Username")?.to_string(),
        auth,
        proxy_chain,
        upstream_proxy: build_synced_upstream_proxy(
            &payload.upstream_proxy,
            existing,
            preserve_auth,
        ),
        // ProxyCommand text is device-local protected data and never enters cloud snapshots.
        proxy_command: existing.and_then(|connection| connection.proxy_command.clone()),
        options,
        created_at: parse_connection_sync_timestamp(&payload.created_at, "connection created_at")?,
        last_used_at: payload
            .last_used_at
            .as_deref()
            .map(|value| parse_connection_sync_timestamp(value, "connection last_used_at"))
            .transpose()?,
        updated_at: Some(record_updated_at),
        color: payload.color.clone(),
        icon_background_color: payload.icon_background_color.clone(),
        icon: payload.icon.clone(),
        tags: payload.tags.clone(),
        post_connect_command: None,
        privilege_credentials: existing
            .map(|connection| connection.privilege_credentials.clone())
            .unwrap_or_default(),
    })
}

fn build_saved_connections_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<SavedConnectionsSyncSnapshot> {
    let mut records: Vec<SavedConnectionSyncRecord> = data
        .connections
        .iter()
        .map(build_saved_connection_sync_record)
        .collect::<Result<_, _>>()?;
    records.extend(
        active_connection_tombstones(&data.connection_tombstones)
            .iter()
            .map(build_saved_connection_tombstone_record)
            .collect::<Result<Vec<_>>>()?,
    );
    records.sort_by(|left, right| left.id.cmp(&right.id));

    let revision = sha256_hex(
        &records
            .iter()
            // The exported record includes updated_at, so the snapshot revision must change with it.
            .map(|record| {
                (
                    &record.id,
                    &record.revision,
                    &record.updated_at,
                    record.deleted,
                )
            })
            .collect::<Vec<_>>(),
    )?;

    Ok(SavedConnectionsSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_serial_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<SerialProfilesSyncSnapshot> {
    let mut records = data.serial_profiles.clone();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;

    Ok(SerialProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_telnet_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<TelnetProfilesSyncSnapshot> {
    let mut records = data.telnet_profiles.clone();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;

    Ok(TelnetProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_mosh_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<MoshProfilesSyncSnapshot> {
    let mut records = data.mosh_profiles.clone();
    for profile in &mut records {
        profile.auth = portable_mosh_auth(&profile.auth);
        for hop in &mut profile.proxy_chain {
            hop.auth = portable_mosh_auth(&hop.auth);
        }
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;
    Ok(MoshProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_standalone_sftp_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<StandaloneSftpProfilesSyncSnapshot> {
    let mut records = data.standalone_sftp_profiles.clone();
    for profile in &mut records {
        make_standalone_sftp_profile_portable(profile);
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;
    Ok(StandaloneSftpProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

pub(crate) fn make_standalone_sftp_profile_portable(profile: &mut StandaloneSftpProfile) {
    profile.auth = portable_mosh_auth(&profile.auth);
    for hop in &mut profile.proxy_chain {
        hop.auth = portable_mosh_auth(&hop.auth);
    }
    profile.upstream_proxy = portable_upstream_proxy(&profile.upstream_proxy);
    if profile.proxy_command.is_some() {
        profile.proxy_command = Some(SavedProxyCommand {
            keychain_id: None,
            plaintext_command: None,
        });
    }
    if let Some(endpoint) = &mut profile.secondary_endpoint {
        make_standalone_sftp_endpoint_portable(endpoint);
    }
}

fn make_standalone_sftp_endpoint_portable(endpoint: &mut StandaloneSftpEndpoint) {
    endpoint.auth = portable_mosh_auth(&endpoint.auth);
    for hop in &mut endpoint.proxy_chain {
        hop.auth = portable_mosh_auth(&hop.auth);
    }
    endpoint.upstream_proxy = portable_upstream_proxy(&endpoint.upstream_proxy);
    if endpoint.proxy_command.is_some() {
        endpoint.proxy_command = Some(SavedProxyCommand {
            keychain_id: None,
            plaintext_command: None,
        });
    }
}

fn portable_mosh_auth(auth: &SavedAuth) -> SavedAuth {
    match auth {
        SavedAuth::Password { .. } => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
        SavedAuth::Key { key_path, .. } => SavedAuth::Key {
            key_path: key_path.clone(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        SavedAuth::ManagedKey { key_id, .. } => SavedAuth::ManagedKey {
            key_id: key_id.clone(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        SavedAuth::Certificate {
            key_path, cert_path, ..
        } => SavedAuth::Certificate {
            key_path: key_path.clone(),
            cert_path: cert_path.clone(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        SavedAuth::KeyboardInteractive => SavedAuth::KeyboardInteractive,
        SavedAuth::Agent => SavedAuth::Agent,
        SavedAuth::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => SavedAuth::with_kerberos_preferred(
            portable_mosh_auth(fallback),
            server_identity.clone(),
            *delegate_credentials,
        ),
    }
}

fn conventional_auth_target_matches(left: &SavedAuth, right: &SavedAuth) -> bool {
    let left = left.conventional_fallback();
    let right = right.conventional_fallback();
    match (left, right) {
        (SavedAuth::Password { .. }, SavedAuth::Password { .. })
        | (SavedAuth::KeyboardInteractive, SavedAuth::KeyboardInteractive)
        | (SavedAuth::Agent, SavedAuth::Agent) => true,
        (
            SavedAuth::Key { key_path: left, .. },
            SavedAuth::Key { key_path: right, .. },
        ) => left == right,
        (
            SavedAuth::ManagedKey { key_id: left, .. },
            SavedAuth::ManagedKey { key_id: right, .. },
        ) => left == right,
        (
            SavedAuth::Certificate {
                key_path: left_key,
                cert_path: left_cert,
                ..
            },
            SavedAuth::Certificate {
                key_path: right_key,
                cert_path: right_cert,
                ..
            },
        ) => left_key == right_key && left_cert == right_cert,
        _ => false,
    }
}

fn conventional_fallback_mut(auth: &mut SavedAuth) -> &mut SavedAuth {
    match auth {
        SavedAuth::KerberosPreferred { fallback, .. } => conventional_fallback_mut(fallback),
        auth => auth,
    }
}

fn preserve_local_auth_secret(incoming: &mut SavedAuth, existing: &SavedAuth) {
    if !conventional_auth_target_matches(incoming, existing) {
        return;
    }
    let incoming = conventional_fallback_mut(incoming);
    let existing = existing.conventional_fallback();
    // Only device-local protected references and zeroizing legacy owners cross this boundary.
    match (incoming, existing) {
        (
            SavedAuth::Password {
                keychain_id: incoming_keychain_id,
                plaintext_password: incoming_plaintext,
            },
            SavedAuth::Password {
                keychain_id: existing_keychain_id,
                plaintext_password: existing_plaintext,
            },
        ) => {
            *incoming_keychain_id = existing_keychain_id.clone();
            *incoming_plaintext = existing_plaintext.clone();
        }
        (
            SavedAuth::Key {
                has_passphrase: incoming_has_passphrase,
                passphrase_keychain_id: incoming_keychain_id,
                plaintext_passphrase: incoming_plaintext,
                ..
            },
            SavedAuth::Key {
                has_passphrase: existing_has_passphrase,
                passphrase_keychain_id: existing_keychain_id,
                plaintext_passphrase: existing_plaintext,
                ..
            },
        )
        | (
            SavedAuth::Certificate {
                has_passphrase: incoming_has_passphrase,
                passphrase_keychain_id: incoming_keychain_id,
                plaintext_passphrase: incoming_plaintext,
                ..
            },
            SavedAuth::Certificate {
                has_passphrase: existing_has_passphrase,
                passphrase_keychain_id: existing_keychain_id,
                plaintext_passphrase: existing_plaintext,
                ..
            },
        ) => {
            *incoming_has_passphrase = *existing_has_passphrase;
            *incoming_keychain_id = existing_keychain_id.clone();
            *incoming_plaintext = existing_plaintext.clone();
        }
        (
            SavedAuth::ManagedKey {
                passphrase_keychain_id: incoming_keychain_id,
                plaintext_passphrase: incoming_plaintext,
                ..
            },
            SavedAuth::ManagedKey {
                passphrase_keychain_id: existing_keychain_id,
                plaintext_passphrase: existing_plaintext,
                ..
            },
        ) => {
            *incoming_keychain_id = existing_keychain_id.clone();
            *incoming_plaintext = existing_plaintext.clone();
        }
        _ => {}
    }
}

fn preserve_proxy_chain_local_secrets(
    incoming_proxy_chain: &mut [SavedProxyHop],
    existing_proxy_chain: &[SavedProxyHop],
) {
    for hop in incoming_proxy_chain {
        if let Some(existing_hop) = existing_proxy_chain.iter().find(|candidate| {
            candidate.host == hop.host
                && candidate.port == hop.port
                && candidate.username == hop.username
                && conventional_auth_target_matches(&candidate.auth, &hop.auth)
        }) {
            preserve_local_auth_secret(&mut hop.auth, &existing_hop.auth);
        }
    }
}

fn preserve_standalone_sftp_local_secrets(
    incoming: &mut StandaloneSftpProfile,
    existing: &StandaloneSftpProfile,
) {
    preserve_standalone_sftp_endpoint_local_secrets(
        &mut incoming.auth,
        &mut incoming.proxy_chain,
        &mut incoming.upstream_proxy,
        &mut incoming.proxy_command,
        &existing.auth,
        &existing.proxy_chain,
        &existing.upstream_proxy,
        existing.proxy_command.as_ref(),
    );
    if let (Some(incoming_endpoint), Some(existing_endpoint)) = (
        &mut incoming.secondary_endpoint,
        &existing.secondary_endpoint,
    ) {
        preserve_standalone_sftp_endpoint_local_secrets(
            &mut incoming_endpoint.auth,
            &mut incoming_endpoint.proxy_chain,
            &mut incoming_endpoint.upstream_proxy,
            &mut incoming_endpoint.proxy_command,
            &existing_endpoint.auth,
            &existing_endpoint.proxy_chain,
            &existing_endpoint.upstream_proxy,
            existing_endpoint.proxy_command.as_ref(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn preserve_standalone_sftp_endpoint_local_secrets(
    incoming_auth: &mut SavedAuth,
    incoming_proxy_chain: &mut [SavedProxyHop],
    incoming_upstream_proxy: &mut SavedUpstreamProxyPolicy,
    incoming_proxy_command: &mut Option<SavedProxyCommand>,
    existing_auth: &SavedAuth,
    existing_proxy_chain: &[SavedProxyHop],
    existing_upstream_proxy: &SavedUpstreamProxyPolicy,
    existing_proxy_command: Option<&SavedProxyCommand>,
) {
    preserve_local_auth_secret(incoming_auth, existing_auth);
    for hop in incoming_proxy_chain {
        if let Some(existing_hop) = existing_proxy_chain.iter().find(|candidate| {
            candidate.host == hop.host
                && candidate.port == hop.port
                && candidate.username == hop.username
                && conventional_auth_target_matches(&candidate.auth, &hop.auth)
        }) {
            preserve_local_auth_secret(&mut hop.auth, &existing_hop.auth);
        }
    }
    preserve_standalone_sftp_upstream_proxy_secret(
        incoming_upstream_proxy,
        existing_upstream_proxy,
    );
    if incoming_proxy_command.is_some() && existing_proxy_command.is_some() {
        *incoming_proxy_command = existing_proxy_command.cloned();
    }
}

fn preserve_standalone_sftp_upstream_proxy_secret(
    incoming: &mut SavedUpstreamProxyPolicy,
    existing: &SavedUpstreamProxyPolicy,
) {
    let (
        SavedUpstreamProxyPolicy::Custom {
            proxy: incoming_proxy,
        },
        SavedUpstreamProxyPolicy::Custom {
            proxy: existing_proxy,
        },
    ) = (incoming, existing)
    else {
        return;
    };
    if incoming_proxy.protocol == existing_proxy.protocol
        && incoming_proxy.host == existing_proxy.host
        && incoming_proxy.port == existing_proxy.port
        && matches!(
            (&incoming_proxy.auth, &existing_proxy.auth),
            (
                SavedUpstreamProxyAuth::Password { username: left, .. },
                SavedUpstreamProxyAuth::Password { username: right, .. }
            ) if left == right
        )
    {
        incoming_proxy.auth = existing_proxy.auth.clone();
    }
}

fn build_remote_desktop_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<RemoteDesktopProfilesSyncSnapshot> {
    let mut records = data.remote_desktop_profiles.clone();
    for profile in &mut records {
        // Snapshots are portable asset metadata, never a transport for local credential handles.
        profile.credential_ref = None;
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;

    Ok(RemoteDesktopProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_saved_connection_sync_record(
    connection: &SavedConnection,
) -> Result<SavedConnectionSyncRecord> {
    let mut payload = ConnectionInfo::from(connection);
    // Protected-store references identify one device and must never enter a portable snapshot.
    payload.upstream_proxy = portable_upstream_proxy(&payload.upstream_proxy);
    let options = connection.options.clone();
    Ok(SavedConnectionSyncRecord {
        id: connection.id.clone(),
        revision: sha256_hex(&(&payload, &options))?,
        updated_at: connection_sync_updated_at(connection).to_rfc3339(),
        deleted: false,
        payload: Some(payload),
        options: Some(options),
    })
}

fn build_saved_connection_tombstone_record(
    tombstone: &DeletedConnectionTombstone,
) -> Result<SavedConnectionSyncRecord> {
    Ok(SavedConnectionSyncRecord {
        id: tombstone.id.clone(),
        revision: sha256_hex(&(
            tombstone.id.as_str(),
            tombstone.deleted_at.to_rfc3339(),
            true,
        ))?,
        updated_at: tombstone.deleted_at.to_rfc3339(),
        deleted: true,
        payload: None,
        options: None,
    })
}

fn connection_sync_updated_at(connection: &SavedConnection) -> DateTime<Utc> {
    connection
        .updated_at
        .or(connection.last_used_at)
        .unwrap_or(connection.created_at)
}

fn parse_connection_sync_timestamp(value: &str, field_name: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .with_context(|| format!("Invalid {field_name} '{value}'"))
}

fn saved_auth_from_connection_info(payload: &ConnectionInfo) -> SavedAuth {
    let fallback = match payload.auth_type {
        AuthType::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
        AuthType::Key => SavedAuth::Key {
            key_path: payload.key_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::ManagedKey => SavedAuth::ManagedKey {
            key_id: payload.managed_key_id.clone().unwrap_or_default(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::Certificate => SavedAuth::Certificate {
            key_path: payload.key_path.clone().unwrap_or_default(),
            cert_path: payload.cert_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::KeyboardInteractive => SavedAuth::KeyboardInteractive,
        AuthType::Agent => SavedAuth::Agent,
    };
    if payload.gssapi_authentication {
        SavedAuth::with_kerberos_preferred(
            fallback,
            payload.gssapi_server_identity.clone(),
            payload.gssapi_delegate_credentials,
        )
    } else {
        fallback
    }
}

fn saved_auth_from_proxy_hop_info(hop: &ProxyHopInfo) -> SavedAuth {
    let fallback = match hop.auth_type {
        AuthType::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
        AuthType::Key => SavedAuth::Key {
            key_path: hop.key_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::ManagedKey => SavedAuth::ManagedKey {
            key_id: hop.managed_key_id.clone().unwrap_or_default(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::Certificate => SavedAuth::Certificate {
            key_path: hop.key_path.clone().unwrap_or_default(),
            cert_path: hop.cert_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthType::KeyboardInteractive => SavedAuth::KeyboardInteractive,
        AuthType::Agent => SavedAuth::Agent,
    };
    if hop.gssapi_authentication {
        SavedAuth::with_kerberos_preferred(
            fallback,
            hop.gssapi_server_identity.clone(),
            hop.gssapi_delegate_credentials,
        )
    } else {
        fallback
    }
}

fn build_synced_proxy_chain(
    proxy_chain: &[ProxyHopInfo],
    existing: Option<&SavedConnection>,
    preserve_auth: bool,
) -> Vec<SavedProxyHop> {
    proxy_chain
        .iter()
        .map(|hop| {
            let mut auth = saved_auth_from_proxy_hop_info(hop);
            if preserve_auth
                && let Some(existing_auth) = existing.and_then(|connection| {
                    connection
                        .proxy_chain
                        .iter()
                        .find(|candidate| {
                            candidate.host == hop.host
                                && candidate.port == hop.port
                                && candidate.username == hop.username
                        })
                        .map(|candidate| &candidate.auth)
                })
            {
                preserve_local_auth_secret(&mut auth, existing_auth);
            }
            SavedProxyHop {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth,
                agent_forwarding: hop.agent_forwarding,
                identity_agent: hop.identity_agent.clone(),
                agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                ssh_algorithms: hop.ssh_algorithms.clone(),
            }
        })
        .collect()
}

fn portable_upstream_proxy(policy: &SavedUpstreamProxyPolicy) -> SavedUpstreamProxyPolicy {
    match policy {
        SavedUpstreamProxyPolicy::Custom { proxy } => {
            let mut proxy = proxy.clone();
            if let SavedUpstreamProxyAuth::Password {
                username,
                ..
            } = &proxy.auth
            {
                proxy.auth = SavedUpstreamProxyAuth::Password {
                    username: username.clone(),
                    keychain_id: None,
                    plaintext_password: None,
                };
            }
            SavedUpstreamProxyPolicy::Custom { proxy }
        }
        SavedUpstreamProxyPolicy::UseGlobal => SavedUpstreamProxyPolicy::UseGlobal,
        SavedUpstreamProxyPolicy::Direct => SavedUpstreamProxyPolicy::Direct,
    }
}

fn build_synced_upstream_proxy(
    incoming: &SavedUpstreamProxyPolicy,
    existing: Option<&SavedConnection>,
    preserve_auth: bool,
) -> SavedUpstreamProxyPolicy {
    let mut synced_policy = portable_upstream_proxy(incoming);
    if !preserve_auth {
        return synced_policy;
    }
    let (
        SavedUpstreamProxyPolicy::Custom {
            proxy: incoming_proxy,
        },
        Some(SavedUpstreamProxyPolicy::Custom {
            proxy: existing_proxy,
        }),
    ) = (
        &mut synced_policy,
        existing.map(|connection| &connection.upstream_proxy),
    )
    else {
        return synced_policy;
    };
    let (
        SavedUpstreamProxyAuth::Password {
            username: incoming_username,
            keychain_id: incoming_keychain_id,
            ..
        },
        SavedUpstreamProxyAuth::Password {
            username: existing_username,
            keychain_id: existing_keychain_id,
            ..
        },
    ) = (&mut incoming_proxy.auth, &existing_proxy.auth)
    else {
        return synced_policy;
    };
    if incoming_proxy.protocol == existing_proxy.protocol
        && incoming_proxy.host == existing_proxy.host
        && incoming_proxy.port == existing_proxy.port
        && incoming_username == existing_username
    {
        *incoming_keychain_id = existing_keychain_id.clone();
    }
    synced_policy
}

fn active_connection_tombstones(
    tombstones: &[DeletedConnectionTombstone],
) -> Vec<DeletedConnectionTombstone> {
    let cutoff = Utc::now() - Duration::days(CONNECTION_TOMBSTONE_RETENTION_DAYS);
    tombstones
        .iter()
        .filter(|tombstone| tombstone.deleted_at >= cutoff)
        .cloned()
        .collect()
}

fn sha256_hex<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("Failed to serialize sync payload")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod mosh_tests {
    use super::*;

    fn password_profile(keychain_id: Option<&str>) -> MoshProfile {
        MoshProfile::new(
            "Mobile shell",
            "example.test",
            22,
            "alice",
            SavedAuth::Password {
                keychain_id: keychain_id.map(str::to_string),
                plaintext_password: None,
            },
        )
    }

    #[test]
    fn mosh_snapshot_strips_device_local_credential_references() {
        let mut profile = password_profile(Some("local-keychain-entry"));
        profile.proxy_chain.push(SavedProxyHop {
            host: "jump.example.test".to_string(),
            port: 22,
            username: "jump".to_string(),
            auth: SavedAuth::Password {
                keychain_id: Some("local-proxy-keychain-entry".to_string()),
                plaintext_password: None,
            },
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        });
        let data = ConnectionStoreData {
            mosh_profiles: vec![profile],
            ..ConnectionStoreData::default()
        };

        let snapshot = build_mosh_profiles_sync_snapshot(&data).expect("snapshot must build");

        assert_eq!(snapshot.records.len(), 1);
        assert!(matches!(
            snapshot.records[0].auth,
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None
            }
        ));
        let json = serde_json::to_string(&snapshot).expect("snapshot must serialize");
        assert!(!json.contains("local-keychain-entry"));
        assert!(!json.contains("local-proxy-keychain-entry"));
    }

    #[test]
    fn mosh_snapshot_apply_preserves_matching_local_credential() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-mosh-sync-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut store = ConnectionStore::load(&path).expect("store must load");
        let mut local = password_profile(Some("local-keychain-entry"));
        local.proxy_chain.push(SavedProxyHop {
            host: "jump.example.test".to_string(),
            port: 22,
            username: "jump".to_string(),
            auth: SavedAuth::Password {
                keychain_id: Some("local-proxy-keychain-entry".to_string()),
                plaintext_password: None,
            },
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        });
        let mut incoming = local.clone();
        incoming.name = "Renamed mobile shell".to_string();
        incoming.updated_at = local.updated_at + chrono::Duration::seconds(1);
        incoming.auth = SavedAuth::with_kerberos_preferred(
            portable_mosh_auth(&incoming.auth),
            Some("host/mosh.example.test".to_string()),
            true,
        );
        for hop in &mut incoming.proxy_chain {
            hop.auth = SavedAuth::with_kerberos_preferred(
                portable_mosh_auth(&hop.auth),
                Some("host/jump.example.test".to_string()),
                false,
            );
        }
        store.data.mosh_profiles.push(local);

        let applied = store
            .apply_mosh_profiles_snapshot(MoshProfilesSyncSnapshot {
                revision: "remote-revision".to_string(),
                exported_at: Utc::now().to_rfc3339(),
                records: vec![incoming],
            })
            .expect("snapshot must apply");

        assert_eq!(applied, 1);
        let profile = &store.mosh_profiles()[0];
        assert_eq!(profile.name, "Renamed mobile shell");
        assert!(matches!(
            &profile.auth,
            SavedAuth::KerberosPreferred {
                server_identity: Some(server_identity),
                delegate_credentials: true,
                fallback,
            } if server_identity == "host/mosh.example.test"
                && matches!(
                    fallback.as_ref(),
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,
                    } if keychain_id == "local-keychain-entry"
                )
        ));
        assert!(matches!(
            &profile.proxy_chain[0].auth,
            SavedAuth::KerberosPreferred {
                server_identity: Some(server_identity),
                delegate_credentials: false,
                fallback,
            } if server_identity == "host/jump.example.test"
                && matches!(
                    fallback.as_ref(),
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,
                    } if keychain_id == "local-proxy-keychain-entry"
                )
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mosh_runtime_secret_debug_is_redacted() {
        let secret = "mosh-bootstrap-password";
        let runtime = SavedMoshProfileRuntimeSecrets {
            auth: Some(SecretString::from(secret)),
            proxy_chain: vec![Some(SecretString::from("proxy-secret"))],
        };

        let debug = format!("{runtime:?}");
        assert!(!debug.contains(secret));
        assert!(!debug.contains("proxy-secret"));
        assert!(debug.contains("redacted secret"));
    }
}
