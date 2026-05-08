fn auth_label(auth_type: AuthType) -> String {
    match auth_type {
        AuthType::Password => "Password",
        AuthType::Key => "Key",
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

fn connections_deleted_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.toast.connections_deleted")
        .replace("{{count}}", &count.to_string())
}

fn connections_moved_label(i18n: &I18n, count: usize, group: String) -> String {
    i18n.t("sessionManager.toast.connections_moved")
        .replace("{{count}}", &count.to_string())
        .replace("{{group}}", &group)
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
    let (auth_tab, password, key_path, cert_path, passphrase, save_password) = match &conn.auth {
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
            cert_path.clone(),
            String::new(),
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
        ),
        SavedAuth::Agent => (
            SshAuthTab::Agent,
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
        cert_path,
        passphrase,
        save_password,
        group: group_label_for_form(conn.group.as_deref()),
        color: conn.color.clone().unwrap_or_default(),
        tags: conn.tags.clone(),
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
    }
}

fn proxy_hop_draft_from_form(hop: &super::new_connection::NewConnectionProxyHop) -> ProxyHopDraft {
    ProxyHopDraft {
        host: hop.host.clone(),
        port: hop.port.clone(),
        username: hop.username.clone(),
        auth: ConnectionAuthDraft {
            kind: auth_draft_kind(hop.auth_tab),
            password: SecretString::from(hop.password.clone()),
            key_path: hop.key_path.clone(),
            cert_path: hop.cert_path.clone(),
            passphrase: SecretString::from(hop.passphrase.clone()),
            save_password: true,
            ..ConnectionAuthDraft::default()
        },
        agent_forwarding: hop.agent_forwarding,
    }
}

fn auth_draft_from_form(form: &NewConnectionForm) -> ConnectionAuthDraft {
    ConnectionAuthDraft {
        kind: auth_draft_kind(form.auth_tab),
        password: SecretString::from(form.password.clone()),
        password_keychain_id: form.saved_password_keychain_id.clone(),
        password_loaded: form.password_loaded,
        save_password: form.save_password,
        key_path: form.key_path.clone(),
        cert_path: form.cert_path.clone(),
        passphrase: SecretString::from(form.passphrase.clone()),
    }
}

fn auth_draft_kind(tab: SshAuthTab) -> ConnectionAuthDraftKind {
    match tab {
        SshAuthTab::Password => ConnectionAuthDraftKind::Password,
        SshAuthTab::DefaultKey => ConnectionAuthDraftKind::DefaultKey,
        SshAuthTab::SshKey => ConnectionAuthDraftKind::SshKey,
        SshAuthTab::Certificate => ConnectionAuthDraftKind::Certificate,
        SshAuthTab::Agent => ConnectionAuthDraftKind::Agent,
        SshAuthTab::TwoFactor => ConnectionAuthDraftKind::TwoFactor,
    }
}

pub(super) fn ssh_config_from_saved_connection(
    store: &ConnectionStore,
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
        agent_forwarding: conn.options.agent_forwarding,
        strict_host_key_checking: true,
        ..SshConfig::default()
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
        SavedAuth::Agent => AuthMethod::Agent,
    })
}

fn group_label_for_form(group: Option<&str>) -> String {
    group.unwrap_or_default().to_string()
}
