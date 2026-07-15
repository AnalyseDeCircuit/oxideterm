pub fn preflight_export(
    store: &ConnectionStore,
    connection_ids: &[String],
    embed_keys: bool,
    include_managed_keys: bool,
    portable_secret_count: usize,
) -> ExportPreflightResult {
    let mut result = ExportPreflightResult {
        total_connections: connection_ids.len(),
        can_export: true,
        portable_secret_count,
        ..ExportPreflightResult::default()
    };
    let mut managed_key_ids = HashSet::new();

    for id in connection_ids {
        let Some(conn) = store.get(id) else {
            continue;
        };
        collect_managed_key_id(&conn.auth, &mut managed_key_ids);
        if !include_managed_keys && auth_uses_managed_key(&conn.auth) {
            result
                .blocked_managed_key_connections
                .push(conn.name.clone());
        }
        count_auth_preflight(&conn.name, &conn.auth, embed_keys, true, &mut result);
        for hop in &conn.proxy_chain {
            collect_managed_key_id(&hop.auth, &mut managed_key_ids);
            if !include_managed_keys && auth_uses_managed_key(&hop.auth) {
                result
                    .blocked_managed_key_connections
                    .push(conn.name.clone());
            }
            count_auth_preflight(
                &format!("{} (proxy)", conn.name),
                &hop.auth,
                embed_keys,
                false,
                &mut result,
            );
        }
    }
    result.managed_key_count = managed_key_ids.len();
    result.blocked_managed_key_connections.sort();
    result.blocked_managed_key_connections.dedup();
    if !include_managed_keys && !result.blocked_managed_key_connections.is_empty() {
        result.can_export = false;
    }

    result
}

pub fn export_connections_to_oxide(
    store: &ConnectionStore,
    connection_ids: &[String],
    password: &str,
    options: OxideExportOptions,
) -> Result<Vec<u8>, OxideFileError> {
    export_connections_to_oxide_inner(store, connection_ids, password, options, None)
}

pub fn export_connections_to_oxide_with_progress<F>(
    store: &ConnectionStore,
    connection_ids: &[String],
    password: &str,
    options: OxideExportOptions,
    mut on_progress: F,
) -> Result<Vec<u8>, OxideFileError>
where
    F: FnMut(&str, usize, usize),
{
    export_connections_to_oxide_inner(
        store,
        connection_ids,
        password,
        options,
        Some(&mut on_progress),
    )
}

fn export_connections_to_oxide_inner(
    store: &ConnectionStore,
    connection_ids: &[String],
    password: &str,
    options: OxideExportOptions,
    mut on_progress: Option<&mut dyn FnMut(&str, usize, usize)>,
) -> Result<Vec<u8>, OxideFileError> {
    validate_password(password)?;

    let total_steps = connection_ids.len() + 9;
    let mut current_step = 0usize;
    let has_progress = on_progress.is_some();
    let mut report_progress = |stage: &str| {
        current_step += 1;
        if let Some(callback) = on_progress.as_deref_mut() {
            callback(stage, current_step, total_steps);
        }
    };

    let selected: HashSet<&str> = connection_ids.iter().map(String::as_str).collect();
    let forwards_by_connection = options
        .forwards
        .iter()
        .filter(|forward| selected.contains(forward.connection_id.as_str()))
        .fold(
            HashMap::<&str, Vec<EncryptedForward>>::new(),
            |mut map, forward| {
                map.entry(forward.connection_id.as_str())
                    .or_default()
                    .push(export_forward(forward));
                map
            },
        );

    let mut encrypted_connections = Vec::new();
    let mut managed_key_ids = HashSet::new();
    for id in connection_ids {
        let conn = store
            .get(id)
            .ok_or_else(|| OxideFileError::InvalidFormat(format!("Connection {id} not found")))?;
        if let SavedAuth::ManagedKey { key_id, .. } = &conn.auth {
            managed_key_ids.insert(key_id.clone());
        }
        for hop in &conn.proxy_chain {
            if let SavedAuth::ManagedKey { key_id, .. } = &hop.auth {
                managed_key_ids.insert(key_id.clone());
            }
        }
        encrypted_connections.push(export_connection(
            store,
            conn,
            &options,
            forwards_by_connection
                .get(id.as_str())
                .cloned()
                .unwrap_or_default(),
        )?);
        report_progress("collecting_connections");
    }
    report_progress("collecting_portable_secrets");

    let quick_command_counts =
        count_quick_commands_for_export(options.quick_commands_json.as_deref());
    let serial_profiles_count =
        count_serial_profiles_for_export(options.serial_profiles_json.as_deref());
    let has_extra_payload = options.app_settings_json.is_some()
        || options.quick_commands_json.is_some()
        || options.serial_profiles_json.is_some()
        || !options.plugin_settings.is_empty()
        || !options.portable_secrets.is_empty();
    let mut payload = EncryptedPayload {
        version: if has_extra_payload { 2 } else { 1 },
        connections: encrypted_connections,
        app_settings_json: options.app_settings_json,
        quick_commands_json: options.quick_commands_json,
        serial_profiles_json: options.serial_profiles_json,
        plugin_settings: options.plugin_settings,
        portable_secrets: options.portable_secrets,
        checksum: String::new(),
    };
    payload.checksum = compute_checksum(&payload)?;
    report_progress("computing_checksum");

    let metadata = OxideMetadata {
        exported_at: Utc::now(),
        exported_by: format!("OxideTerm v{}", env!("CARGO_PKG_VERSION")),
        description: options.description,
        num_connections: payload.connections.len(),
        connection_names: payload
            .connections
            .iter()
            .map(|conn| conn.name.clone())
            .collect(),
        has_app_settings: payload.app_settings_json.as_ref().map(|_| true),
        has_quick_commands: payload.quick_commands_json.as_ref().map(|_| true),
        quick_commands_count: quick_command_counts.map(|counts| counts.0),
        quick_command_categories_count: quick_command_counts.map(|counts| counts.1),
        serial_profiles_count,
        plugin_settings_count: (!payload.plugin_settings.is_empty())
            .then_some(payload.plugin_settings.len()),
        portable_secret_count: (!payload.portable_secrets.is_empty())
            .then_some(payload.portable_secrets.len()),
        managed_key_count: (!managed_key_ids.is_empty()).then_some(managed_key_ids.len()),
    };
    report_progress("building_metadata");

    let oxide_file = if has_progress {
        encrypt_oxide_file_with_progress(&payload, password, metadata, |stage| {
            report_progress(stage);
        })?
    } else {
        encrypt_oxide_file(&payload, password, metadata)?
    };
    let bytes = oxide_file.to_bytes()?;
    report_progress("serializing_file");
    Ok(bytes)
}

fn count_auth_preflight(
    label: &str,
    auth: &SavedAuth,
    embed_keys: bool,
    count_auth_kind: bool,
    result: &mut ExportPreflightResult,
) {
    if count_auth_kind {
        match auth.auth_type() {
            AuthType::Password => result.connections_with_passwords += 1,
            AuthType::Agent => result.connections_with_agent += 1,
            AuthType::KeyboardInteractive => {}
            AuthType::Key | AuthType::ManagedKey | AuthType::Certificate => {
                result.connections_with_keys += 1
            }
        }
    }
    if auth_has_saved_passphrase(auth) {
        match auth {
            SavedAuth::ManagedKey { .. } => result.managed_key_passphrase_count += 1,
            _ => result.key_passphrase_count += 1,
        }
    }
    if !embed_keys {
        return;
    }
    if let Some(path) = auth.key_path() {
        count_key_path(label, path, result);
    }
    if let Some(path) = auth.cert_path() {
        count_key_path(label, path, result);
    }
}

fn collect_managed_key_id(auth: &SavedAuth, managed_key_ids: &mut HashSet<String>) {
    if let SavedAuth::ManagedKey { key_id, .. } = auth {
        managed_key_ids.insert(key_id.clone());
    }
}

fn auth_uses_managed_key(auth: &SavedAuth) -> bool {
    matches!(auth, SavedAuth::ManagedKey { .. })
}

fn auth_has_saved_passphrase(auth: &SavedAuth) -> bool {
    match auth {
        SavedAuth::Key {
            has_passphrase,
            passphrase_keychain_id,
            ..
        }
        | SavedAuth::Certificate {
            has_passphrase,
            passphrase_keychain_id,
            ..
        } => *has_passphrase && passphrase_keychain_id.is_some(),
        SavedAuth::ManagedKey {
            passphrase_keychain_id,
            ..
        } => passphrase_keychain_id.is_some(),
        _ => false,
    }
}

fn count_key_path(label: &str, path: &str, result: &mut ExportPreflightResult) {
    match expand_home(Path::new(path)).and_then(|path| fs::metadata(path).ok()) {
        Some(metadata) => result.total_key_bytes += metadata.len(),
        None => result
            .missing_keys
            .push((label.to_string(), path.to_string())),
    }
}

fn export_connection(
    store: &ConnectionStore,
    conn: &SavedConnection,
    options: &OxideExportOptions,
    forwards: Vec<EncryptedForward>,
) -> Result<EncryptedConnection, OxideFileError> {
    let proxy_chain = export_proxy_chain(store, conn, options)?;
    Ok(EncryptedConnection {
        name: conn.name.clone(),
        group: conn.group.clone(),
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth: export_auth(store, &conn.auth, options)?,
        color: conn.color.clone(),
        icon: conn.icon.clone(),
        tags: conn.tags.clone(),
        options: conn.options.clone(),
        upstream_proxy: export_upstream_proxy_policy(&conn.upstream_proxy),
        proxy_chain,
        forwards,
        privilege_credentials: export_privilege_credentials(store, conn, options)?,
    })
}

fn export_privilege_credentials(
    store: &ConnectionStore,
    conn: &SavedConnection,
    options: &OxideExportOptions,
) -> Result<Vec<EncryptedPrivilegeCredential>, OxideFileError> {
    if !options.include_passwords {
        return Ok(Vec::new());
    }

    conn.privilege_credentials
        .iter()
        .map(|credential| {
            let secret = if credential.keychain_id.is_some() {
                store
                    .get_privilege_credential_secret(&conn.id, &credential.id)
                    .ok()
                    .map(SecretString::into_zeroizing)
            } else {
                None
            };
            Ok(EncryptedPrivilegeCredential {
                id: credential.id.clone(),
                connection_id: conn.id.clone(),
                label: credential.label.clone(),
                kind: credential.kind,
                username_hint: credential.username_hint.clone(),
                prompt_patterns: credential.prompt_patterns.clone(),
                secret,
                enabled: credential.enabled,
                require_click_to_send: credential.require_click_to_send,
                created_at: credential.created_at,
                updated_at: credential.updated_at,
            })
        })
        .collect()
}

fn export_upstream_proxy_policy(policy: &SavedUpstreamProxyPolicy) -> EncryptedUpstreamProxyPolicy {
    match policy {
        SavedUpstreamProxyPolicy::UseGlobal => EncryptedUpstreamProxyPolicy::UseGlobal,
        SavedUpstreamProxyPolicy::Direct => EncryptedUpstreamProxyPolicy::Direct,
        SavedUpstreamProxyPolicy::Custom { proxy } => EncryptedUpstreamProxyPolicy::Custom {
            proxy: EncryptedUpstreamProxyConfig {
                protocol: proxy.protocol,
                host: proxy.host.clone(),
                port: proxy.port,
                auth: export_upstream_proxy_auth(&proxy.auth),
                remote_dns: proxy.remote_dns,
                no_proxy: proxy.no_proxy.clone(),
            },
        },
    }
}

fn export_upstream_proxy_auth(auth: &SavedUpstreamProxyAuth) -> EncryptedUpstreamProxyAuth {
    match auth {
        SavedUpstreamProxyAuth::None => EncryptedUpstreamProxyAuth::None,
        SavedUpstreamProxyAuth::Password { username, .. } => {
            // .oxide archives preserve proxy auth metadata only; the password and
            // local keychain id are intentionally not portable.
            EncryptedUpstreamProxyAuth::Password {
                username: username.clone(),
            }
        }
    }
}

fn export_proxy_chain(
    store: &ConnectionStore,
    conn: &SavedConnection,
    options: &OxideExportOptions,
) -> Result<Vec<EncryptedProxyHop>, OxideFileError> {
    if !conn.proxy_chain.is_empty() {
        return conn
            .proxy_chain
            .iter()
            .map(|hop| export_proxy_hop(store, hop, options))
            .collect();
    }

    let Some(jump_id) = conn.options.jump_host.as_deref() else {
        return Ok(Vec::new());
    };
    let jump = store.get(jump_id).ok_or_else(|| {
        OxideFileError::InvalidFormat(format!(
            "Connection '{}' references jump host '{}' which does not exist. Please ensure all jump hosts are saved before exporting.",
            conn.name, jump_id
        ))
    })?;
    Ok(vec![EncryptedProxyHop {
        host: jump.host.clone(),
        port: jump.port,
        username: jump.username.clone(),
        auth: export_auth(store, &jump.auth, options)?,
    }])
}

fn export_proxy_hop(
    store: &ConnectionStore,
    hop: &SavedProxyHop,
    options: &OxideExportOptions,
) -> Result<EncryptedProxyHop, OxideFileError> {
    Ok(EncryptedProxyHop {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: export_auth(store, &hop.auth, options)?,
    })
}

fn export_auth(
    store: &ConnectionStore,
    auth: &SavedAuth,
    options: &OxideExportOptions,
) -> Result<EncryptedAuth, OxideFileError> {
    match auth {
        SavedAuth::Password { .. } => Ok(EncryptedAuth::Password {
            password: if options.include_passwords {
                store
                    .get_saved_auth_password(auth)
                    .ok()
                    .map(SecretString::into_zeroizing)
                    .unwrap_or_else(|| Zeroizing::new(String::new()))
            } else {
                Zeroizing::new(String::new())
            },
        }),
        SavedAuth::Key { key_path, .. } => Ok(EncryptedAuth::Key {
            key_path: key_path.clone(),
            passphrase: if options.include_key_passphrases {
                store
                    .get_saved_auth_passphrase(auth)
                    .ok()
                    .flatten()
                    .map(SecretString::into_zeroizing)
            } else {
                None
            },
            embedded_key: if options.embed_keys {
                read_and_embed_key(key_path)?
            } else {
                None
            },
            managed_key: None,
        }),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            ..
        } => Ok(EncryptedAuth::Certificate {
            key_path: key_path.clone(),
            cert_path: cert_path.clone(),
            passphrase: if options.include_key_passphrases {
                store
                    .get_saved_auth_passphrase(auth)
                    .ok()
                    .flatten()
                    .map(SecretString::into_zeroizing)
            } else {
                None
            },
            embedded_key: if options.embed_keys {
                read_and_embed_key(key_path)?
            } else {
                None
            },
            embedded_cert: if options.embed_keys {
                read_and_embed_key(cert_path)?
            } else {
                None
            },
            managed_key: None,
        }),
        SavedAuth::ManagedKey { key_id, .. } => {
            if !options.include_managed_keys {
                return Err(OxideFileError::InvalidFormat(
                    "Managed key export is disabled for a selected connection".to_string(),
                ));
            }
            let metadata = store
                .managed_ssh_key_metadata(key_id)
                .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?;
            let private_key = store
                .resolve_managed_ssh_key_private_key(key_id)
                .map_err(|error| OxideFileError::InvalidFormat(error.to_string()))?;
            Ok(EncryptedAuth::Key {
                key_path: managed_key_fallback_filename(&metadata.fingerprint),
                passphrase: if options.include_managed_key_passphrases {
                    store
                        .get_saved_auth_passphrase(auth)
                        .ok()
                        .flatten()
                        .map(SecretString::into_zeroizing)
                } else {
                    None
                },
                // The encoded private key is secret material and remains inside
                // the encrypted .oxide payload with Zeroizing ownership.
                embedded_key: Some(Zeroizing::new(BASE64.encode(
                    private_key.expose_secret().as_bytes(),
                ))),
                managed_key: Some(EncryptedManagedKeyMetadata {
                    key_id: metadata.id,
                    name: metadata.name,
                    fingerprint: Some(metadata.fingerprint),
                    public_key: Some(metadata.public_key),
                    origin: Some("oxide_import".to_string()),
                    requires_passphrase: Some(metadata.requires_passphrase),
                }),
            })
        }
        SavedAuth::KeyboardInteractive => Ok(EncryptedAuth::KeyboardInteractive),
        SavedAuth::Agent => Ok(EncryptedAuth::Agent),
    }
}

fn managed_key_fallback_filename(fingerprint: &str) -> String {
    let sanitized = fingerprint
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("managed-{}.key", sanitized)
}

fn export_forward(forward: &OxideForwardRecord) -> EncryptedForward {
    EncryptedForward {
        id: forward.id.clone(),
        forward_type: forward.forward_type.clone(),
        bind_address: forward.bind_address.clone(),
        bind_port: forward.bind_port,
        target_host: forward.target_host.clone(),
        target_port: forward.target_port,
        description: forward.description.clone(),
        auto_start: forward.auto_start,
    }
}

fn read_and_embed_key(path: &str) -> Result<Option<Zeroizing<String>>, OxideFileError> {
    let Some(path) = expand_home(Path::new(path)) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(&path)?;
    if metadata.len() > EMBEDDED_KEY_MAX_BYTES {
        return Err(OxideFileError::InvalidFormat(
            "Key file exceeds 1MB limit".to_string(),
        ));
    }
    // Embedded private keys are serialized into the encrypted .oxide payload;
    // wipe the raw file bytes after producing the encoded in-memory copy.
    let mut key_bytes = fs::read(path)?;
    let encoded = Zeroizing::new(BASE64.encode(&key_bytes));
    key_bytes.zeroize();
    Ok(Some(encoded))
}

fn count_quick_commands_for_export(snapshot_json: Option<&str>) -> Option<(usize, usize)> {
    let value = serde_json::from_str::<Value>(snapshot_json?).ok()?;
    let commands = value.get("commands")?.as_array()?.len();
    let categories = value.get("categories")?.as_array()?.len();
    Some((commands, categories))
}

fn count_serial_profiles_for_export(snapshot_json: Option<&str>) -> Option<usize> {
    let value = serde_json::from_str::<Value>(snapshot_json?).ok()?;
    value.get("records")?.as_array().map(Vec::len)
}
