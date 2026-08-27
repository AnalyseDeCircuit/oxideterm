use super::*;
use oxideterm_connections::ConnectionX11ForwardingOptions;

pub(super) fn auth_label(auth_type: AuthType) -> String {
    match auth_type {
        AuthType::Password => "Password",
        AuthType::Key => "Key",
        AuthType::ManagedKey => "Managed Key",
        AuthType::Certificate => "Certificate",
        AuthType::KeyboardInteractive => "Keyboard Interactive",
        AuthType::Agent => "Agent",
    }
    .to_string()
}

pub(super) fn add_group_path_segments(group: &str, paths: &mut HashSet<String>) {
    if group.trim().is_empty() {
        return;
    }
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for index in 1..=parts.len() {
        paths.insert(parts[..index].join("/"));
    }
}

pub(super) fn expand_group_path(group: &str, expanded_groups: &mut HashSet<String>) {
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        return;
    }
    for index in 1..parts.len() {
        expanded_groups.insert(parts[..index].join("/"));
    }
}

/// Splits a canonical group path into its parent path and editable leaf name.
pub(super) fn split_session_group_path(group: &str) -> (Option<&str>, &str) {
    group
        .rsplit_once('/')
        .map_or((None, group), |(parent, leaf)| (Some(parent), leaf))
}

/// Builds a canonical path while keeping contextual editors leaf-name only.
pub(super) fn session_group_path_from_leaf(
    parent_path: Option<&str>,
    leaf_name: &str,
) -> Option<String> {
    let leaf_name = leaf_name.trim();
    if leaf_name.is_empty() || leaf_name.contains('/') {
        return None;
    }
    Some(match parent_path.filter(|parent| !parent.is_empty()) {
        Some(parent) => format!("{parent}/{leaf_name}"),
        None => leaf_name.to_string(),
    })
}

/// Returns whether a path is the selected group or one of its descendants.
pub(super) fn session_group_path_is_within(candidate: &str, group: &str) -> bool {
    candidate == group
        || candidate
            .strip_prefix(group)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Rewrites UI group state after a persisted subtree rename.
pub(super) fn renamed_session_group_path(
    candidate: &str,
    old_group: &str,
    new_group: &str,
) -> Option<String> {
    session_group_path_is_within(candidate, old_group).then(|| {
        let suffix = &candidate[old_group.len()..];
        format!("{new_group}{suffix}")
    })
}

pub(super) fn format_last_used(last_used: Option<&str>, i18n: &I18n) -> String {
    let Some(last_used) = last_used else {
        return i18n.t("sessionManager.table.never_used");
    };
    let Ok(date) = DateTime::parse_from_rfc3339(last_used) else {
        return last_used.to_string();
    };
    let date = date.with_timezone(&Utc);
    let now = Utc::now();
    let diff = now.signed_duration_since(date);
    let diff_mins = diff.num_minutes();
    let diff_hours = diff.num_hours();
    let diff_days = diff.num_days();

    if diff_mins < 1 {
        return i18n.t("sessionManager.time.just_now");
    }
    if diff_mins < 60 {
        return i18n
            .t("sessionManager.time.minutes_ago")
            .replace("{{count}}", &diff_mins.to_string());
    }
    if diff_hours < 24 {
        return i18n
            .t("sessionManager.time.hours_ago")
            .replace("{{count}}", &diff_hours.to_string());
    }
    if diff_days < 7 {
        return i18n
            .t("sessionManager.time.days_ago")
            .replace("{{count}}", &diff_days.to_string());
    }

    let local = date.with_timezone(&Local);
    format!("{}/{}/{}", local.year(), local.month(), local.day())
}

pub(super) fn theme_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_THEME_ALPHA)
}

pub(super) fn theme_secondary_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

pub(super) fn theme_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_HOVER_ALPHA)
}

pub(super) fn theme_row_hover_bg(color: u32, has_background: bool) -> Rgba {
    // Full-width rows need a lower-contrast hover than compact buttons and menus.
    color_for_background_or_alpha(
        color,
        has_background,
        BG_ACTIVE_ROW_HOVER_ALPHA,
        ROW_HOVER_ALPHA,
    )
}

pub(super) fn theme_input_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_THEME_ALPHA / 2, 0x80)
}

pub(super) fn theme_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_BORDER_ALPHA)
}

pub(super) fn theme_border_half(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_BORDER_HALF_ALPHA, 0x80)
}

pub(super) fn parse_hex_color(value: &str) -> Option<u32> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded;
    let hex = match hex.len() {
        3 => {
            expanded = hex.chars().flat_map(|ch| [ch, ch]).collect::<String>();
            expanded.as_str()
        }
        6 | 8 => hex,
        _ => return None,
    };
    u32::from_str_radix(&hex[..6], 16).ok()
}

pub(super) fn group_label(i18n: &I18n, group: Option<&str>) -> String {
    group
        .filter(|group| !group.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n.t("sessionManager.folder_tree.ungrouped"))
}

pub(super) fn selected_count_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.table.selected_count")
        .replace("{{count}}", &count.to_string())
}

pub(super) fn confirm_delete_connection_label(i18n: &I18n, name: &str) -> String {
    i18n.t("sessionManager.actions.confirm_delete")
        .replace("{{name}}", name)
}

pub(super) fn confirm_batch_delete_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.actions.confirm_batch_delete")
        .replace("{{count}}", &count.to_string())
}

pub(super) fn connections_deleted_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.toast.connections_deleted")
        .replace("{{count}}", &count.to_string())
}

pub(in crate::workspace) fn duplicate_connection_template_name<'a>(
    source_name: &str,
    existing_names: impl IntoIterator<Item = &'a str>,
) -> String {
    let occupied_names = existing_names
        .into_iter()
        .map(|name| name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let base_name = duplicate_template_base_name(source_name);

    // Match the Tauri duplicate-template flow: the first candidate is
    // "<name> Copy", then numbered copies are appended until the draft is unique.
    for copy_index in 1usize.. {
        let candidate = if copy_index == 1 {
            format!("{base_name} Copy")
        } else {
            format!("{base_name} Copy {copy_index}")
        };
        if !occupied_names.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("unbounded duplicate-name search must eventually find a free name")
}

pub(super) fn duplicate_template_base_name(source_name: &str) -> String {
    let trimmed = source_name.trim();
    let stripped = if let Some(base_name) = trimmed.strip_suffix(" Copy") {
        base_name.trim()
    } else if let Some((base_name, copy_index)) = trimmed.rsplit_once(" Copy ") {
        if !copy_index.is_empty() && copy_index.chars().all(|ch| ch.is_ascii_digit()) {
            base_name.trim()
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    if stripped.is_empty() {
        "Connection".to_string()
    } else {
        stripped.to_string()
    }
}

pub(super) fn connections_moved_label(i18n: &I18n, count: usize, group: String) -> String {
    i18n.t("sessionManager.toast.connections_moved")
        .replace("{{count}}", &count.to_string())
        .replace("{{group}}", &group)
}

pub(in crate::workspace) fn form_from_saved_connection(
    conn: &SavedConnection,
    error: Option<String>,
) -> NewConnectionForm {
    let (auth_tab, password, key_path, managed_key_id, cert_path, passphrase, save_password) =
        match conn.auth.conventional_fallback() {
            SavedAuth::Password {
                keychain_id,
                plaintext_password,
            } => (
                SshAuthTab::Password,
                plaintext_password
                    .as_ref()
                    .map(|password| password.expose_secret().to_string())
                    .unwrap_or_default(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                keychain_id.is_some() || plaintext_password.is_some(),
            ),
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } if key_path.is_empty() => (
                SshAuthTab::DefaultKey,
                String::new(),
                key_path.clone(),
                String::new(),
                String::new(),
                String::new(),
                *has_passphrase
                    || passphrase_keychain_id.is_some()
                    || plaintext_passphrase.is_some(),
            ),
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => (
                SshAuthTab::SshKey,
                String::new(),
                key_path.clone(),
                String::new(),
                String::new(),
                String::new(),
                *has_passphrase
                    || passphrase_keychain_id.is_some()
                    || plaintext_passphrase.is_some(),
            ),
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => (
                SshAuthTab::Certificate,
                String::new(),
                key_path.clone(),
                String::new(),
                cert_path.clone(),
                String::new(),
                *has_passphrase
                    || passphrase_keychain_id.is_some()
                    || plaintext_passphrase.is_some(),
            ),
            SavedAuth::ManagedKey {
                key_id,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => (
                SshAuthTab::ManagedKey,
                String::new(),
                String::new(),
                key_id.clone(),
                String::new(),
                String::new(),
                passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
            ),
            SavedAuth::KeyboardInteractive => (
                SshAuthTab::TwoFactor,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                false,
            ),
            SavedAuth::Agent => (
                SshAuthTab::Agent,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                false,
            ),
            SavedAuth::KerberosPreferred { .. } => unreachable!("fallback auth is conventional"),
        };
    let (gssapi_server_identity, gssapi_delegate_credentials) = conn
        .auth
        .gssapi_options()
        .map(|(identity, delegate)| (identity.unwrap_or_default().to_string(), delegate))
        .unwrap_or_default();
    let upstream_proxy_form = upstream_proxy_form_fields(&conn.upstream_proxy);
    let mut form = NewConnectionForm::default();
    form.name = conn.name.clone();
    form.host = conn.host.clone();
    form.port = conn.port.to_string();
    form.username = conn.username.clone();
    form.auth_tab = auth_tab;
    form.password = password;
    form.saved_password_keychain_id = match conn.auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
        _ => None,
    };
    // Only keychain-backed saved passwords start locked. Other auth modes
    // need an editable password draft if the user switches to password auth.
    form.password_loaded = !connection_has_unloaded_keychain_password(conn);
    form.key_path = key_path;
    form.managed_key_id = managed_key_id;
    form.cert_path = cert_path;
    form.passphrase = passphrase;
    form.gssapi_enabled = conn.auth.gssapi_options().is_some();
    form.gssapi_server_identity = gssapi_server_identity;
    form.gssapi_delegate_credentials = gssapi_delegate_credentials;
    form.save_password = save_password;
    form.group = group_label_for_form(conn.group.as_deref());
    form.notes = conn.notes.clone().unwrap_or_default();
    form.color = conn.color.clone().unwrap_or_default();
    form.icon_background_color = conn.icon_background_color.clone().unwrap_or_default();
    form.icon = conn.icon.clone().unwrap_or_default();
    form.tags = conn.tags.clone();
    form.post_connect_command = conn.post_connect_command().unwrap_or_default().to_string();
    form.proxy_command_enabled = conn.proxy_command.is_some();
    form.proxy_command_keychain_id = conn
        .proxy_command
        .as_ref()
        .and_then(|command| command.keychain_id.clone());
    form.upstream_proxy_policy = upstream_proxy_form.policy;
    form.upstream_proxy_protocol = upstream_proxy_form.protocol;
    form.upstream_proxy_host = upstream_proxy_form.host;
    form.upstream_proxy_port = upstream_proxy_form.port;
    form.upstream_proxy_auth = upstream_proxy_form.auth;
    form.upstream_proxy_username = upstream_proxy_form.username;
    form.upstream_proxy_password_keychain_id = upstream_proxy_form.password_keychain_id;
    form.upstream_proxy_remote_dns = upstream_proxy_form.remote_dns;
    form.upstream_proxy_no_proxy = upstream_proxy_form.no_proxy;
    form.agent_forwarding = conn.options.agent_forwarding;
    form.identity_agent = conn.options.identity_agent.clone().unwrap_or_default();
    form.agent_forwarding_socket = conn.options.agent_forwarding_socket.clone();
    // Probe the saved IdentityAgent when reopening a form so edit,
    // credential-prompt, and duplicate modes never inherit Unknown.
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
    // Preserve compatibility settings when an existing connection enters edit mode.
    form.legacy_ssh_compatibility = conn.options.legacy_ssh_compatibility;
    form.connect_timeout_seconds = conn.options.effective_connect_timeout_seconds();
    form.connect_timeout_seconds_text = form.connect_timeout_seconds.to_string();
    form.dedicated_new_terminal_connection = conn.options.dedicated_new_terminal_connection;
    form.ssh_channel_strategy = conn.options.ssh_channel_strategy;
    form.x11_forwarding = conn.options.x11_forwarding;
    if form.ssh_channel_strategy.requires_dedicated_consumers() {
        form.agent_forwarding = false;
        form.x11_forwarding = ConnectionX11ForwardingOptions::default();
    }
    form.terminal = conn.options.terminal.clone();
    // Every saved-connection form receives the complete non-secret route so
    // edit, duplicate, prompt, and SSH-config entry points cannot diverge.
    form.proxy_hops = conn
        .proxy_chain
        .iter()
        .enumerate()
        .map(|(index, hop)| NewConnectionProxyHop::from_saved(index, hop))
        .collect();
    form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    form.save_connection = true;
    form.error = error;
    form
}

pub(in crate::workspace) fn restore_legacy_jump_host_in_form(
    form: &mut NewConnectionForm,
    connection: &SavedConnection,
    store: &ConnectionStore,
) {
    if !form.proxy_hops.is_empty() {
        return;
    }
    let Some(jump_id) = connection.options.jump_host.as_deref() else {
        return;
    };
    let Some(jump) = store
        .connection_infos()
        .into_iter()
        .find(|candidate| candidate.id == jump_id)
    else {
        return;
    };
    let mut hop = NewConnectionProxyHop::new();
    hop.apply_saved_connection(&jump);
    form.proxy_hops.push(hop);
    form.proxy_chain_expanded = true;
}

pub(super) fn form_from_standalone_sftp_profile(
    profile: &oxideterm_connections::StandaloneSftpProfile,
) -> NewConnectionForm {
    // Edit mode restores only non-secret metadata and protected-store references.
    let upstream_proxy_form = upstream_proxy_form_fields(&profile.upstream_proxy);
    let mut form = NewConnectionForm::default();
    form.transport = crate::workspace::new_connection::NewConnectionTransport::StandaloneSftp;
    // Editing keeps the selected advanced transport discoverable in the shared selector.
    form.advanced_connections_expanded = true;
    form.standalone_sftp_profile_id = Some(profile.id.clone());
    form.standalone_sftp_transfer_mode = profile.transfer_mode;
    form.name = profile.name.clone();
    form.host = profile.host.clone();
    form.port = profile.port.to_string();
    form.username = profile.username.clone();
    form.auth_tab = ssh_auth_tab_from_saved_auth(&profile.auth);
    form.saved_password_keychain_id = match profile.auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
        _ => None,
    };
    form.password_loaded = true;
    form.save_password = match profile.auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => keychain_id.is_some(),
        SavedAuth::Key {
            has_passphrase,
            passphrase_keychain_id,
            ..
        }
        | SavedAuth::Certificate {
            has_passphrase,
            passphrase_keychain_id,
            ..
        } => *has_passphrase || passphrase_keychain_id.is_some(),
        SavedAuth::ManagedKey {
            passphrase_keychain_id,
            ..
        } => passphrase_keychain_id.is_some(),
        SavedAuth::KeyboardInteractive | SavedAuth::Agent => false,
        SavedAuth::KerberosPreferred { .. } => unreachable!("fallback auth is conventional"),
    };
    form.key_path = profile.auth.key_path().unwrap_or_default().to_string();
    form.managed_key_id = profile
        .auth
        .managed_key_id()
        .unwrap_or_default()
        .to_string();
    form.cert_path = profile.auth.cert_path().unwrap_or_default().to_string();
    form.gssapi_enabled = profile.auth.gssapi_options().is_some();
    form.gssapi_server_identity = profile
        .auth
        .gssapi_options()
        .and_then(|(identity, _)| identity.map(ToOwned::to_owned))
        .unwrap_or_default();
    form.gssapi_delegate_credentials = profile
        .auth
        .gssapi_options()
        .is_some_and(|(_, delegate)| delegate);
    form.group = group_label_for_form(profile.group.as_deref());
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.sftp_initial_remote_path = profile.initial_remote_path.clone().unwrap_or_default();
    form.proxy_hops = profile
        .proxy_chain
        .iter()
        .enumerate()
        .map(|(index, hop)| NewConnectionProxyHop::from_saved(index, hop))
        .collect();
    form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    form.proxy_command_enabled = profile.proxy_command.is_some();
    form.proxy_command_keychain_id = profile
        .proxy_command
        .as_ref()
        .and_then(|command| command.keychain_id.clone());
    form.upstream_proxy_policy = upstream_proxy_form.policy;
    form.upstream_proxy_protocol = upstream_proxy_form.protocol;
    form.upstream_proxy_host = upstream_proxy_form.host;
    form.upstream_proxy_port = upstream_proxy_form.port;
    form.upstream_proxy_auth = upstream_proxy_form.auth;
    form.upstream_proxy_username = upstream_proxy_form.username;
    form.upstream_proxy_password_keychain_id = upstream_proxy_form.password_keychain_id;
    form.upstream_proxy_remote_dns = upstream_proxy_form.remote_dns;
    form.upstream_proxy_no_proxy = upstream_proxy_form.no_proxy;
    form.identity_agent = profile.identity_agent.clone().unwrap_or_default();
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
    form.legacy_ssh_compatibility = profile.legacy_ssh_compatibility;
    form.connect_timeout_seconds = profile.connect_timeout_seconds;
    form.connect_timeout_seconds_text = profile.connect_timeout_seconds.to_string();
    if let Some(endpoint) = profile.secondary_endpoint.as_ref() {
        let secondary_upstream_proxy_form = upstream_proxy_form_fields(&endpoint.upstream_proxy);
        let secondary = &mut form.standalone_sftp_secondary;
        secondary.host = endpoint.host.clone();
        secondary.port = endpoint.port.to_string();
        secondary.username = endpoint.username.clone();
        secondary.auth_tab = ssh_auth_tab_from_saved_auth(&endpoint.auth);
        secondary.password_keychain_id = match endpoint.auth.conventional_fallback() {
            SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
            _ => None,
        };
        secondary.save_password = match endpoint.auth.conventional_fallback() {
            SavedAuth::Password { keychain_id, .. } => keychain_id.is_some(),
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id,
                ..
            }
            | SavedAuth::Certificate {
                has_passphrase,
                passphrase_keychain_id,
                ..
            } => *has_passphrase || passphrase_keychain_id.is_some(),
            SavedAuth::ManagedKey {
                passphrase_keychain_id,
                ..
            } => passphrase_keychain_id.is_some(),
            SavedAuth::KeyboardInteractive | SavedAuth::Agent => false,
            SavedAuth::KerberosPreferred { .. } => unreachable!("fallback auth is conventional"),
        };
        secondary.key_path = endpoint.auth.key_path().unwrap_or_default().to_string();
        secondary.managed_key_id = endpoint
            .auth
            .managed_key_id()
            .unwrap_or_default()
            .to_string();
        secondary.cert_path = endpoint.auth.cert_path().unwrap_or_default().to_string();
        secondary.gssapi_enabled = endpoint.auth.gssapi_options().is_some();
        secondary.gssapi_server_identity = endpoint
            .auth
            .gssapi_options()
            .and_then(|(identity, _)| identity.map(ToOwned::to_owned))
            .unwrap_or_default();
        secondary.gssapi_delegate_credentials = endpoint
            .auth
            .gssapi_options()
            .is_some_and(|(_, delegate)| delegate);
        secondary.identity_agent = endpoint.identity_agent.clone().unwrap_or_default();
        secondary.agent_available =
            oxideterm_ssh::ssh_agent_available(identity_agent_selector(&secondary.identity_agent));
        secondary.legacy_ssh_compatibility = endpoint.legacy_ssh_compatibility;
        secondary.ssh_algorithms = endpoint.ssh_algorithms.clone();
        secondary.connect_timeout_seconds = endpoint.connect_timeout_seconds;
        secondary.connect_timeout_seconds_text = endpoint.connect_timeout_seconds.to_string();
        secondary.initial_remote_path = endpoint.initial_remote_path.clone().unwrap_or_default();
        secondary.proxy_hops = endpoint
            .proxy_chain
            .iter()
            .enumerate()
            .map(|(index, hop)| NewConnectionProxyHop::from_saved(index, hop))
            .collect();
        secondary.proxy_chain_expanded = !secondary.proxy_hops.is_empty();
        secondary.proxy_command_enabled = endpoint.proxy_command.is_some();
        secondary.proxy_command_keychain_id = endpoint
            .proxy_command
            .as_ref()
            .and_then(|command| command.keychain_id.clone());
        secondary.upstream_proxy_policy = secondary_upstream_proxy_form.policy;
        secondary.upstream_proxy_protocol = secondary_upstream_proxy_form.protocol;
        secondary.upstream_proxy_host = secondary_upstream_proxy_form.host;
        secondary.upstream_proxy_port = secondary_upstream_proxy_form.port;
        secondary.upstream_proxy_auth = secondary_upstream_proxy_form.auth;
        secondary.upstream_proxy_username = secondary_upstream_proxy_form.username;
        secondary.upstream_proxy_password_keychain_id =
            secondary_upstream_proxy_form.password_keychain_id;
        secondary.upstream_proxy_remote_dns = secondary_upstream_proxy_form.remote_dns;
        secondary.upstream_proxy_no_proxy = secondary_upstream_proxy_form.no_proxy;
    }
    form.focused_field = NewConnectionField::Name;
    form
}

pub(super) fn connection_has_unloaded_keychain_password(conn: &SavedConnection) -> bool {
    matches!(
        &conn.auth,
        SavedAuth::Password {
            keychain_id: Some(_),
            plaintext_password: None,
        }
    )
}

pub(super) struct UpstreamProxyFormFields {
    policy: NewConnectionUpstreamProxyPolicy,
    protocol: SavedUpstreamProxyProtocol,
    host: String,
    port: String,
    auth: NewConnectionUpstreamProxyAuth,
    username: String,
    password_keychain_id: Option<String>,
    remote_dns: bool,
    no_proxy: String,
}

pub(super) fn upstream_proxy_form_fields(
    policy: &SavedUpstreamProxyPolicy,
) -> UpstreamProxyFormFields {
    match policy {
        SavedUpstreamProxyPolicy::UseGlobal => {
            default_upstream_proxy_form_fields(NewConnectionUpstreamProxyPolicy::UseGlobal)
        }
        SavedUpstreamProxyPolicy::Direct => {
            default_upstream_proxy_form_fields(NewConnectionUpstreamProxyPolicy::Direct)
        }
        SavedUpstreamProxyPolicy::Custom { proxy } => {
            let (auth, username, password_keychain_id) = match &proxy.auth {
                SavedUpstreamProxyAuth::None => {
                    (NewConnectionUpstreamProxyAuth::None, String::new(), None)
                }
                SavedUpstreamProxyAuth::Password {
                    username,
                    keychain_id,
                    ..
                } => (
                    NewConnectionUpstreamProxyAuth::Password,
                    username.clone(),
                    keychain_id.clone(),
                ),
            };
            UpstreamProxyFormFields {
                policy: NewConnectionUpstreamProxyPolicy::Custom,
                protocol: proxy.protocol,
                host: proxy.host.clone(),
                port: proxy.port.to_string(),
                auth,
                username,
                password_keychain_id,
                remote_dns: proxy.remote_dns,
                no_proxy: proxy.no_proxy.clone(),
            }
        }
    }
}

pub(super) fn default_upstream_proxy_form_fields(
    policy: NewConnectionUpstreamProxyPolicy,
) -> UpstreamProxyFormFields {
    UpstreamProxyFormFields {
        policy,
        protocol: SavedUpstreamProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: "1080".to_string(),
        auth: NewConnectionUpstreamProxyAuth::None,
        username: String::new(),
        password_keychain_id: None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

#[cfg(test)]
pub(in crate::workspace) fn save_request_from_form(
    form: &mut NewConnectionForm,
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_form_with_proxy_hop_prefix(form, &mut [], id)
}

pub(in crate::workspace) fn save_request_from_form_with_proxy_hop_prefix(
    form: &mut NewConnectionForm,
    proxy_hop_prefix: &mut [NewConnectionProxyHop],
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    validate_save_form_non_secret(form, proxy_hop_prefix)?;
    let persist_password_draft = form.save_password;
    let mut request = save_request_from_draft(
        connection_draft_from_form_with_proxy_hop_prefix(
            form,
            proxy_hop_prefix,
            persist_password_draft,
        ),
        id,
        None,
    )?;
    request.upstream_proxy = saved_upstream_proxy_policy_from_form(form)?;
    request.proxy_command = saved_proxy_command_from_form(form);
    Ok(request)
}

pub(in crate::workspace) fn save_request_from_form_with_existing_auth(
    form: &mut NewConnectionForm,
    id: Option<String>,
    existing_auth: Option<&SavedAuth>,
) -> anyhow::Result<SaveConnectionRequest> {
    validate_save_form_non_secret(form, &[])?;
    let persist_password_draft = form.password_loaded;
    let mut request = save_request_from_draft(
        connection_draft_from_form_with_proxy_hop_prefix(form, &mut [], persist_password_draft),
        id,
        existing_auth,
    )?;
    request.upstream_proxy = saved_upstream_proxy_policy_from_form(form)?;
    request.proxy_command = saved_proxy_command_from_form(form);
    Ok(request)
}

fn validate_save_form_non_secret(
    form: &NewConnectionForm,
    proxy_hop_prefix: &[NewConnectionProxyHop],
) -> anyhow::Result<()> {
    if form.name.trim().is_empty() {
        anyhow::bail!("Connection name is required");
    }
    if form.host.trim().is_empty() {
        anyhow::bail!("Host is required");
    }
    if form.username.trim().is_empty() {
        anyhow::bail!("Username is required");
    }
    let group = form.group.trim();
    if !group.is_empty() && !matches!(group, "Ungrouped" | "未分组") {
        validate_group_name(group)?;
    }
    for hop in proxy_hop_prefix.iter().chain(&form.proxy_hops) {
        if hop.host.trim().is_empty() {
            anyhow::bail!("Proxy host is required");
        }
        if hop.username.trim().is_empty() {
            anyhow::bail!("Proxy username is required");
        }
    }
    if form.upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom {
        if form.upstream_proxy_host.trim().is_empty() {
            anyhow::bail!("Upstream proxy host is required");
        }
        upstream_proxy_port_from_form(form)?;
        if form.upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password
            && form.upstream_proxy_username.trim().is_empty()
        {
            anyhow::bail!("Upstream proxy username is required");
        }
    }
    if form.proxy_command_enabled
        && form.proxy_command.trim().is_empty()
        && form.proxy_command_keychain_id.is_none()
    {
        anyhow::bail!("ProxyCommand value is required");
    }
    Ok(())
}

fn connection_draft_from_form_with_proxy_hop_prefix(
    form: &mut NewConnectionForm,
    proxy_hop_prefix: &mut [NewConnectionProxyHop],
    persist_password_draft: bool,
) -> ConnectionDraft {
    ConnectionDraft {
        name: form.name.clone(),
        host: form.host.clone(),
        port: form.port.clone(),
        username: form.username.clone(),
        auth: auth_draft_from_form(form, persist_password_draft),
        group: form.group.clone(),
        notes: form.notes.clone(),
        color: form.color.clone(),
        icon_background_color: form.icon_background_color.clone(),
        icon: form.icon.clone(),
        tags: form.tags.clone(),
        proxy_hops: proxy_hop_prefix
            .iter_mut()
            .chain(form.proxy_hops.iter_mut())
            .map(proxy_hop_draft_from_form)
            .collect(),
        agent_forwarding: form.agent_forwarding,
        identity_agent: identity_agent_from_form(&form.identity_agent),
        agent_forwarding_socket: form.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: form.legacy_ssh_compatibility,
        ssh_algorithms: form.ssh_algorithms.clone(),
        connect_timeout_seconds: form.connect_timeout_seconds,
        dedicated_new_terminal_connection: form.dedicated_new_terminal_connection,
        ssh_channel_strategy: form.ssh_channel_strategy,
        x11_forwarding: form.x11_forwarding,
        post_connect_command: form.post_connect_command.clone(),
        terminal: form.terminal.clone(),
    }
}

pub(super) fn proxy_hop_draft_from_form(
    hop: &mut super::new_connection::NewConnectionProxyHop,
) -> ProxyHopDraft {
    ProxyHopDraft {
        host: hop.host.clone(),
        port: hop.port.clone(),
        username: hop.username.clone(),
        auth: ConnectionAuthDraft {
            kind: auth_draft_kind(hop.auth_tab),
            gssapi_authentication: hop.gssapi_enabled,
            password: take_secret_from_ui_draft(&mut hop.password),
            key_path: hop.key_path.clone(),
            managed_key_id: hop.managed_key_id.clone(),
            cert_path: hop.cert_path.clone(),
            passphrase: take_secret_from_ui_draft(&mut hop.passphrase),
            gssapi_server_identity: hop.gssapi_server_identity.clone(),
            gssapi_delegate_credentials: hop.gssapi_delegate_credentials,
            save_password: true,
            ..ConnectionAuthDraft::default()
        },
        agent_forwarding: hop.agent_forwarding,
        identity_agent: identity_agent_from_form(&hop.identity_agent),
        agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
        ssh_algorithms: hop.ssh_algorithms.clone(),
    }
}

pub(super) fn auth_draft_from_form(
    form: &mut NewConnectionForm,
    persist_password_draft: bool,
) -> ConnectionAuthDraft {
    ConnectionAuthDraft {
        kind: auth_draft_kind(form.auth_tab),
        gssapi_authentication: form.gssapi_enabled,
        password: if form.auth_tab == SshAuthTab::Password && persist_password_draft {
            take_secret_from_ui_draft(&mut form.password)
        } else {
            SecretString::default()
        },
        password_keychain_id: form.saved_password_keychain_id.clone(),
        password_loaded: form.password_loaded,
        save_password: form.save_password,
        key_path: form.key_path.clone(),
        managed_key_id: form.managed_key_id.clone(),
        cert_path: form.cert_path.clone(),
        passphrase: take_secret_from_ui_draft(&mut form.passphrase),
        gssapi_server_identity: form.gssapi_server_identity.clone(),
        gssapi_delegate_credentials: form.gssapi_delegate_credentials,
    }
}

pub(super) fn take_secret_from_ui_draft(value: &mut String) -> SecretString {
    // Move the existing allocation into a zeroizing owner at the persistence boundary.
    SecretString::from(std::mem::take(value))
}

fn saved_proxy_command_from_form(form: &mut NewConnectionForm) -> Option<SavedProxyCommand> {
    form.proxy_command_enabled.then(|| SavedProxyCommand {
        keychain_id: form.proxy_command_keychain_id.clone(),
        // An empty edit draft retains the existing protected value without loading it into UI.
        plaintext_command: (!form.proxy_command.trim().is_empty())
            .then(|| take_secret_from_ui_draft(&mut form.proxy_command)),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RuntimeSecretHandoff {
    Move,
    CopyForTest,
}

impl RuntimeSecretHandoff {
    pub(in crate::workspace) fn zeroizing(self, value: &mut String) -> zeroize::Zeroizing<String> {
        match self {
            Self::Move => zeroize::Zeroizing::new(std::mem::take(value)),
            // A connection test must leave the form reusable. The bounded test
            // copy remains zeroizing and is erased when the test config drops.
            Self::CopyForTest => zeroize::Zeroizing::new(value.clone()),
        }
    }

    pub(in crate::workspace) fn zeroizing_non_empty(
        self,
        value: &mut String,
    ) -> Option<zeroize::Zeroizing<String>> {
        (!value.is_empty()).then(|| self.zeroizing(value))
    }
}

pub(in crate::workspace) fn saved_upstream_proxy_policy_from_form(
    form: &mut NewConnectionForm,
) -> anyhow::Result<SavedUpstreamProxyPolicy> {
    match form.upstream_proxy_policy {
        NewConnectionUpstreamProxyPolicy::UseGlobal => Ok(SavedUpstreamProxyPolicy::UseGlobal),
        NewConnectionUpstreamProxyPolicy::Direct => Ok(SavedUpstreamProxyPolicy::Direct),
        NewConnectionUpstreamProxyPolicy::Custom => Ok(SavedUpstreamProxyPolicy::Custom {
            proxy: saved_upstream_proxy_config_from_form(form)?,
        }),
    }
}

pub(super) fn saved_upstream_proxy_config_from_form(
    form: &mut NewConnectionForm,
) -> anyhow::Result<SavedUpstreamProxyConfig> {
    Ok(SavedUpstreamProxyConfig {
        protocol: form.upstream_proxy_protocol,
        host: form.upstream_proxy_host.trim().to_string(),
        port: upstream_proxy_port_from_form(form)?,
        auth: saved_upstream_proxy_auth_from_form(form),
        remote_dns: form.upstream_proxy_remote_dns,
        no_proxy: form.upstream_proxy_no_proxy.trim().to_string(),
    })
}

pub(super) fn saved_upstream_proxy_auth_from_form(
    form: &mut NewConnectionForm,
) -> SavedUpstreamProxyAuth {
    match form.upstream_proxy_auth {
        NewConnectionUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
        NewConnectionUpstreamProxyAuth::Password => SavedUpstreamProxyAuth::Password {
            username: form.upstream_proxy_username.trim().to_string(),
            keychain_id: form.upstream_proxy_password_keychain_id.clone(),
            // Only a visible draft secret crosses into persistence when the
            // user typed one; otherwise an existing keychain id remains intact.
            plaintext_password: (!form.upstream_proxy_password.is_empty())
                .then(|| take_secret_from_ui_draft(&mut form.upstream_proxy_password)),
        },
    }
}

pub(super) fn upstream_proxy_port_from_form(form: &NewConnectionForm) -> anyhow::Result<u16> {
    let port = form.upstream_proxy_port.trim().parse::<u16>()?;
    Ok(port.max(1))
}

pub(in crate::workspace) fn upstream_proxy_config_from_form(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    form: &mut NewConnectionForm,
    secret_handoff: RuntimeSecretHandoff,
) -> anyhow::Result<Option<UpstreamProxyConfig>> {
    match form.upstream_proxy_policy {
        NewConnectionUpstreamProxyPolicy::UseGlobal => upstream_proxy_config_from_saved_policy(
            store,
            settings,
            &SavedUpstreamProxyPolicy::UseGlobal,
        )
        .map_err(anyhow::Error::msg),
        NewConnectionUpstreamProxyPolicy::Direct => Ok(None),
        NewConnectionUpstreamProxyPolicy::Custom => Ok(Some(
            runtime_upstream_proxy_config_from_form(store, form, secret_handoff)?,
        )),
    }
}

pub(super) fn runtime_upstream_proxy_config_from_form(
    store: &ConnectionStore,
    form: &mut NewConnectionForm,
    secret_handoff: RuntimeSecretHandoff,
) -> anyhow::Result<UpstreamProxyConfig> {
    // Parse the non-secret port before taking ownership of a visible password draft.
    if form.upstream_proxy_host.trim().is_empty() {
        anyhow::bail!("Upstream proxy host is required");
    }
    let port = upstream_proxy_port_from_form(form)?;
    let auth = match form.upstream_proxy_auth {
        NewConnectionUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        NewConnectionUpstreamProxyAuth::Password => {
            let username = form.upstream_proxy_username.trim().to_string();
            if username.is_empty() {
                anyhow::bail!("Upstream proxy username is required");
            }
            let password = if form.upstream_proxy_password.is_empty() {
                let saved_auth = SavedUpstreamProxyAuth::Password {
                    username: username.clone(),
                    keychain_id: form.upstream_proxy_password_keychain_id.clone(),
                    plaintext_password: None,
                };
                store
                    .get_saved_upstream_proxy_password(&saved_auth)?
                    .into_zeroizing()
            } else {
                secret_handoff.zeroizing(&mut form.upstream_proxy_password)
            };
            UpstreamProxyAuth::Password { username, password }
        }
    };

    Ok(UpstreamProxyConfig {
        protocol: match form.upstream_proxy_protocol {
            SavedUpstreamProxyProtocol::Socks5 => UpstreamProxyProtocol::Socks5,
            SavedUpstreamProxyProtocol::HttpConnect => UpstreamProxyProtocol::HttpConnect,
        },
        host: form.upstream_proxy_host.trim().to_string(),
        port,
        auth,
        remote_dns: form.upstream_proxy_remote_dns,
        no_proxy: form.upstream_proxy_no_proxy.trim().to_string(),
    })
}

pub(super) fn auth_draft_kind(tab: SshAuthTab) -> ConnectionAuthDraftKind {
    match tab {
        SshAuthTab::Password => ConnectionAuthDraftKind::Password,
        SshAuthTab::DefaultKey => ConnectionAuthDraftKind::DefaultKey,
        SshAuthTab::SshKey => ConnectionAuthDraftKind::SshKey,
        SshAuthTab::ManagedKey => ConnectionAuthDraftKind::ManagedKey,
        SshAuthTab::Certificate => ConnectionAuthDraftKind::Certificate,
        SshAuthTab::Agent => ConnectionAuthDraftKind::Agent,
        SshAuthTab::TwoFactor => ConnectionAuthDraftKind::TwoFactor,
    }
}

pub(super) fn group_label_for_form(group: Option<&str>) -> String {
    group.unwrap_or_default().to_string()
}
