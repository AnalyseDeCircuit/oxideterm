impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let settings_store = SettingsStore::load_default()?;
        let connection_store = ConnectionStore::load(default_connections_path())?;
        let settings = settings_store.settings().clone();
        let local_shells = scan_shells();
        let tokens = tokens_from_settings(&settings);
        let detected_graphics = detect_graphics(window);
        let render_profile_override = render_profile_from_env();
        let render_policy = compute_render_policy(
            render_profile_override.unwrap_or(settings.appearance.render_profile),
            &detected_graphics,
        );
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_secs(
                settings.connection_pool.idle_timeout_secs as u64,
            )),
            ..ConnectionPoolConfig::default()
        });
        let (forwarding_event_tx, forwarding_event_rx) = std::sync::mpsc::channel();
        let forwarding_registry = match SavedForwardStore::load(default_saved_forwards_path()) {
            Ok(store) => {
                ForwardingRegistry::new_with_event_sender_and_store(forwarding_event_tx, store)
            }
            Err(error) => {
                eprintln!("failed to load saved forwards store: {error}");
                ForwardingRegistry::new_with_event_sender(forwarding_event_tx)
            }
        };
        // Mirror Tauri's split between SessionTree runtime state and NodeRouter:
        // the router resolves capabilities from this shared node runtime store
        // instead of owning the node lifecycle itself.
        let node_runtime_store = NodeRuntimeStore::default();
        let node_router =
            NodeRouter::with_runtime_store(ssh_registry.clone(), node_runtime_store.clone());
        let (ssh_worker_tx, ssh_worker_rx) = std::sync::mpsc::channel();
        let (forwarding_worker_tx, forwarding_worker_rx) = std::sync::mpsc::channel();
        let (node_event_tx, node_event_rx) = std::sync::mpsc::channel();
        node_router.emitter().subscribe(node_event_tx.clone());
        let (reconnect_worker_tx, reconnect_worker_rx) = std::sync::mpsc::channel();
        let (sftp_worker_tx, sftp_worker_rx) = std::sync::mpsc::channel();
        let (terminal_notice_tx, terminal_notice_rx) = std::sync::mpsc::channel();
        let sftp_transfer_manager = Arc::new(SftpTransferManager::new());
        sftp_transfer_manager.apply_settings(sftp_runtime_settings_from_settings(&settings));
        let sftp_progress_store: Arc<dyn ProgressStore> = {
            let path = default_settings_path()
                .parent()
                .map(|parent| parent.join("sftp_progress.redb"))
                .unwrap_or_else(|| std::path::PathBuf::from("sftp_progress.redb"));
            match RedbProgressStore::new(path) {
                Ok(store) => Arc::new(store),
                Err(error) => {
                    eprintln!("failed to load SFTP progress store: {error}");
                    Arc::new(DummyProgressStore)
                }
            }
        };
        let forwarding_runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("oxideterm-forwarding")
                .build()?,
        );
        let initial_vibrancy_mode = effective_vibrancy_mode(&settings, &render_policy);
        let mut background_image_cache = BackgroundImageRenderCache::default();
        background_image_cache.set_byte_limit(render_policy.image_cache_bytes);
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            panes: HashMap::new(),
            tab_scroll_x: 0.0,
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            search: SearchBarState::default(),
            terminal_command_bar_focused: false,
            terminal_command_bar_draft: String::new(),
            split_drag: None,
            sidebar_resizing: false,
            sidebar_collapsed: settings.sidebar_ui.collapsed,
            sidebar_width: settings.sidebar_ui.width as f32,
            needs_active_pane_focus: false,
            active_sidebar_section: SidebarSection::from_settings_key(
                &settings.sidebar_ui.active_section,
            ),
            active_surface: ActiveSurface::Terminal,
            active_settings_tab: SettingsTab::General,
            terminal_settings_page: TerminalSettingsPage::Display,
            open_settings_select: None,
            select_anchors: HashMap::new(),
            text_input_anchors: HashMap::new(),
            ime_marked_text: None,
            focused_settings_input: None,
            settings_input_draft: String::new(),
            settings_slider_drag: None,
            background_blur_preview: None,
            background_blur_commit_generation: 0,
            background_cache_poll_scheduled: false,
            new_connection_form: None,
            drill_down_parent_node_id: None,
            editing_saved_connection_id: None,
            saved_connection_prompt_action: None,
            open_new_connection_select: None,
            new_connection_caret_visible: true,
            host_key_challenge: None,
            keyboard_interactive_challenge: None,
            ssh_worker_tx,
            ssh_worker_rx,
            ssh_registry,
            forwarding_registry,
            forwarding_runtime,
            forwarding_connection_consumers: HashMap::new(),
            sftp_connection_consumers: HashMap::new(),
            sftp_transfer_manager,
            sftp_progress_store,
            node_runtime_store,
            node_router,
            node_event_tx,
            node_event_rx,
            node_event_generations: HashMap::new(),
            reconnect_orchestrator: ReconnectOrchestratorStore::default(),
            reconnect_worker_tx,
            reconnect_worker_rx,
            pending_reconnect_transfer_resumes: HashMap::new(),
            reconnect_transfer_resume_totals: HashMap::new(),
            reconnect_forward_restore_totals: HashMap::new(),
            terminal_endpoint_sessions: HashMap::new(),
            ssh_nodes: HashMap::new(),
            saved_ssh_nodes: HashMap::new(),
            terminal_ssh_nodes: HashMap::new(),
            expanded_ssh_nodes: HashSet::new(),
            active_ssh_node_id: None,
            next_ssh_node_id: 1,
            forward_tab_nodes: HashMap::new(),
            forwarding_view: forwards::ForwardsViewState::default(),
            forwarding_port_detection_by_node: HashMap::new(),
            forwarding_port_profiler_nodes: HashSet::new(),
            sftp_tab_nodes: HashMap::new(),
            sftp_view_node: None,
            sftp_local_path_memory: HashMap::new(),
            sftp_path_memory: HashMap::new(),
            sftp_remote_home_by_node: HashMap::new(),
            ide_tab_surfaces: HashMap::new(),
            ide_surface_subscriptions: HashMap::new(),
            ide_tab_nodes: HashMap::new(),
            ide_last_closed_at_by_node: HashMap::new(),
            sftp_view: sftp::SftpViewState::default(),
            sftp_worker_tx,
            sftp_worker_rx,
            forwarding_worker_tx,
            forwarding_worker_rx,
            forwarding_event_rx,
            i18n: I18n::new(locale_from_settings(settings.general.language)),
            tokens,
            detected_graphics,
            render_profile_override,
            render_policy,
            applied_vibrancy_mode: initial_vibrancy_mode,
            background_image_cache,
            settings_store,
            connection_store,
            session_manager: SessionManagerState::default(),
            auto_route_modal: AutoRouteModalState::default(),
            settings_connection_new_group: String::new(),
            settings_selected_ssh_hosts: HashSet::new(),
            settings_connection_status: None,
            local_shells,
            terminal_notice_tx,
            terminal_notice_rx,
            workspace_toasts: Vec::new(),
        };
        workspace.restore_session_tree_snapshot();
        let _ = apply_window_vibrancy(window, initial_vibrancy_mode);
        let window_handle = window.window_handle();
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(530)).await;
                // Keep the workspace polling loop tied to the WorkspaceApp entity itself.
                // The window root is gpui_component::Root<WorkspaceApp>, so downcasting a
                // window handle to WorkspaceApp is not valid during startup.
                if cx
                    .update_window(window_handle, |_, window, cx| {
                        weak.update(cx, |workspace, cx| {
                            workspace.poll_ssh_worker_results(window, cx);
                            workspace.poll_node_events(cx);
                            workspace.poll_reconnect_worker_results(window, cx);
                            workspace.poll_sftp_worker_results(cx);
                            workspace.maybe_start_sftp_remote_load(cx);
                            workspace.poll_forwarding_worker_results(cx);
                            workspace.poll_forwarding_events(cx);
                            workspace.sync_ssh_node_lifecycle(cx);
                            workspace.maybe_start_forwards_port_scan(cx);
                            workspace.maybe_refresh_forwards_stats(cx);
                            if workspace.new_connection_form.is_some()
                                || workspace.keyboard_interactive_challenge.is_some()
                                || workspace.focused_settings_input.is_some()
                                || workspace.session_manager.focused_input.is_some()
                                || workspace.sftp_view.focused_input.is_some()
                            {
                                workspace.new_connection_caret_visible =
                                    !workspace.new_connection_caret_visible;
                                cx.notify();
                            } else if !workspace.new_connection_caret_visible {
                                workspace.new_connection_caret_visible = true;
                            }
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let sftp_window_handle = window.window_handle();
        cx.spawn(async move |weak, cx| {
            loop {
                // Tauri fires nodeSftpListDir immediately from the SFTP path effect. Keep
                // native SFTP navigation off the slower caret/lifecycle loop so folder
                // changes do not wait for the 530ms workspace tick.
                Timer::after(Duration::from_millis(60)).await;
                if cx
                    .update_window(sftp_window_handle, |_, _window, cx| {
                        weak.update(cx, |workspace, cx| {
                            workspace.poll_sftp_worker_results(cx);
                            workspace.maybe_start_sftp_remote_load(cx);
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Ok(workspace)
    }

    pub(crate) fn terminal_preferences_for_tab_kind(
        &self,
        kind: &TabKind,
    ) -> TerminalUiPreferences {
        self.terminal_preferences_for_background_key(tab_background_key(kind))
    }

    pub(crate) fn terminal_preferences_for_pane(&self, pane_id: PaneId) -> TerminalUiPreferences {
        let key = self
            .tabs
            .iter()
            .find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .is_some_and(|root| root.contains_pane(pane_id))
                    .then_some(tab_background_key(&tab.kind))
            })
            .unwrap_or("local_terminal");
        self.terminal_preferences_for_background_key(key)
    }

    fn terminal_preferences_for_background_key(
        &self,
        background_key: &str,
    ) -> TerminalUiPreferences {
        let settings = self.settings_store.settings();
        let terminal = &settings.terminal;
        let in_band_transfer = &terminal.in_band_transfer;
        let trzsz_policy =
            (in_band_transfer.enabled && in_band_transfer.provider == "trzsz").then(|| {
                oxideterm_terminal::TrzszTransferPolicy {
                    allow_directory: in_band_transfer.allow_directory,
                    max_chunk_bytes: in_band_transfer.max_chunk_bytes.max(1) as usize,
                    max_file_count: in_band_transfer.max_file_count.max(1) as usize,
                    max_total_bytes: in_band_transfer.max_total_bytes.max(1) as u64,
                }
            });
        TerminalUiPreferences {
            font_family: terminal
                .font_family
                .terminal_family_name(&terminal.custom_font_family),
            font_size: terminal.font_size as f32,
            line_height: terminal.line_height as f32,
            cursor_shape: match terminal.cursor_style {
                SettingsCursorStyle::Block => TerminalCursorShape::Block,
                SettingsCursorStyle::Underline => TerminalCursorShape::Underline,
                SettingsCursorStyle::Bar => TerminalCursorShape::Bar,
            },
            cursor_blink: terminal.cursor_blink,
            scrollback_lines: terminal.scrollback.clamp(500, 20_000) as usize,
            paste_protection: terminal.paste_protection,
            smart_copy: terminal.smart_copy,
            osc52_clipboard: terminal.osc52_clipboard,
            copy_on_select: terminal.copy_on_select,
            middle_click_paste: terminal.middle_click_paste,
            selection_requires_shift: terminal.selection_requires_shift,
            bidi_enabled: terminal.unicode.bidi_enabled,
            terminal_encoding: session_terminal_encoding(terminal.terminal_encoding),
            render_policy: self.render_policy.clone(),
            background: self.terminal_background_preferences(background_key),
            paste_labels: TerminalPasteLabels {
                title_template: self.i18n.t("terminal.paste.title"),
                more_lines_template: self.i18n.t("terminal.paste.more_lines"),
                confirm: self.i18n.t("terminal.paste.confirm"),
                cancel: self.i18n.t("terminal.paste.cancel"),
                paste: self.i18n.t("terminal.paste.paste"),
            },
            trzsz_labels: TerminalTrzszLabels {
                select_upload_directory_title: self
                    .i18n
                    .t("terminal.trzsz.select_upload_directory_title"),
                select_upload_directory_description: self
                    .i18n
                    .t("terminal.trzsz.select_upload_directory_description"),
                select_upload_files_title: self.i18n.t("terminal.trzsz.select_upload_files_title"),
                select_upload_files_description: self
                    .i18n
                    .t("terminal.trzsz.select_upload_files_description"),
                select_download_directory_title: self
                    .i18n
                    .t("terminal.trzsz.select_download_directory_title"),
                select_download_directory_description: self
                    .i18n
                    .t("terminal.trzsz.select_download_directory_description"),
                cancelled_title: self.i18n.t("terminal.trzsz.cancelled_title"),
                cancelled_description: self.i18n.t("terminal.trzsz.cancelled_description"),
                completed_title: self.i18n.t("terminal.trzsz.completed_title"),
                completed_description: self.i18n.t("terminal.trzsz.completed_description"),
                failed_title: self.i18n.t("terminal.trzsz.failed_title"),
                failed_description: self.i18n.t("terminal.trzsz.failed_description"),
                connection_lost_title: self.i18n.t("terminal.trzsz.connection_lost_title"),
                connection_lost_description: self
                    .i18n
                    .t("terminal.trzsz.connection_lost_description"),
                partial_cleanup_title: self.i18n.t("terminal.trzsz.partial_cleanup_title"),
                partial_cleanup_description: self
                    .i18n
                    .t("terminal.trzsz.partial_cleanup_description"),
                version_mismatch_title: self.i18n.t("terminal.trzsz.version_mismatch_title"),
                version_mismatch_description: self
                    .i18n
                    .t("terminal.trzsz.version_mismatch_description"),
                path_invalid_title: self.i18n.t("terminal.trzsz.path_invalid_title"),
                path_invalid_description: self.i18n.t("terminal.trzsz.path_invalid_description"),
                symlink_not_supported_title: self
                    .i18n
                    .t("terminal.trzsz.symlink_not_supported_title"),
                symlink_not_supported_description: self
                    .i18n
                    .t("terminal.trzsz.symlink_not_supported_description"),
                conflict_detected_title: self.i18n.t("terminal.trzsz.conflict_detected_title"),
                conflict_detected_description: self
                    .i18n
                    .t("terminal.trzsz.conflict_detected_description"),
                directory_not_allowed_title: self
                    .i18n
                    .t("terminal.trzsz.directory_not_allowed_title"),
                directory_not_allowed_description: self
                    .i18n
                    .t("terminal.trzsz.directory_not_allowed_description"),
                max_file_count_title: self.i18n.t("terminal.trzsz.max_file_count_title"),
                max_file_count_description: self
                    .i18n
                    .t("terminal.trzsz.max_file_count_description"),
                max_total_bytes_title: self.i18n.t("terminal.trzsz.max_total_bytes_title"),
                max_total_bytes_description: self
                    .i18n
                    .t("terminal.trzsz.max_total_bytes_description"),
                disabled_title: self.i18n.t("terminal.trzsz.disabled_title"),
                disabled_description: self.i18n.t("terminal.trzsz.disabled_description"),
            },
            notice_sink: Some({
                let tx = self.terminal_notice_tx.clone();
                Arc::new(move |notice| {
                    let _ = tx.send(notice);
                })
            }),
            highlight_rules: terminal
                .highlight_rules
                .iter()
                .map(|rule| UiHighlightRule {
                    id: rule.id.clone(),
                    pattern: rule.pattern.clone(),
                    is_regex: rule.is_regex,
                    case_sensitive: rule.case_sensitive,
                    foreground: rule.foreground.clone(),
                    background: rule.background.clone(),
                    render_mode: match rule.render_mode {
                        HighlightRuleRenderMode::Background => {
                            TerminalHighlightRenderMode::Background
                        }
                        HighlightRuleRenderMode::Underline => {
                            TerminalHighlightRenderMode::Underline
                        }
                        HighlightRuleRenderMode::Outline => TerminalHighlightRenderMode::Outline,
                    },
                    enabled: rule.enabled,
                    priority: rule.priority,
                })
                .collect(),
            trzsz_policy,
            theme: TerminalUiTheme::new(
                self.tokens.terminal.background,
                self.tokens.terminal.foreground,
                self.tokens.terminal.cursor,
            ),
        }
    }

    fn terminal_background_preferences(
        &self,
        background_key: &str,
    ) -> Option<TerminalBackgroundPreferences> {
        if !self.render_policy.allow_background_images {
            return None;
        }
        let terminal = &self.settings_store.settings().terminal;
        if !terminal.background_enabled
            || !terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == background_key)
        {
            return None;
        }
        let path = PathBuf::from(terminal.background_image.as_deref()?);
        if !path.exists() {
            return None;
        }
        Some(TerminalBackgroundPreferences {
            path,
            opacity: terminal.background_opacity.clamp(0.0, 1.0) as f32,
            blur: terminal.background_blur.clamp(0, 20) as f32,
            fit: terminal_background_fit(terminal.background_fit),
        })
    }
}
