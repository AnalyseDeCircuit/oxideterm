use super::*;

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

pub(super) fn terminal_serial_parity_from_profile(
    parity: &oxideterm_connections::SerialParity,
) -> oxideterm_terminal::SerialParity {
    match parity {
        oxideterm_connections::SerialParity::None => oxideterm_terminal::SerialParity::None,
        oxideterm_connections::SerialParity::Odd => oxideterm_terminal::SerialParity::Odd,
        oxideterm_connections::SerialParity::Even => oxideterm_terminal::SerialParity::Even,
    }
}

pub(super) fn terminal_serial_flow_from_profile(
    flow: &oxideterm_connections::SerialFlowControl,
) -> oxideterm_terminal::SerialFlowControl {
    match flow {
        oxideterm_connections::SerialFlowControl::None => {
            oxideterm_terminal::SerialFlowControl::None
        }
        oxideterm_connections::SerialFlowControl::Software => {
            oxideterm_terminal::SerialFlowControl::Software
        }
        oxideterm_connections::SerialFlowControl::Hardware => {
            oxideterm_terminal::SerialFlowControl::Hardware
        }
    }
}

pub(in crate::workspace) fn form_from_saved_connection(
    conn: &SavedConnection,
    error: Option<String>,
) -> NewConnectionForm {
    let (auth_tab, password, key_path, managed_key_id, cert_path, passphrase, save_password) =
        match &conn.auth {
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
        };
    let upstream_proxy_form = upstream_proxy_form_fields(&conn.upstream_proxy);
    NewConnectionForm {
        name: conn.name.clone(),
        host: conn.host.clone(),
        port: conn.port.to_string(),
        username: conn.username.clone(),
        auth_tab,
        password,
        saved_password_keychain_id: match &conn.auth {
            SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
            _ => None,
        },
        // Only keychain-backed saved passwords start locked. Other auth modes
        // need an editable password draft if the user switches to password auth.
        password_loaded: !connection_has_unloaded_keychain_password(conn),
        password_visible: false,
        password_loading: false,
        password_error: None,
        key_path,
        managed_key_id,
        cert_path,
        passphrase,
        save_password,
        group: group_label_for_form(conn.group.as_deref()),
        color: conn.color.clone().unwrap_or_default(),
        icon: conn.icon.clone().unwrap_or_default(),
        tags: conn.tags.clone(),
        post_connect_command: conn.post_connect_command().unwrap_or_default().to_string(),
        upstream_proxy_policy: upstream_proxy_form.policy,
        upstream_proxy_protocol: upstream_proxy_form.protocol,
        upstream_proxy_host: upstream_proxy_form.host,
        upstream_proxy_port: upstream_proxy_form.port,
        upstream_proxy_auth: upstream_proxy_form.auth,
        upstream_proxy_username: upstream_proxy_form.username,
        upstream_proxy_password_keychain_id: upstream_proxy_form.password_keychain_id,
        upstream_proxy_remote_dns: upstream_proxy_form.remote_dns,
        upstream_proxy_no_proxy: upstream_proxy_form.no_proxy,
        agent_forwarding: conn.options.agent_forwarding,
        save_connection: true,
        error,
        ..NewConnectionForm::default()
    }
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

pub(in crate::workspace) fn save_request_from_form(
    form: &NewConnectionForm,
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_form_with_existing_auth(form, id, None)
}

pub(in crate::workspace) fn save_request_from_form_with_existing_auth(
    form: &NewConnectionForm,
    id: Option<String>,
    existing_auth: Option<&SavedAuth>,
) -> anyhow::Result<SaveConnectionRequest> {
    let mut request = save_request_from_draft(connection_draft_from_form(form), id, existing_auth)?;
    request.upstream_proxy = saved_upstream_proxy_policy_from_form(form)?;
    Ok(request)
}

pub(super) fn connection_draft_from_form(form: &NewConnectionForm) -> ConnectionDraft {
    ConnectionDraft {
        name: form.name.clone(),
        host: form.host.clone(),
        port: form.port.clone(),
        username: form.username.clone(),
        auth: auth_draft_from_form(form),
        group: form.group.clone(),
        color: form.color.clone(),
        icon: form.icon.clone(),
        tags: form.tags.clone(),
        proxy_hops: form
            .proxy_hops
            .iter()
            .map(proxy_hop_draft_from_form)
            .collect(),
        agent_forwarding: form.agent_forwarding,
        legacy_ssh_compatibility: form.legacy_ssh_compatibility,
        post_connect_command: form.post_connect_command.clone(),
    }
}

pub(super) fn proxy_hop_draft_from_form(
    hop: &super::new_connection::NewConnectionProxyHop,
) -> ProxyHopDraft {
    ProxyHopDraft {
        host: hop.host.clone(),
        port: hop.port.clone(),
        username: hop.username.clone(),
        auth: ConnectionAuthDraft {
            kind: auth_draft_kind(hop.auth_tab),
            password: secret_from_ui_draft(&hop.password),
            key_path: hop.key_path.clone(),
            managed_key_id: hop.managed_key_id.clone(),
            cert_path: hop.cert_path.clone(),
            passphrase: secret_from_ui_draft(&hop.passphrase),
            save_password: true,
            ..ConnectionAuthDraft::default()
        },
        agent_forwarding: hop.agent_forwarding,
        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
    }
}

pub(super) fn auth_draft_from_form(form: &NewConnectionForm) -> ConnectionAuthDraft {
    ConnectionAuthDraft {
        kind: auth_draft_kind(form.auth_tab),
        password: secret_from_ui_draft(&form.password),
        password_keychain_id: form.saved_password_keychain_id.clone(),
        password_loaded: form.password_loaded,
        save_password: form.save_password,
        key_path: form.key_path.clone(),
        managed_key_id: form.managed_key_id.clone(),
        cert_path: form.cert_path.clone(),
        passphrase: secret_from_ui_draft(&form.passphrase),
    }
}

pub(super) fn secret_from_ui_draft(value: &str) -> SecretString {
    // GPUI text inputs require plain String drafts. At the persistence boundary,
    // clone into SecretString's Zeroizing owner before any store/keychain logic sees it.
    SecretString::from(zeroize::Zeroizing::new(value.to_string()))
}

pub(in crate::workspace) fn saved_upstream_proxy_policy_from_form(
    form: &NewConnectionForm,
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
    form: &NewConnectionForm,
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
    form: &NewConnectionForm,
) -> SavedUpstreamProxyAuth {
    match form.upstream_proxy_auth {
        NewConnectionUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
        NewConnectionUpstreamProxyAuth::Password => SavedUpstreamProxyAuth::Password {
            username: form.upstream_proxy_username.trim().to_string(),
            keychain_id: form.upstream_proxy_password_keychain_id.clone(),
            // Only a visible draft secret crosses into persistence when the
            // user typed one; otherwise an existing keychain id remains intact.
            plaintext_password: (!form.upstream_proxy_password.is_empty())
                .then(|| secret_from_ui_draft(&form.upstream_proxy_password)),
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
    form: &NewConnectionForm,
) -> anyhow::Result<Option<UpstreamProxyConfig>> {
    match form.upstream_proxy_policy {
        NewConnectionUpstreamProxyPolicy::UseGlobal => upstream_proxy_config_from_saved_policy(
            store,
            settings,
            &SavedUpstreamProxyPolicy::UseGlobal,
        )
        .map_err(anyhow::Error::msg),
        NewConnectionUpstreamProxyPolicy::Direct => Ok(None),
        NewConnectionUpstreamProxyPolicy::Custom => {
            Ok(Some(runtime_upstream_proxy_config_from_form(store, form)?))
        }
    }
}

pub(super) fn runtime_upstream_proxy_config_from_form(
    store: &ConnectionStore,
    form: &NewConnectionForm,
) -> anyhow::Result<UpstreamProxyConfig> {
    let auth = match form.upstream_proxy_auth {
        NewConnectionUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        NewConnectionUpstreamProxyAuth::Password => {
            let username = form.upstream_proxy_username.trim().to_string();
            let password = if form.upstream_proxy_password.is_empty() {
                let saved_auth = saved_upstream_proxy_auth_from_form(form);
                store
                    .get_saved_upstream_proxy_password(&saved_auth)?
                    .into_zeroizing()
            } else {
                zeroize::Zeroizing::new(form.upstream_proxy_password.clone())
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
        port: upstream_proxy_port_from_form(form)?,
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
