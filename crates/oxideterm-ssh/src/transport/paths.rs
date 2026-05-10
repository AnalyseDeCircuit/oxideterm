fn resolve_key_path(path: &str) -> Result<PathBuf, SshTransportError> {
    if !path.trim().is_empty() {
        return Ok(expand_tilde_path(path));
    }

    default_key_paths()
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            SshTransportError::AuthenticationFailed(
                "No default SSH key found in ~/.ssh".to_string(),
            )
        })
}

fn default_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    [
        "id_ed25519",
        "id_ecdsa",
        "id_rsa",
        "id_dsa",
        "id_ed25519_sk",
        "id_ecdsa_sk",
    ]
    .into_iter()
    .map(|name| ssh.join(name))
    .collect()
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
