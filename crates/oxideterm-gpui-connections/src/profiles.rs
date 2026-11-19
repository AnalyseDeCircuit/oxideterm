// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, MoshUdpPortSelection as SavedMoshUdpPortSelection, SaveConnectionRequest,
    SavedAuth, SavedConnection, SavedMoshProfileRuntimeSecrets, SavedProxyCommand, SavedProxyHop,
    SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
    SecretString, StandaloneSftpEndpoint,
};
use oxideterm_ssh::{AuthMethod, ProxyHopConfig, SshConfig};
use zeroize::Zeroize;

use crate::{
    conversion::take_zeroizing_secret,
    form_state::{
        NewConnectionForm, NewConnectionProxyHop, NewConnectionUpstreamProxyAuth,
        NewConnectionUpstreamProxyPolicy, SshAuthTab, StandaloneSftpSecondaryForm,
        identity_agent_from_form, ssh_auth_tab_from_saved_auth,
    },
    launch::MoshConnectionOptions,
};

fn saved_authentication_plan(
    fallback: SavedAuth,
    enabled: bool,
    server_identity: &str,
    delegate_credentials: bool,
) -> SavedAuth {
    if !enabled {
        return fallback;
    }
    SavedAuth::with_kerberos_preferred(
        fallback,
        (!server_identity.trim().is_empty()).then(|| server_identity.trim().to_string()),
        delegate_credentials,
    )
}

fn runtime_authentication_plan(
    fallback: AuthMethod,
    enabled: bool,
    server_identity: &str,
    delegate_credentials: bool,
) -> AuthMethod {
    if !enabled {
        return fallback;
    }
    AuthMethod::kerberos_preferred(
        fallback,
        (!server_identity.trim().is_empty()).then(|| server_identity.trim().to_string()),
        delegate_credentials,
    )
}

pub fn parse_mosh_udp_port(value: &str) -> anyhow::Result<SavedMoshUdpPortSelection> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(SavedMoshUdpPortSelection::Automatic);
    }
    if let Some((start, end)) = value.split_once(':') {
        let start = start.trim().parse::<u16>()?;
        let end = end.trim().parse::<u16>()?;
        if start == 0 || end == 0 || start > end {
            anyhow::bail!("invalid Mosh UDP port range");
        }
        return Ok(SavedMoshUdpPortSelection::Range { start, end });
    }
    let port = value.parse::<u16>()?;
    if port == 0 {
        anyhow::bail!("invalid Mosh UDP port");
    }
    Ok(SavedMoshUdpPortSelection::Fixed { port })
}

pub fn mosh_password_draft_is_persistent(form: &NewConnectionForm) -> bool {
    form.save_password || (form.mosh_profile_id.is_some() && !form.password.is_empty())
}

pub fn rdp_proxy_policy_from_form(
    form: &mut NewConnectionForm,
) -> Result<SavedUpstreamProxyPolicy, &'static str> {
    Ok(match form.upstream_proxy_policy {
        NewConnectionUpstreamProxyPolicy::UseGlobal => SavedUpstreamProxyPolicy::UseGlobal,
        NewConnectionUpstreamProxyPolicy::Direct => SavedUpstreamProxyPolicy::Direct,
        NewConnectionUpstreamProxyPolicy::Custom => {
            let host = form.upstream_proxy_host.trim().to_string();
            if host.is_empty() {
                return Err("modals.new_connection.remote_desktop_proxy_host_required");
            }
            let port = form
                .upstream_proxy_port
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .ok_or("modals.new_connection.remote_desktop_proxy_port_invalid")?;
            let auth = match form.upstream_proxy_auth {
                NewConnectionUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
                NewConnectionUpstreamProxyAuth::Password => {
                    let username = form.upstream_proxy_username.trim().to_string();
                    if username.is_empty() {
                        return Err("modals.new_connection.remote_desktop_proxy_username_required");
                    }
                    SavedUpstreamProxyAuth::Password {
                        username,
                        keychain_id: form.upstream_proxy_password_keychain_id.clone(),
                        plaintext_password: (!form.upstream_proxy_password.is_empty()).then(|| {
                            SecretString::from(std::mem::take(&mut form.upstream_proxy_password))
                        }),
                    }
                }
            };
            SavedUpstreamProxyPolicy::Custom {
                proxy: SavedUpstreamProxyConfig {
                    protocol: oxideterm_connections::SavedUpstreamProxyProtocol::Socks5,
                    host,
                    port,
                    auth,
                    remote_dns: form.upstream_proxy_remote_dns,
                    no_proxy: form.upstream_proxy_no_proxy.trim().to_string(),
                },
            }
        }
    })
}

pub fn saved_profile_notes(notes: &str) -> Option<String> {
    let notes = notes.trim();
    (!notes.is_empty()).then(|| notes.to_string())
}

fn standalone_sftp_secondary_target_matches(
    form: &StandaloneSftpSecondaryForm,
    endpoint: &StandaloneSftpEndpoint,
) -> bool {
    form.host.trim() == endpoint.host
        && form.port.trim().parse::<u16>().ok() == Some(endpoint.port)
        && form.username.trim() == endpoint.username
}

fn standalone_sftp_secondary_auth_matches(
    form: &StandaloneSftpSecondaryForm,
    auth: &SavedAuth,
) -> bool {
    form.auth_tab == ssh_auth_tab_from_saved_auth(auth)
        && form.gssapi_enabled == auth.gssapi_options().is_some()
        && form.key_path.trim() == auth.key_path().unwrap_or_default()
        && form.cert_path.trim() == auth.cert_path().unwrap_or_default()
        && form.managed_key_id.trim() == auth.managed_key_id().unwrap_or_default()
        && form.gssapi_server_identity.trim()
            == auth
                .gssapi_options()
                .and_then(|(identity, _)| identity)
                .unwrap_or_default()
        && form.gssapi_delegate_credentials
            == auth.gssapi_options().is_some_and(|(_, delegate)| delegate)
}

pub fn saved_standalone_sftp_proxy_hop_from_form(
    hop: &mut NewConnectionProxyHop,
) -> anyhow::Result<SavedProxyHop> {
    let host = hop.host.trim().to_string();
    let username = hop.username.trim().to_string();
    if host.is_empty() || username.is_empty() {
        anyhow::bail!("Proxy host and username are required");
    }
    let port = hop
        .port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| anyhow::anyhow!("Proxy port is invalid"))?;
    let fallback = match hop.auth_tab {
        SshAuthTab::Password => SavedAuth::Password {
            empty_password: hop.empty_password,
            keychain_id: None,
            plaintext_password: (!hop.empty_password && !hop.password.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut hop.password))),
        },
        SshAuthTab::Agent => SavedAuth::Agent,
        SshAuthTab::DefaultKey => SavedAuth::Key {
            key_path: String::new(),
            has_passphrase: !hop.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!hop.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut hop.passphrase))),
        },
        SshAuthTab::SshKey => SavedAuth::Key {
            key_path: hop.key_path.trim().to_string(),
            has_passphrase: !hop.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!hop.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut hop.passphrase))),
        },
        SshAuthTab::ManagedKey => SavedAuth::ManagedKey {
            key_id: hop.managed_key_id.trim().to_string(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!hop.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut hop.passphrase))),
        },
        SshAuthTab::Certificate => SavedAuth::Certificate {
            key_path: hop.key_path.trim().to_string(),
            cert_path: hop.cert_path.trim().to_string(),
            has_passphrase: !hop.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!hop.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut hop.passphrase))),
        },
        SshAuthTab::TwoFactor => SavedAuth::KeyboardInteractive,
    };
    let auth = saved_authentication_plan(
        fallback,
        hop.gssapi_enabled,
        &hop.gssapi_server_identity,
        hop.gssapi_delegate_credentials,
    );
    Ok(SavedProxyHop {
        totp_credential_id: hop.totp_credential_id.clone(),
        host,
        port,
        username,
        auth,
        agent_forwarding: hop.agent_forwarding,
        identity_agent: identity_agent_from_form(&hop.identity_agent),
        agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
        ssh_algorithms: hop.ssh_algorithms.clone(),
    })
}

fn saved_standalone_sftp_secondary_route_from_form(
    connection_store: &ConnectionStore,
    form: &mut StandaloneSftpSecondaryForm,
    existing: Option<&StandaloneSftpEndpoint>,
    missing_credentials_message: &str,
) -> anyhow::Result<(
    Vec<SavedProxyHop>,
    SavedUpstreamProxyPolicy,
    Option<SavedProxyCommand>,
)> {
    let saved_auth_copies = saved_proxy_hop_auth_copies(
        connection_store,
        &form.proxy_hops,
        missing_credentials_message,
    )?;
    let edit_sources = form
        .proxy_hops
        .iter()
        .map(|hop| EditedProxyHopSource {
            persisted_proxy_hop_index: hop.persisted_proxy_hop_index,
            has_explicit_secret_draft: hop.has_explicit_secret_draft(),
        })
        .collect();
    let mut proxy_chain = form
        .proxy_hops
        .iter_mut()
        .map(saved_standalone_sftp_proxy_hop_from_form)
        .collect::<anyhow::Result<Vec<_>>>()?;
    preserve_edited_proxy_hop_auth(
        &mut proxy_chain,
        existing.map_or(&[], |endpoint| endpoint.proxy_chain.as_slice()),
        edit_sources,
        saved_auth_copies,
        missing_credentials_message,
    )?;

    let upstream_proxy = match form.upstream_proxy_policy {
        NewConnectionUpstreamProxyPolicy::UseGlobal => SavedUpstreamProxyPolicy::UseGlobal,
        NewConnectionUpstreamProxyPolicy::Direct => SavedUpstreamProxyPolicy::Direct,
        NewConnectionUpstreamProxyPolicy::Custom => {
            let host = form.upstream_proxy_host.trim().to_string();
            if host.is_empty() {
                anyhow::bail!("Upstream proxy host is required");
            }
            let port = form
                .upstream_proxy_port
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .ok_or_else(|| anyhow::anyhow!("Upstream proxy port is invalid"))?;
            let auth = match form.upstream_proxy_auth {
                NewConnectionUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
                NewConnectionUpstreamProxyAuth::Password => {
                    let username = form.upstream_proxy_username.trim().to_string();
                    if username.is_empty() {
                        anyhow::bail!("Upstream proxy username is required");
                    }
                    SavedUpstreamProxyAuth::Password {
                        username,
                        keychain_id: form.upstream_proxy_password_keychain_id.clone(),
                        plaintext_password: (!form.upstream_proxy_password.is_empty()).then(|| {
                            SecretString::from(std::mem::take(&mut form.upstream_proxy_password))
                        }),
                    }
                }
            };
            SavedUpstreamProxyPolicy::Custom {
                proxy: SavedUpstreamProxyConfig {
                    protocol: form.upstream_proxy_protocol,
                    host,
                    port,
                    auth,
                    remote_dns: form.upstream_proxy_remote_dns,
                    no_proxy: form.upstream_proxy_no_proxy.trim().to_string(),
                },
            }
        }
    };
    let proxy_command = if form.proxy_command_enabled {
        if form.proxy_command.trim().is_empty() && form.proxy_command_keychain_id.is_none() {
            anyhow::bail!("ProxyCommand value is required");
        }
        Some(SavedProxyCommand {
            keychain_id: form.proxy_command_keychain_id.clone(),
            plaintext_command: (!form.proxy_command.trim().is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.proxy_command))),
        })
    } else {
        None
    };
    Ok((proxy_chain, upstream_proxy, proxy_command))
}

pub fn saved_standalone_sftp_secondary_endpoint_from_form(
    connection_store: &ConnectionStore,
    form: &mut StandaloneSftpSecondaryForm,
    existing: Option<&StandaloneSftpEndpoint>,
    missing_credentials_message: &str,
) -> anyhow::Result<StandaloneSftpEndpoint> {
    let host = form.host.trim().to_string();
    let username = form.username.trim().to_string();
    if host.is_empty() {
        anyhow::bail!("Second SFTP host is required");
    }
    if username.is_empty() {
        anyhow::bail!("Second SFTP username is required");
    }
    let port = form
        .port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| anyhow::anyhow!("Second SFTP port is invalid"))?;
    let can_preserve_auth = existing.is_some_and(|endpoint| {
        form.empty_password == endpoint.auth.uses_empty_password()
            && standalone_sftp_secondary_target_matches(form, endpoint)
            && standalone_sftp_secondary_auth_matches(form, &endpoint.auth)
            && form.password.is_empty()
            && form.passphrase.is_empty()
    });
    let auth = if can_preserve_auth {
        existing.expect("checked above").auth.clone()
    } else {
        let fallback = match form.auth_tab {
            SshAuthTab::Password => SavedAuth::Password {
                empty_password: form.empty_password,

                keychain_id: None,
                plaintext_password: (form.save_password && !form.password.is_empty())
                    .then(|| SecretString::from(std::mem::take(&mut form.password))),
            },
            SshAuthTab::Agent => SavedAuth::Agent,
            SshAuthTab::DefaultKey => SavedAuth::Key {
                key_path: String::new(),
                has_passphrase: !form.passphrase.is_empty(),
                passphrase_keychain_id: None,
                plaintext_passphrase: (!form.passphrase.is_empty())
                    .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
            },
            SshAuthTab::SshKey => SavedAuth::Key {
                key_path: form.key_path.trim().to_string(),
                has_passphrase: !form.passphrase.is_empty(),
                passphrase_keychain_id: None,
                plaintext_passphrase: (!form.passphrase.is_empty())
                    .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
            },
            SshAuthTab::ManagedKey => SavedAuth::ManagedKey {
                key_id: form.managed_key_id.trim().to_string(),
                passphrase_keychain_id: None,
                plaintext_passphrase: (!form.passphrase.is_empty())
                    .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
            },
            SshAuthTab::Certificate => SavedAuth::Certificate {
                key_path: form.key_path.trim().to_string(),
                cert_path: form.cert_path.trim().to_string(),
                has_passphrase: !form.passphrase.is_empty(),
                passphrase_keychain_id: None,
                plaintext_passphrase: (!form.passphrase.is_empty())
                    .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
            },
            SshAuthTab::TwoFactor => SavedAuth::KeyboardInteractive,
        };
        saved_authentication_plan(
            fallback,
            form.gssapi_enabled,
            &form.gssapi_server_identity,
            form.gssapi_delegate_credentials,
        )
    };
    let (proxy_chain, upstream_proxy, proxy_command) =
        saved_standalone_sftp_secondary_route_from_form(
            connection_store,
            form,
            existing,
            missing_credentials_message,
        )?;
    let mut endpoint = StandaloneSftpEndpoint::new(host, port, username, auth);
    endpoint.proxy_chain = proxy_chain;
    endpoint.upstream_proxy = upstream_proxy;
    endpoint.proxy_command = proxy_command;
    endpoint.connect_timeout_seconds = form.connect_timeout_seconds;
    endpoint.identity_agent = identity_agent_from_form(&form.identity_agent);
    endpoint.legacy_ssh_compatibility = form.legacy_ssh_compatibility;
    endpoint.ssh_algorithms = form.ssh_algorithms.clone();
    endpoint.initial_remote_path = (!form.initial_remote_path.trim().is_empty())
        .then(|| form.initial_remote_path.trim().to_string());
    endpoint.validate()?;
    Ok(endpoint)
}

pub fn saved_mosh_auth_from_form(form: &mut NewConnectionForm) -> SavedAuth {
    let fallback = match form.auth_tab {
        SshAuthTab::Password => {
            // Editing follows SSH property semantics: a newly entered password is
            // an explicit replacement and crosses directly into protected storage.
            let persist_password = mosh_password_draft_is_persistent(form);
            let plaintext_password = (persist_password && !form.password.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.password)));
            SavedAuth::Password {
                empty_password: form.empty_password,

                // An unchanged edit keeps the protected value by reference; a replacement
                // reuses that same owner while new profiles honor the save-password choice.
                keychain_id: persist_password
                    .then(|| form.saved_password_keychain_id.clone())
                    .flatten(),
                plaintext_password,
            }
        }
        SshAuthTab::Agent => SavedAuth::Agent,
        SshAuthTab::DefaultKey => SavedAuth::Key {
            key_path: String::new(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
        },
        SshAuthTab::SshKey => SavedAuth::Key {
            key_path: form.key_path.trim().to_string(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
        },
        SshAuthTab::ManagedKey => SavedAuth::ManagedKey {
            key_id: form.managed_key_id.trim().to_string(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
        },
        SshAuthTab::Certificate => SavedAuth::Certificate {
            key_path: form.key_path.trim().to_string(),
            cert_path: form.cert_path.trim().to_string(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.passphrase))),
        },
        SshAuthTab::TwoFactor => SavedAuth::KeyboardInteractive,
    };
    saved_authentication_plan(
        fallback,
        form.gssapi_enabled,
        &form.gssapi_server_identity,
        form.gssapi_delegate_credentials,
    )
}

pub fn runtime_mosh_auth_from_form(form: &mut NewConnectionForm) -> AuthMethod {
    let fallback = match form.auth_tab {
        SshAuthTab::Password => {
            if form.empty_password {
                form.password.zeroize();
                AuthMethod::password("")
            } else if form.password.is_empty() {
                AuthMethod::password_prompt()
            } else {
                AuthMethod::password_secret(take_zeroizing_secret(&mut form.password))
            }
        }
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::DefaultKey => AuthMethod::key_secret(
            "",
            (!form.passphrase.is_empty()).then(|| take_zeroizing_secret(&mut form.passphrase)),
        ),
        SshAuthTab::SshKey => AuthMethod::key_secret(
            form.key_path.trim().to_string(),
            (!form.passphrase.is_empty()).then(|| take_zeroizing_secret(&mut form.passphrase)),
        ),
        SshAuthTab::ManagedKey => AuthMethod::managed_key_secret(
            form.managed_key_id.trim().to_string(),
            (!form.passphrase.is_empty()).then(|| take_zeroizing_secret(&mut form.passphrase)),
        ),
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            form.key_path.trim().to_string(),
            form.cert_path.trim().to_string(),
            (!form.passphrase.is_empty()).then(|| take_zeroizing_secret(&mut form.passphrase)),
        ),
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    };
    runtime_authentication_plan(
        fallback,
        form.gssapi_enabled,
        &form.gssapi_server_identity,
        form.gssapi_delegate_credentials,
    )
}

fn runtime_mosh_auth_from_saved(
    auth: &SavedAuth,
    secret: Option<SecretString>,
) -> Option<AuthMethod> {
    if let SavedAuth::KerberosPreferred {
        server_identity,
        delegate_credentials,
        fallback,
    } = auth
    {
        return Some(AuthMethod::kerberos_preferred(
            runtime_mosh_auth_from_saved(fallback, secret)?,
            server_identity.clone(),
            *delegate_credentials,
        ));
    }
    let secret = secret.map(SecretString::into_zeroizing);
    Some(match auth {
        SavedAuth::Password { .. } => AuthMethod::password_secret(secret?),
        SavedAuth::Key { key_path, .. } => AuthMethod::key_secret(key_path.clone(), secret),
        SavedAuth::ManagedKey { key_id, .. } => {
            AuthMethod::managed_key_secret(key_id.clone(), secret)
        }
        SavedAuth::Certificate {
            key_path,
            cert_path,
            ..
        } => AuthMethod::certificate_secret(key_path.clone(), cert_path.clone(), secret),
        SavedAuth::KeyboardInteractive => AuthMethod::KeyboardInteractive,
        SavedAuth::Agent => AuthMethod::Agent,
        SavedAuth::KerberosPreferred { .. } => unreachable!("handled above"),
    })
}

pub fn runtime_mosh_config_from_saved(
    store: &ConnectionStore,
    profile: &oxideterm_connections::MoshProfile,
    mut secrets: SavedMoshProfileRuntimeSecrets,
    auth_override: Option<AuthMethod>,
) -> Option<SshConfig> {
    if secrets.proxy_chain.len() != profile.proxy_chain.len() {
        return None;
    }
    let auth = auth_override
        .or_else(|| runtime_mosh_auth_from_saved(&profile.auth, secrets.auth.take()))?;
    let proxy_chain = profile
        .proxy_chain
        .iter()
        .zip(secrets.proxy_chain)
        .map(|(hop, secret)| {
            Some(ProxyHopConfig {
                totp: store.totp_binding(hop.totp_credential_id.as_deref()),
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: runtime_mosh_auth_from_saved(&hop.auth, secret)?,
                agent_forwarding: hop.agent_forwarding,
                identity_agent: hop.identity_agent.clone(),
                agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                ssh_algorithms: hop.ssh_algorithms.clone(),
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SshConfig {
        host: profile.host.clone(),
        port: profile.ssh_port,
        username: profile.username.clone(),
        auth,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        identity_agent: profile.identity_agent.clone(),
        legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
        ssh_algorithms: profile.ssh_algorithms.clone(),
        strict_host_key_checking: true,
        pre_connect_command: profile.pre_connect_command.clone(),
        ..SshConfig::default()
    })
}

pub fn mosh_options_from_profile(
    profile: &oxideterm_connections::MoshProfile,
) -> MoshConnectionOptions {
    MoshConnectionOptions {
        saved_profile_id: Some(profile.id.clone()),
        server_executable: profile.server_executable.clone(),
        udp_host_override: profile.udp_host_override.clone(),
        udp_port: profile.udp_port,
        ip_family: profile.ip_family,
        prediction: profile.prediction,
        locale: profile.locale.clone(),
        terminal: profile.terminal.clone(),
        public_mcp_open_token: None,
        runtime_connection_attempt_id: None,
    }
}

pub fn saved_proxy_hop_auth_copies(
    connection_store: &ConnectionStore,
    proxy_hops: &[NewConnectionProxyHop],
    missing_credentials_message: &str,
) -> anyhow::Result<Vec<Option<SavedAuth>>> {
    proxy_hops
        .iter()
        .map(|hop| {
            if hop.saved_connection_id.is_empty() || hop.has_explicit_secret_draft() {
                return Ok(None);
            }
            let saved_connection = connection_store
                .get(&hop.saved_connection_id)
                .ok_or_else(|| anyhow::anyhow!(missing_credentials_message.to_string()))?;
            if !hop.matches_saved_connection(saved_connection) {
                return Err(anyhow::anyhow!(missing_credentials_message.to_string()));
            }
            connection_store
                .copy_saved_auth_for_new_owner(&saved_connection.auth)
                .map(Some)
        })
        .collect()
}

pub fn apply_saved_proxy_hop_auth_copies(
    request: &mut SaveConnectionRequest,
    runtime_proxy_hop_count: usize,
    auth_copies: Vec<Option<SavedAuth>>,
) {
    // Runtime ancestors already own their secrets; only form-added hops can reference a
    // saved connection whose credential must be copied into a new keychain owner.
    for (proxy_hop, auth_copy) in request
        .proxy_chain
        .iter_mut()
        .skip(runtime_proxy_hop_count)
        .zip(auth_copies)
    {
        if let Some(auth) = auth_copy {
            proxy_hop.auth = auth;
        }
    }
}

#[derive(Clone, Copy)]
pub struct EditedProxyHopSource {
    pub persisted_proxy_hop_index: Option<usize>,
    pub has_explicit_secret_draft: bool,
}

pub fn preserve_edited_proxy_hop_auth(
    proxy_chain: &mut [SavedProxyHop],
    persisted_proxy_chain: &[SavedProxyHop],
    edit_sources: Vec<EditedProxyHopSource>,
    saved_auth_copies: Vec<Option<SavedAuth>>,
    missing_credentials_message: &str,
) -> anyhow::Result<()> {
    if proxy_chain.len() != edit_sources.len() || proxy_chain.len() != saved_auth_copies.len() {
        anyhow::bail!(missing_credentials_message.to_string());
    }

    for ((request_hop, edit_source), saved_auth_copy) in proxy_chain
        .iter_mut()
        .zip(edit_sources)
        .zip(saved_auth_copies)
    {
        if let Some(saved_auth_copy) = saved_auth_copy {
            request_hop.auth = saved_auth_copy;
            continue;
        }
        if edit_source.has_explicit_secret_draft {
            continue;
        }
        let Some(persisted_proxy_hop_index) = edit_source.persisted_proxy_hop_index else {
            continue;
        };
        let persisted_hop = persisted_proxy_chain
            .get(persisted_proxy_hop_index)
            .ok_or_else(|| anyhow::anyhow!(missing_credentials_message.to_string()))?;
        if !proxy_hop_auth_target_matches(request_hop, persisted_hop) {
            anyhow::bail!(missing_credentials_message.to_string());
        }
        // The unchanged edit row keeps its existing keychain reference; no secret is loaded
        // into the form or copied into another owner during an in-place connection update.
        request_hop.auth = persisted_hop.auth.clone();
    }
    Ok(())
}

pub fn auth_for_duplicate_owner(
    connection_store: &ConnectionStore,
    auth: SavedAuth,
) -> anyhow::Result<SavedAuth> {
    match auth {
        SavedAuth::Password {
            keychain_id: _,
            plaintext_password: Some(password),
            ..
        } => Ok(SavedAuth::Password {
            empty_password: false,

            keychain_id: None,
            plaintext_password: Some(password),
        }),
        SavedAuth::Password {
            keychain_id: Some(_),
            plaintext_password: None,
            ..
        } => connection_store.copy_saved_auth_for_new_owner(&auth),
        SavedAuth::Key {
            key_path,
            has_passphrase,
            plaintext_passphrase: Some(passphrase),
            ..
        } => Ok(SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id: None,
            plaintext_passphrase: Some(passphrase),
        }),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase,
            plaintext_passphrase: Some(passphrase),
            ..
        } => Ok(SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase,
            passphrase_keychain_id: None,
            plaintext_passphrase: Some(passphrase),
        }),
        SavedAuth::ManagedKey {
            key_id,
            plaintext_passphrase: Some(passphrase),
            ..
        } => Ok(SavedAuth::ManagedKey {
            key_id,
            passphrase_keychain_id: None,
            plaintext_passphrase: Some(passphrase),
        }),
        SavedAuth::Key {
            passphrase_keychain_id: Some(_),
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id: Some(_),
            ..
        }
        | SavedAuth::ManagedKey {
            passphrase_keychain_id: Some(_),
            ..
        } => connection_store.copy_saved_auth_for_new_owner(&auth),
        auth => Ok(auth),
    }
}

pub fn upstream_proxy_for_duplicate_owner(
    connection_store: &ConnectionStore,
    policy: SavedUpstreamProxyPolicy,
) -> anyhow::Result<SavedUpstreamProxyPolicy> {
    let SavedUpstreamProxyPolicy::Custom { mut proxy } = policy else {
        return Ok(policy);
    };
    if let SavedUpstreamProxyAuth::Password {
        username,
        keychain_id,
        plaintext_password,
    } = proxy.auth
    {
        let password = match plaintext_password {
            Some(password) => Some(password),
            None if keychain_id.is_some() => {
                Some(connection_store.get_saved_upstream_proxy_password(
                    &SavedUpstreamProxyAuth::Password {
                        username: username.clone(),
                        keychain_id,
                        plaintext_password: None,
                    },
                )?)
            }
            None => None,
        };
        // The duplicate persists a fresh protected-store owner during upsert.
        proxy.auth = SavedUpstreamProxyAuth::Password {
            username,
            keychain_id: None,
            plaintext_password: password,
        };
    }
    Ok(SavedUpstreamProxyPolicy::Custom { proxy })
}

fn proxy_hop_auth_target_matches(left: &SavedProxyHop, right: &SavedProxyHop) -> bool {
    left.host == right.host
        && left.port == right.port
        && left.username == right.username
        && left.auth.gssapi_options() == right.auth.gssapi_options()
        && left.auth.uses_empty_password() == right.auth.uses_empty_password()
        && match (
            left.auth.conventional_fallback(),
            right.auth.conventional_fallback(),
        ) {
            (SavedAuth::Password { .. }, SavedAuth::Password { .. })
            | (SavedAuth::KeyboardInteractive, SavedAuth::KeyboardInteractive)
            | (SavedAuth::Agent, SavedAuth::Agent) => true,
            (
                SavedAuth::Key {
                    key_path: left_path,
                    ..
                },
                SavedAuth::Key {
                    key_path: right_path,
                    ..
                },
            ) => left_path == right_path,
            (
                SavedAuth::Certificate {
                    key_path: left_key_path,
                    cert_path: left_cert_path,
                    ..
                },
                SavedAuth::Certificate {
                    key_path: right_key_path,
                    cert_path: right_cert_path,
                    ..
                },
            ) => left_key_path == right_key_path && left_cert_path == right_cert_path,
            (
                SavedAuth::ManagedKey {
                    key_id: left_key_id,
                    ..
                },
                SavedAuth::ManagedKey {
                    key_id: right_key_id,
                    ..
                },
            ) => left_key_id == right_key_id,
            _ => false,
        }
}

pub fn saved_connection_for_open(store: &ConnectionStore, id: &str) -> Option<SavedConnection> {
    store.get(id).cloned()
}

pub fn saved_connection_requires_credentials(config: Option<&SshConfig>) -> bool {
    config.is_none_or(|config| matches!(config.auth, AuthMethod::Password { prompt: true, .. }))
}

#[cfg(test)]
mod saved_connection_open_tests {
    use super::*;

    #[test]
    fn missing_credentials_open_the_authentication_form_without_blocking_configured_methods() {
        assert!(saved_connection_requires_credentials(None));
        for (auth, requires_form) in [
            (AuthMethod::password_prompt(), true),
            (AuthMethod::password("fixture-password"), false),
            (AuthMethod::password(""), false),
            (AuthMethod::key("fixture-key", None), false),
            (AuthMethod::Agent, false),
            (AuthMethod::KeyboardInteractive, false),
            (
                AuthMethod::kerberos_preferred(AuthMethod::password_prompt(), None, false),
                false,
            ),
        ] {
            let config = SshConfig {
                auth,
                ..SshConfig::default()
            };
            assert_eq!(
                saved_connection_requires_credentials(Some(&config)),
                requires_form,
                "auth={:?}",
                config.auth
            );
        }
    }

    fn password_proxy_hop(auth: SavedAuth) -> SavedProxyHop {
        SavedProxyHop {
            totp_credential_id: None,
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        }
    }

    #[test]
    fn unchanged_mosh_password_edit_preserves_protected_store_reference() {
        let mut form = NewConnectionForm::default();
        form.auth_tab = SshAuthTab::Password;
        form.save_password = true;
        form.saved_password_keychain_id = Some("mosh-password-owner".to_string());

        let auth = saved_mosh_auth_from_form(&mut form);

        assert!(matches!(
                    auth,
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,

                    ..
        } if keychain_id == "mosh-password-owner"
                ));
        assert!(form.password.is_empty());
    }

    #[test]
    fn replacement_mosh_password_moves_secret_into_save_request() {
        let mut form = NewConnectionForm::default();
        form.auth_tab = SshAuthTab::Password;
        form.password = "replacement-secret".to_string();
        form.save_password = true;
        form.saved_password_keychain_id = Some("mosh-password-owner".to_string());

        let auth = saved_mosh_auth_from_form(&mut form);

        assert!(matches!(
                    auth,
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: Some(password),

                    ..
        } if keychain_id == "mosh-password-owner" && password == "replacement-secret"
                ));
        assert!(form.password.is_empty());
    }

    #[test]
    fn mosh_edit_persists_an_entered_password_without_new_profile_checkbox_state() {
        let mut form = NewConnectionForm::default();
        form.mosh_profile_id = Some("mosh-profile".to_string());
        form.auth_tab = SshAuthTab::Password;
        form.password = "replacement-secret".to_string();
        form.save_password = false;

        assert!(mosh_password_draft_is_persistent(&form));
        let auth = saved_mosh_auth_from_form(&mut form);

        assert!(matches!(
                    auth,
                    SavedAuth::Password {
                        keychain_id: None,
                        plaintext_password: Some(password),

                    ..
        } if password == "replacement-secret"
                ));
        assert!(form.password.is_empty());
    }

    #[test]
    fn saved_mosh_proxy_chain_becomes_bootstrap_route() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConnectionStore::load(directory.path().join("connections.json")).unwrap();
        let mut profile = oxideterm_connections::MoshProfile::new(
            "Mosh through jump",
            "target.example.com",
            22,
            "ops",
            SavedAuth::Agent,
        );
        profile.proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("jump-password-owner".to_string()),
            plaintext_password: None,
        })];

        let config = runtime_mosh_config_from_saved(
            &store,
            &profile,
            SavedMoshProfileRuntimeSecrets {
                auth: None,
                proxy_chain: vec![Some(SecretString::from("jump-password"))],
            },
            None,
        )
        .expect("Mosh bootstrap route");

        assert_eq!(config.proxy_chain.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            config.proxy_chain.as_ref().unwrap()[0].host,
            "jump.example.com"
        );
        assert!(!format!("{config:?}").contains("jump-password"));
    }

    #[test]
    fn stale_saved_connection_id_is_detected_before_opening() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-stale-saved-connection-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = ConnectionStore::load(path).expect("empty connection store");

        assert!(saved_connection_for_open(&store, "removed-connection").is_none());
    }

    #[test]
    fn unchanged_edited_proxy_hop_preserves_its_keychain_reference() {
        let persisted_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("persisted-proxy-password".to_string()),
            plaintext_password: None,
        })];
        let mut edited_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: None,
            plaintext_password: Some(SecretString::default()),
        })];

        preserve_edited_proxy_hop_auth(
            &mut edited_proxy_chain,
            &persisted_proxy_chain,
            vec![EditedProxyHopSource {
                persisted_proxy_hop_index: Some(0),
                has_explicit_secret_draft: false,
            }],
            vec![None],
            "missing proxy credentials",
        )
        .unwrap();

        assert!(matches!(
                    &edited_proxy_chain[0].auth,
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,

                    ..
        } if keychain_id == "persisted-proxy-password"
                ));
    }

    #[test]
    fn saved_proxy_hop_added_during_edit_uses_its_independent_auth_copy() {
        let mut edited_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: None,
            plaintext_password: Some(SecretString::default()),
        })];

        preserve_edited_proxy_hop_auth(
            &mut edited_proxy_chain,
            &[],
            vec![EditedProxyHopSource {
                persisted_proxy_hop_index: None,
                has_explicit_secret_draft: false,
            }],
            vec![Some(SavedAuth::Password {
                empty_password: false,

                keychain_id: None,
                plaintext_password: Some(SecretString::from("copied-proxy-secret")),
            })],
            "missing proxy credentials",
        )
        .unwrap();

        assert!(matches!(
                    &edited_proxy_chain[0].auth,
                    SavedAuth::Password {
                        keychain_id: None,
                        plaintext_password: Some(password),

                    ..
        } if password == "copied-proxy-secret"
                ));
    }

    #[test]
    fn edited_proxy_hop_never_reuses_credentials_for_another_host() {
        let persisted_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("persisted-proxy-password".to_string()),
            plaintext_password: None,
        })];
        let mut edited_hop = password_proxy_hop(SavedAuth::Password {
            empty_password: false,

            keychain_id: None,
            plaintext_password: Some(SecretString::default()),
        });
        edited_hop.host = "other.example.com".to_string();

        let error = preserve_edited_proxy_hop_auth(
            std::slice::from_mut(&mut edited_hop),
            &persisted_proxy_chain,
            vec![EditedProxyHopSource {
                persisted_proxy_hop_index: Some(0),
                has_explicit_secret_draft: false,
            }],
            vec![None],
            "missing proxy credentials",
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "missing proxy credentials");
    }
}
