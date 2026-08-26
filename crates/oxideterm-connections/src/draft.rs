use std::{fmt, path::PathBuf};

use anyhow::Result;
use chrono::Utc;

use crate::{
    ConnectionOptions, ConnectionTerminalOptions, SaveConnectionRequest, SavedAuth,
    SavedConnection, SavedProxyHop, SavedUpstreamProxyPolicy, SecretString,
    SshAlgorithmPreferences, SshConfigHost,
    ssh_keys::{
        DefaultPrivateKeyStatus, default_private_key_paths_in_ssh_dir, default_private_key_status,
    },
    ssh_paths::default_ssh_dir,
};

pub const IMPORTED_GROUP: &str = "Imported";
pub const SSH_CONFIG_TAG: &str = "ssh-config";
pub const SSH_PROXY_COMMAND_TAG: &str = "ssh-proxy-command";
pub const SSH_REMOTE_COMMAND_TAG: &str = "ssh-remote-command";

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
    pub gssapi_authentication: bool,
    pub gssapi_server_identity: String,
    pub gssapi_delegate_credentials: bool,
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
            .field("gssapi_authentication", &self.gssapi_authentication)
            .field(
                "gssapi_server_identity_configured",
                &!self.gssapi_server_identity.trim().is_empty(),
            )
            .field(
                "gssapi_delegate_credentials",
                &self.gssapi_delegate_credentials,
            )
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
            gssapi_authentication: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
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
    pub identity_agent: Option<String>,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
}

#[derive(Clone, Debug)]
pub struct ConnectionDraft {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth: ConnectionAuthDraft,
    pub group: String,
    pub notes: String,
    pub color: String,
    pub icon_background_color: String,
    pub icon: String,
    pub tags: Vec<String>,
    pub proxy_hops: Vec<ProxyHopDraft>,
    pub connect_timeout_seconds: u64,
    pub agent_forwarding: bool,
    pub identity_agent: Option<String>,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub dedicated_new_terminal_connection: bool,
    pub x11_forwarding: crate::ConnectionX11ForwardingOptions,
    pub post_connect_command: String,
    pub terminal: ConnectionTerminalOptions,
}

pub fn saved_connection_from_ssh_host(host: SshConfigHost) -> Result<SavedConnection> {
    let now = Utc::now();
    let has_proxy_command = host.proxy_command.is_some();
    let has_remote_command = host.remote_command.is_some();
    let post_connect_command = host
        .remote_command
        .as_ref()
        .filter(|command| !command.is_empty())
        .map(|command| command.expose_secret().to_string());
    let auth = saved_auth_from_ssh_host_options(
        host.identity_file,
        host.certificate_file,
        host.gssapi_authentication,
        host.gssapi_server_identity,
        host.gssapi_delegate_credentials,
    );
    let proxy_chain = host
        .proxy_chain
        .into_iter()
        .map(|hop| SavedProxyHop {
            host: hop.host,
            port: hop.port.unwrap_or(22),
            username: hop.user.unwrap_or_else(current_username),
            auth: saved_auth_from_ssh_host_options(
                hop.identity_file,
                hop.certificate_file,
                hop.gssapi_authentication,
                hop.gssapi_server_identity,
                hop.gssapi_delegate_credentials,
            ),
            agent_forwarding: hop.agent_forwarding,
            identity_agent: hop.identity_agent,
            agent_forwarding_socket: hop.agent_forwarding_socket,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        })
        .collect();
    let mut tags = vec![SSH_CONFIG_TAG.to_string()];
    if has_proxy_command {
        tags.push(SSH_PROXY_COMMAND_TAG.to_string());
    }
    if has_remote_command {
        tags.push(SSH_REMOTE_COMMAND_TAG.to_string());
    }
    Ok(SavedConnection {
        id: String::new(),
        version: crate::store::CONFIG_VERSION,
        name: host.alias.clone(),
        group: Some(IMPORTED_GROUP.to_string()),
        notes: None,
        host: host.hostname.unwrap_or(host.alias),
        port: host.port.unwrap_or(22),
        username: host.user.unwrap_or_else(current_username),
        auth,
        proxy_chain,
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        proxy_command: None,
        options: ConnectionOptions {
            connect_timeout_seconds: host.connect_timeout_seconds,
            agent_forwarding: host.agent_forwarding,
            identity_agent: host.identity_agent,
            agent_forwarding_socket: host.agent_forwarding_socket,
            x11_forwarding: host.x11_forwarding,
            // OpenSSH-managed RemoteCommand values use the same persisted
            // ownership boundary as commands entered in the connection form.
            post_connect_command,
            ..ConnectionOptions::default()
        },
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        icon_background_color: None,
        icon: None,
        tags,
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    })
}

fn saved_auth_from_ssh_paths(
    identity_file: Option<String>,
    certificate_file: Option<String>,
) -> SavedAuth {
    match (identity_file, certificate_file) {
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
    }
}

fn saved_auth_from_ssh_host_options(
    identity_file: Option<String>,
    certificate_file: Option<String>,
    gssapi_authentication: bool,
    gssapi_server_identity: Option<String>,
    gssapi_delegate_credentials: bool,
) -> SavedAuth {
    let fallback = saved_auth_from_ssh_paths(identity_file, certificate_file);
    if !gssapi_authentication {
        return fallback;
    }
    SavedAuth::with_kerberos_preferred(
        fallback,
        gssapi_server_identity,
        gssapi_delegate_credentials,
    )
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
        notes: (!draft.notes.trim().is_empty()).then(|| draft.notes.trim().to_string()),
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
        proxy_command: None,
        color: (!draft.color.trim().is_empty()).then(|| draft.color.trim().to_string()),
        icon_background_color: (!draft.icon_background_color.trim().is_empty())
            .then(|| draft.icon_background_color.trim().to_string()),
        icon: (!draft.icon.trim().is_empty()).then(|| draft.icon.trim().to_string()),
        tags: draft.tags,
        connect_timeout_seconds: draft.connect_timeout_seconds,
        agent_forwarding: draft.agent_forwarding,
        identity_agent: draft.identity_agent,
        agent_forwarding_socket: draft.agent_forwarding_socket,
        legacy_ssh_compatibility: draft.legacy_ssh_compatibility,
        ssh_algorithms: draft.ssh_algorithms,
        dedicated_new_terminal_connection: draft.dedicated_new_terminal_connection,
        x11_forwarding: draft.x11_forwarding,
        post_connect_command: (!draft.post_connect_command.trim().is_empty())
            .then(|| draft.post_connect_command.trim().to_string()),
        terminal: draft.terminal,
    })
}

pub fn saved_auth_from_draft(draft: ConnectionAuthDraft) -> SavedAuth {
    let kerberos_enabled = draft.gssapi_authentication;
    let kerberos_server_identity = (!draft.gssapi_server_identity.trim().is_empty())
        .then(|| draft.gssapi_server_identity.trim().to_string());
    let kerberos_delegate_credentials = draft.gssapi_delegate_credentials;
    let fallback = match draft.kind {
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
        ConnectionAuthDraftKind::TwoFactor => SavedAuth::KeyboardInteractive,
        ConnectionAuthDraftKind::Agent => SavedAuth::Agent,
    };
    apply_kerberos_preference(
        fallback,
        kerberos_enabled,
        kerberos_server_identity,
        kerberos_delegate_credentials,
    )
}

fn apply_kerberos_preference(
    fallback: SavedAuth,
    enabled: bool,
    server_identity: Option<String>,
    delegate_credentials: bool,
) -> SavedAuth {
    if enabled {
        SavedAuth::with_kerberos_preferred(fallback, server_identity, delegate_credentials)
    } else {
        fallback
    }
}

fn saved_auth_from_draft_for_save(mut draft: ConnectionAuthDraft) -> Result<SavedAuth> {
    if draft.kind == ConnectionAuthDraftKind::DefaultKey {
        draft.kind = ConnectionAuthDraftKind::SshKey;
        draft.key_path = first_available_default_key_path()?;
    }

    Ok(saved_auth_from_draft(draft))
}

fn saved_auth_from_draft_for_update(
    draft: ConnectionAuthDraft,
    existing_auth: Option<&SavedAuth>,
) -> Result<SavedAuth> {
    if draft.kind == ConnectionAuthDraftKind::Password {
        let kerberos_enabled = draft.gssapi_authentication;
        let kerberos_server_identity = (!draft.gssapi_server_identity.trim().is_empty())
            .then(|| draft.gssapi_server_identity.trim().to_string());
        let fallback = if draft.password_loaded {
            SavedAuth::Password {
                keychain_id: draft.password_keychain_id,
                plaintext_password: Some(draft.password),
            }
        } else {
            existing_auth
                .map(SavedAuth::conventional_fallback)
                .and_then(|auth| match auth {
                    SavedAuth::Password {
                        keychain_id,
                        plaintext_password,
                    } => Some(SavedAuth::Password {
                        keychain_id: keychain_id.clone(),
                        plaintext_password: plaintext_password.clone(),
                    }),
                    _ => None,
                })
                .unwrap_or(SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: None,
                })
        };
        return Ok(apply_kerberos_preference(
            fallback,
            kerberos_enabled,
            kerberos_server_identity,
            draft.gssapi_delegate_credentials,
        ));
    }

    saved_auth_from_draft_for_save(draft)
}

fn saved_proxy_chain_from_drafts(hops: Vec<ProxyHopDraft>) -> Result<Vec<SavedProxyHop>> {
    hops.into_iter()
        .map(|hop| {
            let auth = saved_proxy_hop_auth_from_draft(hop.auth)?;
            Ok(SavedProxyHop {
                host: hop.host.trim().to_string(),
                port: hop.port.trim().parse::<u16>().unwrap_or(22),
                username: hop.username.trim().to_string(),
                auth,
                agent_forwarding: hop.agent_forwarding,
                identity_agent: hop.identity_agent,
                agent_forwarding_socket: hop.agent_forwarding_socket,
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                ssh_algorithms: hop.ssh_algorithms,
            })
        })
        .collect()
}

fn saved_proxy_hop_auth_from_draft(mut auth: ConnectionAuthDraft) -> Result<SavedAuth> {
    if auth.kind == ConnectionAuthDraftKind::DefaultKey {
        auth.kind = ConnectionAuthDraftKind::SshKey;
        auth.key_path = first_loadable_default_key_path(auth.passphrase.expose_secret())
            .map_err(|error| anyhow::anyhow!("No SSH key found for proxy hop: {error}"))?;
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
    first_available_default_key_path_in_ssh_dir(default_ssh_dir())
}

#[cfg(test)]
fn first_available_default_key_path_in_home(home: PathBuf) -> Result<String> {
    first_available_default_key_path_in_ssh_dir(home.join(".ssh"))
}

fn first_available_default_key_path_in_ssh_dir(ssh_dir: PathBuf) -> Result<String> {
    for path in default_private_key_paths_in_ssh_dir(ssh_dir) {
        match default_private_key_status(&path, None) {
            Some(
                DefaultPrivateKeyStatus::Loadable | DefaultPrivateKeyStatus::RequiresPassphrase,
            ) => {
                return Ok(path.to_string_lossy().into_owned());
            }
            None => {}
        }
    }
    anyhow::bail!("No default SSH key found")
}

fn first_loadable_default_key_path(passphrase: &str) -> Result<String> {
    first_loadable_default_key_path_in_ssh_dir(default_ssh_dir(), passphrase)
}

fn first_loadable_default_key_path_in_ssh_dir(
    ssh_dir: PathBuf,
    passphrase: &str,
) -> Result<String> {
    let passphrase = (!passphrase.is_empty()).then_some(passphrase);
    let mut saw_encrypted_key = false;

    for path in default_private_key_paths_in_ssh_dir(ssh_dir) {
        match default_private_key_status(&path, passphrase) {
            Some(DefaultPrivateKeyStatus::Loadable) => {
                return Ok(path.to_string_lossy().into_owned());
            }
            Some(DefaultPrivateKeyStatus::RequiresPassphrase) => {
                saw_encrypted_key = true;
            }
            None => {}
        }
    }

    if saw_encrypted_key {
        anyhow::bail!("Encrypted key requires passphrase")
    } else {
        anyhow::bail!("Key file not found: ~/.ssh/id_*")
    }
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
    fn ssh_config_proxy_jump_becomes_saved_proxy_chain() {
        let connection = saved_connection_from_ssh_host(SshConfigHost {
            alias: "production".to_string(),
            hostname: Some("production.example.com".to_string()),
            user: Some("deployer".to_string()),
            proxy_chain: vec![crate::SshConfigProxyHop {
                host: "jump.example.com".to_string(),
                user: Some("operator".to_string()),
                port: Some(2200),
                identity_file: Some("/keys/jump".to_string()),
                certificate_file: None,
                gssapi_authentication: false,
                gssapi_server_identity: None,
                gssapi_delegate_credentials: false,
                identity_agent: Some("/tmp/jump-agent.sock".to_string()),
                agent_forwarding: true,
                agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
            }],
            identity_agent: Some("/tmp/target-agent.sock".to_string()),
            agent_forwarding: true,
            agent_forwarding_socket: Some("/tmp/target-forward.sock".to_string()),
            ..SshConfigHost::default()
        })
        .unwrap();

        assert_eq!(connection.proxy_chain.len(), 1);
        assert_eq!(connection.proxy_chain[0].host, "jump.example.com");
        assert_eq!(connection.proxy_chain[0].username, "operator");
        assert_eq!(connection.proxy_chain[0].port, 2200);
        assert!(matches!(
            connection.proxy_chain[0].auth,
            SavedAuth::Key { ref key_path, .. } if key_path == "/keys/jump"
        ));
        assert!(connection.options.agent_forwarding);
        assert_eq!(
            connection.options.identity_agent.as_deref(),
            Some("/tmp/target-agent.sock")
        );
        assert_eq!(
            connection.options.agent_forwarding_socket.as_deref(),
            Some("/tmp/target-forward.sock")
        );
        assert!(connection.proxy_chain[0].agent_forwarding);
        assert_eq!(
            connection.proxy_chain[0].identity_agent.as_deref(),
            Some("/tmp/jump-agent.sock")
        );
        assert_eq!(
            connection.proxy_chain[0].agent_forwarding_socket.as_deref(),
            Some("/tmp/jump-forward.sock")
        );
    }

    #[test]
    fn ssh_config_proxy_command_persists_only_a_non_secret_marker() {
        let connection = saved_connection_from_ssh_host(SshConfigHost {
            alias: "edge".to_string(),
            proxy_command: Some(vec![
                SecretString::new("helper-with-token"),
                SecretString::new("credential-value"),
            ]),
            ..SshConfigHost::default()
        })
        .unwrap();
        let serialized = serde_json::to_string(&connection).unwrap();

        assert!(connection.tags.iter().any(|tag| tag == SSH_CONFIG_TAG));
        assert!(
            connection
                .tags
                .iter()
                .any(|tag| tag == SSH_PROXY_COMMAND_TAG)
        );
        assert!(!serialized.contains("helper-with-token"));
        assert!(!serialized.contains("credential-value"));
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
    fn kerberos_preference_preserves_the_conventional_fallback() {
        let existing = SavedAuth::Password {
            keychain_id: Some("password-key".to_string()),
            plaintext_password: None,
        };
        let mut draft = password_draft();
        draft.password_loaded = false;
        draft.gssapi_authentication = true;
        draft.gssapi_server_identity = "host/server.example.com".to_string();
        draft.gssapi_delegate_credentials = true;

        let auth = saved_auth_from_draft_for_update(draft, Some(&existing)).unwrap();

        assert!(matches!(
            auth,
            SavedAuth::KerberosPreferred {
                server_identity: Some(ref identity),
                delegate_credentials: true,
                fallback,
            } if identity == "host/server.example.com"
                && matches!(*fallback, SavedAuth::Password {
                    keychain_id: Some(ref keychain_id),
                    plaintext_password: None,
                } if keychain_id == "password-key")
        ));
    }

    #[test]
    fn proxy_hop_two_factor_is_saved_as_keyboard_interactive() {
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
            notes: String::new(),
            color: String::new(),
            icon_background_color: String::new(),
            icon: String::new(),
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
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                ssh_algorithms: SshAlgorithmPreferences::default(),
            }],
            connect_timeout_seconds: crate::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            x11_forwarding: crate::ConnectionX11ForwardingOptions::default(),
            dedicated_new_terminal_connection: false,
            post_connect_command: String::new(),
            terminal: ConnectionTerminalOptions::default(),
        };

        let request = save_request_from_draft(draft, None, None).unwrap();

        assert!(matches!(
            request.proxy_chain[0].auth,
            SavedAuth::KeyboardInteractive
        ));
    }

    #[test]
    fn saving_default_key_resolves_first_parseable_or_promptable_key_path() {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm-conn-default-key-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let ssh_dir = dir.join(".ssh");
        std::fs::create_dir_all(&ssh_dir).unwrap();
        let rsa = ssh_dir.join("id_rsa");
        let ecdsa = ssh_dir.join("id_ecdsa");
        std::fs::write(&ecdsa, "not a private key").unwrap();
        let mut rng = UnwrapErr(SysRng);
        PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .unwrap()
            .write_openssh_file(&rsa, LineEnding::LF)
            .unwrap();

        let path = first_available_default_key_path_in_home(dir.clone()).unwrap();

        assert_eq!(path, rsa.to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_default_key_can_return_encrypted_key_to_prompt_later() {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm-conn-default-key-encrypted-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let ssh_dir = dir.join(".ssh");
        std::fs::create_dir_all(&ssh_dir).unwrap();
        let encrypted = ssh_dir.join("id_ed25519");
        let mut rng = UnwrapErr(SysRng);
        PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .unwrap()
            .encrypt(&mut rng, "secret")
            .unwrap()
            .write_openssh_file(&encrypted, LineEnding::LF)
            .unwrap();

        let path = first_available_default_key_path_in_home(dir.clone()).unwrap();

        assert_eq!(path, encrypted.to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }
}
