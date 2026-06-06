fn auth_label(auth_type: AuthType) -> String {
    match auth_type {
        AuthType::Password => "Password",
        AuthType::Key => "Key",
        AuthType::ManagedKey => "Managed Key",
        AuthType::Certificate => "Certificate",
        AuthType::Agent => "Agent",
    }
    .to_string()
}

fn add_group_path_segments(group: &str, paths: &mut HashSet<String>) {
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

fn expand_group_path(group: &str, expanded_groups: &mut HashSet<String>) {
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

fn auth_badge_style(
    auth_type: AuthType,
    text_muted: u32,
    text: u32,
) -> (LucideIcon, &'static str, Rgba, Rgba) {
    match auth_type {
        AuthType::Key => (LucideIcon::Key, "Key", rgba(0x10b98133), rgb(0x6ee7b7)),
        AuthType::ManagedKey => (
            LucideIcon::Key,
            "Managed",
            rgba(0x10b98133),
            rgb(0x6ee7b7),
        ),
        AuthType::Password => (LucideIcon::Lock, "Pwd", rgba(0xf59e0b33), rgb(0xfcd34d)),
        AuthType::Agent => (LucideIcon::Bot, "Agent", rgba(0x3b82f633), rgb(0x93c5fd)),
        AuthType::Certificate => (
            LucideIcon::ShieldQuestion,
            "certificate",
            rgba((text_muted << 8) | 0x33),
            rgb(text),
        ),
    }
}

fn auth_badge_width(label: &str) -> f32 {
    MANAGER_AUTH_BADGE_PADDING_X * 2.0
        + MANAGER_AUTH_BADGE_ICON_SIZE
        + MANAGER_AUTH_BADGE_GAP
        + label.chars().count() as f32 * MANAGER_AUTH_BADGE_CHAR_WIDTH
}

fn format_last_used(last_used: Option<&str>, i18n: &I18n) -> String {
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

fn theme_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_THEME_ALPHA)
}

fn theme_panel_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

fn theme_secondary_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

fn theme_active_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_THEME_ALPHA)
}

fn theme_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_HOVER_ALPHA)
}

fn theme_input_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_THEME_ALPHA / 2, 0x80)
}

fn theme_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, BG_ACTIVE_BORDER_ALPHA)
}

fn theme_border_half(color: u32, has_background: bool) -> Rgba {
    color_for_background_or_alpha(color, has_background, BG_ACTIVE_BORDER_HALF_ALPHA, 0x80)
}

fn parse_hex_color(value: &str) -> Option<u32> {
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

fn group_label(i18n: &I18n, group: Option<&str>) -> String {
    group
        .filter(|group| !group.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n.t("sessionManager.folder_tree.ungrouped"))
}

fn selected_count_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.table.selected_count")
        .replace("{{count}}", &count.to_string())
}

fn confirm_delete_connection_label(i18n: &I18n, name: &str) -> String {
    i18n.t("sessionManager.actions.confirm_delete")
        .replace("{{name}}", name)
}

fn confirm_batch_delete_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.actions.confirm_batch_delete")
        .replace("{{count}}", &count.to_string())
}

fn connections_deleted_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.toast.connections_deleted")
        .replace("{{count}}", &count.to_string())
}

fn duplicate_connection_template_name<'a>(
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

fn duplicate_template_base_name(source_name: &str) -> String {
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

fn connections_moved_label(i18n: &I18n, count: usize, group: String) -> String {
    i18n.t("sessionManager.toast.connections_moved")
        .replace("{{count}}", &count.to_string())
        .replace("{{group}}", &group)
}

fn terminal_serial_parity_from_profile(
    parity: &oxideterm_connections::SerialParity,
) -> oxideterm_terminal::SerialParity {
    match parity {
        oxideterm_connections::SerialParity::None => oxideterm_terminal::SerialParity::None,
        oxideterm_connections::SerialParity::Odd => oxideterm_terminal::SerialParity::Odd,
        oxideterm_connections::SerialParity::Even => oxideterm_terminal::SerialParity::Even,
    }
}

fn terminal_serial_flow_from_profile(
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

fn serial_profile_parity_label(parity: &oxideterm_connections::SerialParity) -> &'static str {
    match parity {
        oxideterm_connections::SerialParity::None => "",
        oxideterm_connections::SerialParity::Odd => "O",
        oxideterm_connections::SerialParity::Even => "E",
    }
}

fn serial_profile_flow_label(flow: &oxideterm_connections::SerialFlowControl) -> &'static str {
    match flow {
        oxideterm_connections::SerialFlowControl::None => "",
        oxideterm_connections::SerialFlowControl::Software => " · XON/XOFF",
        oxideterm_connections::SerialFlowControl::Hardware => " · RTS/CTS",
    }
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

pub(super) fn form_from_saved_connection(
    conn: &SavedConnection,
    error: Option<String>,
) -> NewConnectionForm {
    let (auth_tab, password, key_path, managed_key_id, cert_path, passphrase, save_password) = match &conn.auth {
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
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
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
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
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
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
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
        password_loaded: false,
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
        tags: conn.tags.clone(),
        post_connect_command: conn.post_connect_command().unwrap_or_default().to_string(),
        privilege_credentials: conn.privilege_credentials.clone(),
        agent_forwarding: conn.options.agent_forwarding,
        save_connection: true,
        error,
        ..NewConnectionForm::default()
    }
}

pub(super) fn save_request_from_form(
    form: &NewConnectionForm,
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_form_with_existing_auth(form, id, None)
}

pub(super) fn save_request_from_form_with_existing_auth(
    form: &NewConnectionForm,
    id: Option<String>,
    existing_auth: Option<&SavedAuth>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_draft(connection_draft_from_form(form), id, existing_auth)
}

fn connection_draft_from_form(form: &NewConnectionForm) -> ConnectionDraft {
    ConnectionDraft {
        name: form.name.clone(),
        host: form.host.clone(),
        port: form.port.clone(),
        username: form.username.clone(),
        auth: auth_draft_from_form(form),
        group: form.group.clone(),
        color: form.color.clone(),
        tags: form.tags.clone(),
        proxy_hops: form
            .proxy_hops
            .iter()
            .map(proxy_hop_draft_from_form)
            .collect(),
        agent_forwarding: form.agent_forwarding,
        post_connect_command: form.post_connect_command.clone(),
    }
}

fn proxy_hop_draft_from_form(hop: &super::new_connection::NewConnectionProxyHop) -> ProxyHopDraft {
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
    }
}

fn auth_draft_from_form(form: &NewConnectionForm) -> ConnectionAuthDraft {
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

fn secret_from_ui_draft(value: &str) -> SecretString {
    // GPUI text inputs require plain String drafts. At the persistence boundary,
    // clone into SecretString's Zeroizing owner before any store/keychain logic sees it.
    SecretString::from(zeroize::Zeroizing::new(value.to_string()))
}

fn auth_draft_kind(tab: SshAuthTab) -> ConnectionAuthDraftKind {
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

pub(super) fn ssh_config_from_saved_connection(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
) -> Option<SshConfig> {
    let auth = auth_method_from_saved_auth(store, &conn.auth)?;
    let proxy_chain = proxy_chain_config_from_saved_connection(store, conn)?;
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy: upstream_proxy_config_from_saved_policy(store, settings, &conn.upstream_proxy),
        agent_forwarding: conn.options.agent_forwarding,
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
}

pub(super) fn upstream_proxy_config_from_saved_policy(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    policy: &SavedUpstreamProxyPolicy,
) -> Option<UpstreamProxyConfig> {
    match policy {
        SavedUpstreamProxyPolicy::UseGlobal => settings
            .network
            .upstream_proxy
            .as_ref()
            .and_then(|proxy| upstream_proxy_config_from_global_proxy(store, proxy)),
        SavedUpstreamProxyPolicy::Direct => None,
        SavedUpstreamProxyPolicy::Custom { proxy } => {
            Some(upstream_proxy_config_from_saved_proxy(store, proxy)?)
        }
    }
}

fn upstream_proxy_config_from_global_proxy(
    store: &ConnectionStore,
    proxy: &SettingsUpstreamProxyConfig,
) -> Option<UpstreamProxyConfig> {
    let auth = match &proxy.auth {
        SettingsUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        SettingsUpstreamProxyAuth::Password {
            username,
            keychain_id,
        } => UpstreamProxyAuth::Password {
            username: username.clone(),
            password: store
                .get_global_upstream_proxy_password(keychain_id.as_deref()?)
                .ok()?
                .into_zeroizing(),
        },
    };

    // Global proxy passwords live in the shared keychain slot referenced by
    // settings metadata; only this runtime config owns the hydrated secret.
    Some(UpstreamProxyConfig {
        protocol: match proxy.protocol {
            SettingsUpstreamProxyProtocol::Socks5 => UpstreamProxyProtocol::Socks5,
            SettingsUpstreamProxyProtocol::HttpConnect => UpstreamProxyProtocol::HttpConnect,
        },
        host: proxy.host.clone(),
        port: proxy.port,
        auth,
        remote_dns: proxy.remote_dns,
        no_proxy: proxy.no_proxy.clone(),
    })
}

fn upstream_proxy_config_from_saved_proxy(
    store: &ConnectionStore,
    proxy: &SavedUpstreamProxyConfig,
) -> Option<UpstreamProxyConfig> {
    let auth = match &proxy.auth {
        SavedUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        SavedUpstreamProxyAuth::Password { username, .. } => UpstreamProxyAuth::Password {
            username: username.clone(),
            password: store
                .get_saved_upstream_proxy_password(&proxy.auth)
                .ok()?
                .into_zeroizing(),
        },
    };

    Some(UpstreamProxyConfig {
        protocol: match proxy.protocol {
            SavedUpstreamProxyProtocol::Socks5 => UpstreamProxyProtocol::Socks5,
            SavedUpstreamProxyProtocol::HttpConnect => UpstreamProxyProtocol::HttpConnect,
        },
        host: proxy.host.clone(),
        port: proxy.port,
        auth,
        remote_dns: proxy.remote_dns,
        no_proxy: proxy.no_proxy.clone(),
    })
}

pub(super) fn proxy_chain_config_from_saved_connection(
    store: &ConnectionStore,
    conn: &SavedConnection,
) -> Option<Vec<ProxyHopConfig>> {
    conn.proxy_chain
        .iter()
        .map(|hop| {
            Some(ProxyHopConfig {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: auth_method_from_saved_auth(store, &hop.auth)?,
                agent_forwarding: hop.agent_forwarding,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
        })
        .collect()
}

pub(super) fn auth_method_from_saved_auth(
    store: &ConnectionStore,
    auth: &SavedAuth,
) -> Option<AuthMethod> {
    Some(match auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => AuthMethod::password_secret(password.clone().into_zeroizing()),
        SavedAuth::Password {
            keychain_id: Some(_),
            ..
        } => {
            AuthMethod::password_secret(store.get_saved_auth_password(auth).ok()?.into_zeroizing())
        }
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        } => return None,
        SavedAuth::Key {
            key_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::key_secret(
            key_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::certificate_secret(
            key_path.clone(),
            cert_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        SavedAuth::ManagedKey {
            key_id,
            passphrase_keychain_id,
            ..
        } => AuthMethod::managed_key_secret(
            key_id.clone(),
            passphrase_keychain_id
                .as_ref()
                .and_then(|_| store.get_saved_auth_passphrase(auth).ok().flatten())
                .map(SecretString::into_zeroizing),
        ),
        SavedAuth::Agent => AuthMethod::Agent,
    })
}

pub(super) fn managed_key_resolver_from_store(store: &ConnectionStore) -> ManagedKeyResolver {
    let store = store.clone();
    Arc::new(move |key_id| {
        store
            .resolve_managed_ssh_key_private_key(key_id)
            .map(SecretString::into_zeroizing)
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))
    })
}

fn group_label_for_form(group: Option<&str>) -> String {
    group.unwrap_or_default().to_string()
}
