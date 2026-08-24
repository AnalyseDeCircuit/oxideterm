// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::{
    WorkspaceNotificationKind, WorkspaceNotificationScope, WorkspaceNotificationSeverity,
};
use gpui::App;
use oxideterm_connections::{
    ConnectionStore, SavedAuth, SavedConnection, SavedProxyHop, StandaloneSftpEndpoint,
    StandaloneSftpTransferMode,
};
use oxideterm_gpui_terminal::TerminalNoticeVariant;

struct PreparedMoshConnect {
    config: SshConfig,
    title: String,
    options: MoshConnectionOptions,
}

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

fn parse_mosh_udp_port(value: &str) -> anyhow::Result<SavedMoshUdpPortSelection> {
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

fn mosh_password_draft_is_persistent(form: &NewConnectionForm) -> bool {
    form.save_password || (form.mosh_profile_id.is_some() && !form.password.is_empty())
}

fn saved_profile_notes(notes: &str) -> Option<String> {
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

fn saved_standalone_sftp_proxy_hop_from_form(
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
            keychain_id: None,
            plaintext_password: Some(SecretString::from(std::mem::take(&mut hop.password))),
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

fn saved_standalone_sftp_secondary_endpoint_from_form(
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
        standalone_sftp_secondary_target_matches(form, endpoint)
            && standalone_sftp_secondary_auth_matches(form, &endpoint.auth)
            && form.password.is_empty()
            && form.passphrase.is_empty()
    });
    let auth = if can_preserve_auth {
        existing.expect("checked above").auth.clone()
    } else {
        let fallback = match form.auth_tab {
            SshAuthTab::Password => SavedAuth::Password {
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

fn saved_mosh_auth_from_form(form: &mut NewConnectionForm) -> SavedAuth {
    let fallback = match form.auth_tab {
        SshAuthTab::Password => {
            // Editing follows SSH property semantics: a newly entered password is
            // an explicit replacement and crosses directly into protected storage.
            let persist_password = mosh_password_draft_is_persistent(form);
            let plaintext_password = (persist_password && !form.password.is_empty())
                .then(|| SecretString::from(std::mem::take(&mut form.password)));
            SavedAuth::Password {
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

fn runtime_mosh_auth_from_form(form: &mut NewConnectionForm) -> AuthMethod {
    let fallback = match form.auth_tab {
        SshAuthTab::Password => {
            AuthMethod::password_secret(take_zeroizing_secret(&mut form.password))
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

fn runtime_mosh_config_from_saved(
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
        ..SshConfig::default()
    })
}

pub(in crate::workspace) fn mosh_options_from_profile(
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
    }
}

fn saved_proxy_hop_auth_copies(
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

fn apply_saved_proxy_hop_auth_copies(
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
struct EditedProxyHopSource {
    persisted_proxy_hop_index: Option<usize>,
    has_explicit_secret_draft: bool,
}

fn preserve_edited_proxy_hop_auth(
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

fn auth_for_duplicate_owner(
    connection_store: &ConnectionStore,
    auth: SavedAuth,
) -> anyhow::Result<SavedAuth> {
    match auth {
        SavedAuth::Password {
            keychain_id: _,
            plaintext_password: Some(password),
        } => Ok(SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
        }),
        SavedAuth::Password {
            keychain_id: Some(_),
            plaintext_password: None,
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

fn upstream_proxy_for_duplicate_owner(
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

impl WorkspaceApp {
    pub(super) fn report_saved_next_hop_error(&mut self, i18n_key: &str, cx: &mut Context<Self>) {
        self.report_saved_next_hop_message(self.i18n.t(i18n_key), cx);
    }

    pub(super) fn report_saved_next_hop_message(
        &mut self,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(false), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_save_runtime_node_form(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.ssh_nodes.get(&node_id).map(|node| node.title.clone()) else {
            let message = self.i18n.t("ssh.form.runtime_node_missing");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
            return;
        };
        let Some(runtime_snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            let message = self.i18n.t("ssh.form.runtime_node_missing");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
            return;
        };
        let parent_id = runtime_snapshot.parent_id.clone();
        let proxy_hops = match parent_id
            .as_ref()
            .map(|parent_id| self.runtime_proxy_hops_for_parent_path(parent_id))
            .transpose()
        {
            Ok(hops) => hops.unwrap_or_default(),
            Err(error) => {
                let message = error.to_string();
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(message), cx);
                });
                return;
            }
        };

        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_runtime_config(
            runtime_snapshot.config,
            Some(&title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.proxy_hops = proxy_hops;
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.save_connection = true;
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn runtime_proxy_hops_for_parent_path(
        &self,
        parent_id: &NodeId,
    ) -> anyhow::Result<Vec<NewConnectionProxyHop>> {
        let mut configs = Vec::new();
        let mut cursor = Some(parent_id.clone());
        while let Some(node_id) = cursor {
            let snapshot = self
                .node_router
                .node_runtime_snapshot(&node_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}: {}",
                        self.i18n.t("ssh.form.runtime_node_missing"),
                        node_id.0
                    )
                })?;
            configs.push(snapshot.config);
            cursor = snapshot.parent_id;
        }
        configs.reverse();

        Ok(configs
            .into_iter()
            .flat_map(|config| {
                let embedded_hops = config.proxy_chain.unwrap_or_default().into_iter();
                embedded_hops
                    .chain(std::iter::once(ProxyHopConfig {
                        host: config.host,
                        port: config.port,
                        username: config.username,
                        auth: config.auth,
                        agent_forwarding: config.agent_forwarding,
                        identity_agent: config.identity_agent,
                        agent_forwarding_socket: config.agent_forwarding_socket,
                        legacy_ssh_compatibility: config.legacy_ssh_compatibility,
                        ssh_algorithms: config.ssh_algorithms,
                        strict_host_key_checking: true,
                        trust_host_key: None,
                        expected_host_key_fingerprint: None,
                    }))
                    .map(proxy_hop_form_from_runtime_config)
            })
            .collect())
    }

    pub(in crate::workspace) fn close_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if !self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_connection_form_exit(delay, cx)
        }) {
            return;
        }
        // Closing the form drops any unconsumed second-endpoint credentials immediately.
        self.pending_standalone_sftp_pair_launches.clear();
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn submit_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.submit_new_connection_form_with_action(
            NewConnectionSubmitAction::SaveAndConnect,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn submit_new_connection_form_with_action(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (transport, local_shell_id, drill_down_parent_id, mode) = {
            let state = self.connection_form_state(cx);
            (
                state.form.as_ref().map(|form| form.transport),
                state
                    .form
                    .as_ref()
                    .and_then(|form| form.local_shell_id.clone()),
                state.drill_down_parent_node_id.clone(),
                state.mode(),
            )
        };
        if matches!(
            transport,
            Some(
                NewConnectionTransport::Ssh
                    | NewConnectionTransport::Mosh
                    | NewConnectionTransport::StandaloneSftp
            )
        ) && self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| !connection_timeout_drafts_valid(form))
        {
            let message = self.i18n.t("ssh.form.connect_timeout_invalid");
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(message);
                }
            });
            cx.notify();
            return;
        }
        if transport == Some(NewConnectionTransport::LocalTerminal)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            // The modal is only another launch surface; the terminal tab owns the local PTY.
            let selected_shell = self.resolved_local_shell(local_shell_id.as_deref());
            self.close_new_connection_form(window, cx);
            if let Some(shell) = selected_shell {
                let mut terminal_config = self.local_terminal_config();
                terminal_config.shell = Some(shell.clone());
                self.edit_settings(
                    |settings| {
                        let recent = &mut settings.local_terminal.recent_shell_ids;
                        recent.retain(|id| id != &shell.id);
                        recent.insert(0, shell.id.clone());
                        recent.truncate(5);
                    },
                    cx,
                );
                let _ = self.create_local_terminal_tab_with_config(
                    terminal_config,
                    shell.label,
                    window,
                    cx,
                );
            } else {
                let _ = self.create_local_terminal_tab(window, cx);
            }
            return;
        }
        if transport == Some(NewConnectionTransport::Serial)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_serial_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::Telnet)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_telnet_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::Mosh)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_mosh_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::StandaloneSftp)
            && drill_down_parent_id.is_none()
        {
            self.submit_standalone_sftp_connection_form(action, window, cx);
            return;
        }
        if transport
            .and_then(remote_desktop_protocol_for_transport)
            .is_some()
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_remote_desktop_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::WslGraphics)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.close_new_connection_form(window, cx);
            self.open_graphics_tab(window, cx);
            return;
        }
        if let Some(parent_id) = drill_down_parent_id {
            match action {
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(Some(&parent_id), window, cx);
                    return;
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    let Some(handoff) = self.save_current_connection_form(Some(&parent_id), cx)
                    else {
                        return;
                    };
                    self.start_saved_form_connection_flow(handoff, Some(parent_id), window, cx);
                    return;
                }
                NewConnectionSubmitAction::Connect => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.save_connection = false;
                        }
                    });
                }
            }
            let terminal_options = self
                .connection_form_state(cx)
                .form
                .as_ref()
                .map(SshTerminalConnectionOptions::from_form)
                .unwrap_or_default();
            self.start_new_connection_flow(
                SshConnectionIntent::DrillDown {
                    parent_id,
                    saved_connection_id: None,
                    terminal_options,
                },
                window,
                cx,
            );
            return;
        }
        match mode {
            NewConnectionFormMode::SavedConnectionPrompt => {
                self.submit_saved_connection_prompt(window, cx);
            }
            NewConnectionFormMode::EditProperties => {
                self.save_editing_connection(window, cx);
            }
            NewConnectionFormMode::DuplicateTemplate => {
                self.save_duplicate_connection_template(window, cx);
            }
            NewConnectionFormMode::NewConnection => match action {
                NewConnectionSubmitAction::Connect => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.save_connection = false;
                        }
                    });
                    let terminal_options = self
                        .connection_form_state(cx)
                        .form
                        .as_ref()
                        .map(SshTerminalConnectionOptions::from_form)
                        .unwrap_or_default();
                    self.start_new_connection_flow(
                        SshConnectionIntent::Connect(terminal_options),
                        window,
                        cx,
                    );
                }
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(None, window, cx);
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    let Some(handoff) = self.save_current_connection_form(None, cx) else {
                        return;
                    };
                    self.start_saved_form_connection_flow(handoff, None, window, cx);
                }
            },
        }
    }

    pub(super) fn save_new_connection_without_connecting(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .save_current_connection_form(drill_down_parent_id, cx)
            .is_some()
        {
            self.close_new_connection_form(window, cx);
        }
    }

    pub(super) fn save_current_connection_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> Option<SavedConnectionRuntimeHandoff> {
        self.ensure_new_connection_save_name_is_unique(drill_down_parent_id, cx);
        let request = match self.save_request_for_current_form(drill_down_parent_id, cx) {
            Some(Ok(request)) => request,
            Some(Err(error)) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
                cx.notify();
                return None;
            }
            None => return None,
        };
        let auth_override = self.with_connection_form_mut(cx, |_this, form, _cx| {
            let form = form?;
            (form.auth_tab == SshAuthTab::Password && !form.save_password)
                .then(|| AuthMethod::password_secret(take_zeroizing_secret(&mut form.password)))
        });

        // The Save and Save & Connect buttons mean "persist this draft now",
        // so duplicate-name and keychain failures should block connection start.
        match self.connection_store.upsert_with_runtime_secrets(request) {
            Ok((connection, secrets)) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                Some(SavedConnectionRuntimeHandoff {
                    connection_id: connection.id,
                    secrets,
                    auth_override,
                })
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.save_failed")
                        ));
                    }
                });
                cx.notify();
                None
            }
        }
    }

    fn start_saved_form_connection_flow(
        &mut self,
        handoff: SavedConnectionRuntimeHandoff,
        drill_down_parent_id: Option<NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.connection_store.get(&handoff.connection_id).cloned() else {
            self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
            return;
        };
        let Some(mut config) = ssh_config_from_saved_connection_with_runtime_secrets(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
            handoff.secrets,
            handoff.auth_override,
        ) else {
            self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
            return;
        };
        let intent = if let Some(parent_id) = drill_down_parent_id {
            let prefix_count = match self.runtime_proxy_hops_for_parent_path(&parent_id) {
                Ok(hops) => hops.len(),
                Err(error) => {
                    self.report_saved_next_hop_message(error.to_string(), cx);
                    return;
                }
            };
            if prefix_count > 0 {
                let Some(proxy_chain) = config.proxy_chain.as_mut() else {
                    self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
                    return;
                };
                if proxy_chain.len() < prefix_count {
                    self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
                    return;
                }
                // Existing parent nodes already own the persisted prefix path.
                proxy_chain.drain(..prefix_count);
                if proxy_chain.is_empty() {
                    config.proxy_chain = None;
                }
            }
            SshConnectionIntent::DrillDown {
                parent_id,
                saved_connection_id: Some(connection.id.clone()),
                terminal_options: SshTerminalConnectionOptions {
                    terminal: connection.options.terminal,
                    dedicated_new_terminal_connection: connection
                        .options
                        .dedicated_new_terminal_connection,
                },
            }
        } else {
            SshConnectionIntent::ConnectSaved(connection.id.clone())
        };
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.save_connection = false;
            }
        });
        self.start_new_connection_config_flow(config, connection.name, intent, window, cx);
    }

    pub(super) fn ensure_new_connection_save_name_is_unique(
        &mut self,
        _drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) {
        let occupied_names: Vec<String> = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect();
        self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return;
            };
            let fallback_name = if form.name.trim().is_empty() {
                let host = form.host.trim();
                let username = form.username.trim();
                if host.is_empty() || username.is_empty() {
                    return;
                }
                format!("{username}@{host}")
            } else {
                form.name.trim().to_string()
            };
            let name_exists = occupied_names
                .iter()
                .any(|name| name.trim().eq_ignore_ascii_case(&fallback_name));
            let next_name = if name_exists {
                // New/save-as flows create a fresh connection id, so avoid storing a
                // second indistinguishable row when the draft name already exists.
                duplicate_connection_template_name(
                    &fallback_name,
                    occupied_names.iter().map(String::as_str),
                )
            } else {
                fallback_name
            };
            form.name = next_name;
        });
    }

    pub(super) fn save_request_for_current_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> Option<anyhow::Result<SaveConnectionRequest>> {
        let mut runtime_proxy_hops = match drill_down_parent_id {
            Some(parent_id) => match self.runtime_proxy_hops_for_parent_path(parent_id) {
                Ok(hops) => hops,
                Err(error) => return Some(Err(error)),
            },
            None => Vec::new(),
        };
        self.with_connection_form_mut(cx, |this, form, _cx| {
            let form = form?;
            Some((|| {
                let missing_credentials_message =
                    this.i18n.t("sessions.saved_next_hop.missing_credentials");
                let auth_copies = saved_proxy_hop_auth_copies(
                    &this.connection_store,
                    &form.proxy_hops,
                    &missing_credentials_message,
                )?;
                let runtime_proxy_hop_count = runtime_proxy_hops.len();
                let mut request = save_request_from_form_with_proxy_hop_prefix(
                    form,
                    &mut runtime_proxy_hops,
                    None,
                )?;
                apply_saved_proxy_hop_auth_copies(
                    &mut request,
                    runtime_proxy_hop_count,
                    auth_copies,
                );
                Ok(request)
            })())
        })
    }

    pub(super) fn submit_serial_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((config, terminal_options, mut save_request)) =
            self.with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let port_path = form.serial_port_path.trim().to_string();
                let baud_rate = form.serial_baud_rate.trim().parse::<u32>().ok();
                if port_path.is_empty() {
                    form.error = Some(this.i18n.t("modals.new_connection.serial_port_required"));
                    cx.notify();
                    return None;
                }
                let Some(baud_rate) = baud_rate.filter(|baud| *baud > 0) else {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.serial_invalid_baud_rate"),
                    );
                    cx.notify();
                    return None;
                };
                let config = SerialSessionConfig {
                    port_path: port_path.clone(),
                    baud_rate,
                    data_bits: form.serial_data_bits,
                    stop_bits: form.serial_stop_bits,
                    parity: form.serial_parity,
                    flow_control: form.serial_flow_control,
                };
                let editing_profile_id = form.serial_profile_id.clone();
                let existing_connect_on_open = editing_profile_id.as_deref().and_then(|id| {
                    this.connection_store
                        .serial_profiles()
                        .iter()
                        .find(|profile| profile.id == id)
                        .map(|profile| profile.connect_on_open)
                });
                // Editing always updates the persisted asset and preserves hidden sync metadata.
                let should_save_profile =
                    editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
                let save_request = should_save_profile.then(|| SaveSerialProfileRequest {
                    id: editing_profile_id,
                    name: serial_profile_name_or_port(&form.serial_profile_name, &port_path),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    notes: saved_profile_notes(&form.notes),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    port_path,
                    baud_rate: Some(baud_rate),
                    data_bits: Some(form.serial_data_bits),
                    stop_bits: Some(form.serial_stop_bits),
                    parity: Some(serial_profile_parity_from_terminal(form.serial_parity)),
                    flow_control: Some(serial_profile_flow_from_terminal(form.serial_flow_control)),
                    terminal: form.terminal.clone(),
                    connect_on_open: existing_connect_on_open,
                });
                form.pending = true;
                form.error = None;
                Some((config, form.terminal.clone(), save_request))
            })
        else {
            return;
        };

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("serial save action must build a serial profile request");
            match self.connection_store.upsert_serial_profile(request) {
                Ok(_) => {
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            ));
                        }
                    });
                }
            }
            cx.notify();
            return;
        }

        if action == NewConnectionSubmitAction::SaveAndConnect {
            let request = save_request
                .take()
                .expect("serial save-and-open action must build a serial profile request");
            match self.connection_store.upsert_serial_profile(request) {
                Ok(_) => self.queue_cloud_sync_dirty_refresh(cx),
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        match self.create_serial_terminal_tab(config, terminal_options, window, cx) {
            Ok(session_id) => {
                if let Some(request) = save_request {
                    match self.connection_store.upsert_serial_profile(request) {
                        Ok(profile) => {
                            self.register_terminal_saved_connection(
                                session_id,
                                oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                                profile.id,
                                cx,
                            );
                            self.queue_cloud_sync_dirty_refresh(cx);
                        }
                        Err(error) => {
                            let message = format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            );
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                    }
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn submit_telnet_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((config, terminal_options, mut save_request)) =
            self.with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let host = form.host.trim().to_string();
                let port = form.port.trim().parse::<u16>().ok();
                if host.is_empty() {
                    form.error = Some(this.i18n.t("modals.new_connection.telnet_host_required"));
                    cx.notify();
                    return None;
                }
                let Some(port) = port else {
                    form.error = Some(this.i18n.t("modals.new_connection.telnet_invalid_port"));
                    cx.notify();
                    return None;
                };
                let editing_profile_id = form.telnet_profile_id.clone();
                let existing_connect_on_open = editing_profile_id.as_deref().and_then(|id| {
                    this.connection_store
                        .telnet_profiles()
                        .iter()
                        .find(|profile| profile.id == id)
                        .map(|profile| profile.connect_on_open)
                });
                // Editing always updates the persisted asset and preserves hidden sync metadata.
                let should_save_profile =
                    editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
                let save_request = should_save_profile.then(|| SaveTelnetProfileRequest {
                    id: editing_profile_id,
                    name: telnet_profile_name_or_endpoint(&form.telnet_profile_name, &host, port),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    notes: saved_profile_notes(&form.notes),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    host: host.clone(),
                    port,
                    terminal: form.terminal.clone(),
                    connect_on_open: existing_connect_on_open,
                });
                let config = TelnetSessionConfig { host, port };
                let terminal_options = form.terminal.clone();
                form.pending = true;
                form.error = None;
                Some((config, terminal_options, save_request))
            })
        else {
            return;
        };

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("telnet save action must build a telnet profile request");
            match self.connection_store.upsert_telnet_profile(request) {
                Ok(_) => {
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            ));
                        }
                    });
                }
            }
            cx.notify();
            return;
        }

        let mut connected_profile_id = None;
        if action == NewConnectionSubmitAction::SaveAndConnect {
            let request = save_request
                .take()
                .expect("telnet save-and-open action must build a telnet profile request");
            match self.connection_store.upsert_telnet_profile(request) {
                Ok(profile) => {
                    connected_profile_id = Some(profile.id);
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        // Telnet is opened as a native local terminal transport. It does not
        // create an SSH node, so SSH-only saved-connection/test flows stay out.
        match self.create_telnet_terminal_tab(config, terminal_options, window, cx) {
            Ok(session_id) => {
                if let Some(request) = save_request {
                    match self.connection_store.upsert_telnet_profile(request) {
                        Ok(profile) => {
                            connected_profile_id = Some(profile.id);
                            self.queue_cloud_sync_dirty_refresh(cx);
                        }
                        Err(error) => {
                            let message = format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            );
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                    }
                }
                if let Some(profile_id) = connected_profile_id {
                    self.telnet_terminal_profile_ids
                        .insert(session_id, profile_id.clone());
                    self.register_terminal_saved_connection(
                        session_id,
                        oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                        profile_id,
                        cx,
                    );
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn submit_mosh_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prepared = self.with_connection_form_mut(cx, |this, form, cx| {
            let form = form?;
            let host = form.host.trim().to_string();
            let username = form.username.trim().to_string();
            let ssh_port = form
                .port
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0);
            if host.is_empty() || username.is_empty() || ssh_port.is_none() {
                form.error = Some(this.i18n.t("ssh.form.validation_required"));
                cx.notify();
                return None;
            }
            match form.auth_tab {
                SshAuthTab::SshKey if form.key_path.trim().is_empty() => {
                    form.error = Some(this.i18n.t("ssh.form.key_path_required"));
                    cx.notify();
                    return None;
                }
                SshAuthTab::ManagedKey if form.managed_key_id.trim().is_empty() => {
                    form.error = Some(this.i18n.t("ssh.form.managed_key_required"));
                    cx.notify();
                    return None;
                }
                SshAuthTab::Certificate
                    if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() =>
                {
                    form.error = Some(this.i18n.t("ssh.form.certificate_paths_required"));
                    cx.notify();
                    return None;
                }
                _ => {}
            }
            if let Err(error) = validate_proxy_chain_form(form) {
                form.error = Some(error);
                cx.notify();
                return None;
            }
            let udp_port = match parse_mosh_udp_port(&form.mosh_udp_port) {
                Ok(udp_port) => udp_port,
                Err(_) => {
                    form.error = Some(this.i18n.t("mosh.form.invalid_udp_port"));
                    cx.notify();
                    return None;
                }
            };
            let server_executable = form.mosh_server_executable.trim().to_string();
            if server_executable.is_empty() {
                form.error = Some(this.i18n.t("mosh.form.server_executable_required"));
                cx.notify();
                return None;
            }
            let title = if form.name.trim().is_empty() {
                format!("{username}@{host}")
            } else {
                form.name.trim().to_string()
            };
            let options = MoshConnectionOptions {
                saved_profile_id: None,
                server_executable: server_executable.clone(),
                udp_host_override: (!form.mosh_udp_host.trim().is_empty())
                    .then(|| form.mosh_udp_host.trim().to_string()),
                udp_port,
                ip_family: form.mosh_ip_family,
                prediction: form.mosh_prediction,
                locale: (!form.mosh_locale.trim().is_empty())
                    .then(|| form.mosh_locale.trim().to_string()),
                terminal: form.terminal.clone(),
                public_mcp_open_token: None,
            };
            let ssh_port = ssh_port.expect("validated Mosh SSH port must exist");

            if action == NewConnectionSubmitAction::Connect {
                let saved_proxy_hop_auth = match saved_proxy_hop_auth_from_store(
                    &this.connection_store,
                    form,
                    &this.i18n.t("sessions.saved_next_hop.missing_credentials"),
                ) {
                    Ok(auth) => auth,
                    Err(error) => {
                        form.error = Some(error);
                        cx.notify();
                        return None;
                    }
                };
                let proxy_chain =
                    proxy_chain_from_form(form, RuntimeSecretHandoff::Move, saved_proxy_hop_auth);
                let auth = runtime_mosh_auth_from_form(form);
                let config = SshConfig {
                    host,
                    port: ssh_port,
                    username,
                    auth,
                    proxy_chain,
                    identity_agent: identity_agent_from_form(&form.identity_agent),
                    legacy_ssh_compatibility: form.legacy_ssh_compatibility,
                    ssh_algorithms: form.ssh_algorithms.clone(),
                    strict_host_key_checking: true,
                    ..SshConfig::default()
                };
                form.pending = true;
                form.error = Some(this.i18n.t("ssh.form.checking_host_key"));
                return Some((
                    Some(PreparedMoshConnect {
                        config,
                        title,
                        options,
                    }),
                    None,
                    None,
                ));
            }

            let existing_profile = form
                .mosh_profile_id
                .as_deref()
                .and_then(|id| this.connection_store.get_mosh_profile(id))
                .cloned();
            let proxy_chain = match (|| -> anyhow::Result<Vec<SavedProxyHop>> {
                let missing_credentials_message =
                    this.i18n.t("sessions.saved_next_hop.missing_credentials");
                let saved_auth_copies = saved_proxy_hop_auth_copies(
                    &this.connection_store,
                    &form.proxy_hops,
                    &missing_credentials_message,
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
                    existing_profile
                        .as_ref()
                        .map_or(&[], |profile| profile.proxy_chain.as_slice()),
                    edit_sources,
                    saved_auth_copies,
                    &missing_credentials_message,
                )?;
                Ok(proxy_chain)
            })() {
                Ok(proxy_chain) => proxy_chain,
                Err(error) => {
                    form.error = Some(error.to_string());
                    cx.notify();
                    return None;
                }
            };
            let auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
                && form.auth_tab == SshAuthTab::Password
                && !mosh_password_draft_is_persistent(form))
            .then(|| runtime_mosh_auth_from_form(form));
            let auth = saved_mosh_auth_from_form(form);
            let request = SaveMoshProfileRequest {
                id: form.mosh_profile_id.clone(),
                name: title,
                group: serial_profile_group_from_form(&form.group, &this.i18n),
                notes: saved_profile_notes(&form.notes),
                icon: asset_icon_from_form(&form.icon),
                color: asset_color_from_form(&form.color),
                icon_background_color: asset_color_from_form(&form.icon_background_color),
                host,
                ssh_port,
                username,
                auth,
                proxy_chain,
                server_executable,
                udp_host_override: options.udp_host_override,
                udp_port,
                ip_family: options.ip_family,
                prediction: options.prediction,
                locale: options.locale,
                identity_agent: identity_agent_from_form(&form.identity_agent),
                legacy_ssh_compatibility: form.legacy_ssh_compatibility,
                ssh_algorithms: form.ssh_algorithms.clone(),
                terminal: form.terminal.clone(),
            };
            form.pending = true;
            form.error = None;
            Some((None, Some(request), auth_override))
        });
        let Some((direct_connect, save_request, auth_override)) = prepared else {
            return;
        };

        if let Some(connect) = direct_connect {
            self.start_ssh_preflight(
                connect.config,
                connect.title,
                SshConnectionIntent::Mosh(connect.options),
                cx,
            );
            cx.notify();
            return;
        }

        let request = save_request.expect("Mosh save action must build a profile request");
        let (profile, runtime_secrets) = match self
            .connection_store
            .upsert_mosh_profile_with_runtime_secrets(request)
        {
            Ok(saved) => saved,
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error =
                            Some(format!("{}: {error}", self.i18n.t("mosh.form.save_failed")));
                    }
                });
                cx.notify();
                return;
            }
        };
        self.queue_cloud_sync_dirty_refresh(cx);
        if action == NewConnectionSubmitAction::Save {
            self.update_connection_form_state(cx, ConnectionFormState::clear);
            cx.notify();
            return;
        }

        let Some(config) = runtime_mosh_config_from_saved(&profile, runtime_secrets, auth_override)
        else {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.pending = false;
                    form.error = Some(self.i18n.t("mosh.form.missing_credentials"));
                }
            });
            cx.notify();
            return;
        };
        let title = profile.name.clone();
        let options = mosh_options_from_profile(&profile);
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
            }
        });
        self.start_ssh_preflight(config, title, SshConnectionIntent::Mosh(options), cx);
        cx.notify();
    }

    pub(super) fn submit_standalone_sftp_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == NewConnectionSubmitAction::Connect {
            let initial_remote_path =
                self.connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| {
                        let path = form.sftp_initial_remote_path.trim();
                        (!path.is_empty()).then(|| path.to_string())
                    });
            self.start_new_connection_flow(
                SshConnectionIntent::StandaloneSftp {
                    saved_profile_id: None,
                    initial_remote_path,
                    pair_launch_token: None,
                },
                window,
                cx,
            );
            return;
        }

        let prepared = self.with_connection_form_mut(cx, |this, form, cx| {
            let form = form?;
            if form.name.trim().is_empty()
                && !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
            {
                form.name = format!("{}@{}", form.username.trim(), form.host.trim());
            }
            let existing_profile = form
                .standalone_sftp_profile_id
                .as_deref()
                .and_then(|id| this.connection_store.get_standalone_sftp_profile(id))
                .cloned();
            let existing_auth = existing_profile
                .as_ref()
                .map(|profile| profile.auth.clone());
            let base_request = if existing_auth.is_some() {
                save_request_from_form_with_existing_auth(
                    form,
                    form.standalone_sftp_profile_id.clone(),
                    existing_auth.as_ref(),
                )
            } else {
                save_request_from_form_with_proxy_hop_prefix(
                    form,
                    &mut [],
                    form.standalone_sftp_profile_id.clone(),
                )
            };
            let base_request = match base_request {
                Ok(request) => request,
                Err(error) => {
                    form.error = Some(error.to_string());
                    cx.notify();
                    return None;
                }
            };
            let auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
                && form.auth_tab == SshAuthTab::Password
                && !form.save_password)
                .then(|| AuthMethod::password_secret(take_zeroizing_secret(&mut form.password)));
            let secondary_endpoint =
                if form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote {
                    match saved_standalone_sftp_secondary_endpoint_from_form(
                        &this.connection_store,
                        &mut form.standalone_sftp_secondary,
                        existing_profile
                            .as_ref()
                            .and_then(|profile| profile.secondary_endpoint.as_ref()),
                        &this.i18n.t("sessions.saved_next_hop.missing_credentials"),
                    ) {
                        Ok(endpoint) => Some(endpoint),
                        Err(error) => {
                            form.error = Some(error.to_string());
                            cx.notify();
                            return None;
                        }
                    }
                } else {
                    None
                };
            let secondary_auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
                && form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote
                && form.standalone_sftp_secondary.auth_tab == SshAuthTab::Password
                && !form.standalone_sftp_secondary.save_password)
                .then(|| {
                    AuthMethod::password_secret(take_zeroizing_secret(
                        &mut form.standalone_sftp_secondary.password,
                    ))
                });
            let request = SaveStandaloneSftpProfileRequest {
                id: base_request.id,
                name: base_request.name,
                group: base_request.group,
                notes: base_request.notes,
                icon: base_request.icon,
                color: base_request.color,
                icon_background_color: base_request.icon_background_color,
                host: base_request.host,
                port: base_request.port,
                username: base_request.username,
                auth: base_request.auth,
                connect_timeout_seconds: base_request.connect_timeout_seconds,
                proxy_chain: base_request.proxy_chain,
                upstream_proxy: base_request.upstream_proxy,
                proxy_command: base_request.proxy_command,
                identity_agent: base_request.identity_agent,
                legacy_ssh_compatibility: base_request.legacy_ssh_compatibility,
                ssh_algorithms: base_request.ssh_algorithms,
                initial_remote_path: (!form.sftp_initial_remote_path.trim().is_empty())
                    .then(|| form.sftp_initial_remote_path.trim().to_string()),
                transfer_mode: form.standalone_sftp_transfer_mode,
                secondary_endpoint,
            };
            form.pending = true;
            form.error = None;
            Some((request, auth_override, secondary_auth_override))
        });
        let Some((request, auth_override, secondary_auth_override)) = prepared else {
            return;
        };

        let (profile, runtime_secrets) = match self
            .connection_store
            .upsert_standalone_sftp_profile_with_runtime_secrets(request)
        {
            Ok(saved) => saved,
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("sftp.standalone.save_failed")
                        ));
                    }
                });
                cx.notify();
                return;
            }
        };
        self.queue_cloud_sync_dirty_refresh(cx);
        if action == NewConnectionSubmitAction::Save {
            self.update_connection_form_state(cx, ConnectionFormState::clear);
            self.close_new_connection_select(cx);
            cx.notify();
            return;
        }

        let mut runtime_secrets = runtime_secrets;
        let secondary_runtime_secrets = runtime_secrets.secondary_endpoint.take();
        let Some(config) = ssh_config_from_standalone_sftp_profile_with_runtime_secrets(
            &self.connection_store,
            self.settings_store.settings(),
            &profile,
            runtime_secrets,
            auth_override,
        ) else {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.pending = false;
                    form.error = Some(self.i18n.t("sftp.standalone.missing_credentials"));
                }
            });
            cx.notify();
            return;
        };
        let secondary_config = match (
            profile.secondary_endpoint.as_ref(),
            secondary_runtime_secrets,
        ) {
            (Some(endpoint), Some(secrets)) => {
                ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets(
                    &self.connection_store,
                    self.settings_store.settings(),
                    &profile.name,
                    endpoint,
                    secrets,
                    secondary_auth_override,
                )
            }
            (None, None) => None,
            _ => None,
        };
        if profile.transfer_mode == StandaloneSftpTransferMode::RemoteRemote
            && secondary_config.is_none()
        {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.pending = false;
                    form.error = Some(self.i18n.t("sftp.standalone.missing_credentials"));
                }
            });
            cx.notify();
            return;
        }
        let pair_launch_token = secondary_config.map(|secondary_config| {
            let token = uuid::Uuid::new_v4().to_string();
            self.pending_standalone_sftp_pair_launches.insert(
                token.clone(),
                PendingStandaloneSftpPairLaunch {
                    saved_profile_id: profile.id.clone(),
                    title: profile.name.clone(),
                    primary_initial_remote_path: profile.initial_remote_path.clone(),
                    secondary_initial_remote_path: profile
                        .secondary_endpoint
                        .as_ref()
                        .and_then(|endpoint| endpoint.initial_remote_path.clone()),
                    primary_config: None,
                    secondary_config: Some(secondary_config),
                },
            );
            token
        });
        let intent = SshConnectionIntent::StandaloneSftp {
            saved_profile_id: Some(profile.id.clone()),
            initial_remote_path: profile.initial_remote_path.clone(),
            pair_launch_token,
        };
        self.start_new_connection_config_flow(config, profile.name, intent, window, cx);
    }

    pub(super) fn submit_remote_desktop_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((mut profile, save_request, mut runtime_password, ssh_gateway_connection_id)) =
            self.with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let Some(protocol) = remote_desktop_protocol_for_transport(form.transport) else {
                    return None;
                };
                let host = form.host.trim().to_string();
                let port = form
                    .port
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port > 0);
                if host.is_empty() {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_host_required"),
                    );
                    cx.notify();
                    return None;
                }
                let Some(port) = port else {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_invalid_port"),
                    );
                    cx.notify();
                    return None;
                };
                if protocol == RemoteDesktopProtocol::Rdp && form.username.trim().is_empty() {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_username_required"),
                    );
                    cx.notify();
                    return None;
                }
                if form
                    .remote_desktop_ssh_gateway_connection_id
                    .as_deref()
                    .is_some_and(|connection_id| this.connection_store.get(connection_id).is_none())
                {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_ssh_gateway_missing"),
                    );
                    cx.notify();
                    return None;
                }
                let editing_profile_id = form.remote_desktop_profile_id.clone();
                let existing_profile = editing_profile_id
                    .as_deref()
                    .and_then(|id| this.connection_store.get_remote_desktop_profile(id))
                    .cloned();
                let has_saved_credential = form.saved_password_keychain_id.is_some();
                if protocol == RemoteDesktopProtocol::Rdp
                    && action != NewConnectionSubmitAction::Save
                    && form.password.is_empty()
                    && !has_saved_credential
                {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_password_required"),
                    );
                    cx.notify();
                    return None;
                }
                let label = remote_desktop_profile_label(&form.name, protocol, &host, port);
                let username =
                    Some(form.username.trim().to_string()).filter(|username| !username.is_empty());
                let password = if !form.password.is_empty() {
                    // Move the UI draft into a zeroizing type before saving or starting a worker.
                    Some(SecretString::from(std::mem::take(&mut form.password)))
                } else {
                    None
                };
                let save_credential = form.save_password;
                let ssh_gateway_connection_id =
                    form.remote_desktop_ssh_gateway_connection_id.clone();
                let should_save =
                    editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
                let clear_credential =
                    editing_profile_id.is_some() && has_saved_credential && !save_credential;
                let (credential_to_save, runtime_password) = if should_save && save_credential {
                    // Saving and connecting reloads the protected value below instead of cloning it.
                    (password, None)
                } else {
                    (None, password)
                };
                let domain = existing_profile
                    .as_ref()
                    .and_then(|profile| profile.domain.clone());
                let read_only = existing_profile
                    .as_ref()
                    .is_some_and(|profile| profile.read_only);
                let save_request = should_save.then(|| SaveRemoteDesktopProfileRequest {
                    id: editing_profile_id,
                    name: label.clone(),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    notes: saved_profile_notes(&form.notes),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    protocol,
                    host: host.clone(),
                    port,
                    username: username.clone(),
                    domain: domain.clone(),
                    ssh_gateway_connection_id: form
                        .remote_desktop_ssh_gateway_connection_id
                        .clone(),
                    credential_ref: None,
                    credential: credential_to_save,
                    clear_credential,
                    read_only,
                    session_options: form.remote_desktop_session_options,
                });
                let profile = RemoteDesktopConnectionProfile {
                    id: format!("new-remote-desktop-{}", uuid::Uuid::new_v4()),
                    label,
                    protocol,
                    endpoint: RemoteDesktopEndpoint::new(host, port),
                    transport_endpoint: None,
                    username,
                    domain,
                    credential_ref: None,
                    read_only,
                    // A reconnect reuses the profile, so keep the user's per-session
                    // redirection choices on the profile instead of rebuilding defaults.
                    session_options: form.remote_desktop_session_options,
                };
                form.pending = true;
                form.error = None;
                Some((
                    profile,
                    save_request,
                    runtime_password,
                    ssh_gateway_connection_id,
                ))
            })
        else {
            return;
        };

        if let Some(request) = save_request {
            match self.connection_store.upsert_remote_desktop_profile(request) {
                Ok(saved) => {
                    profile.id = saved.id;
                    profile.label = saved.name;
                    profile.credential_ref = saved.credential_ref;
                    self.queue_cloud_sync_dirty_refresh(cx);
                    if action != NewConnectionSubmitAction::Save && runtime_password.is_none() {
                        match self
                            .connection_store
                            .get_remote_desktop_credential(&profile.id)
                        {
                            Ok(password) => runtime_password = password,
                            Err(error) => {
                                self.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.pending = false;
                                        form.error = Some(format!(
                                            "{}: {error}",
                                            self.i18n.t(
                                                "sessionManager.remote_desktop_profiles.open_failed"
                                            )
                                        ));
                                    }
                                });
                                cx.notify();
                                return;
                            }
                        }
                    }
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n
                                    .t("modals.new_connection.remote_desktop_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        self.update_connection_form_state(cx, ConnectionFormState::clear);
        if action != NewConnectionSubmitAction::Save {
            let runtime_password =
                runtime_password.map(|secret| RemoteDesktopSecret::from(secret.into_zeroizing()));
            self.open_remote_desktop_connection_with_gateway(
                profile,
                runtime_password,
                ssh_gateway_connection_id,
                window,
                cx,
            );
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            intent,
            SshConnectionIntent::Test | SshConnectionIntent::TestStandaloneSftp
        ) && self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.auth_tab == SshAuthTab::TwoFactor)
        {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(self.i18n.t("ssh.form.test_not_supported_kbi"));
                }
            });
            cx.notify();
            return;
        }
        let secret_handoff = if matches!(
            intent,
            SshConnectionIntent::Test | SshConnectionIntent::TestStandaloneSftp
        ) {
            RuntimeSecretHandoff::CopyForTest
        } else {
            RuntimeSecretHandoff::Move
        };
        let Some((config, title)) = self.build_new_connection_config(secret_handoff, cx) else {
            return;
        };
        self.start_new_connection_config_flow(config, title, intent, window, cx);
    }

    fn start_new_connection_config_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if intent == SshConnectionIntent::Test {
            self.start_ssh_test_flow(config, title, cx);
            return;
        }
        let mut config = config;
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.set_form_feedback(None, Some(error.clone()), cx)
            });
            if !reported_to_form {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(error), cx);
                });
            }
            cx.notify();
            return;
        }
        if matches!(&intent, SshConnectionIntent::DrillDown { .. }) {
            // Tauri DrillDownDialog calls tree_drill_down and then
            // connect_tree_node; it does not run a local direct host-key
            // preflight because the child may only be reachable through the
            // parent tunnel. Native keeps that node-only path here.
            self.continue_verified_ssh_flow(config, title, intent, window, cx);
            return;
        }
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.pending = true;
                form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
            }
        });

        if config.proxy_chain.is_some()
            && !matches!(
                intent,
                SshConnectionIntent::StandaloneSftp { .. }
                    | SshConnectionIntent::StandaloneSftpSecondary { .. }
                    | SshConnectionIntent::TestStandaloneSftp
            )
        {
            self.start_proxy_session_tree_connect(config, title, intent, None, window, cx);
            cx.notify();
            return;
        }
        self.start_ssh_preflight(config, title, intent, cx);
        cx.notify();
    }

    pub(crate) fn open_saved_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = saved_connection_for_open(&self.connection_store, id) else {
            // Saved rows can outlive an external store update. Report the
            // stale reference without exposing its identifier or connection data.
            tracing::warn!("Saved connection lookup failed before opening");
            let title = self.i18n.t("sessionManager.toast.connection_not_found");
            self.push_command_palette_toast(title.clone(), None, TerminalNoticeVariant::Error, cx);
            self.push_notification_entry(
                WorkspaceNotificationKind::Connection,
                WorkspaceNotificationSeverity::Error,
                title,
                None,
                WorkspaceNotificationScope::Global,
                Some("saved-connection-not-found".to_string()),
            );
            cx.notify();
            return;
        };
        let Some(config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &conn,
        ) else {
            if self.try_reuse_active_saved_connection_terminal(id, &conn, window, cx) {
                return;
            }
            self.open_saved_connection_prompt(
                id,
                SavedConnectionPromptAction::Connect,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_required"),
                ),
                window,
                cx,
            );
            return;
        };
        let title = conn.name.clone();
        self.start_saved_connection_flow(id.to_string(), config, title, window, cx);
    }

    pub(in crate::workspace) fn open_saved_connection_prompt(
        &mut self,
        id: &str,
        action: SavedConnectionPromptAction,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_saved_connection(&conn, error);
        restore_legacy_jump_host_in_form(&mut form, &conn, &self.connection_store);
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.editing_saved_connection_id = Some(id.to_string());
            state.saved_connection_prompt_action = Some(action);
        });
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection_editor(
        &mut self,
        id: &str,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_saved_connection(&conn, error);
        restore_legacy_jump_host_in_form(&mut form, &conn, &self.connection_store);
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.editing_saved_connection_id = Some(id.to_string());
        });
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection_reconnect_editor(
        &mut self,
        node_id: NodeId,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_editor(id, None, window, cx);
        if self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .as_deref()
            == Some(id)
        {
            // This marker is consumed after a successful save so normal
            // connection edits keep their existing save-only behavior.
            self.update_connection_form_state(cx, |state| {
                state.editing_saved_connection_connect_after_save_node_id = Some(node_id);
            });
        }
    }

    pub(in crate::workspace) fn open_runtime_node_reconnect_editor(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.ssh_nodes.get(&node_id).map(|node| node.title.clone()) else {
            return;
        };
        let Some(runtime_snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_runtime_config(
            runtime_snapshot.config,
            Some(&title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.save_connection = false;
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn submit_saved_connection_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self
            .connection_form_state(cx)
            .saved_connection_prompt_action
        else {
            return;
        };
        let Some(id) = self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .clone()
        else {
            return;
        };
        let secret_handoff = match action {
            SavedConnectionPromptAction::Test => RuntimeSecretHandoff::CopyForTest,
            SavedConnectionPromptAction::Connect => RuntimeSecretHandoff::Move,
        };
        let Some((mut config, title)) = self.build_new_connection_config(secret_handoff, cx) else {
            return;
        };
        if config.proxy_command.is_none()
            && let Some(conn) = self.connection_store.get(&id)
            && let Some(saved_config) = ssh_config_from_saved_connection_with_auth(
                &self.connection_store,
                self.settings_store.settings(),
                conn,
                Some(AuthMethod::Agent),
            )
            && saved_config.proxy_command.is_some()
        {
            // Imported ProxyCommand routing remains attached when only auth is supplied by a prompt.
            config.proxy_command = saved_config.proxy_command;
            config.proxy_chain = None;
            config.upstream_proxy = None;
        }
        if config.proxy_chain.is_none()
            && config.proxy_command.is_none()
            && let Some(conn) = self.connection_store.get(&id)
            && let Some(proxy_chain) =
                proxy_chain_config_from_saved_connection(&self.connection_store, conn)
            && !proxy_chain.is_empty()
        {
            config.proxy_chain = Some(proxy_chain);
            config.strict_host_key_checking = true;
        }

        match action {
            SavedConnectionPromptAction::Connect => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = true;
                        form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
                    }
                });
                self.start_saved_connection_flow(id, config, title, window, cx);
            }
            SavedConnectionPromptAction::Test => {
                self.start_ssh_test_flow(config, title, cx);
            }
        }
    }

    pub(super) fn sync_saved_connection_node_title(&mut self, saved_connection_id: &str) -> bool {
        let Some(title) = self
            .connection_store
            .get(saved_connection_id)
            .map(|connection| connection.name.clone())
        else {
            return false;
        };
        sync_saved_connection_node_title_for_nodes(&mut self.ssh_nodes, saved_connection_id, &title)
    }

    pub(super) fn sync_saved_connection_x11_forwarding(&self, saved_connection_id: &str) -> bool {
        let Some(options) = self
            .connection_store
            .get(saved_connection_id)
            .map(|connection| connection.options.x11_forwarding)
        else {
            return false;
        };
        // Only the non-secret policy is updated in place. Existing shells and
        // the registry-owned physical connection retain their current owners.
        self.node_router.update_saved_connection_x11_forwarding(
            saved_connection_id,
            x11_forward_policy(options),
        ) > 0
    }

    pub(super) fn save_editing_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .clone()
        else {
            return;
        };
        let Some(existing_connection) = self.connection_store.get(&id).cloned() else {
            return;
        };
        let existing_auth = existing_connection.auth.clone();
        let Some(save_request) = self.with_connection_form_mut(cx, |this, form, _cx| {
            let form = form?;
            Some((|| -> anyhow::Result<SaveConnectionRequest> {
                let missing_credentials_message =
                    this.i18n.t("sessions.saved_next_hop.missing_credentials");
                let saved_auth_copies = saved_proxy_hop_auth_copies(
                    &this.connection_store,
                    &form.proxy_hops,
                    &missing_credentials_message,
                )?;
                let edit_sources = form
                    .proxy_hops
                    .iter()
                    .map(|hop| EditedProxyHopSource {
                        persisted_proxy_hop_index: hop.persisted_proxy_hop_index,
                        has_explicit_secret_draft: hop.has_explicit_secret_draft(),
                    })
                    .collect();
                let mut request = save_request_from_form_with_existing_auth(
                    form,
                    Some(id.clone()),
                    Some(&existing_auth),
                )?;
                preserve_edited_proxy_hop_auth(
                    &mut request.proxy_chain,
                    &existing_connection.proxy_chain,
                    edit_sources,
                    saved_auth_copies,
                    &missing_credentials_message,
                )?;
                Ok(request)
            })())
        }) else {
            return;
        };
        match save_request {
            Ok(request) => {
                match self.connection_store.upsert(request) {
                    Ok(_) => {
                        self.sync_saved_connection_node_title(&id);
                        self.sync_saved_connection_x11_forwarding(&id);
                        self.apply_saved_connection_terminal_preferences(&id, cx);
                        let connect_after_save_node_id =
                            self.update_connection_form_state(cx, |state| {
                                let node_id = state
                                    .editing_saved_connection_connect_after_save_node_id
                                    .take();
                                state.clear();
                                node_id
                            });
                        self.queue_cloud_sync_dirty_refresh(cx);
                        if let Some(node_id) = connect_after_save_node_id {
                            if let Some(conn) = self.connection_store.get(&id).cloned()
                                && let Some(config) = ssh_config_from_saved_connection(
                                    &self.connection_store,
                                    self.settings_store.settings(),
                                    &conn,
                                )
                            {
                                let title = conn.name.clone();
                                // Drop the stale failed runtime node before
                                // materializing the edited connection again.
                                self.remove_inactive_session_tree_node(&node_id, window, cx);
                                self.start_saved_connection_flow(id, config, title, window, cx);
                            } else {
                                self.open_saved_connection_prompt(
                                    &id,
                                    SavedConnectionPromptAction::Connect,
                                    Some(
                                        self.i18n.t(
                                            "sessionManager.edit_properties.password_placeholder",
                                        ),
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            let message = self.i18n.t("sessionManager.edit_properties.save");
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                            self.focus_active_pane(window, cx);
                        }
                    }
                    Err(error) => {
                        self.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.error = Some(error.to_string());
                            }
                        });
                    }
                }
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn save_duplicate_connection_template(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = self
            .connection_form_state(cx)
            .duplicating_saved_connection_id
            .clone()
        else {
            return;
        };
        let source_connection = self.connection_store.get(&source_id).cloned();
        let source_auth = source_connection
            .as_ref()
            .map(|connection| connection.auth.clone());
        let Some(save_request) = self.with_connection_form_mut(cx, |this, form, _cx| {
            let form = form?;
            Some((|| {
                let source_connection = source_connection
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Source connection is no longer available"))?;
                let missing_credentials_message =
                    this.i18n.t("sessions.saved_next_hop.missing_credentials");
                let saved_auth_copies = saved_proxy_hop_auth_copies(
                    &this.connection_store,
                    &form.proxy_hops,
                    &missing_credentials_message,
                )?;
                let edit_sources = form
                    .proxy_hops
                    .iter()
                    .map(|hop| EditedProxyHopSource {
                        persisted_proxy_hop_index: hop.persisted_proxy_hop_index,
                        has_explicit_secret_draft: hop.has_explicit_secret_draft(),
                    })
                    .collect();
                let mut request =
                    save_request_from_form_with_existing_auth(form, None, source_auth.as_ref())?;
                preserve_edited_proxy_hop_auth(
                    &mut request.proxy_chain,
                    &source_connection.proxy_chain,
                    edit_sources,
                    saved_auth_copies,
                    &missing_credentials_message,
                )?;
                request.auth = auth_for_duplicate_owner(&this.connection_store, request.auth)?;
                for hop in &mut request.proxy_chain {
                    hop.auth = auth_for_duplicate_owner(
                        &this.connection_store,
                        std::mem::replace(&mut hop.auth, SavedAuth::Agent),
                    )?;
                }
                request.upstream_proxy = upstream_proxy_for_duplicate_owner(
                    &this.connection_store,
                    request.upstream_proxy,
                )?;
                if request.proxy_command.is_some()
                    && let Some(source_command) = source_connection.proxy_command.as_ref()
                {
                    // A duplicate receives a new protected-store owner instead of sharing an id.
                    request.proxy_command = Some(SavedProxyCommand {
                        keychain_id: None,
                        plaintext_command: Some(
                            this.connection_store
                                .get_saved_proxy_command(source_command)?,
                        ),
                    });
                }
                Ok::<SaveConnectionRequest, anyhow::Error>(request)
            })())
        }) else {
            return;
        };
        match save_request {
            Ok(request) => match self.connection_store.upsert(request) {
                Ok(_) => {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                    let message = self.i18n.t("sessionManager.toast.connection_duplicated");
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.focus_active_pane(window, cx);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.error = Some(error.to_string());
                        }
                    });
                }
            },
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_saved_connection_flow(
        &mut self,
        id: String,
        mut config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.set_form_feedback(None, Some(error.clone()), cx)
            });
            if !reported_to_form {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(error), cx);
                });
            }
            cx.notify();
            return;
        }
        let message = self.i18n.t("ssh.form.checking_host_key");
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(message), cx);
        });
        if config.proxy_chain.is_some() {
            self.start_proxy_session_tree_connect(
                config,
                title,
                SshConnectionIntent::ConnectSaved(id),
                None,
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.start_ssh_preflight(config, title, SshConnectionIntent::ConnectSaved(id), cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_standalone_sftp_profile(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .get_standalone_sftp_profile(id)
            .cloned()
        else {
            return;
        };
        if self.standalone_sftp_sessions.contains_key(id) {
            let secondary_endpoint_id = format!("{id}:secondary");
            if profile.transfer_mode == StandaloneSftpTransferMode::RemoteRemote
                && self
                    .standalone_sftp_sessions
                    .contains_key(&secondary_endpoint_id)
            {
                self.open_standalone_sftp_pair_tab_surface(
                    id.to_string(),
                    secondary_endpoint_id,
                    profile.name,
                    profile.initial_remote_path,
                    profile
                        .secondary_endpoint
                        .and_then(|endpoint| endpoint.initial_remote_path),
                    cx,
                );
            } else {
                self.open_standalone_sftp_tab_surface(
                    id.to_string(),
                    profile.name,
                    profile.initial_remote_path,
                    cx,
                );
            }
            return;
        }
        let mut runtime_secrets = match self
            .connection_store
            .load_standalone_sftp_profile_runtime_secrets(id)
        {
            Ok(secrets) => secrets,
            Err(error) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(error.to_string()), cx);
                });
                return;
            }
        };
        let secondary_runtime_secrets = runtime_secrets.secondary_endpoint.take();
        let Some(config) = ssh_config_from_standalone_sftp_profile_with_runtime_secrets(
            &self.connection_store,
            self.settings_store.settings(),
            &profile,
            runtime_secrets,
            None,
        ) else {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager
                    .set_status(Some(self.i18n.t("sftp.standalone.missing_credentials")), cx);
            });
            return;
        };
        let secondary_config = match (
            profile.secondary_endpoint.as_ref(),
            secondary_runtime_secrets,
        ) {
            (Some(endpoint), Some(secrets)) => {
                ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets(
                    &self.connection_store,
                    self.settings_store.settings(),
                    &profile.name,
                    endpoint,
                    secrets,
                    None,
                )
            }
            (None, None) => None,
            _ => None,
        };
        if profile.transfer_mode == StandaloneSftpTransferMode::RemoteRemote
            && secondary_config.is_none()
        {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager
                    .set_status(Some(self.i18n.t("sftp.standalone.missing_credentials")), cx);
            });
            return;
        }
        let pair_launch_token = secondary_config.map(|secondary_config| {
            let token = uuid::Uuid::new_v4().to_string();
            self.pending_standalone_sftp_pair_launches.insert(
                token.clone(),
                PendingStandaloneSftpPairLaunch {
                    saved_profile_id: profile.id.clone(),
                    title: profile.name.clone(),
                    primary_initial_remote_path: profile.initial_remote_path.clone(),
                    secondary_initial_remote_path: profile
                        .secondary_endpoint
                        .as_ref()
                        .and_then(|endpoint| endpoint.initial_remote_path.clone()),
                    primary_config: None,
                    secondary_config: Some(secondary_config),
                },
            );
            token
        });
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
        });
        self.start_ssh_preflight(
            config,
            profile.name,
            SshConnectionIntent::StandaloneSftp {
                saved_profile_id: Some(profile.id),
                initial_remote_path: profile.initial_remote_path,
                pair_launch_token,
            },
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_mosh_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.connection_store.get_mosh_profile(id).cloned() else {
            return;
        };
        let runtime_secrets = match self.connection_store.load_mosh_profile_runtime_secrets(id) {
            Ok(secrets) => secrets,
            Err(_) => {
                self.open_saved_mosh_profile_editor(id, window, cx);
                let missing_credentials = self
                    .i18n
                    .t("sessionManager.mosh_profiles.missing_credentials");
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(missing_credentials);
                        form.focused_field = NewConnectionField::Name;
                        form.field_focused = false;
                    }
                });
                return;
            }
        };
        let Some(config) = runtime_mosh_config_from_saved(&profile, runtime_secrets, None) else {
            // Portable profiles intentionally omit credentials. Open the editor at the
            // matching secret field so the device-local value can be supplied safely.
            self.open_saved_mosh_profile_editor(id, window, cx);
            let missing_credentials = self
                .i18n
                .t("sessionManager.mosh_profiles.missing_credentials");
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(missing_credentials);
                    form.focused_field = match form.auth_tab {
                        SshAuthTab::Password => NewConnectionField::Password,
                        SshAuthTab::DefaultKey
                        | SshAuthTab::SshKey
                        | SshAuthTab::ManagedKey
                        | SshAuthTab::Certificate => NewConnectionField::Passphrase,
                        SshAuthTab::Agent | SshAuthTab::TwoFactor => NewConnectionField::Name,
                    };
                    form.field_focused = false;
                }
            });
            return;
        };
        let title = profile.name.clone();
        let options = mosh_options_from_profile(&profile);
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
        });
        self.start_ssh_preflight(config, title, SshConnectionIntent::Mosh(options), cx);
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_preflight(
        &self,
        mut config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        cx: &App,
    ) {
        let tx = self.ssh_worker_sender(cx);
        let host = config.host.clone();
        let port = config.port;
        let connect_timeout_seconds = config.timeout_secs;
        let upstream_proxy = config.upstream_proxy.take();
        let worker_config = config;
        let worker_title = title;
        let routed_preflight = worker_config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
            && matches!(
                &intent,
                SshConnectionIntent::StandaloneSftp { .. }
                    | SshConnectionIntent::StandaloneSftpSecondary { .. }
                    | SshConnectionIntent::TestStandaloneSftp
                    | SshConnectionIntent::Mosh(_)
            );
        let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
        let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
        std::thread::spawn(move || {
            let (checked_host, checked_port, status) = match tokio::runtime::Runtime::new() {
                Ok(runtime) if routed_preflight => {
                    // Routed SFTP and Mosh preflight authenticate through every hop. The
                    // bounded zeroizing copy survives only until the accepted launch starts.
                    let mut route_config = worker_config.clone();
                    route_config.upstream_proxy = upstream_proxy.clone();
                    runtime.block_on(
                        SshTransportClient::new(route_config)
                            .with_prompt_handler(prompt_handler)
                            .with_managed_key_resolver(managed_key_resolver)
                            .preflight_route_host_keys(),
                    )
                }
                Ok(runtime) => (
                    host.clone(),
                    port,
                    runtime.block_on(check_host_key_with_route(
                        &host,
                        port,
                        connect_timeout_seconds,
                        upstream_proxy.as_ref(),
                        worker_config.proxy_command.as_ref(),
                    )),
                ),
                Err(error) => (
                    host.clone(),
                    port,
                    HostKeyStatus::Error {
                        message: format!("failed to initialize SSH runtime: {error}"),
                    },
                ),
            };
            let _ = tx.send(SshConnectionWorkerResult::Preflight {
                config: worker_config,
                upstream_proxy,
                title: worker_title,
                intent,
                host: checked_host,
                port: checked_port,
                status,
            });
        });
    }
}

fn saved_connection_for_open(store: &ConnectionStore, id: &str) -> Option<SavedConnection> {
    store.get(id).cloned()
}

#[cfg(test)]
mod saved_connection_open_tests {
    use super::*;

    fn password_proxy_hop(auth: SavedAuth) -> SavedProxyHop {
        SavedProxyHop {
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
            } if password == "replacement-secret"
        ));
        assert!(form.password.is_empty());
    }

    #[test]
    fn saved_mosh_proxy_chain_becomes_bootstrap_route() {
        let mut profile = oxideterm_connections::MoshProfile::new(
            "Mosh through jump",
            "target.example.com",
            22,
            "ops",
            SavedAuth::Agent,
        );
        profile.proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            keychain_id: Some("jump-password-owner".to_string()),
            plaintext_password: None,
        })];

        let config = runtime_mosh_config_from_saved(
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
            keychain_id: Some("persisted-proxy-password".to_string()),
            plaintext_password: None,
        })];
        let mut edited_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
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
            } if keychain_id == "persisted-proxy-password"
        ));
    }

    #[test]
    fn saved_proxy_hop_added_during_edit_uses_its_independent_auth_copy() {
        let mut edited_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
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
            } if password == "copied-proxy-secret"
        ));
    }

    #[test]
    fn edited_proxy_hop_never_reuses_credentials_for_another_host() {
        let persisted_proxy_chain = vec![password_proxy_hop(SavedAuth::Password {
            keychain_id: Some("persisted-proxy-password".to_string()),
            plaintext_password: None,
        })];
        let mut edited_hop = password_proxy_hop(SavedAuth::Password {
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
