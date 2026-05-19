impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let mut settings_store = SettingsStore::load_default()?;
        settings_store.settings_mut().sidebar_ui.zen_mode = false;
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
        let ai_chat_path = default_ai_conversations_path();
        let (ai_chat_store, ai_chat) = match oxideterm_ai::AiChatPersistenceStore::load(&ai_chat_path)
        {
            Ok((store, state)) => (store, state),
            Err(error) => {
                eprintln!("failed to load AI chat store: {error}");
                return Err(error);
            }
        };
        let ai_rag_data_dir = default_rag_data_dir();
        if let Err(error) = fs::create_dir_all(&ai_rag_data_dir) {
            eprintln!("failed to create AI RAG data directory: {error}");
        }
        let ai_rag_store = Arc::new(oxideterm_ai::RagStore::new(&ai_rag_data_dir)?);
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
        let (connection_trace_tx, connection_trace_rx) = std::sync::mpsc::channel();
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
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
        // The SSH pool idle timer is long-lived backend work, matching Tauri's
        // registry-owned timeout task rather than tying disconnects to a GPUI
        // render/update turn.
        ssh_registry.set_task_runtime(forwarding_runtime.handle().clone());
        let ai_agent_fs = NodeAgentIdeFileSystem::new(
            node_router.clone(),
            crate::workspace::ide::node_agent_mode_from_settings(&settings),
        );
        let ai_key_store = oxideterm_ai::AiProviderKeyStore::new();
        let ai_mcp_registry = oxideterm_ai::McpRegistry::new(ai_key_store.clone());
        let cloud_sync_store = oxideterm_cloud_sync::state::CloudSyncStateStore::load(
            oxideterm_cloud_sync::state::default_cloud_sync_state_path(settings_store.path()),
        )?;
        let initial_vibrancy_mode = effective_vibrancy_mode(&settings, &render_policy);
        let mut background_image_cache = BackgroundImageRenderCache::default();
        background_image_cache.set_byte_limit(render_policy.image_cache_bytes);
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            tab_navigation_history: Vec::new(),
            tab_navigation_index: None,
            tab_navigation_replaying: false,
            tab_navigation_observed_tab: None,
            panes: HashMap::new(),
            tab_scroll_x: 0.0,
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            search: SearchBarState::default(),
            terminal_command_bar_focused: false,
            terminal_command_bar_draft: String::new(),
            terminal_command_suggestions_open: false,
            terminal_command_suggestion_highlighted: None,
            terminal_broadcast_enabled: false,
            terminal_broadcast_targets: HashSet::new(),
            terminal_broadcast_menu_open: false,
            terminal_quick_commands_open: false,
            terminal_quick_command_pending: None,
            detached_local_terminals: HashMap::new(),
            detached_local_terminals_popover_open: false,
            terminal_cast_player: None,
            terminal_cast_seek_dragging: false,
            command_palette: CommandPaletteState {
                open: false,
                raw_query: String::new(),
                mode: PaletteMode::All,
                selected_index: 0,
                scroll_handle: ScrollHandle::new(),
                ssh_config_hosts: Vec::new(),
                ssh_config_hosts_loading: false,
                error: None,
            },
            shortcuts_modal: ShortcutsModalState {
                open: false,
                query: String::new(),
            },
            settings_reset_confirm_open: false,
            quick_commands: QuickCommandsState::load(settings_store.path()),
            split_drag: None,
            sidebar_resizing: false,
            sidebar_collapsed: settings.sidebar_ui.collapsed,
            sidebar_width: settings.sidebar_ui.width as f32,
            ai_sidebar_resizing: false,
            ai_sidebar_width: (settings.sidebar_ui.ai_sidebar_width as f32)
                .clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH),
            ai_overlay_window_size: Some(current_window_size(window)),
            ai_overlay_window_bounds_subscription: None,
            knowledge_window_activation_subscription: None,
            needs_active_pane_focus: false,
            active_sidebar_section: SidebarSection::from_settings_key(
                &settings.sidebar_ui.active_section,
            ),
            active_surface: ActiveSurface::Terminal,
            active_settings_tab: SettingsTab::General,
            terminal_settings_page: TerminalSettingsPage::Display,
            open_settings_select: None,
            ai_new_provider_type: "openai_compatible".to_string(),
            ai_provider_settings_expanded: true,
            ai_tool_use_expanded: true,
            ai_context_windows_expanded: true,
            ai_model_reasoning_expanded: false,
            expanded_ai_providers: HashMap::new(),
            expanded_ai_provider_models: HashSet::new(),
            expanded_ai_context_providers: HashSet::new(),
            expanded_ai_model_reasoning_providers: HashSet::new(),
            ai_model_selector_open: false,
            ai_model_selector_search_focused: false,
            ai_model_selector_search_query: String::new(),
            ai_model_selector_expanded_providers: HashSet::new(),
            ai_model_selector_provider_online: HashMap::new(),
            ai_model_selector_probe_generations: HashMap::new(),
            ai_chat,
            ai_chat_list_state: ListState::new(0, ListAlignment::Top, px(AI_CHAT_LIST_OVERDRAW_PX)),
            ai_chat_list_cache: RefCell::new(AiChatListStateCache::default()),
            ai_markdown_cache: RefCell::new(AiMarkdownDocumentCache::default()),
            ai_context_token_cache: RefCell::new(AiContextTokenBreakdownCache::default()),
            ai_chat_store,
            ai_runtime_epoch: uuid::Uuid::new_v4().to_string(),
            ai_command_record_sequence: 0,
            ai_command_records: VecDeque::new(),
            ai_cli_agent_sessions: HashMap::new(),
            ai_conversation_list_open: false,
            ai_chat_menu_open: false,
            ai_profile_selector_open: false,
            ai_safety_menu_open: false,
            ai_safety_confirm_open: false,
            ai_summarize_confirm_open: false,
            ai_clear_all_confirm_open: false,
            ai_delete_message_confirm: None,
            ai_safety_bypass_conversations: HashSet::new(),
            ai_chat_draft: String::new(),
            ai_chat_input_focused: false,
            ai_editing_message_id: None,
            ai_editing_message_draft: String::new(),
            ai_editing_message_focused: false,
            ai_thinking_expansion_state: HashMap::new(),
            ai_tool_call_expansion_state: HashSet::new(),
            ai_chat_autocomplete_index: 0,
            ai_chat_autocomplete_suppressed: false,
            ai_context_popover_open: false,
            ai_model_switch_warning_percentage: None,
            ai_context_trim_notice_count: None,
            ai_context_trim_notice_sequence: 0,
            ai_chat_include_context: false,
            ai_chat_include_all_panes: false,
            ai_chat_loading: false,
            ai_chat_stream_generation: 0,
            ai_chat_stream_task: None,
            ai_chat_stream_rx: None,
            ai_chat_stream_polling: false,
            ai_pending_tool_approvals: HashMap::new(),
            ai_agent_fs,
            ai_mcp_registry,
            ai_rag_store,
            ai_mcp_add_dialog: None,
            knowledge_selected_collection_id: None,
            knowledge_create_dialog_open: false,
            knowledge_new_document_dialog_open: false,
            knowledge_embedding_config_expanded: false,
            knowledge_new_collection_name: String::new(),
            knowledge_new_document_title: String::new(),
            knowledge_new_document_format: "markdown".to_string(),
            knowledge_import_progress: None,
            knowledge_embedding_progress: None,
            knowledge_reindex_progress: None,
            knowledge_reindex_cancel: None,
            knowledge_reindex_rx: None,
            knowledge_reindex_polling: false,
            knowledge_delete_confirm: None,
            knowledge_external_edit: None,
            knowledge_error: None,
            ai_compaction_rx: None,
            ai_compaction_polling: false,
            ai_compacting_conversations: HashSet::new(),
            ai_compaction_notice: None,
            ai_pending_chat_after_compaction: None,
            next_ai_chat_sequence: 0,
            ai_key_store,
            ai_provider_key_status: HashMap::new(),
            ai_provider_key_status_pending: HashSet::new(),
            ai_provider_key_status_tx: None,
            ai_provider_key_status_rx: None,
            ai_provider_key_status_polling: false,
            ai_model_refresh_generations: HashMap::new(),
            ai_model_refreshing: HashSet::new(),
            ai_model_refresh_tx: None,
            ai_model_refresh_rx: None,
            ai_model_refresh_polling: false,
            ai_model_refresh_pending: 0,
            next_ai_model_refresh_generation: 0,
            next_ai_model_selector_probe_generation: 0,
            ai_model_selector_probe_rx: None,
            ai_model_selector_probe_tx: None,
            ai_model_selector_probe_polling: false,
            ai_model_selector_probe_pending: 0,
            show_ai_enable_confirm: false,
            ai_provider_key_remove_confirm: None,
            ai_provider_remove_confirm: None,
            select_anchors: HashMap::new(),
            text_input_anchors: HashMap::new(),
            ime_marked_text: None,
            focused_settings_input: None,
            settings_input_draft: String::new(),
            settings_slider_drag: None,
            keybinding_recording_action_id: None,
            keybinding_recording_combo: None,
            keybinding_conflict_action_ids: Vec::new(),
            keybinding_search_query: String::new(),
            keybinding_scope_filter: KeybindingScopeFilter::All,
            keybinding_reset_all_confirm_open: false,
            theme_editor: None,
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
            wsl_graphics: Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new()),
            forwarding_connection_consumers: HashMap::new(),
            sftp_connection_consumers: HashMap::new(),
            sftp_transfer_manager,
            sftp_progress_store,
            node_runtime_store,
            node_router,
            node_event_tx,
            node_event_rx,
            node_event_generations: HashMap::new(),
            reconnect_orchestrator: ReconnectOrchestratorStore::new(
                reconnect_timing_from_settings(&settings),
                reconnect_max_attempts_from_settings(&settings),
            ),
            reconnect_worker_tx,
            reconnect_worker_rx,
            pending_reconnect_node_ids: HashSet::new(),
            reconnect_debounce_scheduled: false,
            reconnect_debounce_generation: 0,
            reconnect_pipeline_active_node: None,
            reconnect_requeue_counts: HashMap::new(),
            active_connection_chain: None,
            connecting_node_locks: HashSet::new(),
            pending_reconnect_cascade_nodes: VecDeque::new(),
            last_ssh_active_probe_at: None,
            ssh_active_probe_in_flight: false,
            pending_reconnect_transfer_resumes: HashMap::new(),
            reconnect_transfer_resume_totals: HashMap::new(),
            reconnect_transfer_resume_successes: HashMap::new(),
            pending_ide_restore_transfer_counts: HashMap::new(),
            reconnect_forward_restore_totals: HashMap::new(),
            reconnect_forward_restore_tokens: HashMap::new(),
            notification_center: NotificationCenterState::default(),
            terminal_endpoint_sessions: HashMap::new(),
            ssh_nodes: HashMap::new(),
            saved_ssh_nodes: HashMap::new(),
            terminal_ssh_nodes: HashMap::new(),
            pending_ssh_terminal_opens: VecDeque::new(),
            expanded_ssh_nodes: HashSet::new(),
            active_ssh_node_id: None,
            next_ssh_node_id: 1,
            forward_tab_nodes: HashMap::new(),
            forwarding_view: forwards::ForwardsViewState::default(),
            forwarding_port_detection_by_node: HashMap::new(),
            forwarding_port_profiler_nodes: HashSet::new(),
            file_manager: FileManagerState::load(settings_store.path()),
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
            launcher: LauncherState::new(settings.launcher.enabled),
            graphics: GraphicsState::new(),
            connection_monitor: ConnectionMonitorState::new(profiler_update_tx, profiler_update_rx),
            cloud_sync_store,
            cloud_sync_service: oxideterm_cloud_sync::operation::CloudSyncOperationService::new(),
            cloud_sync_pending_preview: None,
            cloud_sync_progress: None,
            cloud_sync_rx: None,
            cloud_sync_polling: false,
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
            connection_trace_tx,
            connection_trace_rx,
            connection_trace_toasts: HashMap::new(),
            connection_trace_nodes: HashMap::new(),
            connection_trace_attempt_seq: 0,
            zen_hint_expires_at: None,
            workspace_tooltip: None,
            workspace_tooltip_pending: None,
            workspace_tooltip_generation: 0,
        };
        workspace.ai_overlay_window_bounds_subscription =
            Some(cx.observe_window_bounds(window, |this, window, cx| {
                this.update_ai_sidebar_overlay_for_window_bounds(window, cx);
            }));
        workspace.knowledge_window_activation_subscription =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.knowledge_sync_external_edit(false, cx);
                }
            }));
        if workspace.ai_sidebar_visible() {
            workspace.bootstrap_ai_mcp_registry();
        }
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
                            workspace.poll_launcher_worker_results(cx);
                            workspace.poll_graphics_worker_results(window, cx);
                            workspace.poll_connection_monitor_updates(cx);
                            workspace.maybe_refresh_connection_monitor(cx);
                            workspace.maybe_start_sftp_remote_load(cx);
                            workspace.poll_forwarding_worker_results(cx);
                            workspace.poll_forwarding_events(cx);
                            workspace.sync_ssh_node_lifecycle(cx);
                            workspace.maybe_probe_active_ssh_connections(cx);
                            workspace.maybe_start_forwards_port_scan(cx);
                            workspace.maybe_refresh_forwards_stats(cx);
                            if workspace.any_terminal_recording_active(cx) {
                                cx.notify();
                            }
                            if workspace.new_connection_form.is_some()
                                || workspace.keyboard_interactive_challenge.is_some()
                                || workspace.focused_settings_input.is_some()
                                || workspace.session_manager.focused_input.is_some()
                                || workspace.sftp_view.focused_input.is_some()
                                || workspace.graphics.focused_input.is_some()
                                || workspace.ai_editing_message_focused
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
            command_marks_enabled: terminal.command_marks.enabled,
            command_marks_user_input_observed: terminal.command_marks.user_input_observed,
            command_marks_heuristic_detection: terminal.command_marks.heuristic_detection,
            command_marks_show_hover_actions: terminal.command_marks.show_hover_actions,
            terminal_encoding: session_terminal_encoding(terminal.terminal_encoding),
            show_fps_overlay: terminal.show_fps_overlay,
            render_policy: self.render_policy.clone(),
            background: self.terminal_background_preferences(background_key),
            paste_labels: TerminalPasteLabels {
                title_template: self.i18n.t("terminal.paste.title"),
                more_lines_template: self.i18n.t("terminal.paste.more_lines"),
                confirm: self.i18n.t("terminal.paste.confirm"),
                cancel: self.i18n.t("terminal.paste.cancel"),
                paste: self.i18n.t("terminal.paste.paste"),
            },
            command_selection_labels: TerminalCommandSelectionLabels {
                actions: self.i18n.t("terminal.command_selection.actions"),
                copy: self.i18n.t("terminal.command_selection.copy"),
                copy_title: self.i18n.t("terminal.command_selection.copy_title"),
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
