impl ConnectionStore {
    pub fn export_saved_connections_snapshot(&self) -> Result<SavedConnectionsSyncSnapshot> {
        build_saved_connections_sync_snapshot(&self.data)
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
        let original_data = self.data.clone();
        let mut result = ApplySavedConnectionsSyncSnapshotResult::default();
        let mut deleted_connection_ids = Vec::new();
        let mut keychain_ids_to_delete = Vec::new();

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
                    if existing_by_id
                        .get(&record.id)
                        .is_some_and(|existing| connection_sync_updated_at(existing) > record_updated_at)
                    {
                        result.skipped += 1;
                        result.conflicts += 1;
                        continue;
                    }

                    if let Some(removed) =
                        self.remove_connection_with_tombstone_at(&record.id, record_updated_at)
                    {
                        deleted_connection_ids.push(removed.id.clone());
                        keychain_ids_to_delete.extend(collect_connection_keychain_ids(&removed));
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
                        .find(|candidate| candidate.name == payload.name && candidate.id != record.id)
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
                    record_updated_at,
                    baseline,
                    baseline.is_some() && strategy.preserves_local_auth(),
                )?;

                if let Some(existing) = baseline {
                    let existing_keychain_ids: HashSet<String> =
                        collect_connection_keychain_ids(existing).into_iter().collect();
                    let next_keychain_ids: HashSet<String> =
                        collect_connection_keychain_ids(&next_connection).into_iter().collect();
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
            self.data = original_data;
            let _ = self.save();
            return Err(error);
        }

        for keychain_id in keychain_ids_to_delete {
            let _ = self.keychain.delete(&keychain_id);
        }

        Ok(ApplySavedConnectionsSyncOutcome {
            result,
            deleted_connection_ids,
        })
    }
}

fn build_saved_connection_from_sync_payload(
    payload: &ConnectionInfo,
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
        options: ConnectionOptions {
            agent_forwarding: payload.agent_forwarding,
            ..Default::default()
        },
        created_at: parse_connection_sync_timestamp(&payload.created_at, "connection created_at")?,
        last_used_at: payload
            .last_used_at
            .as_deref()
            .map(|value| parse_connection_sync_timestamp(value, "connection last_used_at"))
            .transpose()?,
        updated_at: Some(record_updated_at),
        color: payload.color.clone(),
        tags: payload.tags.clone(),
        post_connect_command: payload.post_connect_command.clone(),
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
            .map(|record| (&record.id, &record.revision, record.deleted))
            .collect::<Vec<_>>(),
    )?;

    Ok(SavedConnectionsSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn build_saved_connection_sync_record(
    connection: &SavedConnection,
) -> Result<SavedConnectionSyncRecord> {
    let payload = ConnectionInfo::from(connection);
    Ok(SavedConnectionSyncRecord {
        id: connection.id.clone(),
        revision: sha256_hex(&payload)?,
        updated_at: connection_sync_updated_at(connection).to_rfc3339(),
        deleted: false,
        payload: Some(payload),
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
        AuthType::Certificate => SavedAuth::Certificate {
            key_path: payload.key_path.clone().unwrap_or_default(),
            cert_path: payload.cert_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
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
        AuthType::Certificate => SavedAuth::Certificate {
            key_path: hop.key_path.clone().unwrap_or_default(),
            cert_path: hop.cert_path.clone().unwrap_or_default(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
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
