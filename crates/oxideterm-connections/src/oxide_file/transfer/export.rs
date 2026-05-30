pub fn preflight_export(
    store: &ConnectionStore,
    connection_ids: &[String],
    embed_keys: bool,
    portable_secret_count: usize,
) -> ExportPreflightResult {
    let mut result = ExportPreflightResult {
        total_connections: connection_ids.len(),
        can_export: true,
        portable_secret_count,
        ..ExportPreflightResult::default()
    };

    for id in connection_ids {
        let Some(conn) = store.get(id) else {
            continue;
        };
        count_auth_preflight(&conn.name, &conn.auth, embed_keys, true, &mut result);
        for hop in &conn.proxy_chain {
            count_auth_preflight(
                &format!("{} (proxy)", conn.name),
                &hop.auth,
                embed_keys,
                false,
                &mut result,
            );
        }
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
    for id in connection_ids {
        let conn = store
            .get(id)
            .ok_or_else(|| OxideFileError::InvalidFormat(format!("Connection {id} not found")))?;
        encrypted_connections.push(export_connection(
            store,
            conn,
            options.embed_keys,
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
    let has_extra_payload = options.app_settings_json.is_some()
        || options.quick_commands_json.is_some()
        || !options.plugin_settings.is_empty()
        || !options.portable_secrets.is_empty();
    let mut payload = EncryptedPayload {
        version: if has_extra_payload { 2 } else { 1 },
        connections: encrypted_connections,
        app_settings_json: options.app_settings_json,
        quick_commands_json: options.quick_commands_json,
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
        plugin_settings_count: (!payload.plugin_settings.is_empty())
            .then_some(payload.plugin_settings.len()),
        portable_secret_count: (!payload.portable_secrets.is_empty())
            .then_some(payload.portable_secrets.len()),
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
            AuthType::Key | AuthType::ManagedKey | AuthType::Certificate => {
                result.connections_with_keys += 1
            }
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
    embed_keys: bool,
    forwards: Vec<EncryptedForward>,
) -> Result<EncryptedConnection, OxideFileError> {
    let proxy_chain = export_proxy_chain(store, conn, embed_keys)?;
    Ok(EncryptedConnection {
        name: conn.name.clone(),
        group: conn.group.clone(),
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth: export_auth(store, &conn.auth, embed_keys)?,
        color: conn.color.clone(),
        tags: conn.tags.clone(),
        options: conn.options.clone(),
        proxy_chain,
        forwards,
    })
}

fn export_proxy_chain(
    store: &ConnectionStore,
    conn: &SavedConnection,
    embed_keys: bool,
) -> Result<Vec<EncryptedProxyHop>, OxideFileError> {
    if !conn.proxy_chain.is_empty() {
        return conn
            .proxy_chain
            .iter()
            .map(|hop| export_proxy_hop(store, hop, embed_keys))
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
        auth: export_auth(store, &jump.auth, embed_keys)?,
    }])
}

fn export_proxy_hop(
    store: &ConnectionStore,
    hop: &SavedProxyHop,
    embed_keys: bool,
) -> Result<EncryptedProxyHop, OxideFileError> {
    Ok(EncryptedProxyHop {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: export_auth(store, &hop.auth, embed_keys)?,
    })
}

fn export_auth(
    store: &ConnectionStore,
    auth: &SavedAuth,
    embed_keys: bool,
) -> Result<EncryptedAuth, OxideFileError> {
    match auth {
        SavedAuth::Password { .. } => Ok(EncryptedAuth::Password {
            password: Zeroizing::new(String::new()),
        }),
        SavedAuth::Key { key_path, .. } => Ok(EncryptedAuth::Key {
            key_path: key_path.clone(),
            passphrase: store
                .get_saved_auth_passphrase(auth)
                .ok()
                .flatten()
                .map(SecretString::into_zeroizing),
            embedded_key: if embed_keys {
                read_and_embed_key(key_path)?
            } else {
                None
            },
        }),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            ..
        } => Ok(EncryptedAuth::Certificate {
            key_path: key_path.clone(),
            cert_path: cert_path.clone(),
            passphrase: store
                .get_saved_auth_passphrase(auth)
                .ok()
                .flatten()
                .map(SecretString::into_zeroizing),
            embedded_key: if embed_keys {
                read_and_embed_key(key_path)?
            } else {
                None
            },
            embedded_cert: if embed_keys {
                read_and_embed_key(cert_path)?
            } else {
                None
            },
        }),
        SavedAuth::ManagedKey { .. } => Err(OxideFileError::InvalidFormat(
            "Managed key .oxide export is not implemented in this slice".to_string(),
        )),
        SavedAuth::Agent => Ok(EncryptedAuth::Agent),
    }
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
