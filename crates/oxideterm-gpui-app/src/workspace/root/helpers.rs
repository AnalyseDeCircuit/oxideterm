fn tab_background_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::Sftp => "sftp",
        TabKind::Ide => "ide",
        TabKind::Forwards => "forwards",
        TabKind::SessionManager => "session_manager",
        TabKind::Settings => "settings",
    }
}

fn terminal_background_fit(fit: BackgroundFit) -> TerminalBackgroundFit {
    match fit {
        BackgroundFit::Cover => TerminalBackgroundFit::Cover,
        BackgroundFit::Contain => TerminalBackgroundFit::Contain,
        BackgroundFit::Fill => TerminalBackgroundFit::Fill,
        BackgroundFit::Tile => TerminalBackgroundFit::Tile,
    }
}

fn sftp_runtime_settings_from_settings(
    settings: &PersistedSettings,
) -> SftpTransferRuntimeSettings {
    SftpTransferRuntimeSettings {
        max_concurrent_transfers: settings.sftp.max_concurrent_transfers.max(1) as usize,
        speed_limit_kbps: if settings.sftp.speed_limit_enabled {
            settings.sftp.speed_limit_kbps.max(0) as usize
        } else {
            0
        },
        directory_parallelism: settings.sftp.directory_parallelism.max(1) as usize,
    }
}

fn session_terminal_encoding(encoding: SettingsTerminalEncoding) -> SessionTerminalEncoding {
    match encoding {
        SettingsTerminalEncoding::Utf8 => SessionTerminalEncoding::Utf8,
        SettingsTerminalEncoding::Gbk => SessionTerminalEncoding::Gbk,
        SettingsTerminalEncoding::Gb18030 => SessionTerminalEncoding::Gb18030,
        SettingsTerminalEncoding::Big5 => SessionTerminalEncoding::Big5,
        SettingsTerminalEncoding::ShiftJis => SessionTerminalEncoding::ShiftJis,
        SettingsTerminalEncoding::EucJp => SessionTerminalEncoding::EucJp,
        SettingsTerminalEncoding::EucKr => SessionTerminalEncoding::EucKr,
        SettingsTerminalEncoding::Windows1252 => SessionTerminalEncoding::Windows1252,
    }
}

fn locale_from_settings(language: Language) -> Locale {
    match language {
        Language::De => Locale::De,
        Language::En => Locale::En,
        Language::EsEs => Locale::EsEs,
        Language::FrFr => Locale::FrFr,
        Language::It => Locale::It,
        Language::Ja => Locale::Ja,
        Language::Ko => Locale::Ko,
        Language::PtBr => Locale::PtBr,
        Language::Vi => Locale::Vi,
        Language::ZhCn => Locale::ZhCn,
        Language::ZhTw => Locale::ZhTw,
    }
}

fn settings_language_from_locale(locale: Locale) -> Language {
    match locale {
        Locale::De => Language::De,
        Locale::En => Language::En,
        Locale::EsEs => Language::EsEs,
        Locale::FrFr => Language::FrFr,
        Locale::It => Language::It,
        Locale::Ja => Language::Ja,
        Locale::Ko => Language::Ko,
        Locale::PtBr => Language::PtBr,
        Locale::Vi => Language::Vi,
        Locale::ZhCn => Language::ZhCn,
        Locale::ZhTw => Language::ZhTw,
    }
}

fn tokens_from_settings(settings: &PersistedSettings) -> ThemeTokens {
    let mut tokens = ThemeTokens::from_builtin(theme_by_id(&settings.terminal.theme));
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: 2.0_f32.min(radius.max(1.0)),
    };
    tokens
}

fn native_vibrancy_mode(mode: FrostedGlassMode) -> NativeVibrancyMode {
    match mode {
        FrostedGlassMode::Off | FrostedGlassMode::Css => NativeVibrancyMode::Off,
        FrostedGlassMode::Native | FrostedGlassMode::System => NativeVibrancyMode::System,
        FrostedGlassMode::Mica => NativeVibrancyMode::Mica,
        FrostedGlassMode::Acrylic => NativeVibrancyMode::Acrylic,
    }
}

fn effective_vibrancy_mode(
    settings: &PersistedSettings,
    policy: &EffectiveRenderPolicy,
) -> NativeVibrancyMode {
    if policy.allow_vibrancy {
        native_vibrancy_mode(settings.appearance.frosted_glass)
    } else {
        NativeVibrancyMode::Off
    }
}

fn render_profile_from_env() -> Option<RenderProfile> {
    let value = std::env::var("OXIDETERM_RENDER_PROFILE").ok()?;
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" => Some(RenderProfile::Auto),
        "quality" | "high-quality" | "high" => Some(RenderProfile::Quality),
        "low-power" | "lowpower" | "low" => Some(RenderProfile::LowPower),
        "compatibility" | "compat" | "safe" | "safe-mode" => Some(RenderProfile::Compatibility),
        _ => None,
    }
}

fn workspace_background(tokens: &ThemeTokens, mode: NativeVibrancyMode) -> Rgba {
    match mode {
        NativeVibrancyMode::Off => rgb(tokens.ui.bg),
        NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
            rgba((tokens.ui.bg << 8) | alpha_byte(tokens.metrics.window_vibrancy_tint_alpha))
        }
    }
}

fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

fn settings_mono_font_family(settings: &PersistedSettings) -> SharedString {
    let family = settings
        .terminal
        .font_family
        .terminal_family_name(&settings.terminal.custom_font_family);
    settings_css_font_family_head(&family).unwrap_or_else(|| gpui_font_family_name(&family))
}

impl WorkspaceApp {
    fn restore_session_tree_snapshot(&mut self) {
        let path = default_session_tree_path();
        let Ok(bytes) = fs::read(&path) else {
            return;
        };
        let Ok(persisted) = serde_json::from_slice::<PersistedNodeTreeSnapshot>(&bytes) else {
            eprintln!("failed to parse session tree snapshot: {}", path.display());
            return;
        };
        let mut restored_nodes = Vec::new();
        let mut restored_ids = HashSet::new();

        for node in persisted.nodes {
            let config = node
                .config
                .or_else(|| saved_origin_config(&self.connection_store, &node.origin));
            let Some(config) = config else {
                continue;
            };
            restored_ids.insert(node.id.clone());
            restored_nodes.push(NodeTreeSnapshotNode {
                id: node.id,
                parent_id: node.parent_id,
                children_ids: node.children_ids,
                depth: node.depth,
                config,
                origin: node.origin,
                state: NodeState::default(),
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                created_at_ms: node.created_at_ms,
                generation: node.generation,
            });
        }

        let restored_roots = persisted
            .root_ids
            .into_iter()
            .filter(|id| restored_ids.contains(id))
            .collect::<Vec<_>>();
        let snapshot = NodeTreeSnapshot {
            version: persisted.version,
            exported_at_ms: persisted.exported_at_ms,
            root_ids: restored_roots,
            nodes: restored_nodes,
        };
        if let Err(error) = self.node_router.apply_tree_snapshot(snapshot.clone()) {
            eprintln!("failed to restore session tree snapshot: {error}");
            return;
        }

        // Rebuild the UI-facing node cache from the SessionTree owner. Runtime
        // ids are deliberately cleared above: after process restart, Tauri also
        // needs reconnect/connect_tree_node to create fresh SSH/SFTP/terminal
        // owners instead of trusting stale ids from disk.
        let mut saved_targets: HashMap<String, (u32, NodeId)> = HashMap::new();
        for node in snapshot.nodes {
            let title = node
                .origin
                .saved_connection_id()
                .and_then(|id| self.connection_store.get(id))
                .map(|connection| connection.name.clone())
                .unwrap_or_else(|| format!("{}@{}", node.config.username, node.config.host));
            if let Some(saved_connection_id) = node.origin.saved_connection_id() {
                let rank = restored_saved_node_rank(&node.origin);
                let entry = saved_targets
                    .entry(saved_connection_id.to_string())
                    .or_insert((rank, node.id.clone()));
                if rank >= entry.0 {
                    *entry = (rank, node.id.clone());
                }
            }
            self.ssh_nodes.insert(
                node.id,
                WorkspaceSshNode {
                    saved_connection_id: node.origin.saved_connection_id().map(str::to_string),
                    config: node.config,
                    title,
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
        for (saved_connection_id, (_, node_id)) in saved_targets {
            self.saved_ssh_nodes.insert(saved_connection_id, node_id);
        }
    }

    fn persist_session_tree_snapshot(&self) {
        let runtime = self.node_router.export_tree_snapshot();
        let nodes = runtime
            .nodes
            .into_iter()
            .filter_map(|node| {
                let config = persistable_session_tree_config(&node);
                if config.is_none() && node.origin.saved_connection_id().is_none() {
                    return None;
                }
                Some(PersistedNodeTreeNode {
                    id: node.id,
                    parent_id: node.parent_id,
                    children_ids: node.children_ids,
                    depth: node.depth,
                    origin: node.origin,
                    config,
                    created_at_ms: node.created_at_ms,
                    generation: node.generation,
                })
            })
            .collect::<Vec<_>>();
        let retained_ids = nodes.iter().map(|node| node.id.clone()).collect::<HashSet<_>>();
        let persisted = PersistedNodeTreeSnapshot {
            version: runtime.version,
            exported_at_ms: runtime.exported_at_ms,
            root_ids: runtime
                .root_ids
                .into_iter()
                .filter(|id| retained_ids.contains(id))
                .collect(),
            nodes,
        };
        let path = default_session_tree_path();
        if let Err(error) = write_session_tree_snapshot(&path, &persisted) {
            eprintln!("failed to persist session tree snapshot: {error}");
        }
    }
}

fn restored_saved_node_rank(origin: &NodeOrigin) -> u32 {
    match origin {
        NodeOrigin::ManualPreset { hop_index, .. } => *hop_index,
        NodeOrigin::Restored { .. } => u32::MAX,
        NodeOrigin::AutoRoute { hop_index, .. } => *hop_index,
        NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => 0,
    }
}

fn write_session_tree_snapshot(path: &PathBuf, snapshot: &PersistedNodeTreeSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn persistable_session_tree_config(node: &NodeTreeSnapshotNode) -> Option<SshConfig> {
    if node.origin.saved_connection_id().is_some() {
        return None;
    }
    config_without_runtime_secret(&node.config).then(|| node.config.clone())
}

fn saved_origin_config(store: &ConnectionStore, origin: &NodeOrigin) -> Option<SshConfig> {
    match origin {
        NodeOrigin::Restored {
            saved_connection_id,
        } => {
            let connection = store.get(saved_connection_id)?;
            self::session_manager::ssh_config_from_saved_connection(store, connection)
        }
        NodeOrigin::ManualPreset {
            saved_connection_id,
            hop_index,
        } => {
            let connection = store.get(saved_connection_id)?;
            saved_manual_preset_hop_config(store, connection, *hop_index)
        }
        NodeOrigin::AutoRoute { .. } | NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => None,
    }
}

fn saved_manual_preset_hop_config(
    store: &ConnectionStore,
    connection: &oxideterm_connections::SavedConnection,
    hop_index: u32,
) -> Option<SshConfig> {
    let hop_index = hop_index as usize;
    if hop_index < connection.proxy_chain.len() {
        let hop = &connection.proxy_chain[hop_index];
        return Some(SshConfig {
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth: self::session_manager::auth_method_from_saved_auth(store, &hop.auth)?,
            proxy_chain: None,
            agent_forwarding: hop.agent_forwarding,
            strict_host_key_checking: true,
            ..SshConfig::default()
        });
    }

    if hop_index == connection.proxy_chain.len() {
        let mut target =
            self::session_manager::ssh_config_from_saved_connection(store, connection)?;
        target.proxy_chain = None;
        return Some(target);
    }

    None
}

fn config_without_runtime_secret(config: &SshConfig) -> bool {
    auth_without_runtime_secret(&config.auth)
        && config.proxy_chain.as_ref().is_none_or(|chain| {
            chain
                .iter()
                .all(|hop| auth_without_runtime_secret(&hop.auth))
        })
}

fn auth_without_runtime_secret(auth: &AuthMethod) -> bool {
    match auth {
        AuthMethod::Password { .. } => false,
        AuthMethod::Key { passphrase, .. } | AuthMethod::Certificate { passphrase, .. } => {
            passphrase.is_none()
        }
        AuthMethod::Agent | AuthMethod::KeyboardInteractive => true,
    }
}
