use std::{fmt, path::PathBuf};

use anyhow::Result;
use chrono::Utc;
use zeroize::Zeroizing;

use crate::{
    ConnectionOptions, SaveConnectionRequest, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyPolicy, SecretString, SshConfigHost,
};

pub const IMPORTED_GROUP: &str = "Imported";
pub const SSH_CONFIG_TAG: &str = "ssh-config";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionAuthDraftKind {
    Password,
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone)]
pub struct ConnectionAuthDraft {
    pub kind: ConnectionAuthDraftKind,
    pub password: SecretString,
    pub password_keychain_id: Option<String>,
    pub password_loaded: bool,
    pub save_password: bool,
    pub key_path: String,
    pub managed_key_id: String,
    pub cert_path: String,
    pub passphrase: SecretString,
}

impl fmt::Debug for ConnectionAuthDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionAuthDraft")
            .field("kind", &self.kind)
            .field("password", &self.password)
            .field("password_keychain_id", &self.password_keychain_id)
            .field("password_loaded", &self.password_loaded)
            .field("save_password", &self.save_password)
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &self.passphrase)
            .finish()
    }
}

impl Default for ConnectionAuthDraft {
    fn default() -> Self {
        Self {
            kind: ConnectionAuthDraftKind::Password,
            password: SecretString::default(),
            password_keychain_id: None,
            password_loaded: true,
            save_password: false,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: SecretString::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProxyHopDraft {
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth: ConnectionAuthDraft,
    pub agent_forwarding: bool,
}

#[derive(Clone, Debug)]
pub struct ConnectionDraft {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth: ConnectionAuthDraft,
    pub group: String,
    pub color: String,
    pub tags: Vec<String>,
    pub proxy_hops: Vec<ProxyHopDraft>,
    pub agent_forwarding: bool,
    pub post_connect_command: String,
}

pub fn saved_connection_from_ssh_host(host: SshConfigHost) -> Result<SavedConnection> {
    let now = Utc::now();
    let auth = match (host.identity_file, host.certificate_file) {
        (Some(key_path), Some(cert_path)) => SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        (Some(key_path), None) => SavedAuth::Key {
            key_path,
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        _ => SavedAuth::Agent,
    };
    Ok(SavedConnection {
        id: String::new(),
        version: crate::store::CONFIG_VERSION,
        name: host.alias.clone(),
        group: Some(IMPORTED_GROUP.to_string()),
        host: host.hostname.unwrap_or(host.alias),
        port: host.port.unwrap_or(22),
        username: host.user.unwrap_or_else(current_username),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        options: ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        tags: vec![SSH_CONFIG_TAG.to_string()],
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    })
}

pub fn save_request_from_draft(
    draft: ConnectionDraft,
    id: Option<String>,
    existing_auth: Option<&SavedAuth>,
) -> Result<SaveConnectionRequest> {
    let port = draft.port.trim().parse::<u16>().unwrap_or(22);
    Ok(SaveConnectionRequest {
        id,
        name: draft.name.trim().to_string(),
        group: Some(draft.group.trim().to_string()),
        host: draft.host.trim().to_string(),
        port,
        username: draft.username.trim().to_string(),
        auth: if existing_auth.is_some() {
            saved_auth_from_draft_for_update(draft.auth, existing_auth)?
        } else {
            saved_auth_from_draft_for_save(draft.auth)?
        },
        proxy_chain: saved_proxy_chain_from_drafts(draft.proxy_hops)?,
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        color: (!draft.color.trim().is_empty()).then(|| draft.color.trim().to_string()),
        tags: draft.tags,
        agent_forwarding: draft.agent_forwarding,
        post_connect_command: (!draft.post_connect_command.trim().is_empty())
            .then(|| draft.post_connect_command.trim().to_string()),
    })
}

pub fn saved_auth_from_draft(draft: ConnectionAuthDraft) -> SavedAuth {
    match draft.kind {
        ConnectionAuthDraftKind::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: draft.save_password.then_some(draft.password),
        },
        ConnectionAuthDraftKind::DefaultKey => SavedAuth::Key {
            key_path: String::new(),
            has_passphrase: !draft.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!draft.passphrase.is_empty()).then_some(draft.passphrase),
        },
        ConnectionAuthDraftKind::SshKey => SavedAuth::Key {
            key_path: draft.key_path.trim().to_string(),
            has_passphrase: !draft.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!draft.passphrase.is_empty()).then_some(draft.passphrase),
        },
        ConnectionAuthDraftKind::ManagedKey => SavedAuth::ManagedKey {
            key_id: draft.managed_key_id.trim().to_string(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!draft.passphrase.is_empty()).then_some(draft.passphrase),
        },
        ConnectionAuthDraftKind::Certificate => SavedAuth::Certificate {
            key_path: draft.key_path.trim().to_string(),
            cert_path: draft.cert_path.trim().to_string(),
            has_passphrase: !draft.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!draft.passphrase.is_empty()).then_some(draft.passphrase),
        },
        ConnectionAuthDraftKind::Agent | ConnectionAuthDraftKind::TwoFactor => SavedAuth::Agent,
    }
}

fn saved_auth_from_draft_for_save(draft: ConnectionAuthDraft) -> Result<SavedAuth> {
    if draft.kind == ConnectionAuthDraftKind::DefaultKey {
        return Ok(SavedAuth::Key {
            key_path: first_available_default_key_path()?,
            has_passphrase: !draft.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!draft.passphrase.is_empty()).then_some(draft.passphrase),
        });
    }

    Ok(saved_auth_from_draft(draft))
}

fn saved_auth_from_draft_for_update(
    draft: ConnectionAuthDraft,
    existing_auth: Option<&SavedAuth>,
) -> Result<SavedAuth> {
    if draft.kind == ConnectionAuthDraftKind::Password && !draft.password_loaded {
        if let Some(SavedAuth::Password {
            keychain_id,
            plaintext_password,
        }) = existing_auth
        {
            return Ok(SavedAuth::Password {
                keychain_id: keychain_id.clone(),
                plaintext_password: plaintext_password.clone(),
            });
        }
        return Ok(SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        });
    }

    if draft.kind == ConnectionAuthDraftKind::Password {
        return Ok(SavedAuth::Password {
            keychain_id: draft.password_keychain_id,
            plaintext_password: Some(draft.password),
        });
    }

    saved_auth_from_draft_for_save(draft)
}

fn saved_proxy_chain_from_drafts(hops: Vec<ProxyHopDraft>) -> Result<Vec<SavedProxyHop>> {
    hops.into_iter()
        .enumerate()
        .map(|(index, hop)| {
            let auth = saved_proxy_hop_auth_from_draft(hop.auth, index + 1)?;
            Ok(SavedProxyHop {
                host: hop.host.trim().to_string(),
                port: hop.port.trim().parse::<u16>().unwrap_or(22),
                username: hop.username.trim().to_string(),
                auth,
                agent_forwarding: hop.agent_forwarding,
            })
        })
        .collect()
}

fn saved_proxy_hop_auth_from_draft(
    mut auth: ConnectionAuthDraft,
    hop_index: usize,
) -> Result<SavedAuth> {
    if auth.kind == ConnectionAuthDraftKind::TwoFactor {
        anyhow::bail!("Proxy hop {hop_index} does not support keyboard-interactive/2FA");
    }
    if auth.kind == ConnectionAuthDraftKind::DefaultKey {
        return Ok(SavedAuth::Key {
            key_path: first_loadable_default_key_path(auth.passphrase.expose_secret())
                .map_err(|error| anyhow::anyhow!("No SSH key found for proxy hop: {error}"))?,
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        });
    }
    if auth.kind == ConnectionAuthDraftKind::Password {
        auth.save_password = true;
    }
    saved_auth_from_draft_for_save(auth)
}

fn current_username() -> String {
    whoami::username()
}

pub fn first_available_default_key_path() -> Result<String> {
    first_available_default_key_path_in_home(
        std::env::home_dir().unwrap_or_else(|| PathBuf::from(".")),
    )
}

fn first_available_default_key_path_in_home(home: PathBuf) -> Result<String> {
    for path in default_key_paths_in_home(home) {
        if path.exists() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    anyhow::bail!("No default SSH key found")
}

fn default_key_paths_in_home(home: PathBuf) -> [PathBuf; 3] {
    let ssh = home.join(".ssh");
    [
        ssh.join("id_ed25519"),
        ssh.join("id_ecdsa"),
        ssh.join("id_rsa"),
    ]
}

fn first_loadable_default_key_path(passphrase: &str) -> Result<String> {
    first_loadable_default_key_path_in_home(
        std::env::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        passphrase,
    )
}

fn first_loadable_default_key_path_in_home(home: PathBuf, passphrase: &str) -> Result<String> {
    let passphrase = (!passphrase.is_empty()).then_some(passphrase);
    let mut saw_encrypted_key = false;

    for path in default_key_paths_in_home(home) {
        if !path.exists() {
            continue;
        }

        let key_data = match std::fs::read_to_string(&path) {
            Ok(contents) => Zeroizing::new(contents),
            Err(_) => continue,
        };
        let is_encrypted =
            key_data.contains("ENCRYPTED") || key_data.contains("Proc-Type: 4,ENCRYPTED");
        if is_encrypted && passphrase.is_none() {
            saw_encrypted_key = true;
            continue;
        }

        match russh::keys::decode_secret_key(&key_data, passphrase) {
            Ok(_) => return Ok(path.to_string_lossy().into_owned()),
            Err(error) if key_error_is_passphrase_related(&error) => {
                saw_encrypted_key = true;
            }
            Err(_) => {}
        }
    }

    if saw_encrypted_key {
        anyhow::bail!("Encrypted key requires passphrase")
    } else {
        anyhow::bail!("Key file not found: ~/.ssh/id_*")
    }
}

fn key_error_is_passphrase_related(error: &russh::keys::Error) -> bool {
    let normalized = error.to_string().to_ascii_lowercase();
    normalized.contains("decrypt")
        || normalized.contains("password")
        || normalized.contains("passphrase")
        || normalized.contains("encrypted")
        || normalized.contains("bcrypt")
        || normalized.contains("kdf")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::{Algorithm, PrivateKey, ssh_key::LineEnding};

    fn password_draft() -> ConnectionAuthDraft {
        ConnectionAuthDraft {
            kind: ConnectionAuthDraftKind::Password,
            password: SecretString::from("secret"),
            save_password: true,
            ..ConnectionAuthDraft::default()
        }
    }

    #[test]
    fn new_password_draft_obeys_save_flag() {
        let mut draft = password_draft();
        draft.save_password = false;
        assert!(matches!(
            saved_auth_from_draft(draft),
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None
            }
        ));
    }

    #[test]
    fn edit_password_unloaded_preserves_existing_auth() {
        let existing = SavedAuth::Password {
            keychain_id: Some("password-key".to_string()),
            plaintext_password: None,
        };
        let mut draft = password_draft();
        draft.password_loaded = false;
        let auth = saved_auth_from_draft_for_update(draft, Some(&existing)).unwrap();
        assert!(matches!(
            auth,
            SavedAuth::Password {
                keychain_id: Some(ref keychain_id),
                plaintext_password: None
            } if keychain_id == "password-key"
        ));
    }

    #[test]
    fn edit_password_loaded_saves_explicit_value() {
        let existing = SavedAuth::Password {
            keychain_id: Some("password-key".to_string()),
            plaintext_password: None,
        };
        let mut draft = password_draft();
        draft.password_keychain_id = Some("password-key".to_string());
        let auth = saved_auth_from_draft_for_update(draft, Some(&existing)).unwrap();
        assert!(matches!(
            auth,
            SavedAuth::Password {
                keychain_id: Some(ref keychain_id),
                plaintext_password: Some(ref password)
            } if keychain_id == "password-key" && password == "secret"
        ));
    }

    #[test]
    fn proxy_hop_two_factor_is_rejected_instead_of_saved_as_agent() {
        let draft = ConnectionDraft {
            name: "Home".to_string(),
            host: "target.example.com".to_string(),
            port: "22".to_string(),
            username: "me".to_string(),
            auth: ConnectionAuthDraft {
                kind: ConnectionAuthDraftKind::Agent,
                ..ConnectionAuthDraft::default()
            },
            group: "Ungrouped".to_string(),
            color: String::new(),
            tags: Vec::new(),
            proxy_hops: vec![ProxyHopDraft {
                host: "jump.example.com".to_string(),
                port: "22".to_string(),
                username: "ops".to_string(),
                auth: ConnectionAuthDraft {
                    kind: ConnectionAuthDraftKind::TwoFactor,
                    ..ConnectionAuthDraft::default()
                },
                agent_forwarding: false,
            }],
            agent_forwarding: false,
            post_connect_command: String::new(),
        };

        let error = save_request_from_draft(draft, None, None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Proxy hop 1 does not support keyboard-interactive/2FA"
        );
    }

    #[test]
    fn default_key_paths_match_tauri_save_order() {
        let home = PathBuf::from("/tmp/home");
        let paths = default_key_paths_in_home(home);

        assert_eq!(paths[0], PathBuf::from("/tmp/home/.ssh/id_ed25519"));
        assert_eq!(paths[1], PathBuf::from("/tmp/home/.ssh/id_ecdsa"));
        assert_eq!(paths[2], PathBuf::from("/tmp/home/.ssh/id_rsa"));
    }

    #[test]
    fn saving_default_key_resolves_first_existing_key_path_like_tauri() {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm-conn-default-key-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let ssh_dir = dir.join(".ssh");
        std::fs::create_dir_all(&ssh_dir).unwrap();
        let rsa = ssh_dir.join("id_rsa");
        let ecdsa = ssh_dir.join("id_ecdsa");
        std::fs::write(&rsa, "rsa").unwrap();
        std::fs::write(&ecdsa, "ecdsa").unwrap();

        let path = first_available_default_key_path_in_home(dir.clone()).unwrap();

        assert_eq!(path, ecdsa.to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_proxy_default_key_uses_first_loadable_default_key_like_tauri() {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm-conn-proxy-default-key-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let ssh_dir = dir.join(".ssh");
        std::fs::create_dir_all(&ssh_dir).unwrap();
        let encrypted = ssh_dir.join("id_ed25519");
        let fallback = ssh_dir.join("id_ecdsa");
        let mut rng = UnwrapErr(SysRng);
        PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .unwrap()
            .encrypt(&mut rng, "secret")
            .unwrap()
            .write_openssh_file(&encrypted, LineEnding::LF)
            .unwrap();
        PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .unwrap()
            .write_openssh_file(&fallback, LineEnding::LF)
            .unwrap();

        let path = first_loadable_default_key_path_in_home(dir.clone(), "").unwrap();

        assert_eq!(path, fallback.to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }
}
