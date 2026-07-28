use super::super::*;

impl WorkspaceApp {
    const WORKSPACE_ASYNC_RUNTIME_WORKER_THREADS: usize = 2;

    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        desktop_presence_rx: Option<oxideterm_desktop_presence::DesktopPresenceReceiver>,
        single_instance_rx: Option<crate::single_instance::SingleInstanceReceiver>,
    ) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let mut settings_store = SettingsStore::load_default()?;
        settings_store.settings_mut().sidebar_ui.zen_mode = false;
        if let Err(error) = ensure_bundled_workspace_backgrounds(settings_store.path()) {
            // A background-gallery failure must not prevent the workspace from opening.
            eprintln!("failed to install built-in workspace backgrounds: {error}");
        }
        let version_migration = VersionMigrationState::from_settings_path(settings_store.path())?;
        let connection_store = ConnectionStore::load(default_connections_path())?;
        let settings = settings_store.settings().clone();
        let i18n = I18n::new(locale_from_settings(settings.general.language));
        oxideterm_network_proxy::install_application_proxy_policy_from_settings(
            &settings,
            &connection_store,
        );
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
        let forwarding_runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("oxideterm-forwarding")
                // Most workspace backend jobs are async IO; keep idle thread
                // stacks bounded. Features that need CPU-heavy parallelism
                // should use a dedicated pool instead of expanding this
                // shared runtime.
                .worker_threads(Self::WORKSPACE_ASYNC_RUNTIME_WORKER_THREADS)
                .build()?,
        );
        // The SSH pool idle timer is long-lived backend work, matching Tauri's
        // registry-owned timeout task rather than tying disconnects to a GPUI
        // render/update turn.
        ssh_registry.set_task_runtime(forwarding_runtime.handle().clone());
        let forwarding_delivery_wake = delivery::ActiveDeliveryWake::default();
        let (forwarding_event_tx, forwarding_event_rx) = std::sync::mpsc::channel();
        let event_wake = forwarding_delivery_wake.clone();
        let forwarding_event_tx = ForwardEventDeliverySender::with_wake(
            forwarding_event_tx,
            Arc::new(move || event_wake.mark()),
        );
        let forwarding_registry = match SavedForwardStore::load(default_saved_forwards_path()) {
            Ok(store) => {
                ForwardingRegistry::new_with_event_delivery_and_store(forwarding_event_tx, store)
            }
            Err(error) => {
                eprintln!("failed to load saved forwards store: {error}");
                ForwardingRegistry::new_with_event_delivery(forwarding_event_tx)
            }
        };
        // Mirror Tauri's split between SessionTree runtime state and NodeRouter:
        // the router resolves capabilities from this shared node runtime store
        // instead of owning the node lifecycle itself.
        let node_runtime_store = NodeRuntimeStore::default();
        let node_router = NodeRouter::with_runtime_store(ssh_registry.clone(), node_runtime_store);
        let forwarding_service = forwards::ForwardingRuntimeService::new(
            forwarding_registry,
            ssh_registry.clone(),
            node_router.clone(),
            forwarding_runtime.clone(),
        );
        let connection_flow = cx.new(ConnectionFlowEntity::new);
        let settings_workspace = cx.new(settings::SettingsWorkspaceEntity::new);
        let settings_workspace_observation =
            cx.observe(&settings_workspace, |_workspace, _settings, cx| {
                // Entity-owned settings workers repaint mounted settings surfaces.
                cx.notify();
            });
        let settings_workspace_subscription = cx.subscribe(
            &settings_workspace,
            |workspace, settings, event: &settings::SettingsWorkspaceEvent, cx| {
                workspace.handle_settings_workspace_event(settings, event, cx);
            },
        );
        let ssh_worker_tx = connection_flow.read(cx).ssh_worker_sender();
        let workspace_runtime = cx.new(|cx| {
            runtime_entity::WorkspaceRuntimeEntity::new_with_ssh_worker_sender(
                ssh_worker_tx,
                ssh_registry.clone(),
                node_router.clone(),
                forwarding_runtime.clone(),
                settings.reconnect.enabled,
                reconnect_timing_from_settings(&settings),
                reconnect_max_attempts_from_settings(&settings),
                cx,
            )
        });
        let runtime_window_handle = window.window_handle();
        let workspace_runtime_subscription = cx.subscribe(
            &workspace_runtime,
            move |workspace, _runtime, event: &runtime_entity::WorkspaceRuntimeEvent, cx| {
                workspace.handle_workspace_runtime_event(event, runtime_window_handle, cx);
            },
        );
        let (forwarding_worker_tx, forwarding_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(forwarding_delivery_wake);
        let forwarding = cx.new(|cx| {
            forwards::ForwardingWorkspaceEntity::new(
                forwarding_worker_tx,
                forwarding_worker_rx,
                forwarding_event_rx,
                forwarding_service.clone(),
                cx,
            )
        });
        let forwarding_subscription = cx.subscribe(
            &forwarding,
            |workspace, _forwarding, event: &forwards::ForwardingWorkspaceEvent, cx| {
                workspace.handle_forwarding_workspace_event(*event, cx);
            },
        );
        let forwarding_observation = cx.observe(&forwarding, |_workspace, _forwarding, cx| {
            // Entity-owned delivery and sampling state repaint every mounted
            // forwarding surface without mirroring fields back to the root.
            cx.notify();
        });
        let (sftp_worker_tx, mut sftp_worker_rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = cx.new(|cx| {
            WorkspaceTerminalEntity::new(
                forwarding_runtime.clone(),
                node_router.clone(),
                settings_store.path(),
                cx,
            )
        });
        let terminal_notice_tx = terminal.read(cx).notice_sender();
        let terminal_subscription = cx.subscribe(
            &terminal,
            |workspace, _terminal, event: &WorkspaceTerminalEvent, cx| {
                workspace.handle_workspace_terminal_event(event, cx);
            },
        );
        let connection_flow_observation =
            cx.observe(&connection_flow, |_workspace, _connection_flow, cx| {
                // Connection-flow lifecycle changes repaint mounted dialogs without root mirrors.
                cx.notify();
            });
        let connection_flow_subscription = cx.subscribe(
            &connection_flow,
            move |workspace, _connection_flow, event: &ConnectionFlowEvent, cx| {
                match event {
                    ConnectionFlowEvent::ConnectionFormClosed => {
                        // Apply runtime cleanup after the Entity has already cleared ownership.
                        workspace.cleanup_cancelled_proxy_connect_runs(cx);
                    }
                    ConnectionFlowEvent::WorkerResultsReady => {
                        workspace
                            .schedule_connection_flow_worker_delivery(runtime_window_handle, cx);
                    }
                }
                cx.notify();
            },
        );
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let host_tools_messages = HostToolsMessages::from_i18n(&i18n);
        let host_tools = cx.new(|cx| {
            let mut host_tools = HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                ssh_registry.clone(),
                cx,
            );
            host_tools.set_messages(host_tools_messages);
            host_tools
        });
        let remote_desktop = cx.new(|_cx| remote_desktop::RemoteDesktopWorkspaceEntity::new());
        let host_tools_subscription = cx.subscribe(
            &host_tools,
            |workspace, _host_tools, event: &HostToolsEvent, _cx| match event {
                HostToolsEvent::ShowNotice(notice) => {
                    workspace.push_host_tools_notice(notice.clone());
                }
                HostToolsEvent::ToolSelected(tool) => {
                    workspace.begin_user_segmented_control_transition(
                        selection_motion::HOST_TOOLS_SWITCHER_ID,
                        connection_monitor::host_tools_tab_index(*tool),
                        _cx,
                    );
                    workspace.clear_ime_selection();
                    workspace.ime_marked_text = None;
                    _cx.notify();
                }
            },
        );
        let sftp_transfer_manager = Arc::new(SftpTransferManager::new());
        sftp_transfer_manager.apply_settings(sftp_runtime_settings_from_settings(&settings));
        let sftp_progress_store: Arc<dyn ProgressStore> = {
            let path = default_settings_path()
                .parent()
                .map(|parent| parent.join("sftp_progress.redb"))
                .unwrap_or_else(|| std::path::PathBuf::from("sftp_progress.redb"));
            // Opening redb can allocate and rebuild indexes, so defer it until
            // a transfer actually needs persisted progress.
            Arc::new(LazyProgressStore::new(path))
        };
        let ai_agent_fs = NodeAgentIdeFileSystem::new(
            node_router.clone(),
            crate::workspace::ide::node_agent_mode_from_settings(&settings),
        );
        let cloud_sync_store = oxideterm_cloud_sync::state::CloudSyncStateStore::load(
            oxideterm_cloud_sync::state::default_cloud_sync_state_path(settings_store.path()),
        )?;
        let initial_vibrancy_mode = effective_vibrancy_mode(&settings, &render_policy);
        let initial_vibrancy_support = apply_window_vibrancy(window, initial_vibrancy_mode);
        let initial_window_opacity = normalized_window_opacity(settings.appearance.window_opacity);
        let _ = apply_window_opacity(window, settings.appearance.window_opacity);
        let mut background_image_cache = BackgroundImageRenderCache::default();
        background_image_cache.set_byte_limit(render_policy.image_cache_bytes);
        let mut background_images = match list_background_images(settings_store.path()) {
            Ok(paths) => paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            Err(error) => {
                eprintln!("failed to load background image gallery: {error}");
                Vec::new()
            }
        };
        if let Some(active_path) = settings.terminal.background_image.as_ref()
            && !background_images.contains(active_path)
        {
            // A pre-gallery GPUI setting may still point directly at a user file.
            background_images.insert(0, active_path.clone());
        }
        let app_lock = app_lock::AppLockState::load(oxideterm_app_lock::AppLockStore::new());
        let ai = ai_state::AiWorkspaceState::new(
            (settings.sidebar_ui.ai_sidebar_width as f32)
                .clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH),
            Some(current_window_size(window)),
        );
        let ai_entity = cx.new(|cx| {
            ai_state::AiWorkspaceEntity::new_with_agent_fs(
                forwarding_runtime.clone(),
                ai.models.key_store.clone(),
                ai_agent_fs,
                cx,
            )
        });
        let ai_window_handle = window.window_handle();
        let ai_entity_subscription = cx.subscribe(
            &ai_entity,
            move |workspace, _ai_entity, event: &ai_state::AiWorkspaceEvent, cx| {
                workspace.handle_ai_workspace_event(event, ai_window_handle, cx);
            },
        );
        let plugin_task_runtime = forwarding_runtime.clone();
        let plugin_entity = cx.new(move |cx| {
            plugin_entity::PluginWorkspaceEntity::new(plugin_task_runtime, plugin_registry, cx)
        });
        let plugin_window_handle = window.window_handle();
        let plugin_entity_subscription = cx.subscribe(
            &plugin_entity,
            move |workspace, _plugin_entity, event: &plugin_entity::PluginWorkspaceEvent, cx| {
                workspace.handle_plugin_workspace_event(event, plugin_window_handle, cx);
            },
        );
        let settings_store_last_modified =
            crate::workspace::settings::settings_store_modified_time(settings_store.path());
        let connection_store_last_modified =
            crate::workspace::settings::settings_store_modified_time(connection_store.path());
        let tab_host = cx.new(|_| tabs::WorkspaceTabHostEntity::new());
        let tab_host_window_handle = window.window_handle();
        let tab_host_subscription = cx.subscribe(
            &tab_host,
            move |workspace, _tab_host, event: &tabs::WorkspaceTabHostEvent, cx| {
                workspace.handle_tab_host_event(event, tab_host_window_handle, cx);
            },
        );
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            main_window_tabs: WorkspaceWindowTabState::new(),
            detached_tabs: HashSet::new(),
            detached_tab_windows: HashMap::new(),
            detached_tab_return_drag: None,
            detached_tab_return_handoff: None,
            next_tab_window_handoff_generation: 0,
            main_window_tabbar_drop_bounds: None,
            node_disconnect_confirm: None,
            node_disconnect_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            pending_auto_close_terminal_sessions: HashSet::new(),
            auto_close_terminal_sessions_scheduled: false,
            tab_host,
            _tab_host_subscription: tab_host_subscription,
            search: SearchBarState::default(),
            terminal_command_bar_focused: false,
            terminal_command_input_collapsed: false,
            terminal_command_bar_draft: String::new(),
            terminal_command_suggestions_open: false,
            terminal_command_suggestion_highlighted: None,
            detached_local_terminals: HashMap::new(),
            detached_local_terminal_order: Vec::new(),
            serial_terminal_configs: HashMap::new(),
            detached_local_terminals_popover_open: false,
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
            version_migration,
            onboarding: OnboardingState::from_settings(&settings),
            shortcuts_modal: ShortcutsModalState {
                open: false,
                query: String::new(),
                scroll_handle: UniformListScrollHandle::new(),
            },
            settings_page: SettingsPageModel::default(),
            settings_workspace,
            _settings_workspace_observation: settings_workspace_observation,
            _settings_workspace_subscription: settings_workspace_subscription,
            settings_navigation_draft: None,
            segmented_control_user_motion:
                selection_motion::UserSegmentedControlMotionState::default(),
            theme_editor_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            knowledge_create_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            knowledge_document_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            ai_mcp_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            help_legal_notice_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            ai_settings_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            ai_text_editor_dialog: None,
            ai_text_editor: None,
            remote_shell_integration: settings::RemoteShellIntegrationUiState::default(),
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
            plugin_entity,
            _plugin_entity_subscription: plugin_entity_subscription,
            split_drag: None,
            sidebar_resizing: false,
            sidebar_resize_hotzone_hovered: false,
            sidebar_collapsed: settings.sidebar_ui.collapsed,
            sidebar_rendered: !settings.sidebar_ui.collapsed,
            sidebar_motion_generation: 0,
            sidebar_width: settings.sidebar_ui.width as f32,
            context_sidebar_rendered: !settings.sidebar_ui.ai_sidebar_collapsed
                && !settings.sidebar_ui.zen_mode
                && settings.ai.enabled,
            context_sidebar_motion_generation: 0,
            ai,
            ai_entity,
            _ai_entity_subscription: ai_entity_subscription,
            active_context_sidebar_panel: ContextSidebarPanel::Assistant,
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
            launch_at_login_enabled: false,
            launch_at_login_loading: false,
            launch_at_login_error: None,
            settings_data_directory_confirm: None,
            settings_data_directory_confirm_presence:
                oxideterm_gpui_ui::motion::ExitPresence::visible(),
            standard_confirm_focused_action: None,
            settings_reset_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            keybinding_reset_all_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(
            ),
            ai_clear_all_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            ai_delete_message_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            tab_close_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            select_anchors: HashMap::new(),
            text_input_anchors: TextInputAnchorStore::default(),
            selectable_text_values: HashMap::new(),
            selectable_text_layouts: HashMap::new(),
            selectable_text_fragments: HashMap::new(),
            selectable_text_generation: 0,
            selectable_text_pending_updates: Rc::new(RefCell::new(
                selectable_text::SelectableTextFrameUpdates::default(),
            )),
            selectable_text_flush_scheduled: Rc::new(Cell::new(false)),
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
            terminal_command_specs_editor_open: false,
            settings_slider_drag: None,
            settings_caret_blink_pause_until: None,
            keybinding_recording_combo: None,
            keybinding_recording_footer_focus: None,
            native_update_notification_open: false,
            native_update_notification_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            native_update_release_notes_open: false,
            native_update_release_notes_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(
            ),
            native_update_release_notes_scroll: MarkdownVirtualListScrollHandle::new(),
            settings_legal_notice_scroll: MarkdownVirtualListScrollHandle::new(),
            desktop_presence_rx,
            single_instance_rx,
            new_connection_caret_visible: true,
            connection_flow,
            _connection_flow_observation: connection_flow_observation,
            _connection_flow_subscription: connection_flow_subscription,
            workspace_runtime,
            _workspace_runtime_subscription: workspace_runtime_subscription,
            ssh_registry,
            forwarding_service,
            forwarding_runtime,
            wsl_graphics: Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new()),
            sftp_transfer_manager,
            sftp_progress_store,
            node_router,
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
            pending_ssh_terminal_opens: VecDeque::new(),
            expanded_ssh_nodes: HashSet::new(),
            active_ssh_node_id: None,
            next_ssh_node_id: 1,
            forwarding,
            _forwarding_subscriptions: vec![forwarding_subscription, forwarding_observation],
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
            host_tools,
            _host_tools_subscription: host_tools_subscription,
            cloud_sync: cloud_sync::CloudSyncWorkspaceState::new(cloud_sync_store),
            sftp_worker_tx,
            i18n,
            tokens,
            detected_graphics,
            render_profile_override,
            render_policy,
            applied_vibrancy_mode: initial_vibrancy_mode,
            vibrancy_support: initial_vibrancy_support,
            applied_window_opacity: initial_window_opacity,
            background_image_cache,
            background_images,
            app_lock,
            settings_store,
            connection_store,
            ssh_config_sync_service: None,
            settings_store_last_modified,
            connection_store_last_modified,
            session_manager: SessionManagerState::default(),
            remote_desktop,
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
            oxide_export_summary_line_list_cache: RefCell::new(VirtualListSignatureCache::default()),
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
            local_shells,
            local_shell_launcher_open: false,
            local_shell_launcher_selected_id: None,
            terminal,
            _terminal_subscription: terminal_subscription,
            terminal_notice_tx,
            workspace_toast_next_id: 1,
            workspace_toasts: Vec::new(),
            plugin_progress_toasts: HashMap::new(),
            connection_trace_toasts: HashMap::new(),
            zen_hint_expires_at: None,
            terminal_font_size_hud: None,
            terminal_font_size_hud_generation: 0,
            workspace_tooltip: None,
            workspace_tooltip_pending: None,
            workspace_tooltip_generation: 0,
        };
        workspace.ai.chat.overlay_window_bounds_subscription =
            Some(cx.observe_window_bounds(window, |this, window, cx| {
                this.update_ai_sidebar_overlay_for_window_bounds(window, cx);
            }));
        workspace.ai.knowledge.window_activation_subscription =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.knowledge_sync_external_edit(false, cx);
                }
            }));
        cx.spawn(async move |weak, cx| {
            // The receiver lives with the GPUI entity task, so every worker
            // result crosses onto the UI thread exactly once without polling.
            while let Some(result) = sftp_worker_rx.recv().await {
                if weak
                    .update(cx, |workspace, cx| {
                        workspace.handle_sftp_worker_result(result, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if workspace.ai_sidebar_visible() {
            workspace.ensure_ai_chat_initialized(cx);
            workspace.bootstrap_ai_mcp_registry(cx);
        }
        if workspace.version_migration.open {
            workspace.refresh_cli_companion_status(cx);
        }
        workspace.bootstrap_cloud_sync_controller(cx);
        workspace.sync_ssh_config_sync_service();
        workspace.restore_session_tree_snapshot();
        workspace.schedule_launcher_worker_delivery(cx);
        let window_handle = window.window_handle();
        workspace.schedule_graphics_worker_delivery(window_handle, cx);
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(530)).await;
                // Keep the workspace polling loop tied to the WorkspaceApp entity itself.
                // Startup may still be between window construction and active workspace
                // restoration, so use the captured weak entity instead of the window root.
                if cx
                    .update_window(window_handle, |_, _window, cx| {
                        weak.update(cx, |workspace, cx| {
                            workspace.poll_external_settings_store_changes(cx);
                            workspace.maybe_refresh_connection_monitor(cx);
                            workspace.maybe_refresh_active_terminal_git(cx);
                            workspace.maybe_refresh_active_terminal_project(cx);
                            if workspace.any_terminal_recording_active(cx) {
                                cx.notify();
                            }
                            if workspace.sync_active_privilege_prompt_inline_hint(cx) {
                                // Privilege prompts are rendered as terminal ghost text,
                                // so the workspace heartbeat only mirrors the prompt
                                // state into the active pane instead of repainting a
                                // command-bar chip.
                                cx.notify();
                            }
                            if workspace.active_ime_target_blinks_caret(cx) {
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
        workspace.schedule_automatic_native_update_check(cx);
        Ok(workspace)
    }

    pub(crate) fn prepare_terminal_preferences_for_tab_kind(
        &self,
        kind: &TabKind,
        cx: &mut Context<Self>,
    ) -> TerminalUiPreferences {
        // The large CJK fallback is terminal-only, so keep an empty workspace
        // lean and register it immediately before the first terminal is built.
        if let Err(error) = bundled_fonts::load_terminal_cjk_fallback_regular(&cx.text_system()) {
            eprintln!(
                "failed to load bundled CJK terminal fallback; falling back to system fonts: {error}"
            );
        }
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

    pub(in crate::workspace) fn terminal_preferences_for_background_key(
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
        let clear_screen_shortcut = crate::keybindings::action_definition("terminal.clearScreen")
            .and_then(|definition| {
                crate::keybindings::effective_combo(
                    definition,
                    &settings.keybindings.overrides,
                    crate::keybindings::KeybindingSide::current(),
                )
            });
        let clear_screen_shortcut = clear_screen_shortcut
            .as_ref()
            .map(crate::keybindings::format_combo);
        TerminalUiPreferences {
            font_family: terminal
                .font_family
                .terminal_family_name(&terminal.custom_font_family),
            cjk_font_family: terminal_cjk_font_family_preference(&terminal.cjk_font_family),
            font_ligatures: terminal.font_ligatures,
            font_size: terminal.font_size as f32,
            line_height: terminal.line_height as f32,
            cursor_shape: match terminal.cursor_style {
                SettingsCursorStyle::Block => TerminalCursorShape::Block,
                SettingsCursorStyle::Underline => TerminalCursorShape::Underline,
                SettingsCursorStyle::Bar => TerminalCursorShape::Bar,
            },
            cursor_blink: terminal.cursor_blink,
            scrollback_lines: terminal.scrollback.clamp(500, 20_000) as usize,
            smooth_scroll: terminal.smooth_scroll,
            paste_protection: terminal.paste_protection,
            smart_copy: terminal.smart_copy,
            osc52_clipboard: terminal.osc52_clipboard,
            osc52_clipboard_read: terminal.osc52_clipboard_read,
            copy_on_select: terminal.copy_on_select,
            middle_click_paste: terminal.middle_click_paste,
            open_links_with_modifier: terminal.open_links_with_modifier,
            detect_file_paths_as_links: terminal.detect_file_paths_as_links,
            selection_requires_shift: terminal.selection_requires_shift,
            free_type_mode: terminal.free_type_mode,
            backspace_sequence: terminal.backspace_sequence,
            delete_sequence: terminal.delete_sequence,
            bidi_enabled: terminal.unicode.bidi_enabled,
            current_directory_awareness_enabled: terminal.command_bar.current_directory_awareness,
            command_marks_enabled: terminal.command_marks.enabled,
            command_marks_user_input_observed: terminal.command_marks.user_input_observed,
            command_marks_heuristic_detection: terminal.command_marks.heuristic_detection,
            command_marks_show_hover_actions: terminal.command_marks.show_hover_actions,
            terminal_encoding: session_terminal_encoding(terminal.terminal_encoding),
            show_performance_overlay: terminal.show_fps_overlay,
            render_policy: self.render_policy,
            background: self.terminal_background_preferences(background_key),
            transparent_background: self.window_background_preferences().is_some(),
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
                copy_command: self.i18n.t("terminal.command_selection.copy_command"),
                send_to_ai: self.i18n.t("terminal.command_selection.send_to_ai"),
                fill_command_bar: self.i18n.t("terminal.command_selection.fill_command_bar"),
                insert_selection_into_command: self
                    .i18n
                    .t("terminal.command_selection.insert_selection_into_command"),
                replace_command_with_selection: self
                    .i18n
                    .t("terminal.command_selection.replace_command_with_selection"),
                find: self.i18n.t("terminal.command_selection.find"),
                select_command: self.i18n.t("terminal.command_selection.select_command"),
                previous_command: self.i18n.t("terminal.command_selection.previous_command"),
                next_command: self.i18n.t("terminal.command_selection.next_command"),
                clear_screen: self.i18n.t("terminal.command_selection.clear_screen"),
                clear_screen_shortcut,
            },
            modem_labels: TerminalModemLabels {
                binary_transfer: self.i18n.t("terminal.modem.binary_transfer"),
                xmodem_upload: self.i18n.t("terminal.modem.xmodem_upload"),
                xmodem_receive: self.i18n.t("terminal.modem.xmodem_receive"),
                ymodem_upload: self.i18n.t("terminal.modem.ymodem_upload"),
                ymodem_receive: self.i18n.t("terminal.modem.ymodem_receive"),
                zmodem_upload: self.i18n.t("terminal.modem.zmodem_upload"),
                zmodem_receive: self.i18n.t("terminal.modem.zmodem_receive"),
            },
            serial_control_labels: TerminalSerialControlLabels {
                serial: self.i18n.t("terminal.serial_control.serial"),
                connected: self.i18n.t("terminal.serial_control.connected"),
                disconnected: self.i18n.t("terminal.serial_control.disconnected"),
                closed: self.i18n.t("terminal.serial_control.closed"),
                port_available: self.i18n.t("terminal.serial_control.port_available"),
                port_missing: self.i18n.t("terminal.serial_control.port_missing"),
                port_unknown: self.i18n.t("terminal.serial_control.port_unknown"),
                refresh: self.i18n.t("terminal.serial_control.refresh"),
                reconnect: self.i18n.t("terminal.serial_control.reconnect"),
                send_break: self.i18n.t("terminal.serial_control.send_break"),
                dtr: self.i18n.t("terminal.serial_control.dtr"),
                rts: self.i18n.t("terminal.serial_control.rts"),
                on: self.i18n.t("terminal.serial_control.on"),
                off: self.i18n.t("terminal.serial_control.off"),
                flow_none: self.i18n.t("terminal.serial_control.flow_none"),
                flow_software: self.i18n.t("terminal.serial_control.flow_software"),
                flow_hardware: self.i18n.t("terminal.serial_control.flow_hardware"),
                send_mode: self.i18n.t("terminal.serial_control.send_mode"),
                display_mode: self.i18n.t("terminal.serial_control.display_mode"),
                line_ending: self.i18n.t("terminal.serial_control.line_ending"),
                local_echo: self.i18n.t("terminal.serial_control.local_echo"),
                text_mode: self.i18n.t("terminal.serial_control.text_mode"),
                hex_mode: self.i18n.t("terminal.serial_control.hex_mode"),
                mixed_mode: self.i18n.t("terminal.serial_control.mixed_mode"),
                line_ending_lf: self.i18n.t("terminal.serial_control.line_ending_lf"),
                line_ending_crlf: self.i18n.t("terminal.serial_control.line_ending_crlf"),
                line_ending_cr: self.i18n.t("terminal.serial_control.line_ending_cr"),
                line_ending_none: self.i18n.t("terminal.serial_control.line_ending_none"),
                reconnect_failed: self.i18n.t("terminal.serial_control.reconnect_failed"),
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
            highlight_rules: Arc::from(
                terminal
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
                            HighlightRuleRenderMode::Outline => {
                                TerminalHighlightRenderMode::Outline
                            }
                        },
                        enabled: rule.enabled,
                        priority: rule.priority,
                    })
                    .collect::<Vec<_>>(),
            ),
            trzsz_policy,
            theme: TerminalUiTheme::from_tokens(self.tokens),
        }
    }

    pub(in crate::workspace) fn terminal_background_preferences(
        &self,
        background_key: &str,
    ) -> Option<TerminalBackgroundPreferences> {
        let terminal = &self.settings_store.settings().terminal;
        if !background_scope_includes_content(
            terminal.background_scope,
            &terminal.background_enabled_tabs,
            background_key,
        ) {
            return None;
        }
        self.background_image_preferences()
    }

    pub(in crate::workspace) fn window_background_preferences(
        &self,
    ) -> Option<TerminalBackgroundPreferences> {
        if !background_scope_includes_window(
            self.settings_store.settings().terminal.background_scope,
        ) {
            return None;
        }
        self.background_image_preferences()
    }

    pub(in crate::workspace) fn background_surface_active(&self, background_key: &str) -> bool {
        self.window_background_preferences().is_some()
            || self
                .terminal_background_preferences(background_key)
                .is_some()
    }

    pub(in crate::workspace) fn workspace_chrome_background(&self, color: u32) -> Rgba {
        if self.window_background_preferences().is_some() {
            rgba((color << 8) | alpha_byte(self.tokens.metrics.panel_vibrancy_alpha))
        } else {
            rgb(color)
        }
    }

    pub(in crate::workspace) fn workspace_sidebar_background(&self, color: u32) -> Rgba {
        sidebar_surface_background(
            color,
            self.window_background_preferences().is_some(),
            self.tokens.metrics.sidebar_vibrancy_alpha,
        )
    }

    pub(in crate::workspace) fn context_sidebar_content_background(&self, color: u32) -> Rgba {
        // The context sidebar frame owns the sole full-height tint. Nested AI
        // and Host Tools roots stay transparent so opacity cannot stack.
        context_sidebar_inner_surface_background(
            color,
            self.window_background_preferences().is_some(),
        )
    }

    fn background_image_preferences(&self) -> Option<TerminalBackgroundPreferences> {
        if !self.render_policy.allow_background_images {
            return None;
        }
        let terminal = &self.settings_store.settings().terminal;
        if !terminal.background_enabled {
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

pub(in crate::workspace) fn ai_chat_initialization_error(
    error: &anyhow::Error,
) -> AiChatInitializationError {
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
