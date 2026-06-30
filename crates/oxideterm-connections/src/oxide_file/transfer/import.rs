pub fn apply_oxide_import(
    store: &mut ConnectionStore,
    bytes: &[u8],
    password: &str,
    strategy: ImportConflictStrategy,
) -> Result<ImportResultEnvelope, OxideFileError> {
    apply_oxide_import_with_options(
        store,
        bytes,
        password,
        OxideImportOptions {
            conflict_strategy: strategy,
            ..OxideImportOptions::default()
        },
    )
}

pub fn apply_oxide_import_with_options(
    store: &mut ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
) -> Result<ImportResultEnvelope, OxideFileError> {
    apply_oxide_import_with_options_inner(store, bytes, password, options, None)
}

pub fn apply_oxide_import_with_options_with_progress<F>(
    store: &mut ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
    mut on_progress: F,
) -> Result<ImportResultEnvelope, OxideFileError>
where
    F: FnMut(&str, usize, usize),
{
    apply_oxide_import_with_options_inner(store, bytes, password, options, Some(&mut on_progress))
}

fn apply_oxide_import_with_options_inner(
    store: &mut ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
    mut on_progress: Option<&mut dyn FnMut(&str, usize, usize)>,
) -> Result<ImportResultEnvelope, OxideFileError> {
    const APPLY_IMPORT_TOTAL_STEPS: usize = 10;
    let mut current_step = 1usize;
    let mut report_progress = |stage: &str, current: usize| {
        if let Some(callback) = on_progress.as_deref_mut() {
            callback(stage, current, APPLY_IMPORT_TOTAL_STEPS);
        }
    };
    let file = OxideFile::from_bytes(bytes)?;
    report_progress("parsing_file", current_step);
    let payload = decrypt_oxide_file_with_progress(&file, password, |stage| {
        current_step += 1;
        report_progress(stage, current_step);
    })?;
    let EncryptedPayload {
        connections,
        app_settings_json,
        quick_commands_json,
        serial_profiles_json,
        raw_tcp_profiles_json,
        raw_udp_profiles_json,
        plugin_settings,
        portable_secrets,
        ..
    } = payload;
    current_step += 1;
    report_progress("filtering_selection", current_step);
    let mut selected_connections =
        filter_selected_connections(connections, options.selected_names.as_ref());
    let total_available_forwards = selected_connections
        .iter()
        .map(|connection| connection.forwards.len())
        .sum::<usize>();
    let forward_filter = options.selected_forward_ids.as_ref();
    let forward_selection = filter_selected_forward_ids(&mut selected_connections, forward_filter);
    let total_selected_forwards = selected_connections
        .iter()
        .map(|connection| connection.forwards.len())
        .sum::<usize>();
    current_step += 1;
    report_progress("collecting_existing", current_step);
    let plans = plan_import(store, &selected_connections, options.conflict_strategy);
    let mut result = ImportResultEnvelope {
        app_settings_json,
        quick_commands_json,
        serial_profiles_json: serial_profiles_json.clone(),
        raw_tcp_profiles_json: raw_tcp_profiles_json.clone(),
        raw_udp_profiles_json: raw_udp_profiles_json.clone(),
        plugin_settings,
        portable_secrets: if options.import_portable_secrets {
            portable_secrets.clone()
        } else {
            Vec::new()
        },
        ..ImportResultEnvelope::default()
    };
    result.skipped_forwards += forward_selection.skipped;
    result.errors.extend(
        forward_selection
            .missing
            .into_iter()
            .map(|id| format!("Forward id not found in .oxide payload: {id}")),
    );
    let credential_counts =
        count_sensitive_credentials_for_import(&selected_connections, &plans, &options);
    result.restored_connection_passwords = credential_counts.restored_connection_passwords;
    result.restored_key_passphrases = credential_counts.restored_key_passphrases;
    result.restored_managed_keys = credential_counts.restored_managed_keys;
    result.restored_managed_key_passphrases =
        credential_counts.restored_managed_key_passphrases;
    result.restored_privilege_credentials = credential_counts.restored_privilege_credentials;
    result.skipped_sensitive_credentials = credential_counts.skipped_sensitive_credentials;
    let mut connections_to_save = Vec::new();
    let mut restored_managed_keys = HashMap::new();
    let mut imported_managed_keys = Vec::new();

    current_step += 1;
    report_progress("preparing_connections", current_step);
    for (conn, action) in selected_connections.into_iter().zip(plans) {
        match action {
            PlannedImportAction::Skip => {
                result.skipped += 1;
            }
            PlannedImportAction::Import => {
                let saved = encrypted_connection_to_saved(
                    store,
                    conn,
                    None,
                    None,
                    &mut restored_managed_keys,
                    &mut imported_managed_keys,
                    &options,
                )?;
                if options.import_forwards {
                    result.imported_forwards += saved.1.len();
                    result.forward_records.extend(saved.1);
                }
                connections_to_save.push(saved.0);
                result.imported += 1;
            }
            PlannedImportAction::Rename(new_name) => {
                let original = conn.name.clone();
                let saved = encrypted_connection_to_saved(
                    store,
                    conn,
                    Some(new_name.clone()),
                    None,
                    &mut restored_managed_keys,
                    &mut imported_managed_keys,
                    &options,
                )?;
                if options.import_forwards {
                    result.imported_forwards += saved.1.len();
                    result.forward_records.extend(saved.1);
                }
                connections_to_save.push(saved.0);
                result.imported += 1;
                result.renamed += 1;
                result.renames.push((original, new_name));
            }
            PlannedImportAction::Replace(existing_id) => {
                let saved = encrypted_connection_to_saved(
                    store,
                    conn,
                    None,
                    Some(existing_id.clone()),
                    &mut restored_managed_keys,
                    &mut imported_managed_keys,
                    &options,
                )?;
                if options.import_forwards {
                    result.imported_forwards += saved.1.len();
                    result.forward_records.extend(saved.1);
                    result.forward_replace_owner_ids.push(existing_id.clone());
                }
                connections_to_save.push(saved.0);
                result.imported += 1;
                result.replaced += 1;
            }
            PlannedImportAction::Merge(existing_id) => {
                let existing = store.get(&existing_id).cloned();
                let saved = encrypted_connection_to_saved(
                    store,
                    conn,
                    None,
                    Some(existing_id.clone()),
                    &mut restored_managed_keys,
                    &mut imported_managed_keys,
                    &options,
                )?;
                let (saved_connection, forward_records) = saved;
                let merged = if let Some(existing) = existing {
                    merge_saved_connection(existing, saved_connection)
                } else {
                    saved_connection
                };
                if options.import_forwards {
                    result.imported_forwards += forward_records.len();
                    result.forward_records.extend(forward_records);
                    result.forward_merge_owner_ids.push(existing_id);
                }
                connections_to_save.push(merged);
                result.imported += 1;
                result.merged += 1;
            }
        }
    }

    current_step += 1;
    report_progress("applying_connections", current_step);
    store.upsert_imported_connections_and_managed_keys_transaction(
        connections_to_save,
        imported_managed_keys,
    )?;

    current_step += 1;
    report_progress("saving_config", current_step);
    if !options.import_forwards {
        result.skipped_forwards = total_selected_forwards;
    } else if forward_filter.is_some() {
        result.skipped_forwards = total_available_forwards.saturating_sub(total_selected_forwards);
    }

    if let Some(snapshot_json) = serial_profiles_json {
        let serial_profiles_snapshot: SerialProfilesSyncSnapshot =
            serde_json::from_str(&snapshot_json).map_err(|error| {
                OxideFileError::InvalidFormat(format!(
                    "Invalid serial profiles snapshot in .oxide payload: {error}"
                ))
            })?;
        let serial_profiles_count = serial_profiles_snapshot.records.len();
        if options.import_serial_profiles {
            result.imported_serial_profiles =
                store.apply_serial_profiles_snapshot(serial_profiles_snapshot).map_err(|error| {
                    OxideFileError::InvalidFormat(format!(
                        "Failed to import serial profiles from .oxide payload: {error}"
                    ))
                })?;
            result.skipped_serial_profiles =
                serial_profiles_count.saturating_sub(result.imported_serial_profiles);
        } else {
            result.skipped_serial_profiles = serial_profiles_count;
        }
    }

    if let Some(snapshot_json) = raw_tcp_profiles_json {
        let raw_tcp_profiles_snapshot: RawTcpProfilesSyncSnapshot =
            serde_json::from_str(&snapshot_json).map_err(|error| {
                OxideFileError::InvalidFormat(format!(
                    "Invalid Raw TCP profiles snapshot in .oxide payload: {error}"
                ))
            })?;
        let raw_tcp_profiles_count = raw_tcp_profiles_snapshot.records.len();
        if options.import_raw_tcp_profiles {
            result.imported_raw_tcp_profiles = store
                .apply_raw_tcp_profiles_snapshot(raw_tcp_profiles_snapshot)
                .map_err(|error| {
                    OxideFileError::InvalidFormat(format!(
                        "Failed to import Raw TCP profiles from .oxide payload: {error}"
                    ))
                })?;
            result.skipped_raw_tcp_profiles =
                raw_tcp_profiles_count.saturating_sub(result.imported_raw_tcp_profiles);
        } else {
            result.skipped_raw_tcp_profiles = raw_tcp_profiles_count;
        }
    }

    if let Some(snapshot_json) = raw_udp_profiles_json {
        let raw_udp_profiles_snapshot: RawUdpProfilesSyncSnapshot =
            serde_json::from_str(&snapshot_json).map_err(|error| {
                OxideFileError::InvalidFormat(format!(
                    "Invalid Raw UDP profiles snapshot in .oxide payload: {error}"
                ))
            })?;
        let raw_udp_profiles_count = raw_udp_profiles_snapshot.records.len();
        if options.import_raw_udp_profiles {
            result.imported_raw_udp_profiles = store
                .apply_raw_udp_profiles_snapshot(raw_udp_profiles_snapshot)
                .map_err(|error| {
                    OxideFileError::InvalidFormat(format!(
                        "Failed to import Raw UDP profiles from .oxide payload: {error}"
                    ))
                })?;
            result.skipped_raw_udp_profiles =
                raw_udp_profiles_count.saturating_sub(result.imported_raw_udp_profiles);
        } else {
            result.skipped_raw_udp_profiles = raw_udp_profiles_count;
        }
    }

    if options.import_portable_secrets {
        for secret in &portable_secrets {
            if secret.kind == "ai_provider_key" && !secret.id.trim().is_empty() {
                result.imported_portable_secrets += 1;
            } else {
                result.skipped_portable_secrets += 1;
            }
        }
    } else {
        result.skipped_portable_secrets = portable_secrets.len();
    }

    Ok(result)
}

#[derive(Debug, Default)]
struct SensitiveCredentialImportCounts {
    restored_connection_passwords: usize,
    restored_key_passphrases: usize,
    restored_managed_keys: usize,
    restored_managed_key_passphrases: usize,
    restored_privilege_credentials: usize,
    skipped_sensitive_credentials: usize,
}

impl SensitiveCredentialImportCounts {
    fn add_restored(&mut self, other: Self) {
        self.restored_connection_passwords += other.restored_connection_passwords;
        self.restored_key_passphrases += other.restored_key_passphrases;
        self.restored_managed_keys += other.restored_managed_keys;
        self.restored_managed_key_passphrases += other.restored_managed_key_passphrases;
        self.restored_privilege_credentials += other.restored_privilege_credentials;
    }

    fn restored_total(&self) -> usize {
        self.restored_connection_passwords
            + self.restored_key_passphrases
            + self.restored_managed_keys
            + self.restored_managed_key_passphrases
            + self.restored_privilege_credentials
    }
}

fn count_sensitive_credentials_for_import(
    connections: &[EncryptedConnection],
    plans: &[PlannedImportAction],
    options: &OxideImportOptions,
) -> SensitiveCredentialImportCounts {
    let mut counts = SensitiveCredentialImportCounts::default();
    for (connection, plan) in connections.iter().zip(plans.iter()) {
        let connection_counts = count_sensitive_credentials_for_connection(connection, options);
        if matches!(plan, PlannedImportAction::Skip) {
            counts.skipped_sensitive_credentials += connection_counts.restored_total();
        } else {
            counts.add_restored(connection_counts);
        }
    }
    counts
}

fn count_sensitive_credentials_for_connection(
    connection: &EncryptedConnection,
    options: &OxideImportOptions,
) -> SensitiveCredentialImportCounts {
    let mut counts = SensitiveCredentialImportCounts::default();
    count_sensitive_credentials_for_auth(&connection.auth, options, &mut counts);
    for hop in &connection.proxy_chain {
        count_sensitive_credentials_for_auth(&hop.auth, options, &mut counts);
    }
    counts.restored_privilege_credentials += connection
        .privilege_credentials
        .iter()
        .filter(|credential| credential.secret.is_some())
        .count();
    counts
}

fn count_sensitive_credentials_for_auth(
    auth: &EncryptedAuth,
    options: &OxideImportOptions,
    counts: &mut SensitiveCredentialImportCounts,
) {
    // This reports only presence/count metadata; secret values stay in their
    // zeroizing archive owners and are never cloned into UI-facing summaries.
    match auth {
        EncryptedAuth::Password { password } => {
            if !password.is_empty() {
                counts.restored_connection_passwords += 1;
            }
        }
        EncryptedAuth::Key {
            passphrase,
            managed_key,
            ..
        } => {
            if managed_key.is_some() && options.restore_managed_keys {
                counts.restored_managed_keys += 1;
                if passphrase.is_some() && options.restore_managed_key_passphrases {
                    counts.restored_managed_key_passphrases += 1;
                }
            } else if passphrase.is_some() {
                counts.restored_key_passphrases += 1;
            }
        }
        EncryptedAuth::Certificate { passphrase, .. } => {
            if passphrase.is_some() {
                counts.restored_key_passphrases += 1;
            }
        }
        EncryptedAuth::KeyboardInteractive => {}
        EncryptedAuth::Agent => {}
    }
}

#[derive(Debug, Default)]
struct ForwardSelectionResult {
    skipped: usize,
    missing: Vec<String>,
}

fn filter_selected_forward_ids(
    connections: &mut [EncryptedConnection],
    selected_forward_ids: Option<&Vec<String>>,
) -> ForwardSelectionResult {
    let Some(selected_forward_ids) = selected_forward_ids else {
        return ForwardSelectionResult::default();
    };
    let requested = selected_forward_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    if requested.is_empty() {
        return ForwardSelectionResult::default();
    }

    let mut matched = HashSet::new();
    let mut skipped = 0usize;
    for connection in connections {
        connection.forwards.retain(|forward| {
            let keep = forward
                .id
                .as_ref()
                .is_some_and(|id| requested.contains(id));
            if keep {
                if let Some(id) = &forward.id {
                    matched.insert(id.clone());
                }
            } else {
                skipped += 1;
            }
            keep
        });
    }
    let missing = requested.difference(&matched).cloned().collect::<Vec<_>>();
    ForwardSelectionResult { skipped, missing }
}

fn encrypted_connection_to_saved(
    store: &ConnectionStore,
    conn: EncryptedConnection,
    name_override: Option<String>,
    id_override: Option<String>,
    restored_managed_keys: &mut HashMap<String, String>,
    imported_managed_keys: &mut Vec<ImportedManagedSshKey>,
    import_options: &OxideImportOptions,
) -> Result<(SavedConnection, Vec<OxideForwardRecord>), OxideFileError> {
    let id = id_override.unwrap_or_else(|| Uuid::new_v4().to_string());
    let credential_connection_id = id.clone();
    let forward_records = import_forwards(&id, conn.forwards);
    let mut options = conn.options;
    options.jump_host = None;
    let now = Utc::now();
    Ok((
        SavedConnection {
            id,
            version: CONFIG_VERSION,
            name: name_override.unwrap_or(conn.name),
            group: conn.group,
            host: conn.host,
            port: conn.port,
            username: conn.username,
            auth: import_auth(
                store,
                conn.auth,
                restored_managed_keys,
                imported_managed_keys,
                import_options,
            )?,
            proxy_chain: conn
                .proxy_chain
                .into_iter()
                .map(|hop| {
                    import_proxy_hop(
                        store,
                        hop,
                        restored_managed_keys,
                        imported_managed_keys,
                        import_options,
                    )
                })
                .collect::<Result<_, _>>()?,
            upstream_proxy: import_upstream_proxy_policy(conn.upstream_proxy),
            options,
            created_at: now,
            last_used_at: None,
            updated_at: Some(now),
            color: conn.color,
            tags: conn.tags,
            post_connect_command: None,
            privilege_credentials: import_privilege_credentials(
                &credential_connection_id,
                conn.privilege_credentials,
            ),
        },
        forward_records,
    ))
}

fn import_privilege_credentials(
    connection_id: &str,
    credentials: Vec<EncryptedPrivilegeCredential>,
) -> Vec<SavedPrivilegeCredential> {
    credentials
        .into_iter()
        .map(|credential| SavedPrivilegeCredential {
            id: credential.id,
            connection_id: connection_id.to_string(),
            label: credential.label,
            kind: credential.kind,
            username_hint: credential.username_hint,
            prompt_patterns: credential.prompt_patterns,
            keychain_id: None,
            plaintext_secret: credential.secret.map(SecretString::from),
            enabled: credential.enabled,
            require_click_to_send: credential.require_click_to_send,
            created_at: credential.created_at,
            updated_at: credential.updated_at,
        })
        .collect()
}

fn import_forwards(
    connection_id: &str,
    forwards: Vec<EncryptedForward>,
) -> Vec<OxideForwardRecord> {
    forwards
        .into_iter()
        .map(|forward| OxideForwardRecord {
            id: forward.id,
            connection_id: connection_id.to_string(),
            forward_type: forward.forward_type,
            bind_address: forward.bind_address,
            bind_port: forward.bind_port,
            target_host: forward.target_host,
            target_port: forward.target_port,
            description: forward.description,
            auto_start: forward.auto_start,
        })
        .collect()
}

fn import_proxy_hop(
    store: &ConnectionStore,
    hop: EncryptedProxyHop,
    restored_managed_keys: &mut HashMap<String, String>,
    imported_managed_keys: &mut Vec<ImportedManagedSshKey>,
    import_options: &OxideImportOptions,
) -> Result<SavedProxyHop, OxideFileError> {
    Ok(SavedProxyHop {
        host: hop.host,
        port: hop.port,
        username: hop.username,
        auth: import_auth(
            store,
            hop.auth,
            restored_managed_keys,
            imported_managed_keys,
            import_options,
        )?,
        agent_forwarding: false,
        legacy_ssh_compatibility: false,
    })
}

fn import_upstream_proxy_policy(policy: EncryptedUpstreamProxyPolicy) -> SavedUpstreamProxyPolicy {
    match policy {
        EncryptedUpstreamProxyPolicy::UseGlobal => SavedUpstreamProxyPolicy::UseGlobal,
        EncryptedUpstreamProxyPolicy::Direct => SavedUpstreamProxyPolicy::Direct,
        EncryptedUpstreamProxyPolicy::Custom { proxy } => SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: proxy.protocol,
                host: proxy.host,
                port: proxy.port,
                auth: import_upstream_proxy_auth(proxy.auth),
                remote_dns: proxy.remote_dns,
                no_proxy: proxy.no_proxy,
            },
        },
    }
}

fn import_upstream_proxy_auth(auth: EncryptedUpstreamProxyAuth) -> SavedUpstreamProxyAuth {
    match auth {
        EncryptedUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
        EncryptedUpstreamProxyAuth::Password { username } => {
            // Passwords and local keychain ids are never portable in .oxide files.
            SavedUpstreamProxyAuth::Password {
                username,
                keychain_id: None,
                plaintext_password: None,
            }
        }
    }
}

fn import_auth(
    store: &ConnectionStore,
    auth: EncryptedAuth,
    restored_managed_keys: &mut HashMap<String, String>,
    imported_managed_keys: &mut Vec<ImportedManagedSshKey>,
    import_options: &OxideImportOptions,
) -> Result<SavedAuth, OxideFileError> {
    Ok(match auth {
        EncryptedAuth::Password { password } => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: (!password.is_empty()).then(|| SecretString::from(password)),
        },
        EncryptedAuth::Key {
            key_path,
            passphrase,
            embedded_key,
            managed_key,
        } => {
            if managed_key.is_some() && import_options.restore_managed_keys {
                let managed_passphrase = if import_options.restore_managed_key_passphrases {
                    passphrase
                } else {
                    None
                };
                prepare_managed_key_restore(
                    store,
                    &key_path,
                    managed_passphrase,
                    embedded_key,
                    managed_key,
                    restored_managed_keys,
                    imported_managed_keys,
                )?
                .expect("managed key metadata was provided")
            } else {
                SavedAuth::Key {
                    key_path: embedded_key
                        .map(|encoded| extract_embedded_file(&key_path, encoded))
                        .transpose()?
                        .unwrap_or(key_path),
                    has_passphrase: passphrase.is_some(),
                    passphrase_keychain_id: None,
                    plaintext_passphrase: passphrase.map(SecretString::from),
                }
            }
        }
        EncryptedAuth::Certificate {
            key_path,
            cert_path,
            passphrase,
            embedded_key,
            embedded_cert,
            managed_key: _,
        } => SavedAuth::Certificate {
            key_path: embedded_key
                .map(|encoded| extract_embedded_file(&key_path, encoded))
                .transpose()?
                .unwrap_or(key_path),
            cert_path: embedded_cert
                .map(|encoded| extract_embedded_file(&cert_path, encoded))
                .transpose()?
                .unwrap_or(cert_path),
            has_passphrase: passphrase.is_some(),
            passphrase_keychain_id: None,
            plaintext_passphrase: passphrase.map(SecretString::from),
        },
        EncryptedAuth::KeyboardInteractive => SavedAuth::KeyboardInteractive,
        EncryptedAuth::Agent => SavedAuth::Agent,
    })
}

fn prepare_managed_key_restore(
    store: &ConnectionStore,
    key_path: &str,
    passphrase: Option<Zeroizing<String>>,
    embedded_key: Option<Zeroizing<String>>,
    managed_key: Option<EncryptedManagedKeyMetadata>,
    restored_managed_keys: &mut HashMap<String, String>,
    imported_managed_keys: &mut Vec<ImportedManagedSshKey>,
) -> Result<Option<SavedAuth>, OxideFileError> {
    let Some(metadata) = managed_key else {
        return Ok(None);
    };
    let Some(encoded_key) = embedded_key else {
        return Err(OxideFileError::InvalidFormat(format!(
            "Managed key '{}' is missing embedded key data",
            metadata.name
        )));
    };

    if let Some(restored_id) = restored_managed_keys.get(&metadata.key_id) {
        return Ok(Some(SavedAuth::ManagedKey {
            key_id: restored_id.clone(),
            passphrase_keychain_id: None,
            plaintext_passphrase: passphrase.map(SecretString::from),
        }));
    }

    if let Some(fingerprint) = metadata.fingerprint.as_deref() {
        if let Some(existing) = store
            .managed_ssh_keys()
            .into_iter()
            .find(|key| key.fingerprint == fingerprint)
        {
            restored_managed_keys.insert(metadata.key_id, existing.id.clone());
            return Ok(Some(SavedAuth::ManagedKey {
                key_id: existing.id,
                passphrase_keychain_id: None,
                plaintext_passphrase: passphrase.map(SecretString::from),
            }));
        }

        if let Some(pending) = imported_managed_keys
            .iter()
            .find(|entry| entry.key.fingerprint == fingerprint)
        {
            restored_managed_keys.insert(metadata.key_id, pending.key.id.clone());
            return Ok(Some(SavedAuth::ManagedKey {
                key_id: pending.key.id.clone(),
                passphrase_keychain_id: None,
                plaintext_passphrase: passphrase.map(SecretString::from),
            }));
        }
    }

    let decoded = Zeroizing::new(
        BASE64
            .decode(encoded_key.as_bytes())
            .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?,
    );
    let private_key = Zeroizing::new(
        String::from_utf8(decoded.to_vec()).map_err(|error| {
            OxideFileError::InvalidFormat(format!(
                "Managed key '{key_path}' is not valid UTF-8: {error}"
            ))
        })?,
    );
    let key_id = Uuid::new_v4().to_string();
    let secret_id = format!("managed-key-{key_id}");
    let now = Utc::now();
    let fingerprint = metadata
        .fingerprint
        .clone()
        .unwrap_or_else(|| format!("imported-{key_id}"));
    let key = ManagedSshKey {
        id: key_id.clone(),
        secret_id,
        name: metadata
            .name
            .trim()
            .is_empty()
            .then_some("Managed SSH Key")
            .unwrap_or(metadata.name.trim())
            .to_string(),
        fingerprint,
        public_key: metadata.public_key.unwrap_or_default(),
        requires_passphrase: metadata.requires_passphrase.unwrap_or(passphrase.is_some()),
        origin: ManagedSshKeyOrigin::OxideImport,
        created_at: now,
        updated_at: now,
    };
    let saved_auth = SavedAuth::ManagedKey {
        key_id: key_id.clone(),
        passphrase_keychain_id: None,
        plaintext_passphrase: passphrase.map(SecretString::from),
    };

    // Staged managed keys are committed with the imported connections so
    // config metadata and secret storage roll back together on failure.
    imported_managed_keys.push(ImportedManagedSshKey {
        key,
        secret: SecretString::from(private_key),
    });
    restored_managed_keys.insert(metadata.key_id, key_id);
    Ok(Some(saved_auth))
}

fn extract_embedded_file(
    original_path: &str,
    encoded: Zeroizing<String>,
) -> Result<String, OxideFileError> {
    let decoded = Zeroizing::new(
        BASE64
            .decode(encoded.as_bytes())
            .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?,
    );
    let imported_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("imported");
    fs::create_dir_all(&imported_dir)?;
    let base = Path::new(original_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("imported-key");
    let path = unique_imported_path(&imported_dir, base);
    fs::write(&path, decoded.as_slice())?;
    #[cfg(unix)]
    {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(path.to_string_lossy().to_string())
}

fn unique_imported_path(dir: &Path, base: &str) -> PathBuf {
    let initial = dir.join(base);
    if !initial.exists() {
        return initial;
    }
    let original = PathBuf::from(base);
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(base);
    let ext = original
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for index in 1..=1000 {
        let candidate = dir.join(format!("{stem}_{index}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}_{}{ext}", Uuid::new_v4()))
}

fn merge_saved_connection(
    mut existing: SavedConnection,
    imported: SavedConnection,
) -> SavedConnection {
    existing.group = imported.group.or(existing.group);
    existing.host = imported.host;
    existing.port = imported.port;
    existing.username = imported.username;
    existing.auth = merge_auth(existing.auth, imported.auth);
    let imported_has_proxy_chain = !imported.proxy_chain.is_empty();
    if imported_has_proxy_chain {
        existing.proxy_chain = imported.proxy_chain;
    }
    existing.upstream_proxy = imported.upstream_proxy;
    let legacy_post_connect_command = imported.post_connect_command;
    existing.options = merge_options(existing.options, imported.options, imported_has_proxy_chain);
    if existing.options.post_connect_command.is_none() {
        existing.options.post_connect_command = legacy_post_connect_command;
    }
    existing.color = imported.color.or(existing.color);
    existing.tags = merge_tags(existing.tags, imported.tags);
    existing.post_connect_command = None;
    existing
}

fn merge_auth(existing: SavedAuth, imported: SavedAuth) -> SavedAuth {
    match (&existing, &imported) {
        (
            SavedAuth::Password {
                keychain_id: Some(_),
                ..
            },
            SavedAuth::Password {
                plaintext_password: None,
                keychain_id: None,
            },
        ) => existing,
        _ => imported,
    }
}

fn merge_options(
    mut existing: ConnectionOptions,
    imported: ConnectionOptions,
    imported_has_proxy_chain: bool,
) -> ConnectionOptions {
    if imported.keep_alive_interval != 0 {
        existing.keep_alive_interval = imported.keep_alive_interval;
    }
    existing.compression |= imported.compression;
    if imported.term_type.is_some() {
        existing.term_type = imported.term_type;
    }
    existing.agent_forwarding |= imported.agent_forwarding;
    existing.legacy_ssh_compatibility |= imported.legacy_ssh_compatibility;
    existing.post_connect_command = imported.post_connect_command.or(existing.post_connect_command);
    if imported_has_proxy_chain {
        existing.jump_host = None;
    }
    existing
}

fn merge_tags(mut existing: Vec<String>, imported: Vec<String>) -> Vec<String> {
    for tag in imported {
        if !existing.contains(&tag) {
            existing.push(tag);
        }
    }
    existing
}
