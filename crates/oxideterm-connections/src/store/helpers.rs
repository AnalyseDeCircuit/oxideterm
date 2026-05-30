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
        SavedAuth::ManagedKey { .. } | SavedAuth::Agent => Ok(false),
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

fn existing_password_keychain_id(auth: Option<&SavedAuth>) -> Option<String> {
    match auth {
        Some(SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        }) => Some(keychain_id.clone()),
        _ => None,
    }
}

fn collect_connection_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    collect_keychain_ids_for_parts(&connection.auth, &connection.proxy_chain)
}

fn collect_keychain_ids_for_parts(auth: &SavedAuth, proxy_chain: &[SavedProxyHop]) -> Vec<String> {
    let mut ids = collect_keychain_ids_for_auth(auth);
    for hop in proxy_chain {
        ids.extend(collect_keychain_ids_for_auth(&hop.auth));
    }
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

fn new_password_keychain_id() -> String {
    format!("oxide_conn_{}", Uuid::new_v4())
}

fn new_key_passphrase_keychain_id() -> String {
    format!("oxide_conn_key_{}", Uuid::new_v4())
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
