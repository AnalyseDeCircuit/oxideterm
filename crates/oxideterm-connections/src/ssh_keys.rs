use std::path::PathBuf;

use zeroize::Zeroizing;

use crate::ssh_paths::default_ssh_dir;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshKeyInfo {
    pub name: String,
    pub path: String,
    pub key_type: String,
    pub has_passphrase: bool,
}

pub fn list_available_ssh_keys() -> Vec<SshKeyInfo> {
    default_private_key_paths_in_ssh_dir(default_ssh_dir())
        .into_iter()
        .filter_map(|path| {
            let status = default_private_key_status(&path, None)?;
            let name = path.file_name()?.to_str()?;
            Some(SshKeyInfo {
                name: name.to_string(),
                path: path.to_string_lossy().into_owned(),
                key_type: ssh_key_type_from_name(name).to_string(),
                has_passphrase: status == DefaultPrivateKeyStatus::RequiresPassphrase,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DefaultPrivateKeyStatus {
    Loadable,
    RequiresPassphrase,
}

#[cfg(test)]
pub(crate) fn default_private_key_paths_in_home(home: PathBuf) -> Vec<PathBuf> {
    default_private_key_paths_in_ssh_dir(home.join(".ssh"))
}

pub(crate) fn default_private_key_paths_in_ssh_dir(ssh: PathBuf) -> Vec<PathBuf> {
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

pub(crate) fn default_private_key_status(
    path: &PathBuf,
    passphrase: Option<&str>,
) -> Option<DefaultPrivateKeyStatus> {
    if !path.exists() || default_key_candidate_name(path).is_none() {
        return None;
    }
    let key_data = Zeroizing::new(std::fs::read_to_string(path).ok()?);
    if private_key_text_looks_unsupported_direct_key(&key_data) {
        return None;
    }
    match russh::keys::decode_secret_key(&key_data, passphrase) {
        Ok(key) => {
            if key.algorithm().to_string().starts_with("sk-") {
                None
            } else {
                Some(DefaultPrivateKeyStatus::Loadable)
            }
        }
        Err(error) if private_key_error_is_passphrase_related(&error) => {
            Some(DefaultPrivateKeyStatus::RequiresPassphrase)
        }
        Err(_) => None,
    }
}

fn default_key_candidate_name(path: &PathBuf) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("id_") && !name.ends_with(".pub") && !name.ends_with("-cert.pub") {
        Some(name)
    } else {
        None
    }
}

fn private_key_text_looks_unsupported_direct_key(private_key: &str) -> bool {
    private_key.contains("-----BEGIN DSA PRIVATE KEY-----")
        || private_key.contains("ssh-dss")
        || private_key.contains("sk-ecdsa-sha2-nistp256")
        || private_key.contains("sk-ssh-ed25519")
}

pub(crate) fn private_key_error_is_passphrase_related(error: &russh::keys::Error) -> bool {
    let normalized = error.to_string().to_ascii_lowercase();
    normalized.contains("decrypt")
        || normalized.contains("password")
        || normalized.contains("passphrase")
        || normalized.contains("encrypted")
        || normalized.contains("bcrypt")
        || normalized.contains("kdf")
        || normalized.contains("crypto")
        || normalized.contains("cryptographic")
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
    use std::path::PathBuf;

    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::{Algorithm, PrivateKey, ssh_key::LineEnding};

    use super::{
        DefaultPrivateKeyStatus, default_private_key_paths_in_home, default_private_key_status,
        ssh_key_type_from_name,
    };

    fn unique_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-conn-ssh-keys-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn infers_key_type_from_tauri_api_names() {
        assert_eq!(ssh_key_type_from_name("id_ed25519"), "ED25519");
        assert_eq!(ssh_key_type_from_name("id_ecdsa"), "ECDSA");
        assert_eq!(ssh_key_type_from_name("id_rsa"), "RSA");
        assert_eq!(ssh_key_type_from_name("id_dsa"), "DSA");
        assert_eq!(ssh_key_type_from_name("custom"), "Unknown");
    }

    #[test]
    fn default_private_key_paths_keep_preferred_order_before_extra_candidates() {
        let home = unique_temp_dir("paths");
        let ssh = home.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::write(ssh.join("id_work"), "").unwrap();
        std::fs::write(ssh.join("id_ed25519_sk.pub"), "").unwrap();
        std::fs::write(ssh.join("id_ed25519-cert.pub"), "").unwrap();

        let names = default_private_key_paths_in_home(home.clone())
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["id_ed25519", "id_ecdsa", "id_rsa", "id_work"]);
        let _ = std::fs::remove_dir_all(home);
    }

    fn write_test_key(path: &PathBuf, passphrase: Option<&str>) {
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = match passphrase {
            Some(passphrase) => key.encrypt(&mut rng, passphrase).unwrap(),
            None => key,
        };
        key.write_openssh_file(path, LineEnding::LF).unwrap();
    }

    #[test]
    fn default_private_key_status_distinguishes_loadable_promptable_and_invalid() {
        let home = unique_temp_dir("status");
        let loadable = home.join("id_work");
        let encrypted = home.join("id_secret");
        let invalid = home.join("id_invalid");
        write_test_key(&loadable, None);
        write_test_key(&encrypted, Some("secret-pass"));
        std::fs::write(&invalid, "not a private key").unwrap();

        assert_eq!(
            default_private_key_status(&loadable, None),
            Some(DefaultPrivateKeyStatus::Loadable)
        );
        assert_eq!(
            default_private_key_status(&encrypted, None),
            Some(DefaultPrivateKeyStatus::RequiresPassphrase)
        );
        assert_eq!(default_private_key_status(&invalid, None), None);
        let _ = std::fs::remove_dir_all(home);
    }
}
