use anyhow::Result;
use chrono::Utc;

use crate::{
    ConnectionOptions, SaveConnectionRequest, SavedAuth, SavedConnection, SavedProxyHop,
    SshConfigHost,
};

pub const IMPORTED_GROUP: &str = "Imported";
pub const SSH_CONFIG_TAG: &str = "ssh-config";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionAuthDraftKind {
    Password,
    DefaultKey,
    SshKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone, Debug)]
pub struct ConnectionAuthDraft {
    pub kind: ConnectionAuthDraftKind,
    pub password: String,
    pub password_keychain_id: Option<String>,
    pub password_loaded: bool,
    pub save_password: bool,
    pub key_path: String,
    pub cert_path: String,
    pub passphrase: String,
}

impl Default for ConnectionAuthDraft {
    fn default() -> Self {
        Self {
            kind: ConnectionAuthDraftKind::Password,
            password: String::new(),
            password_keychain_id: None,
            password_loaded: true,
            save_password: false,
            key_path: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
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
        name: host.alias.clone(),
        group: Some(IMPORTED_GROUP.to_string()),
        host: host.hostname.unwrap_or(host.alias),
        port: host.port.unwrap_or(22),
        username: host.user.unwrap_or_else(current_username),
        auth,
        proxy_chain: Vec::new(),
        options: ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        tags: vec![SSH_CONFIG_TAG.to_string()],
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
            saved_auth_from_draft_for_update(draft.auth, existing_auth)
        } else {
            saved_auth_from_draft(draft.auth)
        },
        proxy_chain: saved_proxy_chain_from_drafts(draft.proxy_hops),
        color: (!draft.color.trim().is_empty()).then(|| draft.color.trim().to_string()),
        tags: draft.tags,
        agent_forwarding: draft.agent_forwarding,
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

fn saved_auth_from_draft_for_update(
    draft: ConnectionAuthDraft,
    existing_auth: Option<&SavedAuth>,
) -> SavedAuth {
    if draft.kind == ConnectionAuthDraftKind::Password && !draft.password_loaded {
        if let Some(SavedAuth::Password {
            keychain_id,
            plaintext_password,
        }) = existing_auth
        {
            return SavedAuth::Password {
                keychain_id: keychain_id.clone(),
                plaintext_password: plaintext_password.clone(),
            };
        }
        return SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        };
    }

    if draft.kind == ConnectionAuthDraftKind::Password {
        return SavedAuth::Password {
            keychain_id: draft.password_keychain_id,
            plaintext_password: Some(draft.password),
        };
    }

    saved_auth_from_draft(draft)
}

fn saved_proxy_chain_from_drafts(hops: Vec<ProxyHopDraft>) -> Vec<SavedProxyHop> {
    hops.into_iter()
        .map(|hop| SavedProxyHop {
            host: hop.host.trim().to_string(),
            port: hop.port.trim().parse::<u16>().unwrap_or(22),
            username: hop.username.trim().to_string(),
            auth: saved_proxy_hop_auth_from_draft(hop.auth),
            agent_forwarding: hop.agent_forwarding,
        })
        .collect()
}

fn saved_proxy_hop_auth_from_draft(mut auth: ConnectionAuthDraft) -> SavedAuth {
    if auth.kind == ConnectionAuthDraftKind::Password {
        auth.save_password = true;
    }
    saved_auth_from_draft(auth)
}

fn current_username() -> String {
    whoami::username()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_draft() -> ConnectionAuthDraft {
        ConnectionAuthDraft {
            kind: ConnectionAuthDraftKind::Password,
            password: "secret".to_string(),
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
        let auth = saved_auth_from_draft_for_update(draft, Some(&existing));
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
        let auth = saved_auth_from_draft_for_update(draft, Some(&existing));
        assert!(matches!(
            auth,
            SavedAuth::Password {
                keychain_id: Some(ref keychain_id),
                plaintext_password: Some(ref password)
            } if keychain_id == "password-key" && password == "secret"
        ));
    }
}
