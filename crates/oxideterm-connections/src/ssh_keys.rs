use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshKeyInfo {
    pub name: String,
    pub path: String,
    pub key_type: String,
    pub has_passphrase: bool,
}

pub fn list_available_ssh_keys() -> Vec<SshKeyInfo> {
    let ssh_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .into_iter()
        .filter_map(|name| {
            let path = ssh_dir.join(name);
            path.exists().then(|| SshKeyInfo {
                name: name.to_string(),
                path: path.to_string_lossy().into_owned(),
                key_type: ssh_key_type_from_name(name).to_string(),
                has_passphrase: false,
            })
        })
        .collect()
}

fn ssh_key_type_from_name(name: &str) -> &'static str {
    if name.contains("ed25519") {
        "ED25519"
    } else if name.contains("ecdsa") {
        "ECDSA"
    } else if name.contains("rsa") {
        "RSA"
    } else if name.contains("dsa") {
        "DSA"
    } else {
        "Unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::ssh_key_type_from_name;

    #[test]
    fn infers_key_type_from_tauri_api_names() {
        assert_eq!(ssh_key_type_from_name("id_ed25519"), "ED25519");
        assert_eq!(ssh_key_type_from_name("id_ecdsa"), "ECDSA");
        assert_eq!(ssh_key_type_from_name("id_rsa"), "RSA");
        assert_eq!(ssh_key_type_from_name("id_dsa"), "DSA");
        assert_eq!(ssh_key_type_from_name("custom"), "Unknown");
    }
}
