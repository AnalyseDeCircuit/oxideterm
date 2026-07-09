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
                    fs::remove_file(&self.path).with_context(|| {
                        format!("failed to remove current store {}", self.path.display())
                    })?;
                    let parent = self
                        .path
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .unwrap_or_else(|| Path::new("."));
                    sync_directory(parent).with_context(|| {
                        format!("failed to sync store directory {}", parent.display())
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

    pub fn export_raw_tcp_profiles_snapshot(&self) -> Result<RawTcpProfilesSyncSnapshot> {
        build_raw_tcp_profiles_sync_snapshot(&self.data)
    }

    pub fn export_raw_udp_profiles_snapshot(&self) -> Result<RawUdpProfilesSyncSnapshot> {
        build_raw_udp_profiles_sync_snapshot(&self.data)
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

    pub fn apply_raw_tcp_profiles_snapshot(
        &mut self,
        snapshot: RawTcpProfilesSyncSnapshot,
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
                .raw_tcp_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.raw_tcp_profiles.push(profile);
                applied += 1;
            }
        }
        if applied > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(applied)
    }

    pub fn apply_raw_udp_profiles_snapshot(
        &mut self,
        snapshot: RawUdpProfilesSyncSnapshot,
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
                .raw_udp_profiles
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                if profile.updated_at >= existing.updated_at {
                    *existing = profile;
                    applied += 1;
                }
            } else {
                self.data.raw_udp_profiles.push(profile);
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
    let auth = if preserve_auth {
        existing
            .map(|connection| connection.auth.clone())
            .unwrap_or_else(|| saved_auth_from_connection_info(payload))
    } else {
        saved_auth_from_connection_info(payload)
    };
    let proxy_chain = build_synced_proxy_chain(&payload.proxy_chain, existing, preserve_auth);

    Ok(SavedConnection {
        id: payload.id.clone(),
        version: CONFIG_VERSION,
        name: non_empty(payload.name.trim(), "Connection name")?.to_string(),
        group: normalize_optional_group_name(payload.group.as_deref())?,
        host: non_empty(payload.host.trim(), "Host")?.to_string(),
        port: payload.port.max(1),
        username: non_empty(payload.username.trim(), "Username")?.to_string(),
        auth,
        proxy_chain,
        upstream_proxy: payload.upstream_proxy.clone(),
        options: synced_options
            .cloned()
            .unwrap_or_else(|| ConnectionOptions {
                // Older snapshots exposed only these three option fields through
                // ConnectionInfo, so retain that wire-compatible fallback.
                agent_forwarding: payload.agent_forwarding,
                legacy_ssh_compatibility: payload.legacy_ssh_compatibility,
                post_connect_command: payload.post_connect_command.clone(),
                ..Default::default()
            }),
        created_at: parse_connection_sync_timestamp(&payload.created_at, "connection created_at")?,
        last_used_at: payload
            .last_used_at
            .as_deref()
            .map(|value| parse_connection_sync_timestamp(value, "connection last_used_at"))
            .transpose()?,
        updated_at: Some(record_updated_at),
        color: payload.color.clone(),
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

fn build_raw_tcp_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<RawTcpProfilesSyncSnapshot> {
    let mut records = data.raw_tcp_profiles.clone();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;

    Ok(RawTcpProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_raw_udp_profiles_sync_snapshot(
    data: &ConnectionStoreData,
) -> Result<RawUdpProfilesSyncSnapshot> {
    let mut records = data.raw_udp_profiles.clone();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|profile| (&profile.id, profile.updated_at.to_rfc3339()))
            .collect::<Vec<_>>(),
    )?;

    Ok(RawUdpProfilesSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_saved_connection_sync_record(
    connection: &SavedConnection,
) -> Result<SavedConnectionSyncRecord> {
    let payload = ConnectionInfo::from(connection);
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
    match payload.auth_type {
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
    }
}

fn saved_auth_from_proxy_hop_info(hop: &ProxyHopInfo) -> SavedAuth {
    match hop.auth_type {
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
            let preserved_auth = preserve_auth.then(|| {
                existing.and_then(|connection| {
                    connection
                        .proxy_chain
                        .iter()
                        .find(|candidate| {
                            candidate.host == hop.host
                                && candidate.port == hop.port
                                && candidate.username == hop.username
                        })
                        .map(|candidate| candidate.auth.clone())
                })
            });
            SavedProxyHop {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: preserved_auth
                    .flatten()
                    .unwrap_or_else(|| saved_auth_from_proxy_hop_info(hop)),
                agent_forwarding: hop.agent_forwarding,
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            }
        })
        .collect()
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
