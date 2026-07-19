fn default_key_paths() -> Vec<PathBuf> {
    let Some(ssh_dir) = crate::local_paths::default_ssh_dir() else {
        return Vec::new();
    };
    default_key_paths_in_ssh_dir(ssh_dir)
}

fn preferred_default_key_paths() -> Vec<PathBuf> {
    let Some(ssh_dir) = crate::local_paths::default_ssh_dir() else {
        return Vec::new();
    };
    preferred_default_key_paths_in_ssh_dir(ssh_dir)
}

fn preferred_default_key_paths_in_ssh_dir(ssh_dir: PathBuf) -> Vec<PathBuf> {
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .into_iter()
        .map(|name| ssh_dir.join(name))
        .collect()
}

#[cfg(test)]
fn default_key_paths_in_home(home: PathBuf) -> Vec<PathBuf> {
    default_key_paths_in_ssh_dir(home.join(".ssh"))
}

fn default_key_paths_in_ssh_dir(ssh: PathBuf) -> Vec<PathBuf> {
    let preferred_names = ["id_ed25519", "id_ecdsa", "id_rsa"];
    let mut paths = preferred_names
        .iter()
        .map(|name| ssh.join(name))
        .collect::<Vec<_>>();

    let Ok(entries) = std::fs::read_dir(&ssh) else {
        return paths;
    };
    let mut discovered = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| default_key_candidate_name(path).is_some())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| !preferred_names.contains(&name))
        })
        .collect::<Vec<_>>();
    discovered.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .cmp(right.file_name().unwrap_or_default())
    });
    paths.extend(discovered);
    paths
}

fn load_first_available_default_key(passphrase: Option<&str>) -> Result<PrivateKey, SshTransportError> {
    load_first_available_key(default_key_paths(), passphrase)
}

fn load_first_available_key(
    paths: impl IntoIterator<Item = PathBuf>,
    passphrase: Option<&str>,
) -> Result<PrivateKey, SshTransportError> {
    let mut saw_encrypted_key = false;

    for path in paths {
        if !path.exists() {
            continue;
        }

        let key_data = match std::fs::read_to_string(&path) {
            Ok(contents) => Zeroizing::new(contents),
            Err(_) => continue,
        };
        match decode_private_key_for_auth(&key_data, passphrase) {
            Ok(key) => return Ok(key),
            Err(error) if private_key_auth_error_is_missing_passphrase(&error) => {
                saw_encrypted_key = true;
            }
            Err(_) => {}
        }
    }

    if saw_encrypted_key {
        Err(SshTransportError::AuthenticationFailed(
            "Passphrase required".to_string(),
        ))
    } else {
        Err(SshTransportError::AuthenticationFailed(
            "No default SSH key found in ~/.ssh".to_string(),
        ))
    }
}

fn load_agent_fallback_keys(
    paths: impl IntoIterator<Item = PathBuf>,
    agent_public_keys: &HashSet<String>,
) -> Vec<Arc<PrivateKey>> {
    let mut loaded_public_keys = HashSet::new();
    let mut keys = Vec::new();

    for path in paths {
        if !private_key_permissions_are_strict(&path) {
            tracing::warn!(
                key_file_name = path.file_name().and_then(|name| name.to_str()),
                "Ignoring default SSH key with unsafe file permissions"
            );
            continue;
        }
        let Ok(key) = load_secret_key_for_auth(&path, None) else {
            // Agent fallback is non-interactive. Encrypted, unsupported, or
            // unreadable keys remain available through explicit key auth.
            continue;
        };
        let public_key = key.public_key().public_key_base64();
        if agent_public_keys.contains(&public_key) || !loaded_public_keys.insert(public_key) {
            continue;
        }
        keys.push(Arc::new(key));
    }

    keys
}

#[cfg(unix)]
fn private_key_permissions_are_strict(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o077 == 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn private_key_permissions_are_strict(path: &PathBuf) -> bool {
    // Windows access control is enforced by the platform rather than POSIX
    // mode bits, so this boundary only verifies that the candidate is a file.
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn default_key_candidate_name(path: &PathBuf) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("id_") && !name.ends_with(".pub") && !name.ends_with("-cert.pub") {
        Some(name)
    } else {
        None
    }
}

fn expand_tilde_path(path: &str) -> PathBuf {
    if path == "~" {
        return crate::local_paths::local_home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = crate::local_paths::local_home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod path_auth_tests {
    use super::*;
    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::ssh_key::LineEnding;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-ssh-auth-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_test_key(path: &PathBuf, passphrase: Option<&str>) {
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = match passphrase {
            Some(passphrase) => key.encrypt(&mut rng, passphrase).unwrap(),
            None => key,
        };
        key.write_openssh_file(path, LineEnding::LF).unwrap();
        set_test_private_key_permissions(path);
    }

    #[cfg(unix)]
    fn set_test_private_key_permissions(path: &PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(not(unix))]
    fn set_test_private_key_permissions(_path: &PathBuf) {}

    #[test]
    fn default_key_paths_keep_tauri_priority_before_extra_candidates() {
        let home = unique_temp_dir("default-paths");
        let ssh = home.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::write(ssh.join("id_work"), "").unwrap();
        std::fs::write(ssh.join("id_ed25519_sk.pub"), "").unwrap();
        std::fs::write(ssh.join("id_ed25519-cert.pub"), "").unwrap();

        let paths = default_key_paths_in_home(home.clone());

        assert_eq!(
            paths
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["id_ed25519", "id_ecdsa", "id_rsa", "id_work"]
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn agent_fallback_uses_only_preferred_default_key_names() {
        let ssh = unique_temp_dir("preferred-defaults");

        let paths = preferred_default_key_paths_in_ssh_dir(ssh.clone());

        assert_eq!(
            paths
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["id_ed25519", "id_ecdsa", "id_rsa"]
        );
        let _ = std::fs::remove_dir_all(ssh);
    }

    #[cfg(unix)]
    #[test]
    fn agent_fallback_rejects_private_keys_readable_by_other_users() {
        use std::os::unix::fs::PermissionsExt;

        let dir = unique_temp_dir("unsafe-permissions");
        let unsafe_key = dir.join("id_ed25519");
        write_test_key(&unsafe_key, None);
        std::fs::set_permissions(&unsafe_key, std::fs::Permissions::from_mode(0o644)).unwrap();

        let keys = load_agent_fallback_keys([unsafe_key], &HashSet::new());

        assert!(keys.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_fallback_skips_keys_already_offered_by_the_agent() {
        let dir = unique_temp_dir("agent-dedupe");
        let key_path = dir.join("id_ed25519");
        write_test_key(&key_path, None);
        let key = load_secret_key_for_auth(&key_path, None).unwrap();
        let agent_public_keys = HashSet::from([key.public_key().public_key_base64()]);

        let keys = load_agent_fallback_keys([key_path], &agent_public_keys);

        assert!(keys.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_fallback_skips_encrypted_key_and_uses_next_unencrypted_key() {
        let dir = unique_temp_dir("agent-encrypted-fallback");
        let encrypted = dir.join("id_ed25519");
        let fallback = dir.join("id_ecdsa");
        write_test_key(&encrypted, Some("secret-pass"));
        write_test_key(&fallback, None);

        let keys = load_agent_fallback_keys([encrypted, fallback], &HashSet::new());

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].algorithm(), Algorithm::Ed25519);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_key_loader_falls_back_after_encrypted_candidate_without_passphrase() {
        let dir = unique_temp_dir("fallback");
        let encrypted = dir.join("id_ed25519");
        let fallback = dir.join("id_rsa");
        write_test_key(&encrypted, Some("secret-pass"));
        write_test_key(&fallback, None);

        let key = load_first_available_key(vec![encrypted, fallback], None).unwrap();

        assert_eq!(key.algorithm(), Algorithm::Ed25519);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_key_loader_reports_passphrase_required_when_all_candidates_are_encrypted() {
        let dir = unique_temp_dir("encrypted");
        let encrypted = dir.join("id_ed25519");
        write_test_key(&encrypted, Some("secret-pass"));

        let error = load_first_available_key(vec![encrypted], None).unwrap_err();

        assert!(error.to_string().contains("Passphrase required"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_key_loader_uses_passphrase_for_encrypted_candidate() {
        let dir = unique_temp_dir("passphrase");
        let encrypted = dir.join("id_ed25519");
        let fallback = dir.join("id_rsa");
        write_test_key(&encrypted, Some("secret-pass"));
        write_test_key(&fallback, None);

        let key = load_first_available_key(vec![encrypted, fallback], Some("secret-pass")).unwrap();

        assert_eq!(key.algorithm(), Algorithm::Ed25519);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_key_loader_skips_unparseable_extra_candidates() {
        let dir = unique_temp_dir("skip-invalid");
        let invalid = dir.join("id_work");
        let fallback = dir.join("id_other");
        std::fs::write(&invalid, "not a private key").unwrap();
        write_test_key(&fallback, None);

        let key = load_first_available_key(vec![invalid, fallback], None).unwrap();

        assert_eq!(key.algorithm(), Algorithm::Ed25519);
        let _ = std::fs::remove_dir_all(dir);
    }
}
