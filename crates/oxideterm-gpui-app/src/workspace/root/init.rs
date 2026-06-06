impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let mut settings_store = SettingsStore::load_default()?;
        settings_store.settings_mut().sidebar_ui.zen_mode = false;
        let connection_store = ConnectionStore::load(default_connections_path())?;
        let settings = settings_store.settings().clone();
        // Native plugin discovery intentionally stops at manifest parsing.
        // Legacy Tauri ESM plugins remain visible in Plugin Manager, but
        // the native path never evaluates JS or creates a WebView runtime.
        let plugin_registry = plugin_host::NativePluginRegistry::discover(settings_store.path());
        let local_shells = scan_shells();
        let tokens = tokens_from_settings(&settings);
        let detected_graphics = detect_graphics(window);
        let render_profile_override = render_profile_from_env();
        let render_policy = compute_render_policy(
            render_profile_override.unwrap_or(settings.appearance.render_profile),
            &detected_graphics,
        );
        // Tauri drops backdrop-blur classes under safe render profiles; keep
        // the GPUI shared backdrop layer tied to the same render-policy switch.
        set_tauri_backdrop_blur_allowed(render_policy.allow_background_blur);
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
        let ai_chat_store = None;
        let ai_chat = oxideterm_ai::AiChatState::default();
        let ai_chat_initialization_error = None;
        let ai_rag_store = LazyAiRagStore::default();
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
        let (native_plugin_confirm_tx, native_plugin_confirm_rx) = std::sync::mpsc::channel();
        let (native_plugin_terminal_tx, native_plugin_terminal_rx) = std::sync::mpsc::channel();
        let (native_plugin_sync_tx, native_plugin_sync_rx) = std::sync::mpsc::channel();
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
        let ai_acp_runtime_registry = oxideterm_ai::AcpRuntimeRegistry::default();
        let cloud_sync_store = oxideterm_cloud_sync::state::CloudSyncStateStore::load(
            oxideterm_cloud_sync::state::default_cloud_sync_state_path(settings_store.path()),
        )?;
        let cloud_sync_form =
            CloudSyncFormDraft::from_settings(&cloud_sync_store.state().settings);
        let initial_vibrancy_mode = effective_vibrancy_mode(&settings, &render_policy);
        let mut background_image_cache = BackgroundImageRenderCache::default();
        background_image_cache.set_byte_limit(render_policy.image_cache_bytes);
        let settings_store_last_modified =
            crate::workspace::settings::settings_store_modified_time(settings_store.path());
        let connection_store_last_modified =
            crate::workspace::settings::settings_store_modified_time(connection_store.path());
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            tab_navigation_history: Vec::new(),
            tab_navigation_index: None,
            tab_navigation_replaying: false,
            tab_navigation_observed_tab: None,
            tab_drag: None,
            tab_close_confirm: None,
            panes: HashMap::new(),
            tab_scroll_handle: ScrollHandle::new(),
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
            serial_terminal_configs: HashMap::new(),
            detached_local_terminals_popover_open: false,
            terminal_cast_player: None,
            terminal_cast_seek_dragging: false,
            command_palette: CommandPaletteState {
                open: false,
                raw_query: String::new(),
                mode: PaletteMode::All,
                selected_index: 0,
                scroll_handle: UniformListScrollHandle::new(),
                ssh_config_hosts: Vec::new(),
                ssh_config_hosts_loading: false,
                error: None,
            },
            onboarding: OnboardingState::from_settings(&settings),
            shortcuts_modal: ShortcutsModalState {
                open: false,
                query: String::new(),
                scroll_handle: UniformListScrollHandle::new(),
            },
            settings_page: SettingsPageModel::default(),
            settings_managed_key_dialog: None,
            settings_managed_key_status: None,
            settings_managed_key_file_path: String::new(),
            settings_managed_key_file_name: String::new(),
            settings_managed_key_file_passphrase: String::new(),
            settings_managed_key_paste_name: String::new(),
            settings_managed_key_paste_private_key: String::new(),
            settings_managed_key_paste_passphrase: String::new(),
            settings_managed_key_rename_name: String::new(),
            settings_connection_import_source: ConnectionImportSource::SecureCrt,
            settings_connection_import_paths: Vec::new(),
            settings_connection_import_preview: None,
            settings_selected_connection_import_drafts: HashSet::new(),
            settings_connection_import_duplicate_strategy: ConnectionImportDuplicateStrategy::Skip,
            settings_connection_import_target_group: String::new(),
            settings_network_proxy_password_status: None,
            settings_network_proxy_test_host: String::new(),
            settings_network_proxy_test_port: "22".to_string(),
            settings_network_proxy_test_pending: false,
            settings_network_proxy_test_status: None,
            settings_local_privilege_draft: PrivilegeCredentialDraft::default(),
            settings_local_privilege_error: None,
            quick_commands: QuickCommandsState::load(settings_store.path()),
            // Quick command popovers can contain user-sized command sets; keep
            // their rows on the same variable-height list path as migrated
            // browser popovers instead of constructing every row on each render.
            quick_command_list_state: ListState::new(
                QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
                    QUICK_COMMAND_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            quick_command_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Detached local terminals are a bounded popover list, but the
            // number of retained background shells is user-driven, so keep it
            // on the same ListState path as other browser-style popovers.
            detached_local_terminal_list_state: ListState::new(
                DETACHED_LOCAL_TERMINAL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT),
                    DETACHED_LOCAL_TERMINAL_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            detached_local_terminal_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Plugin Manager has the same browser-page structure as Settings:
            // a small set of variable-height sections inside a scroll region.
            plugin_manager_section_list_state: ListState::new(
                PLUGIN_MANAGER_SECTION_LIST_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT),
                    PLUGIN_MANAGER_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            plugin_manager_active_tab: plugin_manager::NativePluginManagerTab::Installed,
            plugin_manager_install_url_draft: String::new(),
            plugin_manager_install_checksum_draft: String::new(),
            plugin_manager_registry_url_draft: String::new(),
            plugin_manager_available_updates: Vec::new(),
            plugin_manager_operation_status: plugin_manager::NativePluginManagerOperationStatus::Idle,
            plugin_manager_pending_overwrite: None,
            plugin_manager_delivery_rx: None,
            plugin_manager_delivery_polling: false,
            plugin_manager_expanded_plugin_ids: HashSet::new(),
            active_native_plugin_sidebar_panel: None,
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
            active_session_sidebar_view_mode: ActiveSessionSidebarViewMode::Tree,
            active_session_sidebar_focused_node_id: settings
                .tree_ui
                .focused_node_id
                .clone()
                .map(NodeId::new),
            // Session sidebar is a browser-style tree/focus list from Tauri's
            // Sidebar.tsx. The same ListState is resynced by mode-specific
            // row signatures so switching views does not leave stale row
            // measurements behind.
            active_session_sidebar_list_state: ListState::new(
                ACTIVE_SESSION_SIDEBAR_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT),
                    ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            active_session_sidebar_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            open_settings_select: None,
            settings_select_focus_origin: None,
            // Settings tabs are variable-height browser sections, not a single
            // flex tree. Initialize the shared GPUI ListState here and let the
            // settings surface reset it by active tab/signature during render.
            settings_section_list_state: ListState::new(
                SETTINGS_SECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT),
                    SETTINGS_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            settings_section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            settings_data_directory_confirm: None,
            // Execution profiles live inside the OxideSens section chrome, but
            // user-created profile counts are unbounded. Keep the rows on their
            // own ListState so scrolling the settings page does not rebuild all
            // profile cards.
            ai_execution_profile_list_state: ListState::new(
                AI_EXECUTION_PROFILE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_EXECUTION_PROFILE_LIST_ESTIMATED_HEIGHT),
                    AI_EXECUTION_PROFILE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            ai_execution_profile_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Expanded context/reasoning override tables are per-provider model
            // rows. Store ListState by provider id to preserve each table's
            // measurements independently.
            ai_context_model_list_states: RefCell::new(HashMap::new()),
            ai_context_model_list_caches: RefCell::new(HashMap::new()),
            ai_reasoning_model_list_states: RefCell::new(HashMap::new()),
            ai_reasoning_model_list_caches: RefCell::new(HashMap::new()),
            // Provider model chips keep Tauri's wrapped chip look, but the
            // chip rows are virtualized per provider when users expand all
            // models.
            ai_provider_model_chip_list_states: RefCell::new(HashMap::new()),
            ai_provider_model_chip_list_caches: RefCell::new(HashMap::new()),
            // Provider cards stay visually grouped in one OxideSens section,
            // but the card list itself is virtual. This avoids outer settings
            // scroll jumps when a provider expands or collapses.
            ai_provider_card_list_state: ListState::new(
                AI_PROVIDER_CARD_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
                    AI_PROVIDER_CARD_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            ai_provider_card_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // MCP server cards are part of OxideSens but grow with user config
            // and live status/tool chips. Keep them out of ordinary flex trees.
            ai_mcp_server_list_state: ListState::new(
                AI_MCP_SERVER_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
                    AI_MCP_SERVER_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            ai_mcp_server_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            ai_model_selector_open: false,
            ai_model_selector_scope: None,
            ai_model_selector_focus_origin: None,
            ai_model_selector_search_focused: false,
            ai_model_selector_search_query: String::new(),
            ai_model_selector_expanded_providers: HashSet::new(),
            ai_model_selector_highlighted_model: None,
            ai_model_selector_provider_online: HashMap::new(),
            ai_model_selector_probe_generations: HashMap::new(),
            ai_model_selector_status_signature: 0,
            ai_chat,
            ai_chat_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                ai_chat_virtual_list_spec(),
            ),
            ai_chat_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            ai_markdown_cache: RefCell::new(AiMarkdownDocumentCache::default()),
            ai_context_token_cache: RefCell::new(AiContextTokenBreakdownCache::default()),
            ai_chat_store,
            ai_chat_initialized: false,
            ai_chat_initialization_error,
            ai_inline_panel: AiInlinePanelState::default(),
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
            standard_confirm_focused_action: None,
            ai_safety_bypass_conversations: HashSet::new(),
            ai_chat_draft: String::new(),
            ai_chat_input_focused: false,
            ai_chat_footer_focus: None,
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
            ai_acp_runtime_registry,
            ai_acp_agent_probe_pending: HashSet::new(),
            ai_acp_agent_probe_tx: None,
            ai_acp_agent_probe_rx: None,
            ai_acp_agent_probe_polling: false,
            ai_rag_store,
            ai_mcp_add_dialog: None,
            knowledge_reindex_cancel: None,
            knowledge_reindex_rx: None,
            knowledge_reindex_polling: false,
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
            select_anchors: HashMap::new(),
            text_input_anchors: HashMap::new(),
            selectable_text_values: HashMap::new(),
            selectable_text_layouts: HashMap::new(),
            selectable_text_fragments: HashMap::new(),
            selectable_text_generation: 0,
            selectable_text_autoscroll_position: None,
            selectable_text_autoscroll_scheduled: false,
            selectable_text_scroll_handles: RefCell::new(HashMap::new()),
            mermaid_zoom: None,
            ime_marked_text: None,
            pending_platform_text_commit: None,
            next_platform_text_commit_generation: 0,
            selected_ime_target: None,
            selected_ime_range: None,
            ime_drag_selection: None,
            focused_settings_input: None,
            settings_input_draft: String::new(),
            settings_slider_drag: None,
            settings_caret_blink_pause_until: None,
            keybinding_recording_combo: None,
            keybinding_recording_footer_focus: None,
            portable_settings_dialog: None,
            portable_settings_action_pending: None,
            portable_settings_action_error: None,
            portable_status_snapshot: None,
            portable_status_error: None,
            portable_exportable_secret_count: None,
            portable_settings_refresh_pending: false,
            native_update_state: settings::NativeUpdateUiState::Idle,
            native_update_rx: None,
            native_update_polling: false,
            native_update_cancel: None,
            portable_current_password: String::new(),
            portable_new_password: String::new(),
            portable_confirm_password: String::new(),
            new_connection_form: None,
            drill_down_parent_node_id: None,
            editing_saved_connection_id: None,
            duplicating_saved_connection_id: None,
            saved_connection_prompt_action: None,
            open_new_connection_select: None,
            new_connection_select_focus_origin: None,
            new_connection_caret_visible: true,
            host_key_challenge: None,
            active_proxy_connect_run: None,
            keyboard_interactive_challenge: None,
            keyboard_interactive_timer_generation: 0,
            ssh_worker_tx,
            ssh_worker_rx,
            ssh_registry,
            forwarding_registry,
            forwarding_runtime,
            wsl_graphics: Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new()),
            forwarding_connection_consumers: HashMap::new(),
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
            notification_sidebar_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE),
                    NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN,
                ),
            ),
            notification_sidebar_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            event_log_sidebar_scroll_handle: UniformListScrollHandle::new(),
            terminal_endpoint_sessions: HashMap::new(),
            ssh_nodes: HashMap::new(),
            saved_ssh_nodes: HashMap::new(),
            terminal_ssh_nodes: HashMap::new(),
            terminal_privilege_connection_ids: HashMap::new(),
            pending_ssh_terminal_opens: VecDeque::new(),
            expanded_ssh_nodes: HashSet::new(),
            active_ssh_node_id: None,
            next_ssh_node_id: 1,
            forward_tab_nodes: HashMap::new(),
            // Forwards is a variable-height browser page with optional banner,
            // form, error, and remote-port sections. Keep it on the same
            // ListState section-list path as Settings and Cloud Sync.
            forwards_section_list_state: ListState::new(
                FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            forwards_section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // The table body is independently virtualized inside the Forwards
            // page section so a long forwarding registry does not rebuild every
            // row while the outer section list is measuring page chrome.
            forwards_table_row_list_state: ListState::new(
                FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_TABLE_ROW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            forwards_table_row_list_cache: RefCell::new(VirtualListSignatureCache::default()),
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
            // WSL launcher rows are browser-list content: keep their row
            // estimate/overscan centralized instead of rebuilding every distro
            // row through a plain flex tree.
            launcher_wsl_list_state: ListState::new(
                LAUNCHER_WSL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(LAUNCHER_WSL_LIST_ESTIMATED_HEIGHT),
                    LAUNCHER_WSL_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            launcher_wsl_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // macOS launcher keeps the Tauri grid visual, but the scroll owner
            // is a GPUI ListState of grid rows so large application catalogs do
            // not build every icon tile on every render.
            launcher_app_grid_list_state: ListState::new(
                LAUNCHER_APP_GRID_INITIAL_ROW_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(LAUNCHER_APP_GRID_ESTIMATED_ROW_HEIGHT),
                    LAUNCHER_APP_GRID_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            launcher_app_grid_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            graphics: GraphicsState::new(),
            connection_monitor: ConnectionMonitorState::new(profiler_update_tx, profiler_update_rx),
            // Monitor pages are variable-height browser sections; keep the
            // summary page and pool body on shared ListState-backed render paths.
            connection_monitor_section_list_state: ListState::new(
                CONNECTION_MONITOR_SECTION_LIST_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CONNECTION_MONITOR_SECTION_LIST_ESTIMATED_HEIGHT),
                    CONNECTION_MONITOR_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            connection_monitor_section_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            connection_pool_body_list_state: ListState::new(
                CONNECTION_POOL_BODY_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CONNECTION_POOL_BODY_LIST_ESTIMATED_HEIGHT),
                    CONNECTION_POOL_BODY_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            connection_pool_body_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            cloud_sync_store,
            cloud_sync_service: oxideterm_cloud_sync::operation::CloudSyncOperationService::new(),
            cloud_sync_form,
            // Cloud Sync is a variable-height browser page with optional preview
            // and rollback sections; render it through the shared section list
            // instead of rebuilding one scroll-sized flex tree.
            cloud_sync_section_list_state: ListState::new(
                CLOUD_SYNC_SECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
                    CLOUD_SYNC_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            cloud_sync_section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Cloud Sync rollback backups/history are nested record lists inside
            // the page sections; give each list its own state so they do not
            // share measurements with the outer section list.
            cloud_sync_rollback_backup_list_state: ListState::new(
                CLOUD_SYNC_ROLLBACK_BACKUP_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
                    CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            cloud_sync_rollback_backup_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            cloud_sync_history_list_state: ListState::new(
                CLOUD_SYNC_HISTORY_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
                    CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            cloud_sync_history_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            cloud_sync_open_select: None,
            cloud_sync_focused_select: None,
            cloud_sync_select_focus_origin: None,
            cloud_sync_select_highlighted: None,
            cloud_sync_confirm: None,
            cloud_sync_confirm_focused_action: None,
            cloud_sync_pending_preview: None,
            cloud_sync_preview_selection: None,
            cloud_sync_progress: None,
            cloud_sync_rx: None,
            cloud_sync_polling: false,
            cloud_sync_active_action: None,
            cloud_sync_auto_upload_generation: 0,
            cloud_sync_dirty_refresh_scheduled: false,
            cloud_sync_dirty_refresh_generation: 0,
            cloud_sync_upload_after_current: false,
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
            settings_store_last_modified,
            connection_store_last_modified,
            plugin_registry,
            plugin_runtime_host: Arc::new(tokio::sync::Mutex::new(
                plugin_runtime::NativePluginRuntimeHost::default(),
            )),
            native_plugin_confirm_tx,
            native_plugin_confirm_rx,
            native_plugin_confirm: None,
            native_plugin_confirm_polling: false,
            native_plugin_terminal_tx,
            native_plugin_terminal_rx,
            native_plugin_terminal_ui_requests: VecDeque::new(),
            native_plugin_terminal_polling: false,
            native_plugin_sync_tx,
            native_plugin_sync_rx,
            native_plugin_sync_polling: false,
            native_plugin_runtime_services_started: false,
            native_plugin_layout_snapshot: serde_json::Value::Null,
            native_plugin_layout_polling: false,
            native_plugin_session_tree_snapshot: serde_json::Value::Null,
            native_plugin_session_polling: false,
            native_plugin_saved_forwards_snapshot: serde_json::Value::Null,
            native_plugin_saved_forwards_polling: false,
            native_plugin_transfer_snapshot: serde_json::Value::Null,
            native_plugin_transfer_polling: false,
            native_plugin_transfer_progress_last_emitted: None,
            native_plugin_profiler_snapshot: serde_json::Value::Null,
            native_plugin_profiler_polling: false,
            native_plugin_profiler_last_emitted: None,
            native_plugin_ide_snapshot: serde_json::Value::Null,
            native_plugin_ide_polling: false,
            native_plugin_ai_snapshot: serde_json::Value::Null,
            native_plugin_ai_polling: false,
            native_plugin_event_log_last_id: 0,
            native_plugin_event_log_polling: false,
            session_manager: SessionManagerState::default(),
            // Session manager folder tree is a nested browser tree. Virtualize
            // the root rows first; expanded child rows stay grouped under their
            // parent until tree-row flattening is migrated.
            session_manager_folder_tree_list_state: ListState::new(
                SESSION_MANAGER_FOLDER_TREE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(SESSION_MANAGER_FOLDER_TREE_LIST_ESTIMATED_HEIGHT),
                    SESSION_MANAGER_FOLDER_TREE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            session_manager_folder_tree_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            // .oxide export can contain many saved connections. Keep the
            // selectable record rows on the shared variable-list path while the
            // dialog chrome remains ordinary GPUI layout.
            oxide_export_connection_list_state: ListState::new(
                OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_connection_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Import file metadata may preview many connection names before the
            // full import preview is opened; keep that read-only list virtual.
            oxide_import_connection_preview_list_state: ListState::new(
                OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_connection_preview_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            // Forward export is grouped by owner connection. Virtualize group
            // rows so a large forwarding registry does not rebuild every group.
            oxide_export_forward_group_list_state: ListState::new(
                OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_forward_group_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            // Export preflight warnings can grow with selected content; keep
            // the compact warning body virtual while preserving the Tauri
            // 64px scroll window.
            oxide_export_summary_line_list_state: ListState::new(
                OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_summary_line_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            // Import preview forward details are read-only rows inside the
            // .oxide dialog, so keep them on a dedicated ListState.
            oxide_import_forward_detail_list_state: ListState::new(
                OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_forward_detail_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            // Import preview can show several connection-name groups at once.
            // Each group needs an isolated ListState/cache so virtual row
            // measurements do not leak between conflict categories.
            oxide_import_name_group_list_states: RefCell::new(HashMap::new()),
            oxide_import_name_group_list_caches: RefCell::new(HashMap::new()),
            auto_route_modal: AutoRouteModalState::default(),
            local_shells,
            terminal_notice_tx,
            terminal_notice_rx,
            workspace_toasts: Vec::new(),
            plugin_progress_toasts: HashMap::new(),
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
            workspace.ensure_ai_chat_initialized();
            workspace.bootstrap_ai_mcp_registry();
        }
        workspace.bootstrap_cloud_sync_controller(cx);
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
                            workspace.poll_node_events(window, cx);
                            workspace.poll_reconnect_worker_results(window, cx);
                            workspace.poll_sftp_worker_results(cx);
                            workspace.poll_launcher_worker_results(cx);
                            workspace.poll_graphics_worker_results(window, cx);
                            workspace.poll_connection_monitor_updates(cx);
                            workspace.poll_external_settings_store_changes(cx);
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
                            if workspace.active_privilege_prompt_helper_should_refresh(cx) {
                                // Tauri TerminalCommandBar polls the visible buffer only
                                // when an SSH saved connection has privilege metadata.
                                // Native uses the workspace heartbeat for the same
                                // scoped refresh so the chip appears as prompts arrive.
                                cx.notify();
                            }
                            if workspace.active_ime_target_blinks_caret() {
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
        // Keep render-time background checks off the filesystem hot path.
        // GPUI image fallback and the blurred-image loader already handle
        // missing files; doing path.exists() here made settings pages with many
        // translucent cards stat the same image repeatedly while scrolling.
        Some(TerminalBackgroundPreferences {
            path,
            opacity: terminal.background_opacity.clamp(0.0, 1.0) as f32,
            blur: terminal.background_blur.clamp(0, 20) as f32,
            fit: terminal_background_fit(terminal.background_fit),
        })
    }
}

fn ai_chat_initialization_error(error: &anyhow::Error) -> AiChatInitializationError {
    let message = error.to_string();
    if message.contains("Database already open") || message.contains("Cannot acquire lock") {
        return AiChatInitializationError {
            message_key: "ai.chat.database_locked",
            can_retry: true,
        };
    }
    if message.contains("requires format upgrade")
        || message.contains("upgrade required")
        || message.contains("manual upgrade required")
    {
        return AiChatInitializationError {
            message_key: "ai.chat.database_upgrade_required",
            can_retry: false,
        };
    }
    AiChatInitializationError {
        message_key: "ai.chat.load_failed_generic",
        can_retry: true,
    }
}
