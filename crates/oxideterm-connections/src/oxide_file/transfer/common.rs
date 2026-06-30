fn filter_selected_connections(
    connections: Vec<EncryptedConnection>,
    selected_names: Option<&Vec<String>>,
) -> Vec<EncryptedConnection> {
    let Some(selected_names) = selected_names else {
        return connections;
    };
    let names = selected_names
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    connections
        .into_iter()
        .filter(|connection| names.contains(connection.name.as_str()))
        .collect()
}

fn validate_password(password: &str) -> Result<(), OxideFileError> {
    if password.len() < 6 {
        Err(OxideFileError::PasswordTooShort)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn decrypt_payload(bytes: &[u8], password: &str) -> Result<EncryptedPayload, OxideFileError> {
    let file = OxideFile::from_bytes(bytes)?;
    decrypt_oxide_file_with_progress(&file, password, |_| {})
}

fn count_quick_commands(snapshot_json: Option<&str>) -> (bool, usize, usize) {
    let Some(json) = snapshot_json else {
        return (false, 0, 0);
    };
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return (true, 0, 0);
    };
    let commands = value
        .get("commands")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let categories = value
        .get("categories")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    (true, commands, categories)
}

fn count_serial_profiles(snapshot_json: Option<&str>) -> usize {
    let Some(json) = snapshot_json else {
        return 0;
    };
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return 0;
    };
    value
        .get("records")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn count_raw_tcp_profiles(snapshot_json: Option<&str>) -> usize {
    let Some(json) = snapshot_json else {
        return 0;
    };
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return 0;
    };
    value
        .get("records")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn plugin_settings_by_plugin(settings: &[EncryptedPluginSetting]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for setting in settings {
        if let Some(plugin_id) = parse_plugin_id_from_setting_storage_key(&setting.storage_key) {
            *counts.entry(plugin_id).or_insert(0) += 1;
        }
    }
    counts
}

fn parse_plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";
    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    (!plugin_id.is_empty() && !setting_id.is_empty()).then(|| plugin_id.to_string())
}

fn connection_has_embedded_key(conn: &EncryptedConnection) -> bool {
    auth_has_embedded_key(&conn.auth)
        || conn
            .proxy_chain
            .iter()
            .any(|hop| auth_has_embedded_key(&hop.auth))
}

fn auth_has_embedded_key(auth: &EncryptedAuth) -> bool {
    match auth {
        EncryptedAuth::Key { embedded_key, .. } => embedded_key.is_some(),
        EncryptedAuth::Certificate {
            embedded_key,
            embedded_cert,
            ..
        } => embedded_key.is_some() || embedded_cert.is_some(),
        _ => false,
    }
}

fn expand_home(path: &Path) -> Option<PathBuf> {
    if path.starts_with("~") {
        dirs::home_dir().map(|home| home.join(path.strip_prefix("~").unwrap_or(path)))
    } else {
        Some(path.to_path_buf())
    }
}
