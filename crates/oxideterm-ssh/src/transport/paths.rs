fn default_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    default_key_paths_in_home(home)
}

fn default_key_paths_in_home(home: PathBuf) -> Vec<PathBuf> {
    let ssh = home.join(".ssh");
    [
        "id_ed25519",
        "id_ecdsa",
        "id_rsa",
    ]
    .into_iter()
    .map(|name| ssh.join(name))
    .collect()
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

        if passphrase.is_none() && private_key_file_looks_encrypted(&path) {
            saw_encrypted_key = true;
            continue;
        }

        match load_secret_key(&path, passphrase) {
            Ok(key) => return Ok(key),
            Err(error) => {
                if passphrase.is_none() && private_key_error_is_passphrase_related(&error) {
                    saw_encrypted_key = true;
                }
            }
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

fn private_key_file_looks_encrypted(path: &PathBuf) -> bool {
    std::fs::read_to_string(path)
        .map(|contents| {
            let contents = Zeroizing::new(contents);
            contents.contains("ENCRYPTED") || contents.contains("Proc-Type: 4,ENCRYPTED")
        })
        .unwrap_or(false)
}

fn private_key_error_is_passphrase_related(error: &russh::keys::Error) -> bool {
    let normalized = error.to_string().to_ascii_lowercase();
    normalized.contains("decrypt")
        || normalized.contains("password")
        || normalized.contains("passphrase")
        || normalized.contains("encrypted")
        || normalized.contains("bcrypt")
        || normalized.contains("kdf")
}

fn expand_tilde_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod path_auth_tests {
    use super::*;
    use russh::keys::ssh_key::{LineEnding, rand_core::OsRng};

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
        let mut rng = OsRng;
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = match passphrase {
            Some(passphrase) => key.encrypt(&mut rng, passphrase).unwrap(),
            None => key,
        };
        key.write_openssh_file(path, LineEnding::LF).unwrap();
    }

    #[test]
    fn default_key_paths_match_tauri_order() {
        let home = PathBuf::from("/tmp/home");

        let paths = default_key_paths_in_home(home);

        assert_eq!(
            paths
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["id_ed25519", "id_ecdsa", "id_rsa"]
        );
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
}
