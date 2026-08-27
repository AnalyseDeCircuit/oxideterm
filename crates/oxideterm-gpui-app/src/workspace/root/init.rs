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
        let window_registry = window_registry::WorkspaceWindowRegistry::default();
        let window_intents = cx.new(|cx| {
            WorkspaceWindowIntentEntity::new(desktop_presence_rx, single_instance_rx, cx)
        });
        let window_intent_subscription = cx.subscribe(
            &window_intents,
            |workspace, _window_intents, intent: &window_intent::WindowIntent, cx| {
                workspace.enqueue_window_intent(intent, cx);
            },
        );
        let window_button_layout_subscription =
            cx.observe_button_layout_changed(window, |_workspace, _window, cx| cx.notify());
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
        // Shell history is already the user's persistence boundary; OxideTerm keeps only a
        // process-owned index and never copies the document into credential storage.
        let local_terminal_command_history =
            SharedTerminalCommandHistory::from_commands(load_local_shell_history_commands());
        let session_log_directory = settings_store
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("logs")
            .join("terminal");
        let session_log_retention_days = settings.terminal.session_log.retention_days.max(0) as u64;
        cx.background_executor()
            .spawn(async move {
                // Cleanup is one-shot and non-fatal; it never owns or observes a terminal connection.
                let _ =
                    prune_terminal_session_logs(&session_log_directory, session_log_retention_days);
            })
            .detach();
        oxideterm_network_proxy::install_application_proxy_policy_from_settings(
            &settings,
            &connection_store,
        );
        // Native plugin discovery intentionally stops at manifest parsing.
        // Legacy Tauri ESM plugins remain visible in Plugin Manager, but
        // the native path never evaluates JS or creates a WebView runtime.
        let plugin_registry = plugin_host::NativePluginRegistry::discover(settings_store.path());
        // Capture one stable root for this window. Shell `cd` must not silently
        // replace the available workflow catalog.
        let skill_workspace_root = std::env::current_dir().ok();
        let plugin_roots = plugin_registry
            .plugins()
            .iter()
            .filter(|plugin| {
                matches!(
                    plugin.state,
                    plugin_host::NativePluginState::ReadyManifestOnly
                        | plugin_host::NativePluginState::ReadyWasm
                        | plugin_host::NativePluginState::ReadyProcess
                        | plugin_host::NativePluginState::Active
                )
            })
            .map(|plugin| plugin.install_dir.clone())
            .collect();
        let disabled_paths = settings
            .ai
            .skills
            .disabled_paths
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        let skill_registry =
            oxideterm_skills::SkillRegistry::discover(&oxideterm_skills::SkillDiscoveryOptions {
                workspace_root: skill_workspace_root.clone(),
                settings_path: Some(settings_store.path().to_path_buf()),
                plugin_roots,
                disabled_paths,
            });
        let skill_registry = std::sync::Arc::new(parking_lot::RwLock::new(skill_registry));
        let local_shells = scan_shells();
        let tokens = tokens_from_settings(&settings);
        let initial_viewport_width = current_window_size(window).0;
        let initial_sidebar_width = sidebar::clamp_responsive_sidebar_width(
            settings.sidebar_ui.width as f32,
            initial_viewport_width,
            tokens.metrics.sidebar_min_width,
            tokens.metrics.sidebar_max_width,
        );
        let initial_context_sidebar_width = sidebar::clamp_responsive_sidebar_width(
            settings.sidebar_ui.ai_sidebar_width as f32,
            initial_viewport_width,
            AI_SIDEBAR_ABSOLUTE_MIN_WIDTH,
            AI_SIDEBAR_ABSOLUTE_MAX_WIDTH,
        );
        let overlay_exit_duration = oxideterm_gpui_ui::motion::duration(
            &tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(overlay_exit_duration, cx));
        let overlay_observation = cx.observe(&overlay, |_workspace, _overlay, cx| {
            // Entity-owned timers and delivery repaint the mounted window portal.
            cx.notify();
        });
        let input_caret = ime::WorkspaceCaretVisibility::default();
        let workspace_input = cx.new({
            let input_caret = input_caret.clone();
            move |_cx| ime::WorkspaceInputEntity::new(input_caret)
        });
        let workspace_input_observation = cx.observe(&workspace_input, |_, _, cx| {
            // The input Entity only notifies when its window-scoped caret phase changes.
            cx.notify();
        });
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
            i18n.t("ssh.form.single_channel_forwarding_unavailable"),
        );
        let connection_flow = cx.new(ConnectionFlowEntity::new);
        let settings_path = settings_store.path().to_path_buf();
        let connections_path = connection_store.path().to_path_buf();
        let settings_workspace = cx.new(move |cx| {
            let mut settings = settings::SettingsWorkspaceEntity::new(cx);
            settings.start_external_store_watch(settings_path, connections_path, cx);
            settings
        });
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
        let terminal_triggers =
            settings::TerminalTriggersSettingsState::load(settings_store.path());
        let file_manager = cx.new(|cx| FileManagerState::load(settings_store.path(), cx));
        let file_manager_observation =
            cx.observe(&file_manager, |_workspace, _file_manager, cx| {
                // Entity-owned local file operations repaint every mounted file-manager surface.
                cx.notify();
            });
        let file_manager_subscription = cx.subscribe(
            &file_manager,
            |workspace, _file_manager, event: &FileManagerWorkspaceEvent, cx| {
                match event {
                    FileManagerWorkspaceEvent::Error(error) => workspace.push_file_manager_toast(
                        workspace.i18n.t("fileManager.error"),
                        Some(error.clone()),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                    FileManagerWorkspaceEvent::OperationSucceeded => workspace
                        .push_file_manager_toast(
                            workspace.i18n.t("fileManager.operationSuccess"),
                            None,
                            TerminalNoticeVariant::Success,
                            cx,
                        ),
                    FileManagerWorkspaceEvent::OpenEntry(entry) => {
                        workspace.open_file_manager_entry(entry.clone(), cx);
                    }
                }
                cx.notify();
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
        workspace_runtime.update(cx, |runtime, _cx| {
            runtime.configure_remote_shell_integration(
                settings.terminal.remote_shell_integration_mode,
                settings.terminal.command_bar.current_directory_awareness,
            );
        });
        let ssh_consumer_prompt_handler = workspace_runtime.read(cx).native_ssh_prompt_handler();
        let ssh_consumer_managed_key_resolver = managed_key_resolver_from_store(&connection_store);
        let workspace_runtime_subscription = cx.subscribe(
            &workspace_runtime,
            |workspace, _runtime, event: &runtime_entity::WorkspaceRuntimeEvent, cx| {
                workspace.enqueue_runtime_window_effect(*event, cx);
            },
        );
        let public_mcp = public_mcp::PublicMcpWorkspaceBridge::start(
            settings_store.path(),
            forwarding_runtime.handle(),
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
        let sftp_view = cx.new(sftp::SftpWorkspaceEntity::new);
        let sftp_observation = cx.observe(&sftp_view, |_workspace, _sftp, cx| {
            // Entity-owned SFTP state repaints every mounted SFTP surface.
            cx.notify();
        });
        let sftp_subscription = cx.subscribe(
            &sftp_view,
            |workspace, _sftp, event: &sftp::SftpWorkspaceEvent, cx| {
                match event {
                    sftp::SftpWorkspaceEvent::WorkerEffectsReady(effects) => {
                        workspace.handle_sftp_worker_effects(effects, cx);
                    }
                    sftp::SftpWorkspaceEvent::OpenFileRequested { pane, file } => {
                        workspace.open_or_preview_sftp_file(*pane, file, cx);
                    }
                    sftp::SftpWorkspaceEvent::TransferStateRequested { id, state } => {
                        workspace.set_sftp_transfer_state(*id, *state, cx);
                    }
                    sftp::SftpWorkspaceEvent::CancelOrRemoveTransferRequested { id } => {
                        workspace.cancel_or_remove_sftp_transfer(*id, cx);
                    }
                    sftp::SftpWorkspaceEvent::ResumeIncompleteTransferRequested { transfer_id } => {
                        workspace.resume_sftp_incomplete_transfer(transfer_id.clone(), cx);
                    }
                    sftp::SftpWorkspaceEvent::DiscardIncompleteTransferRequested {
                        transfer_id,
                    } => {
                        workspace.discard_sftp_incomplete_transfer(transfer_id.clone(), cx);
                    }
                    sftp::SftpWorkspaceEvent::TooltipRequested { id, label, x, y } => {
                        workspace.queue_workspace_tooltip(id, label, *x, *y, cx);
                    }
                    sftp::SftpWorkspaceEvent::TooltipCleared { id } => {
                        workspace.clear_workspace_tooltip(id, cx);
                    }
                    sftp::SftpWorkspaceEvent::PreviewSaveRequested {
                        path,
                        content,
                        encoding,
                        line_ending,
                        generation,
                        delivery,
                    } => {
                        if !workspace.spawn_remote_sftp_preview_save(
                            path.clone(),
                            content.clone(),
                            encoding.clone(),
                            *line_ending,
                            *generation,
                            delivery.clone(),
                            cx,
                        ) {
                            let _ = delivery.send(sftp::SftpWorkerResult::PreviewSaved {
                                generation: *generation,
                                path: path.clone(),
                                content: content.clone(),
                                network_error_message: workspace
                                    .i18n
                                    .t("sftp.errors.connection_lost"),
                                result: Err("SFTP connection unavailable".to_string()),
                            });
                        }
                    }
                    sftp::SftpWorkspaceEvent::RemoteLoadReady {
                        surface_id,
                        remote_id,
                        delivery,
                    } => {
                        workspace.request_visible_sftp_remote_load(
                            *surface_id,
                            remote_id.clone(),
                            delivery.clone(),
                            cx,
                        );
                    }
                }
                cx.notify();
            },
        );
        let terminal = cx.new(|cx| {
            WorkspaceTerminalEntity::new(
                forwarding_runtime.clone(),
                node_router.clone(),
                settings_store.path(),
                cx,
            )
        });
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
                        workspace.enqueue_connection_flow_window_effect(cx);
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
            host_tools.set_ssh_consumer_context(
                node_router.clone(),
                ssh_consumer_prompt_handler.clone(),
                ssh_consumer_managed_key_resolver.clone(),
            );
            host_tools.set_messages(host_tools_messages);
            host_tools
        });
        let session_manager = cx.new(SessionManagerState::new);
        let session_manager_observation =
            cx.observe(&session_manager, |_workspace, _session_manager, cx| {
                // Entity-owned manager state repaints every mounted manager surface.
                cx.notify();
            });
        let session_manager_subscription = cx.subscribe(
            &session_manager,
            |workspace, _session_manager, event: &SessionManagerWorkspaceEvent, cx| {
                workspace.handle_session_manager_workspace_event(event, cx);
            },
        );
        let remote_desktop = cx.new(|_cx| remote_desktop::RemoteDesktopWorkspaceEntity::new());
        let graphics_backend = Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new());
        let graphics = cx.new(|cx| {
            GraphicsWorkspaceEntity::new(graphics_backend, forwarding_runtime.clone(), cx)
        });
        let graphics_observation = cx.observe(&graphics, |_workspace, _graphics, cx| {
            // Entity-owned session and frame delivery repaints mounted graphics surfaces.
            cx.notify();
        });
        let graphics_subscription = cx.subscribe(
            &graphics,
            |workspace, _graphics, event: &graphics::GraphicsWorkspaceEvent, cx| match event {
                graphics::GraphicsWorkspaceEvent::WorkerResultsReady => {
                    workspace.enqueue_graphics_window_effect(cx);
                }
            },
        );
        let host_tools_subscription = cx.subscribe(
            &host_tools,
            |workspace, _host_tools, event: &HostToolsEvent, cx| match event {
                HostToolsEvent::ShowNotice(notice) => {
                    workspace.push_host_tools_notice(notice.clone(), cx);
                }
                HostToolsEvent::ToolSelected(tool) => {
                    workspace.begin_user_segmented_control_transition(
                        selection_motion::HOST_TOOLS_SWITCHER_ID,
                        connection_monitor::host_tools_tab_index(*tool),
                        cx,
                    );
                    workspace.clear_ime_selection();
                    workspace.ime_marked_text = None;
                    cx.notify();
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
        let cloud_sync =
            cx.new(|cx| cloud_sync::CloudSyncWorkspaceEntity::new(cloud_sync_store, cx));
        let cloud_sync_observation = cx.observe(&cloud_sync, |_workspace, _cloud_sync, cx| {
            // Entity-owned delivery and timers repaint every mounted Cloud Sync surface.
            cx.notify();
        });
        let cloud_sync_subscription = cx.subscribe(
            &cloud_sync,
            |workspace, _cloud_sync, event: &cloud_sync::CloudSyncWorkspaceEvent, cx| {
                workspace.enqueue_cloud_sync_window_effect(event.clone(), cx);
            },
        );
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
        settings_workspace.update(cx, |settings, _cx| {
            settings.initialize_background_gallery(background_images);
        });
        let app_lock = app_lock::AppLockState::load(oxideterm_app_lock::AppLockStore::new());
        let ai_key_store = oxideterm_ai::AiProviderKeyStore::new();
        let ai_entity = cx.new(|cx| {
            let mut entity = ai_state::AiWorkspaceEntity::new_with_agent_fs(
                forwarding_runtime.clone(),
                ai_key_store,
                ai_agent_fs,
                cx,
            );
            entity.configure_chat_surface(
                initial_context_sidebar_width,
                Some(current_window_size(window)),
            );
            entity
        });
        let ai_entity_subscription = cx.subscribe(
            &ai_entity,
            |workspace, _ai_entity, event: &ai_state::AiWorkspaceEvent, cx| {
                workspace.enqueue_ai_window_effect(event, cx);
            },
        );
        let acp_entity =
            cx.new(|cx| acp_workspace::AcpWorkspaceEntity::new(forwarding_runtime.clone(), cx));
        let acp_entity_subscription = cx.subscribe(
            &acp_entity,
            |workspace, _acp_entity, _event: &acp_workspace::AcpWorkspaceEvent, cx| {
                workspace.forward_acp_workspace_deliveries(cx);
            },
        );
        let ai_background_tasks = cx.new(|cx| {
            ai_background_tasks::AiBackgroundTaskEntity::new(forwarding_runtime.clone(), cx)
        });
        let ai_background_tasks_subscription = cx.subscribe(
            &ai_background_tasks,
            |workspace, _tasks, event: &ai_background_tasks::AiBackgroundTaskEvent, cx| {
                workspace.handle_ai_background_task_event(*event, cx);
            },
        );
        let ai_runtime_context = cx.new(|cx| {
            ai_runtime_context::AiRuntimeContextEntity::attach_release_shutdown(cx);
            ai_runtime_context::AiRuntimeContextEntity::new()
        });
        let plugin_task_runtime = forwarding_runtime.clone();
        let plugin_entity = cx.new(move |cx| {
            plugin_entity::PluginWorkspaceEntity::new(plugin_task_runtime, plugin_registry, cx)
        });
        let plugin_entity_subscription = cx.subscribe(
            &plugin_entity,
            |workspace, _plugin_entity, event: &plugin_entity::PluginWorkspaceEvent, cx| {
                workspace.enqueue_plugin_window_effect(event, cx);
            },
        );
        let tab_host = cx.new(|_| tabs::WorkspaceTabHostEntity::new());
        let tab_host_subscription = cx.subscribe(
            &tab_host,
            |workspace, _tab_host, event: &tabs::WorkspaceTabHostEvent, cx| {
                workspace.enqueue_tab_host_window_effect(*event, cx);
            },
        );
        let command_palette =
            cx.new(|_| command_palette::CommandPaletteEntity::new(forwarding_runtime.clone()));
        let command_palette_observation = cx.observe(&command_palette, |_, _, cx| cx.notify());
        let sender_context_menu_labels = oxideterm_gpui_editor::EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        };
        let compact_sender_placeholder = i18n.t("terminal.command_bar.command_placeholder");
        let expanded_sender_placeholder = i18n.t("terminal.sender.placeholder");
        let terminal_command_sender = cx.new(|cx| {
            terminal_command_sender::TerminalCommandSenderEntity::new(
                tokens,
                compact_sender_placeholder,
                expanded_sender_placeholder,
                sender_context_menu_labels,
                cx,
            )
        });
        let terminal_command_sender_observation =
            cx.observe(&terminal_command_sender, |_, _, cx| cx.notify());
        let ide_workspace = cx.new({
            let fs = ai_entity.read(cx).agent_fs().clone();
            let backend_runtime = forwarding_runtime.clone();
            move |_| ide::IdeWorkspaceEntity::new(fs, backend_runtime)
        });
        let ide_workspace_subscription = cx.subscribe(
            &ide_workspace,
            |workspace, _ide_workspace, event: &ide::IdeWorkspaceEvent, cx| {
                workspace.handle_ide_workspace_event(event, cx);
            },
        );
        let mut workspace = Self {
            focus_handle,
            main_window_tabs: WorkspaceWindowTabState::new(),
            tab_rename_dialog: None,
            detached_tab_return_drag: None,
            detached_tab_return_handoff: None,
            next_tab_window_handoff_generation: 0,
            main_window_tabbar_drop_bounds: None,
            pending_auto_close_terminal_sessions: HashSet::new(),
            auto_close_terminal_sessions_scheduled: false,
            tab_host,
            _tab_host_subscription: tab_host_subscription,
            search: SearchBarState::default(),
            terminal_recording_menu_open: false,
            terminal_highlight_popover_open: false,
            terminal_trigger_settings_pane: None,
            terminal_trigger_shell_confirmation_pending: false,
            terminal_triggers,
            terminal_trigger_runtime:
                terminal_triggers_runtime::TerminalTriggerRuntimeState::default(),
            terminal_saved_connection_refs: HashMap::new(),
            terminal_semantic_highlight_section_expanded: true,
            terminal_rule_highlight_section_expanded: true,
            terminal_command_context_highlight_section_expanded: true,
            terminal_command_sender,
            _terminal_command_sender_observation: terminal_command_sender_observation,
            local_terminal_command_history,
            ssh_terminal_command_histories: HashMap::new(),
            detached_local_terminals: HashMap::new(),
            detached_local_terminal_order: Vec::new(),
            serial_terminal_configs: HashMap::new(),
            telnet_terminal_profile_ids: HashMap::new(),
            standalone_connections: standalone_connections::StandaloneConnectionRegistry::default(),
            detached_local_terminals_popover_open: false,
            command_palette,
            _command_palette_observation: command_palette_observation,
            version_migration,
            onboarding: OnboardingState::from_settings(&settings),
            shortcuts_modal: ShortcutsModalState {
                open: false,
                query: String::new(),
                scroll_handle: UniformListScrollHandle::new(),
            },
            settings_workspace,
            _settings_workspace_observation: settings_workspace_observation,
            _settings_workspace_subscription: settings_workspace_subscription,
            segmented_control_user_motion:
                selection_motion::UserSegmentedControlMotionState::default(),
            ai_text_editor_dialog: None,
            ai_text_editor: None,
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
            embedded_sftp_sidebar_resizing: false,
            sidebar_resize_hotzone_hovered: false,
            sidebar_collapsed: settings.sidebar_ui.collapsed,
            sidebar_rendered: !settings.sidebar_ui.collapsed,
            sidebar_motion_generation: 0,
            sidebar_width: initial_sidebar_width,
            context_sidebar_rendered: !settings.sidebar_ui.ai_sidebar_collapsed
                && !settings.sidebar_ui.zen_mode
                && settings.ai.enabled,
            context_sidebar_motion_generation: 0,
            ai_entity,
            acp_entity,
            skill_registry,
            skill_workspace_root,
            loaded_conversation_skills: HashMap::new(),
            ai_background_tasks,
            _ai_background_tasks_subscription: ai_background_tasks_subscription,
            ai_runtime_context,
            _ai_entity_subscription: ai_entity_subscription,
            _acp_entity_subscription: acp_entity_subscription,
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
            standard_confirm_focused_action: None,
            skip_future_ssh_close_confirmations: false,
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
            workspace_input,
            _workspace_input_observation: workspace_input_observation,
            input_caret,
            native_update_notification_open: false,
            native_update_notification_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            native_update_release_notes_scroll: MarkdownVirtualListScrollHandle::new(),
            settings_legal_notice_scroll: MarkdownVirtualListScrollHandle::new(),
            _window_intents: window_intents,
            _window_intent_subscription: window_intent_subscription,
            _window_button_layout_subscription: window_button_layout_subscription,
            window_registry,
            window_effect_delivery_scheduled: false,
            connection_flow,
            _connection_flow_observation: connection_flow_observation,
            _connection_flow_subscription: connection_flow_subscription,
            workspace_runtime,
            _workspace_runtime_subscription: workspace_runtime_subscription,
            public_mcp,
            ssh_registry,
            forwarding_service,
            forwarding_runtime,
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
            ssh_nodes: HashMap::new(),
            saved_ssh_nodes: HashMap::new(),
            expanded_ssh_nodes: HashSet::new(),
            active_ssh_node_id: None,
            next_ssh_node_id: 1,
            forwarding,
            _forwarding_subscriptions: vec![forwarding_subscription, forwarding_observation],
            file_manager,
            _file_manager_observation: file_manager_observation,
            _file_manager_subscription: file_manager_subscription,
            sftp_tab_nodes: HashMap::new(),
            standalone_sftp_tabs: HashMap::new(),
            standalone_sftp_sessions: HashMap::new(),
            dedicated_sftp_connections: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            ssh_consumer_prompt_handler,
            ssh_consumer_managed_key_resolver,
            pending_standalone_sftp_pair_launches: HashMap::new(),
            embedded_sftp_node_id: None,
            sftp_presentation_request: None,
            ide_workspace,
            _ide_workspace_subscription: ide_workspace_subscription,
            sftp_view,
            _sftp_observation: sftp_observation,
            _sftp_subscription: sftp_subscription,
            graphics,
            _graphics_observation: graphics_observation,
            _graphics_subscription: graphics_subscription,
            host_tools,
            _host_tools_subscription: host_tools_subscription,
            cloud_sync,
            _cloud_sync_observation: cloud_sync_observation,
            _cloud_sync_subscription: cloud_sync_subscription,
            i18n,
            tokens,
            detected_graphics,
            render_profile_override,
            render_policy,
            // The native window shell applies the selected mode before the
            // first workspace render and replaces this neutral diagnostic.
            vibrancy_support: VibrancySupport::Supported,
            app_lock,
            settings_store,
            pending_window_ui_state: None,
            window_state_save_task: None,
            connection_store,
            ssh_config_sync_service: None,
            session_manager,
            _session_manager_observation: session_manager_observation,
            _session_manager_subscription: session_manager_subscription,
            remote_desktop,
            remote_desktop_resize_menu_tab_id: None,
            local_shells,
            local_shell_launcher_open: false,
            local_shell_launcher_selected_id: None,
            terminal,
            _terminal_subscription: terminal_subscription,
            overlay,
            _overlay_observation: overlay_observation,
        };
        let workspace_window_bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.clamp_sidebar_widths_to_viewport(current_window_size(window).0, cx);
            this.update_ai_sidebar_overlay_for_window_bounds(window, cx);
            this.capture_main_window_state(window, cx);
        });
        let ai_knowledge_activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.knowledge_sync_external_edit(false, cx);
            }
        });
        workspace.ai_entity.update(cx, |ai, _cx| {
            ai.retain_window_observers(workspace_window_bounds, ai_knowledge_activation);
        });
        workspace.sync_ai_workspace_visibility(cx);
        if workspace.ai_sidebar_visible() {
            workspace.ensure_ai_chat_initialized(cx);
            workspace.bootstrap_ai_mcp_registry(cx);
        }
        if workspace.version_migration.open {
            workspace.refresh_cli_companion_status(cx);
        }
        workspace.bootstrap_cloud_sync_controller(cx);
        workspace.start_public_mcp_delivery(cx);
        workspace.sync_ssh_config_sync_service();
        workspace.restore_session_tree_snapshot();
        workspace.sync_terminal_command_sender_appearance(cx);
        workspace.sync_active_terminal_metadata_context(cx);
        workspace.sync_active_terminal_recording_elapsed_tick(cx);
        workspace.sync_active_privilege_prompt_inline_hint(cx);
        workspace.refresh_terminal_trigger_runtime(cx);
        workspace.schedule_automatic_native_update_check(cx);
        cx.on_release(|workspace, cx| {
            workspace.flush_main_window_state(cx);
            workspace.shutdown_terminal_trigger_runtime();
            // Shutdown ordering is security-sensitive: late broker callbacks
            // fail before user-decision waiters and owner projections disappear.
            workspace.ai_runtime_context.update(cx, |runtime, _cx| {
                runtime.stop_accepting_and_finish_tool_sessions();
            });
            workspace.ai_entity.update(cx, |ai, _cx| {
                ai.cancel_chat_stream();
            });
            workspace.ai_runtime_context.update(cx, |runtime, _cx| {
                runtime.revoke_registered_owner_projections();
            });
        })
        .detach();
        Ok(workspace)
    }

    pub(crate) fn prepare_terminal_preferences_for_tab_kind(
        &self,
        kind: &TabKind,
        cx: &mut Context<Self>,
    ) -> TerminalUiPreferences {
        // The large CJK fallback is terminal-only, so keep an empty workspace
        // lean and register it immediately before the first terminal is built.
        let cjk_font_family = &self.settings_store.settings().terminal.cjk_font_family;
        if let Err(error) =
            bundled_fonts::load_terminal_cjk_fallback_regular(&cx.text_system(), cjk_font_family)
        {
            eprintln!(
                "failed to load bundled CJK terminal fallback; falling back to system fonts: {error}"
            );
        }
        let mut preferences =
            self.terminal_preferences_for_background_key(tab_background_key(kind), cx);
        if *kind == TabKind::LocalTerminal {
            preferences.command_history = self.local_terminal_command_history.clone();
        }
        preferences
    }

    pub(crate) fn terminal_preferences_for_pane(
        &self,
        pane_id: PaneId,
        cx: &App,
    ) -> TerminalUiPreferences {
        let (key, session_id, kind) = self
            .tabs(cx)
            .iter()
            .find_map(|tab| {
                let root = tab.root_pane.as_ref()?;
                root.contains_pane(pane_id).then(|| {
                    (
                        tab_background_key(&tab.kind),
                        root.session_id_for_pane(pane_id),
                        tab.kind.clone(),
                    )
                })
            })
            .unwrap_or(("local_terminal", None, TabKind::LocalTerminal));
        let mut preferences = self.terminal_preferences_for_background_key(key, cx);
        preferences.command_history = match kind {
            TabKind::LocalTerminal => self.local_terminal_command_history.clone(),
            TabKind::SshTerminal => session_id
                .and_then(|session_id| {
                    self.workspace_runtime
                        .read(cx)
                        .ssh_terminal_node_id(session_id)
                })
                .and_then(|node_id| self.ssh_terminal_command_histories.get(&node_id).cloned())
                .unwrap_or_default(),
            _ => SharedTerminalCommandHistory::default(),
        };
        preferences
    }

    pub(in crate::workspace) fn terminal_preference_overrides_for_saved_connection(
        &self,
        saved_connection_id: Option<&str>,
    ) -> TerminalUiPreferenceOverrides {
        let Some(connection) = saved_connection_id
            .and_then(|saved_connection_id| self.connection_store.get(saved_connection_id))
        else {
            return TerminalUiPreferenceOverrides::default();
        };
        let mut overrides = terminal_preference_overrides(
            connection.options.terminal.clone(),
            &self.settings_store.settings().terminal,
        );
        overrides.session_log_context = Some(TerminalSessionLogContext {
            session: connection.name.clone(),
            host: connection.host.clone(),
            username: connection.username.clone(),
            protocol: "ssh".to_string(),
        });
        overrides
    }

    pub(in crate::workspace) fn terminal_preference_overrides_for_local_shell(
        &self,
        shell: Option<&ShellInfo>,
    ) -> TerminalUiPreferenceOverrides {
        let fallback_shell;
        let shell = if let Some(shell) = shell {
            shell
        } else {
            fallback_shell = oxideterm_terminal::default_shell();
            &fallback_shell
        };
        let settings = self.settings_store.settings();
        let semantic_scheme_id = settings
            .local_terminal
            .semantic_scheme_for_shell(&shell.id)
            .map(str::to_string);
        let semantic_scheme = semantic_scheme_id
            .as_ref()
            .map(|scheme_id| ConnectionTerminalOptions {
                semantic_scheme: Some(scheme_id.to_string()),
                ..ConnectionTerminalOptions::default()
            })
            .map(|options| terminal_preference_overrides(options, &settings.terminal))
            .and_then(|overrides| overrides.semantic_scheme);

        TerminalUiPreferenceOverrides {
            semantic_scheme,
            semantic_scheme_id,
            semantic_shell: Some(semantic_shell_dialect(&shell.id)),
            local_shell_id: Some(shell.id.clone()),
            session_log_context: Some(TerminalSessionLogContext {
                session: shell.label.clone(),
                host: "localhost".to_string(),
                username: String::new(),
                protocol: "local".to_string(),
            }),
            ..TerminalUiPreferenceOverrides::default()
        }
    }

    pub(in crate::workspace) fn terminal_preference_overrides_for_ssh_node(
        &self,
        node_id: &NodeId,
    ) -> TerminalUiPreferenceOverrides {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return TerminalUiPreferenceOverrides::default();
        };
        if let Some(saved_connection_id) = node.saved_connection_id.as_deref() {
            return self
                .terminal_preference_overrides_for_saved_connection(Some(saved_connection_id));
        }
        let mut overrides = terminal_preference_overrides(
            node.terminal_options.clone(),
            &self.settings_store.settings().terminal,
        );
        overrides.session_log_context = Some(TerminalSessionLogContext {
            session: node.title.clone(),
            host: node.endpoint.host.clone(),
            username: node.endpoint.username.clone(),
            protocol: "ssh".to_string(),
        });
        overrides
    }

    pub(in crate::workspace) fn apply_saved_connection_terminal_preferences(
        &mut self,
        saved_connection_id: &str,
        cx: &mut Context<Self>,
    ) {
        let preference_overrides =
            self.terminal_preference_overrides_for_saved_connection(Some(saved_connection_id));
        let session_ids = self
            .ssh_nodes
            .values()
            .filter(|node| node.saved_connection_id.as_deref() == Some(saved_connection_id))
            .flat_map(|node| node.terminal_ids.iter().copied())
            .collect::<Vec<_>>();
        let panes = session_ids
            .into_iter()
            .filter_map(|session_id| {
                let location = self.tab_host.read(cx).terminal_location(session_id)?;
                let pane = self
                    .tab_host
                    .read(cx)
                    .panes()
                    .get(&location.pane_id)
                    .cloned()?;
                Some((location.pane_id, pane))
            })
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let application_preferences = self.terminal_preferences_for_pane(pane_id, cx);
            pane.update(cx, |pane, cx| {
                pane.set_preference_overrides(
                    preference_overrides.clone(),
                    application_preferences,
                    cx,
                );
            });
        }
    }

    pub(in crate::workspace) fn terminal_preferences_for_background_key(
        &self,
        background_key: &str,
        cx: &App,
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
        let session_log_directory = self
            .settings_store
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("logs")
            .join("terminal");
        let session_log_settings = &terminal.session_log;
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
            right_click_paste: terminal.right_click_paste,
            open_links_with_modifier: terminal.open_links_with_modifier,
            detect_file_paths_as_links: terminal.detect_file_paths_as_links,
            semantic_coloring: terminal.semantic_coloring,
            semantic_scheme: resolved_terminal_semantic_scheme(
                terminal.semantic_scheme,
                terminal.active_custom_semantic_scheme(),
            ),
            semantic_shell: SemanticShellDialect::Auto,
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
            command_history: SharedTerminalCommandHistory::default(),
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
            kitty_file_transmission_labels: TerminalKittyFileTransmissionLabels {
                title: self.i18n.t("terminal.kitty_file_transmission.title"),
                description: self.i18n.t("terminal.kitty_file_transmission.description"),
                cancel: self.i18n.t("terminal.kitty_file_transmission.cancel"),
                allow: self.i18n.t("terminal.kitty_file_transmission.allow"),
                allowed_title: self
                    .i18n
                    .t("terminal.kitty_file_transmission.allowed_title"),
                allowed_description: self
                    .i18n
                    .t("terminal.kitty_file_transmission.allowed_description"),
                failed_title: self.i18n.t("terminal.kitty_file_transmission.failed_title"),
                failed_description: self
                    .i18n
                    .t("terminal.kitty_file_transmission.failed_description"),
            },
            autosuggest_labels: TerminalAutosuggestLabels {
                history_source: self.i18n.t("terminal.command_bar.source_history"),
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
                manage_triggers: self.i18n.t("terminal.command_selection.manage_triggers"),
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
            },
            tmux_labels: TerminalTmuxLabels {
                tmux: self.i18n.t("terminal.tmux.tmux"),
                initializing: self.i18n.t("terminal.tmux.initializing"),
                previous_window: self.i18n.t("terminal.tmux.previous_window"),
                next_window: self.i18n.t("terminal.tmux.next_window"),
                new_session: self.i18n.t("terminal.tmux.new_session"),
                close_session: self.i18n.t("terminal.tmux.close_session"),
                new_window: self.i18n.t("terminal.tmux.new_window"),
                split_horizontal: self.i18n.t("terminal.tmux.split_horizontal"),
                split_vertical: self.i18n.t("terminal.tmux.split_vertical"),
                close_pane: self.i18n.t("terminal.tmux.close_pane"),
                close_window: self.i18n.t("terminal.tmux.close_window"),
                detach: self.i18n.t("terminal.tmux.detach"),
                resize_left: self.i18n.t("terminal.tmux.resize_left"),
                resize_right: self.i18n.t("terminal.tmux.resize_right"),
                resize_up: self.i18n.t("terminal.tmux.resize_up"),
                resize_down: self.i18n.t("terminal.tmux.resize_down"),
                cancel_mode: self.i18n.t("terminal.tmux.cancel_mode"),
                command_failed: self.i18n.t("terminal.tmux.command_failed"),
                rename_session: self.i18n.t("terminal.tmux.rename_session"),
                rename_window: self.i18n.t("terminal.tmux.rename_window"),
                command: self.i18n.t("terminal.tmux.command"),
                command_prompt: self.i18n.t("terminal.tmux.command_prompt"),
                command_placeholder: self.i18n.t("terminal.tmux.command_placeholder"),
                name_placeholder: self.i18n.t("terminal.tmux.name_placeholder"),
                confirm: self.i18n.t("terminal.tmux.confirm"),
                cancel: self.i18n.t("terminal.tmux.cancel"),
            },
            session_log_options: Some(TerminalSessionLogOptions {
                directory: session_log_directory,
                include_control_sequences: session_log_settings.include_control_sequences,
                retention_days: session_log_settings.retention_days.max(0) as u64,
                max_file_bytes: (session_log_settings.max_file_size_mib.max(1) as u64)
                    .saturating_mul(1024 * 1024),
                file_name_template: session_log_settings.file_name_template.clone(),
                content_template: session_log_settings.content_template.clone(),
                file_mode: session_log_settings.file_mode,
                context: TerminalSessionLogContext::default(),
            }),
            session_log_automatic: session_log_settings.automatic,
            session_log_labels: TerminalSessionLogLabels {
                start_failed: self.i18n.t("terminal.session_log.start_failed"),
                write_failed: self.i18n.t("terminal.session_log.write_failed"),
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
                let tx = self.overlay.read(cx).notice_sender();
                Arc::new(move |notice| {
                    let _ = tx.send(notice);
                })
            }),
            highlight_rules: terminal_highlight_rules(terminal.effective_highlight_rules()),
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

pub(in crate::workspace) fn terminal_preference_overrides(
    options: ConnectionTerminalOptions,
    terminal_settings: &oxideterm_settings::TerminalSettings,
) -> TerminalUiPreferenceOverrides {
    let semantic_scheme_id = options.semantic_scheme.clone();
    let semantic_scheme = options.semantic_scheme.as_deref().and_then(|id| match id {
        "balanced" => Some(resolved_terminal_semantic_scheme(
            oxideterm_settings::TerminalSemanticScheme::Balanced,
            None,
        )),
        "conservative" => Some(resolved_terminal_semantic_scheme(
            oxideterm_settings::TerminalSemanticScheme::Conservative,
            None,
        )),
        custom_id => terminal_settings
            .custom_semantic_schemes
            .iter()
            .find(|scheme| scheme.id == custom_id)
            .map(|scheme| {
                resolved_terminal_semantic_scheme(
                    oxideterm_settings::TerminalSemanticScheme::Balanced,
                    Some(scheme),
                )
            }),
    });
    let highlight_rule_set = options
        .highlight_rule_set
        .as_deref()
        .and_then(|id| terminal_settings.highlight_rule_set(id));
    let highlight_rule_set_id = highlight_rule_set.map(|rule_set| rule_set.id.clone());
    let highlight_rules =
        highlight_rule_set.map(|rule_set| terminal_highlight_rules(&rule_set.rules));
    TerminalUiPreferenceOverrides {
        terminal_encoding: options.encoding.map(terminal_encoding_from_connection),
        backspace_sequence: options
            .backspace_sequence
            .map(terminal_backspace_sequence_from_connection),
        delete_sequence: options
            .delete_sequence
            .map(terminal_delete_sequence_from_connection),
        semantic_scheme,
        semantic_scheme_id,
        highlight_rules,
        highlight_rule_set_id,
        semantic_shell: None,
        local_shell_id: None,
        session_log_available: match options.session_log_policy {
            ConnectionTerminalSessionLogPolicy::Disabled => Some(false),
            ConnectionTerminalSessionLogPolicy::Automatic
            | ConnectionTerminalSessionLogPolicy::Manual => Some(true),
            ConnectionTerminalSessionLogPolicy::Inherit => None,
        },
        session_log_automatic: match options.session_log_policy {
            ConnectionTerminalSessionLogPolicy::Disabled => Some(false),
            ConnectionTerminalSessionLogPolicy::Automatic => Some(true),
            ConnectionTerminalSessionLogPolicy::Manual => Some(false),
            ConnectionTerminalSessionLogPolicy::Inherit => None,
        },
        session_log_context: None,
    }
}

pub(in crate::workspace) fn terminal_highlight_rules(
    rules: &[HighlightRule],
) -> Arc<[UiHighlightRule]> {
    Arc::from(
        rules
            .iter()
            .map(|rule| UiHighlightRule {
                id: rule.id.clone(),
                pattern: rule.pattern.clone(),
                is_regex: rule.is_regex,
                case_sensitive: rule.case_sensitive,
                foreground: rule.foreground.clone(),
                background: rule.background.clone(),
                render_mode: match rule.render_mode {
                    HighlightRuleRenderMode::Background => TerminalHighlightRenderMode::Background,
                    HighlightRuleRenderMode::Underline => TerminalHighlightRenderMode::Underline,
                    HighlightRuleRenderMode::Outline => TerminalHighlightRenderMode::Outline,
                },
                match_scope: match rule.match_scope {
                    HighlightRuleMatchScope::Match => TerminalHighlightMatchScope::Match,
                    HighlightRuleMatchScope::LogicalLine => {
                        TerminalHighlightMatchScope::LogicalLine
                    }
                },
                preserve_background: rule.preserve_background,
                enabled: rule.enabled,
                priority: rule.priority,
            })
            .collect::<Vec<_>>(),
    )
}

fn semantic_shell_dialect(shell_id: &str) -> SemanticShellDialect {
    let shell_id = shell_id.to_ascii_lowercase();
    if shell_id.contains("powershell") || shell_id == "pwsh" {
        SemanticShellDialect::PowerShell
    } else if shell_id.contains("zsh") {
        SemanticShellDialect::Zsh
    } else if shell_id.contains("fish") {
        SemanticShellDialect::Fish
    } else if shell_id.contains("bash") || shell_id.starts_with("wsl") {
        SemanticShellDialect::Bash
    } else {
        SemanticShellDialect::Auto
    }
}

#[cfg(test)]
mod semantic_scheme_tests {
    use super::*;

    #[test]
    fn local_shell_ids_select_the_matching_tree_sitter_dialect() {
        assert_eq!(semantic_shell_dialect("bash"), SemanticShellDialect::Bash);
        assert_eq!(
            semantic_shell_dialect("pwsh"),
            SemanticShellDialect::PowerShell
        );
        assert_eq!(semantic_shell_dialect("zsh"), SemanticShellDialect::Zsh);
        assert_eq!(semantic_shell_dialect("fish"), SemanticShellDialect::Fish);
        assert_eq!(
            semantic_shell_dialect("custom-shell"),
            SemanticShellDialect::Auto
        );
    }

    #[test]
    fn connection_highlight_rule_set_resolves_to_terminal_override() {
        let mut terminal = oxideterm_settings::TerminalSettings::default();
        terminal
            .highlight_rule_sets
            .push(oxideterm_settings::HighlightRuleSet {
                id: "operations".to_string(),
                name: "Operations".to_string(),
                rules: vec![HighlightRule {
                    id: "error".to_string(),
                    pattern: "ERROR".to_string(),
                    ..HighlightRule::default()
                }],
            });

        let overrides = terminal_preference_overrides(
            ConnectionTerminalOptions {
                highlight_rule_set: Some("operations".to_string()),
                ..ConnectionTerminalOptions::default()
            },
            &terminal,
        );

        assert_eq!(
            overrides.highlight_rule_set_id.as_deref(),
            Some("operations")
        );
        assert_eq!(
            overrides
                .highlight_rules
                .as_deref()
                .and_then(|rules| rules.first())
                .map(|rule| rule.pattern.as_str()),
            Some("ERROR")
        );
    }

    #[test]
    fn connection_session_log_policy_controls_availability_and_automatic_start() {
        let terminal = oxideterm_settings::TerminalSettings::default();
        let automatic = terminal_preference_overrides(
            ConnectionTerminalOptions {
                session_log_policy: ConnectionTerminalSessionLogPolicy::Automatic,
                ..ConnectionTerminalOptions::default()
            },
            &terminal,
        );
        assert_eq!(automatic.session_log_available, Some(true));
        assert_eq!(automatic.session_log_automatic, Some(true));

        let manual = terminal_preference_overrides(
            ConnectionTerminalOptions {
                session_log_policy: ConnectionTerminalSessionLogPolicy::Manual,
                ..ConnectionTerminalOptions::default()
            },
            &terminal,
        );
        assert_eq!(manual.session_log_available, Some(true));
        assert_eq!(manual.session_log_automatic, Some(false));

        let disabled = terminal_preference_overrides(
            ConnectionTerminalOptions {
                session_log_policy: ConnectionTerminalSessionLogPolicy::Disabled,
                ..ConnectionTerminalOptions::default()
            },
            &terminal,
        );
        assert_eq!(disabled.session_log_available, Some(false));
        assert_eq!(disabled.session_log_automatic, Some(false));

        let inherited =
            terminal_preference_overrides(ConnectionTerminalOptions::default(), &terminal);
        assert_eq!(inherited.session_log_available, None);
        assert_eq!(inherited.session_log_automatic, None);
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
