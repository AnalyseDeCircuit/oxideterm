fn migrate_legacy_auth_credentials(
    auth: &mut SavedAuth,
    keychain: &ConnectionKeychain,
) -> Result<bool> {
    match auth {
        SavedAuth::Password {
            keychain_id,
            plaintext_password,
        } => {
            if let Some(password) = plaintext_password.take() {
                let next_keychain_id = keychain_id.clone().unwrap_or_else(new_password_keychain_id);
                keychain.store(&next_keychain_id, &password)?;
                *keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::Key {
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            if let Some(passphrase) = plaintext_passphrase.take() {
                let next_keychain_id = passphrase_keychain_id
                    .clone()
                    .unwrap_or_else(new_key_passphrase_keychain_id);
                keychain.store(&next_keychain_id, &passphrase)?;
                *has_passphrase = true;
                *passphrase_keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::Certificate {
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            if let Some(passphrase) = plaintext_passphrase.take() {
                let next_keychain_id = passphrase_keychain_id
                    .clone()
                    .unwrap_or_else(new_key_passphrase_keychain_id);
                keychain.store(&next_keychain_id, &passphrase)?;
                *has_passphrase = true;
                *passphrase_keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::ManagedKey {
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            if let Some(passphrase) = plaintext_passphrase.take() {
                let next_keychain_id = passphrase_keychain_id
                    .clone()
                    .unwrap_or_else(new_key_passphrase_keychain_id);
                keychain.store(&next_keychain_id, &passphrase)?;
                *passphrase_keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::KeyboardInteractive | SavedAuth::Agent => Ok(false),
    }
}

pub fn validate_group_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("group name cannot be empty");
    }
    if name.split('/').any(|part| part.trim().is_empty()) {
        bail!("group path cannot contain empty segments");
    }
    Ok(name.to_string())
}

fn normalize_optional_group_name(group: Option<&str>) -> Result<Option<String>> {
    let Some(group) = group.map(str::trim).filter(|group| !group.is_empty()) else {
        return Ok(None);
    };
    if matches!(group, "Ungrouped" | "未分组") {
        return Ok(None);
    }
    validate_group_name(group).map(Some)
}

fn non_empty<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    if value.is_empty() {
        bail!("{label} is required");
    }
    Ok(value)
}

fn managed_key_display_name(name: Option<String>, fallback: &str) -> String {
    name.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn decode_managed_private_key(
    private_key: &SecretString,
    passphrase: Option<&SecretString>,
) -> Result<PrivateKey> {
    russh::keys::decode_secret_key(
        private_key.expose_secret(),
        passphrase.map(SecretString::expose_secret),
    )
    .map_err(|error| {
        let normalized = error.to_string().to_ascii_lowercase();
        if normalized.contains("encrypted")
            || normalized.contains("decrypt")
            || normalized.contains("password")
            || normalized.contains("passphrase")
            || normalized.contains("bcrypt")
            || normalized.contains("kdf")
        {
            if passphrase.is_some() {
                anyhow::anyhow!("Invalid SSH key passphrase")
            } else {
                anyhow::anyhow!("SSH key requires a passphrase")
            }
        } else {
            anyhow::anyhow!("Invalid SSH private key")
        }
    })
}

fn fingerprint_public_key(public_key: &russh::keys::PublicKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key.public_key_bytes());
    let hash = hasher.finalize();
    format!("SHA256:{}", BASE64.encode(hash).trim_end_matches('='))
}

fn public_key_line_from_private_key(private_key: &PrivateKey) -> String {
    let public_key = private_key.public_key();
    format!(
        "{} {}",
        public_key.algorithm(),
        BASE64.encode(public_key.public_key_bytes())
    )
}

fn managed_key_requires_passphrase(
    private_key: &SecretString,
    passphrase: Option<&SecretString>,
) -> bool {
    let private_key = private_key.expose_secret();
    passphrase.is_some()
        || private_key.contains("ENCRYPTED")
        || private_key.contains("Proc-Type: 4,ENCRYPTED")
}

fn fallback_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Managed SSH Key")
        .to_string()
}

fn managed_key_usage_from_data(data: &ConnectionStoreData, key_id: &str) -> ManagedSshKeyUsage {
    let mut items = Vec::new();
    for connection in &data.connections {
        if matches!(&connection.auth, SavedAuth::ManagedKey { key_id: id, .. } if id == key_id) {
            items.push(ManagedSshKeyUsageItem {
                connection_id: connection.id.clone(),
                connection_name: connection.name.clone(),
                location: "connection".to_string(),
            });
        }

        for (index, hop) in connection.proxy_chain.iter().enumerate() {
            if matches!(&hop.auth, SavedAuth::ManagedKey { key_id: id, .. } if id == key_id) {
                items.push(ManagedSshKeyUsageItem {
                    connection_id: connection.id.clone(),
                    connection_name: connection.name.clone(),
                    location: format!("proxy_chain[{}]", index),
                });
            }
        }
    }

    ManagedSshKeyUsage {
        key_id: key_id.to_string(),
        count: items.len(),
        items,
    }
}

fn existing_password_keychain_id(auth: Option<&SavedAuth>) -> Option<String> {
    match auth {
        Some(SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        }) => Some(keychain_id.clone()),
        _ => None,
    }
}

fn existing_upstream_proxy_password_keychain_id(
    policy: Option<&SavedUpstreamProxyPolicy>,
) -> Option<String> {
    match policy {
        Some(SavedUpstreamProxyPolicy::Custom {
            proxy:
                SavedUpstreamProxyConfig {
                    auth:
                        SavedUpstreamProxyAuth::Password {
                            keychain_id: Some(keychain_id),
                            ..
                        },
                    ..
                },
        }) => Some(keychain_id.clone()),
        _ => None,
    }
}

fn collect_connection_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    collect_keychain_ids_for_parts(
        &connection.auth,
        &connection.proxy_chain,
        &connection.upstream_proxy,
    )
}

fn collect_privilege_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    connection
        .privilege_credentials
        .iter()
        .filter_map(|credential| credential.keychain_id.clone())
        .collect()
}

fn collect_keychain_ids_for_parts(
    auth: &SavedAuth,
    proxy_chain: &[SavedProxyHop],
    upstream_proxy: &SavedUpstreamProxyPolicy,
) -> Vec<String> {
    let mut ids = collect_keychain_ids_for_auth(auth);
    for hop in proxy_chain {
        ids.extend(collect_keychain_ids_for_auth(&hop.auth));
    }
    ids.extend(collect_keychain_ids_for_upstream_proxy(upstream_proxy));
    ids
}

fn collect_keychain_ids_for_auth(auth: &SavedAuth) -> Vec<String> {
    match auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        } => vec![keychain_id.clone()],
        SavedAuth::Key {
            passphrase_keychain_id: Some(keychain_id),
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id: Some(keychain_id),
            ..
        }
        | SavedAuth::ManagedKey {
            passphrase_keychain_id: Some(keychain_id),
            ..
        } => vec![keychain_id.clone()],
        _ => Vec::new(),
    }
}

fn collect_keychain_ids_for_upstream_proxy(policy: &SavedUpstreamProxyPolicy) -> Vec<String> {
    match policy {
        SavedUpstreamProxyPolicy::Custom {
            proxy:
                SavedUpstreamProxyConfig {
                    auth:
                        SavedUpstreamProxyAuth::Password {
                            keychain_id: Some(keychain_id),
                            ..
                        },
                    ..
                },
        } => vec![keychain_id.clone()],
        _ => Vec::new(),
    }
}

fn new_password_keychain_id() -> String {
    format!("oxide_conn_{}", Uuid::new_v4())
}

fn new_key_passphrase_keychain_id() -> String {
    format!("oxide_conn_key_{}", Uuid::new_v4())
}

fn new_upstream_proxy_password_keychain_id() -> String {
    format!("oxide_conn_upstream_proxy_{}", Uuid::new_v4())
}

fn privilege_keychain_id(connection_id: &str, credential_id: &str) -> String {
    format!("privilege:v1:{connection_id}:{credential_id}")
}

fn default_privilege_prompt_patterns(kind: PrivilegeCredentialKind) -> Vec<String> {
    match kind {
        PrivilegeCredentialKind::SudoPassword => vec![
            "[sudo]".to_string(),
            "password for".to_string(),
            "的密码".to_string(),
            "sudo password".to_string(),
        ],
        PrivilegeCredentialKind::SuPassword => {
            vec![
                "su: password".to_string(),
                "password:".to_string(),
                "密码：".to_string(),
            ]
        }
        PrivilegeCredentialKind::CustomPrompt => Vec::new(),
    }
}

fn legacy_privilege_prompt_patterns(kind: PrivilegeCredentialKind) -> Vec<String> {
    match kind {
        PrivilegeCredentialKind::SudoPassword => vec![
            "[sudo] password for".to_string(),
            "sudo password".to_string(),
        ],
        PrivilegeCredentialKind::SuPassword => {
            vec!["Password:".to_string(), "su: Password:".to_string()]
        }
        PrivilegeCredentialKind::CustomPrompt => Vec::new(),
    }
}

fn normalize_privilege_prompt_patterns(
    kind: PrivilegeCredentialKind,
    patterns: Vec<String>,
) -> Vec<String> {
    let patterns = patterns
        .into_iter()
        .map(|pattern| pattern.trim().to_string())
        .filter(|pattern| !pattern.is_empty())
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        return default_privilege_prompt_patterns(kind);
    }
    // Older builds stored narrow English-only defaults. Treat only that exact
    // generated shape as migratable so real custom prompt fragments survive.
    if kind != PrivilegeCredentialKind::CustomPrompt
        && patterns == legacy_privilege_prompt_patterns(kind)
    {
        return default_privilege_prompt_patterns(kind);
    }
    patterns
}

fn normalize_saved_privilege_credential_for_display(
    mut credential: SavedPrivilegeCredential,
) -> SavedPrivilegeCredential {
    credential.prompt_patterns =
        normalize_privilege_prompt_patterns(credential.kind, credential.prompt_patterns);
    credential
}

fn matching_key_passphrase_id(auth: Option<&SavedAuth>, key_path: &str) -> Option<String> {
    matching_key_passphrase(auth, key_path).and_then(|(_, id)| id)
}

fn matching_key_passphrase(
    auth: Option<&SavedAuth>,
    key_path: &str,
) -> Option<(bool, Option<String>)> {
    match auth {
        Some(SavedAuth::Key {
            key_path: existing_key_path,
            has_passphrase,
            passphrase_keychain_id,
            ..
        }) if existing_key_path == key_path => {
            Some((*has_passphrase, passphrase_keychain_id.clone()))
        }
        _ => None,
    }
}

fn matching_certificate_passphrase_id(
    auth: Option<&SavedAuth>,
    key_path: &str,
    cert_path: &str,
) -> Option<String> {
    matching_certificate_passphrase(auth, key_path, cert_path).and_then(|(_, id)| id)
}

fn matching_certificate_passphrase(
    auth: Option<&SavedAuth>,
    key_path: &str,
    cert_path: &str,
) -> Option<(bool, Option<String>)> {
    match auth {
        Some(SavedAuth::Certificate {
            key_path: existing_key_path,
            cert_path: existing_cert_path,
            has_passphrase,
            passphrase_keychain_id,
            ..
        }) if existing_key_path == key_path && existing_cert_path == cert_path => {
            Some((*has_passphrase, passphrase_keychain_id.clone()))
        }
        _ => None,
    }
}

fn matching_managed_key_passphrase_id(auth: Option<&SavedAuth>, key_id: &str) -> Option<String> {
    match auth {
        Some(SavedAuth::ManagedKey {
            key_id: existing_key_id,
            passphrase_keychain_id,
            ..
        }) if existing_key_id == key_id => passphrase_keychain_id.clone(),
        _ => None,
    }
}
