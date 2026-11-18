use super::*;
use crate::workspace::root::init::terminal_highlight_rules;
use crate::workspace::root::init::terminal_preference_overrides;


impl WorkspaceApp {
    pub(in crate::workspace) fn open_appearance_settings(
        &mut self,
        scheme: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.appearance_edit_dark = scheme.or(settings.appearance_edit_dark);
            settings.set_active_tab(SettingsTab::Appearance, cx);
            cx.notify();
        });
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.sync_settings_section_list_state(cx);
        self.settings_workspace.read(cx).section_list.state()
            .scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            });
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn open_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_tab(window, cx);
    }

    pub(in crate::workspace) fn open_connection_importers_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Connections, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.sync_settings_section_list_state(cx);
        // Target the importer row directly so callers do not merely land at
        // the top of a long Connections settings page.
        if let Some(item_ix) = settings_section_list_item_index(
            SettingsTab::Connections,
            SettingsSectionId::ConnectionsImporters,
            self.settings_dynamic_section_counts(cx),
        ) {
            self.settings_workspace.read(cx).section_list.state()
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
        }
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn open_knowledge_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Knowledge, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.sync_settings_section_list_state(cx);
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn close_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let close_active_settings_tab = self
            .active_content_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.clear_terminal_trigger_input_focus(cx);
        self.blur_highlight_input(oxideterm_gpui_settings_view::highlight_page::is_highlight_input, cx);
        self.close_settings_select();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.terminal_triggers.pane = None;
            settings.terminal_triggers.shell_confirmation_pending = false;
            settings.terminal_triggers.cancel_edit();
            settings.close_navigation_editor(cx);
            settings.totp_draft = None;
            settings.totp_error = None;
            settings.close_settings_search(true, cx);
        });
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        if close_active_settings_tab {
            if let Some(tab_id) = self.active_content_tab_id(cx) {
                self.close_tab_by_id(tab_id, window, cx);
            }
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_surface(&self) -> AnyElement {
        self.settings_surface.clone().into_any_element()
    }

    pub(in crate::workspace) fn render_settings_ai_page_section(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let enabled = settings.ai.enabled;
        let content = match section {
            SettingsSectionId::AiGeneralSettings => {
                return self.ai_general_settings_card(settings, cx);
            }
            SettingsSectionId::AiGeneralPrivacy => return self.ai_privacy_settings_card(),
            SettingsSectionId::AiProvidersSettings => {
                let provider_views = self.ai_provider_views_for_settings_render(cx);
                self.ai_provider_settings_section(&provider_views, cx)
            }
            SettingsSectionId::AiContextControls => self.ai_context_controls_section(settings, cx),
            SettingsSectionId::AiContextSystemPrompt => self.ai_system_prompt_section(settings, cx),
            SettingsSectionId::AiContextMemory => self.ai_memory_section(settings, cx),
            SettingsSectionId::AiContextModelWindows => {
                let provider_views = ai_provider_views(settings);
                self.ai_model_context_windows_section(settings, &provider_views, cx)
            }
            SettingsSectionId::AiToolsToolUse => self.ai_tool_use_section(settings, cx),
            SettingsSectionId::AiToolsWebSearch => self.ai_web_search_section(settings, cx),
            SettingsSectionId::AiToolsAgents => self.render_ai_agent_settings(cx),
            SettingsSectionId::AiToolsSkills => self.ai_skills_section(settings, cx),
            SettingsSectionId::AiToolsMcpServers => self.ai_mcp_servers_section(settings, cx),
            _ => return div().into_any_element(),
        };
        self.ai_settings_availability(content, enabled)
    }

    pub(in crate::workspace) fn ai_provider_views_for_settings_render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<AiProviderView> {
        let provider_views = ai_provider_views(self.settings_store.settings());
        self.ensure_ai_provider_key_statuses_for_views(&provider_views, cx);
        provider_views
    }

    pub(in crate::workspace) fn sync_settings_section_list_state(&self, cx: &App) {
        let state = self.settings_workspace.read(cx);
        state.section_list.sync(
            state.route_snapshot().active_tab,
            self.settings_dynamic_section_counts(cx),
            &self.settings_section_list_signatures(cx),
        );
    }

    pub(in crate::workspace) fn settings_section_list_signatures(&self, cx: &App) -> Vec<u64> {
        (0..self.settings_section_list_item_count(cx))
            .map(|index| self.settings_section_signature(index, cx))
            .collect()
    }

    pub(in crate::workspace) fn settings_section_signature(&self, index: usize, cx: &App) -> u64 {
        let mut hasher = DefaultHasher::new();
        // GPUI caches variable-row measurements. Hash only states that can
        // change section height so ListState remeasures affected rows without
        // serializing the entire settings file on every scroll render.
        let route = self.settings_workspace.read(cx).route_snapshot();
        format!("{:?}", route.active_tab).hash(&mut hasher);
        index.hash(&mut hasher);
        let settings = self.settings_store.settings();
        let section = index
            .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
            .and_then(|section_index| {
                settings_tab_section_at(
                    route.active_tab,
                    section_index,
                    self.settings_dynamic_section_counts(cx),
                )
            });

        oxideterm_gpui_settings_view::surface::hash_settings_section_layout(
            self.settings_workspace.read(cx),
            settings,
            section,
            self.app_lock.configured,
            self.local_shells.len(),
            &mut hasher,
        );

        match route.active_tab {
            SettingsTab::Network => {
                settings.network.upstream_proxy.is_some().hash(&mut hasher);
                settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| matches!(proxy.auth, SettingsUpstreamProxyAuth::Password { .. }))
                    .hash(&mut hasher);
                settings
                    .network
                    .upstream_proxy_disclaimer_accepted
                    .hash(&mut hasher);
                settings.network.application_proxy_mode.hash(&mut hasher);
                settings.general.update_proxy.mode.hash(&mut hasher);
                settings.general.update_proxy.protocol.hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .network_proxy_layout_flags()
                    .hash(&mut hasher);
                for client in self.public_mcp.clients() {
                    client.client_ref.as_str().hash(&mut hasher);
                    client.label.hash(&mut hasher);
                    client.enabled.hash(&mut hasher);
                    client.approval_mode.hash(&mut hasher);
                    client.tool_groups.hash(&mut hasher);
                }
                for approval in self.public_mcp.approvals().iter().filter(|approval| {
                    approval.status == oxideterm_public_mcp::ApprovalStatus::Pending
                }) {
                    // A different frozen action can have a different wrapped command height.
                    approval.approval_ref.as_str().hash(&mut hasher);
                }
                self.public_mcp
                    .revealed_credential()
                    .is_some()
                    .hash(&mut hasher);
                self.public_mcp.startup_error().is_some().hash(&mut hasher);
            }
            SettingsTab::Connections => {
                if section == Some(SettingsSectionId::ConnectionsKnownHosts) {
                    let state = &self.settings_workspace.read(cx).known_hosts;
                    state.query.hash(&mut hasher);
                    state.task.is_some().hash(&mut hasher);
                    state.error.hash(&mut hasher);
                    state
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.entries.len())
                        .hash(&mut hasher);
                    state
                        .pending_delete
                        .as_ref()
                        .map(|entry| (&entry.host, entry.port, &entry.fingerprint))
                        .hash(&mut hasher);
                }
                self.connection_store.connections().len().hash(&mut hasher);
                self.connection_store
                    .managed_ssh_keys()
                    .len()
                    .hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .managed_key_status()
                    .is_some()
                    .hash(&mut hasher);
                if section == Some(SettingsSectionId::ConnectionsImporters) {
                    // Importer state only changes the importer card. Invalidating
                    // earlier measured rows makes GPUI move the current scroll anchor.
                    self.settings_workspace
                        .read(cx)
                        .connection_import_list_signature()
                        .hash(&mut hasher);
                }
            }
            SettingsTab::Privilege => {
                self.connection_store
                    .list_privilege_credentials(SSH_COMMON_PRIVILEGE_SCOPE_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0)
                    .hash(&mut hasher);
                self.connection_store
                    .totp_credentials()
                    .len()
                    .hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .totp_draft
                    .is_some()
                    .hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .totp_error
                    .is_some()
                    .hash(&mut hasher);
                self.connection_store.connections().len().hash(&mut hasher);
                self.connection_store
                    .connections()
                    .iter()
                    .map(|connection| connection.privilege_credentials.len())
                    .sum::<usize>()
                    .hash(&mut hasher);
                self.connection_store
                    .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0)
                    .hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .privilege_layout_flags()
                    .hash(&mut hasher);
            }
            SettingsTab::Ai => {
                format!("{:?}", route.ai_page).hash(&mut hasher);
                // Hash expansion state only into the virtual row whose height
                // can change. The compact prompt and memory cards stay stable.
                match section {
                    Some(SettingsSectionId::AiProvidersSettings) => {
                        settings.ai.providers.len().hash(&mut hasher);
                        settings.ai.web_search.enabled.hash(&mut hasher);
                        settings.ai.web_search.provider_id.hash(&mut hasher);
                        self.ai_entity
                            .read(cx)
                            .hash_settings_provider_layout(&mut hasher);
                    }
                    Some(SettingsSectionId::AiContextModelWindows) => {
                        settings.ai.providers.len().hash(&mut hasher);
                        self.ai_entity
                            .read(cx)
                            .hash_settings_context_layout(&mut hasher);
                    }
                    Some(SettingsSectionId::AiToolsToolUse) => {
                        settings.ai.tool_use.disabled_tools.hash(&mut hasher);
                        self.ai_entity
                            .read(cx)
                            .settings_section_expanded(AiSettingsViewSection::ToolUse)
                            .hash(&mut hasher);
                    }
                    Some(SettingsSectionId::AiToolsWebSearch) => {
                        settings.ai.web_search.enabled.hash(&mut hasher);
                        settings.ai.web_search.provider_id.hash(&mut hasher);
                        settings.ai.providers.len().hash(&mut hasher);
                    }
                    Some(SettingsSectionId::AiToolsAgents) => {
                        settings.ai.tool_use.enabled.hash(&mut hasher);
                        let ai = self.ai_entity.read(cx);
                        ai.agents.settings_model_picker_open.hash(&mut hasher);
                        let options = self.global_agent_options();
                        options.enabled.hash(&mut hasher);
                        options
                            .default_model
                            .as_ref()
                            .map(|model| (&model.provider_id, &model.model))
                            .hash(&mut hasher);
                        for provider in ai_provider_views(settings) {
                            provider.id.hash(&mut hasher);
                            provider.enabled.hash(&mut hasher);
                            provider.models.hash(&mut hasher);
                        }
                    }
                    Some(SettingsSectionId::AiToolsSkills) => {
                        let state = self.settings_workspace.read(cx);
                        state.skill_search_query.hash(&mut hasher);
                        format!("{:?}", state.skill_scope_filter).hash(&mut hasher);
                        state.skill_diagnostics_expanded.hash(&mut hasher);
                        let registry = self.skill_registry.read();
                        for skill in registry.records() {
                            skill.id.hash(&mut hasher);
                            skill.skill_path.hash(&mut hasher);
                            state
                                .expanded_skill_paths
                                .contains(&skill.skill_path)
                                .hash(&mut hasher);
                            skill.enabled.hash(&mut hasher);
                            skill.content_hash.hash(&mut hasher);
                        }
                        for diagnostic in registry.diagnostics() {
                            diagnostic.path.hash(&mut hasher);
                            diagnostic.message.hash(&mut hasher);
                        }
                    }
                    _ => {}
                }
            }
            SettingsTab::Knowledge => {
                let ai = self.ai_entity.read(cx);
                ai.knowledge_selected_collection_id().hash(&mut hasher);
                ai.knowledge_document_page_index().hash(&mut hasher);
                ai.knowledge_stats_cache_key().hash(&mut hasher);
                ai.knowledge_embedding_config_expanded().hash(&mut hasher);
                ai.knowledge_error().is_some().hash(&mut hasher);
                ai.knowledge_import_progress().hash(&mut hasher);
                ai.knowledge_embedding_progress().hash(&mut hasher);
                ai.knowledge_reindex_progress().hash(&mut hasher);
            }
            SettingsTab::Keybindings => {
                // Keep the search control mounted while scope filtering replaces tables.
                if index > 0 {
                    let keybinding_state = self.settings_workspace.read(cx);
                    format!("{:?}", keybinding_state.keybinding_scope_filter()).hash(&mut hasher);
                    keybinding_state
                        .keybinding_search_query()
                        .trim()
                        .hash(&mut hasher);
                }
                settings.keybindings.overrides.len().hash(&mut hasher);
                for entry in &self
                    .plugin_entity
                    .read(cx)
                    .registry()
                    .contributions()
                    .runtime_keybindings
                {
                    entry.plugin_id.hash(&mut hasher);
                    entry.normalized_keybinding.hash(&mut hasher);
                    entry.label.hash(&mut hasher);
                }
            }
            _ => {}
        }

        hasher.finish()
    }

    pub(in crate::workspace) fn settings_section_list_item_count(&self, cx: &App) -> usize {
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        settings_model_section_list_item_count(active_tab, self.settings_dynamic_section_counts(cx))
    }

    pub(in crate::workspace) fn settings_dynamic_section_counts(
        &self,
        cx: &App,
    ) -> SettingsDynamicSectionCounts {
        let route = self.settings_workspace.read(cx).route_snapshot();
        let knowledge_has_selected_collection = if route.active_tab == SettingsTab::Knowledge {
            self.knowledge_has_selected_collection(cx)
        } else {
            false
        };
        SettingsDynamicSectionCounts {
            terminal_page: route.terminal_page,
            ai_page: route.ai_page,
            visible_keybinding_scope_count: self.visible_keybinding_scope_count(cx),
            knowledge_has_error: self.ai_entity.read(cx).knowledge_error().is_some(),
            knowledge_has_selected_collection,
        }
    }

    pub(in crate::workspace) fn visible_keybinding_scope_count(&self, cx: &App) -> usize {
        let catalog = self.keybinding_definitions(cx);
        let keybinding_state = self.settings_workspace.read(cx);
        let query = keybinding_state
            .keybinding_search_query()
            .trim()
            .to_lowercase();
        let scope_filter = keybinding_state.keybinding_scope_filter();
        [
            crate::keybindings::ActionScope::Global,
            crate::keybindings::ActionScope::Terminal,
            crate::keybindings::ActionScope::Split,
            crate::keybindings::ActionScope::Palette,
            crate::keybindings::ActionScope::Editor,
            crate::keybindings::ActionScope::Sftp,
            crate::keybindings::ActionScope::FileManager,
            crate::keybindings::ActionScope::Preview,
            crate::keybindings::ActionScope::RemoteDesktop,
            crate::keybindings::ActionScope::Plugin,
            crate::keybindings::ActionScope::AiPanel,
        ]
        .into_iter()
        .filter(|scope| {
            catalog
                .iter()
                .filter(|definition| definition.scope == *scope)
                .filter(|definition| {
                    settings_keybinding_scope_matches(scope_filter, definition.scope)
                })
                .any(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.keybinding_label(definition).to_lowercase();
                    label.contains(&query) || definition.id.to_lowercase().contains(&query)
                })
        })
        .count()
    }

    pub(in crate::workspace) fn knowledge_has_selected_collection(&self, cx: &App) -> bool {
        let rag_store = self.ai_entity.read(cx).rag_store();
        let collections = oxideterm_ai::rag_list_collections(&rag_store, None).unwrap_or_default();
        self.ai_entity
            .read(cx)
            .knowledge_selected_collection_id()
            .filter(|id| collections.iter().any(|collection| collection.id == *id))
            .or_else(|| collections.first().map(|collection| collection.id.as_str()))
            .is_some()
    }

    pub(in crate::workspace) fn render_settings_tab_section(
        &mut self,
        tab: SettingsTab,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Section virtualization only pays off if item rendering is lazy.
        // Dispatch by semantic section ID instead of constructing a full Vec
        // or coupling renderers to hardcoded display indices.
        match tab {
            SettingsTab::General => self.settings_general_section(section, cx),
            SettingsTab::Portable => self.settings_portable_section(section, cx),
            SettingsTab::Terminal => self.settings_terminal_section(section, cx),
            SettingsTab::Appearance => self.settings_appearance_section(section, cx),
            SettingsTab::Connections => self.settings_connections_section(section, cx),
            SettingsTab::Privilege => self.settings_privilege_credentials_section(section, cx),
            SettingsTab::Network => self.settings_network_section(section, cx),
            SettingsTab::Sftp => self.with_settings_controls(cx, |controls, cx| {
                controls.settings_sftp_section(section, cx)
            }),
            SettingsTab::Ide => {
                let custom_cjk = self.settings_workspace.read(cx).ide_cjk_font_custom;
                self.with_settings_controls(cx, |controls, cx| {
                    controls.settings_ide_section(section, custom_cjk, cx)
                })
            }
            SettingsTab::Ai => div().into_any_element(),
            SettingsTab::Knowledge => self.settings_knowledge_section(section, cx),
            SettingsTab::Keybindings => self.settings_keybindings_section(section, cx),
            SettingsTab::Help => self.settings_help_section(section, cx),
        }
    }

    pub(in crate::workspace) fn clear_settings_select_anchors(&mut self) {
        self.select_anchors
            .retain(|id, _| matches!(id, SelectAnchorId::NewConnectionGroup));
    }

    pub(in crate::workspace) fn pause_settings_caret_blink_during_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.focused_settings_input.is_none() {
            return;
        }
        // Browser caret blinking is compositor-local. Native blinking repaints
        // the workspace, so keep the caret visible while a settings scroll is
        // active and let blinking resume shortly after inertial scrolling stops.
        let pause_until = Instant::now() + Duration::from_millis(SETTINGS_SCROLL_CARET_PAUSE_MS);
        self.workspace_input.update(cx, |input, cx| {
            input.pause_settings_caret_until(pause_until, cx);
        });
    }


    pub(in crate::workspace) fn edit_settings(
        &mut self,
        edit: impl FnOnce(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) {
        let previous_settings = self.settings_store.settings().clone();
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
        self.apply_loaded_settings_to_runtime(&previous_settings, &settings, cx);
        let _ = self.settings_store.save();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        self.emit_native_plugin_settings_events(&previous_settings, &settings, cx);
        self.sync_tab_titles(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn reload_after_external_sync(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.reload_watched_stores(
            oxideterm_gpui_settings_view::entity::ExternalStoreChange {
                settings: true,
                connections: true,
            },
            cx,
        )?;
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        Ok(())
    }

    pub(super) fn reload_watched_stores(
        &mut self,
        change: oxideterm_gpui_settings_view::entity::ExternalStoreChange,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let previous_settings = self.settings_store.settings().clone();
        let next_settings = change
            .settings
            .then(|| {
                SettingsStore::load_from_path(self.settings_store.path().to_path_buf()).map_err(
                    |error| format!("Failed to reload settings after external sync: {error}"),
                )
            })
            .transpose()?;
        let next_connections = change
            .connections
            .then(|| {
                ConnectionStore::load(self.connection_store.path().to_path_buf()).map_err(|error| {
                    format!("Failed to reload connections after external sync: {error}")
                })
            })
            .transpose()?;
        if let Some(next_settings) = next_settings {
            self.settings_store = next_settings;
        }
        if let Some(next_connections) = next_connections {
            self.connection_store = next_connections;
        }
        let settings = self.settings_store.settings().clone();
        if !change.connections && settings == previous_settings {
            return Ok(());
        }
        self.sync_ssh_config_sync_service();
        // External sync mutates persisted stores outside the GPUI controls.
        // Re-apply the same runtime side effects used by edit_settings instead
        // of relying on stale in-memory settings or browser-style stores.
        self.apply_loaded_settings_to_runtime(&previous_settings, &settings, cx);
        self.refresh_ai_skill_registry();
        self.emit_native_plugin_settings_events(&previous_settings, &settings, cx);
        self.queue_cloud_sync_dirty_refresh(cx);
        self.sync_tab_titles(cx);
        if previous_settings.appearance.window_opacity != settings.appearance.window_opacity {
            // Detached native windows are separate render roots and need an
            // explicit refresh when CLI or cloud sync changes shared opacity.
            cx.refresh_windows();
        }
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn apply_loaded_settings_to_runtime(
        &mut self,
        previous_settings: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        if previous_settings.sidebar_ui.zen_mode != settings.sidebar_ui.zen_mode {
            self.zen_exit.reset();
            self.apply_workspace_overlay_intent(WorkspaceOverlayIntent::ClearZenHint, cx);
        }
        if previous_settings.keybindings != settings.keybindings {
            crate::keybindings::install_context_keybindings(&settings.keybindings.overrides, cx);
            #[cfg(target_os = "macos")]
            {
                let menus = crate::platform::app_menus(settings);
                // Settings callbacks install the new bindings after this settings update.
                cx.defer(move |cx| cx.set_menus(menus));
            }
        }
        install_application_proxy_policy_from_settings(settings, &self.connection_store);
        if previous_settings.appearance.app_icon != settings.appearance.app_icon {
            // Replacing the macOS application icon decodes the bundled image on the main thread,
            // so unrelated form edits must not repeat that work.
            crate::app_icon::install_runtime_app_icon(settings.appearance.app_icon);
        }
        if let Err(error) =
            bundled_fonts::load_terminal_font_open_critical(settings, &cx.text_system())
        {
            eprintln!(
                "failed to load selected bundled terminal font; falling back to system fonts: {error}"
            );
        }
        if let Err(error) =
            bundled_fonts::load_terminal_font_explicit_secondary_faces(settings, &cx.text_system())
        {
            eprintln!(
                "failed to load selected secondary bundled terminal fonts; falling back to system fonts: {error}"
            );
        }
        self.i18n
            .set_locale(locale_from_settings(settings.general.language));
        if previous_settings.terminal.autosuggest != settings.terminal.autosuggest {
            self.terminal_history_store.configure(
                settings.terminal.autosuggest.save_history,
                settings.terminal.autosuggest.history_retention_days,
            );
        }
        if previous_settings.general.language != settings.general.language {
            self.refresh_native_plugin_terminal_hooks(cx);
            cx.set_menus(crate::platform::app_menus(settings));
            oxideterm_gpui_editor::EditorLanguagePlugins::set_labels(
                self.i18n.t("plugin.language_missing"),
                self.i18n.t("plugin.load_failed_default"),
                self.i18n.t("plugin.language_manage"),
                self.i18n.t("plugin.language_dismiss"),
                cx,
            );
        }
        oxideterm_desktop_presence::set_keep_running_on_close(
            settings.general.minimize_to_tray_on_close,
        );
        self.settings_workspace.update(cx, |state, _| {
            state.appearance_edit_dark.get_or_insert_with(|| {
                settings
                    .appearance
                    .active_scheme(self.system_dark)
                    .is_some_and(oxideterm_settings::AppearanceScheme::is_dark)
            });
        });
        // A new settings update supersedes an in-flight appearance transition.
        self.theme_transition_task = None;
        self.tokens = tokens_from_settings(&settings, self.system_dark);
        self.active_background = settings.resolved_background(self.system_dark);
        self.render_policy = compute_render_policy(
            self.render_profile_override
                .unwrap_or(settings.appearance.render_profile),
            &self.detected_graphics,
        );
        // Settings changes can flip the render profile while a modal is open;
        // update the shared backdrop gate before the next top-layer render.
        set_tauri_backdrop_blur_allowed(self.render_policy.allow_background_blur);
        self.sftp_transfer_manager
            .apply_settings(sftp_runtime_settings_from_settings(&settings));
        if !settings.terminal.command_bar.enabled || !settings.terminal.command_bar.project_tasks {
            // Close stale project task UI when the owning awareness feature is disabled.
            self.close_terminal_project_panel(cx);
        }
        if !settings.terminal.command_bar.enabled
            || !settings.terminal.command_bar.current_directory_awareness
            || !settings.terminal.command_bar.show_current_directory
        {
            // CWD picker state is transient command-bar chrome; disabling the
            // feature should not leave an orphaned popover around.
            self.close_terminal_cwd_picker(cx);
        }
        let saved_group_ids = settings
            .terminal
            .broadcast_groups
            .iter()
            .map(|group| group.id)
            .collect();
        self.terminal.update(cx, |terminal, _| {
            // Settings refresh cannot recruit new windows into an already-running group.
            terminal.sync_groups_mut().retain_groups(&saved_group_ids);
            if terminal
                .selected_broadcast_group_id()
                .is_some_and(|id| !saved_group_ids.contains(&id))
            {
                terminal.clear_selected_broadcast_group();
            }
        });
        self.ssh_registry.set_idle_timeout(Some(Duration::from_secs(
            settings.connection_pool.idle_timeout_secs as u64,
        )));
        self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.configure_ssh_latency(
                settings.terminal.command_bar.enabled && settings.terminal.command_bar.ssh_latency,
                settings.terminal.command_bar.ssh_latency_interval_secs,
            );
            runtime.configure_reconnect(
                settings.reconnect.enabled,
                reconnect_timing_from_settings(&settings),
                reconnect_max_attempts_from_settings(&settings),
                cx,
            );
            runtime.configure_remote_shell_integration(
                settings.terminal.remote_shell_integration_mode,
                settings.terminal.command_bar.current_directory_awareness,
                settings.terminal.remote_shell_integration_deferred.clone(),
            );
        });
        self.tab_host.update(cx, |tab_host, _cx| {
            tab_host
                .configure_terminal_output_highlight(settings.terminal.highlight_tab_on_new_output);
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_agent_fs_mode(crate::workspace::ide::node_agent_mode_from_settings(
                &settings,
            ));
        });
        // Monitoring settings own recurring remote shells and page-scoped GPU work.
        self.apply_host_tool_monitoring_settings(cx);
        if !self.tokens.motion.enabled {
            self.disclosure_motions.clear();
        }
        self.sidebar_collapsed = settings.sidebar_ui.collapsed;
        self.sidebar_motion_generation = self.sidebar_motion_generation.wrapping_add(1);
        self.context_sidebar_motion_generation =
            self.context_sidebar_motion_generation.wrapping_add(1);
        self.sidebar_rendered = !settings.sidebar_ui.collapsed;
        self.context_sidebar_rendered = crate::workspace::sidebar::context_sidebar_panel_visible(
            settings.sidebar_ui.ai_sidebar_collapsed,
            settings.sidebar_ui.zen_mode,
            settings.ai.enabled,
            self.active_context_sidebar_panel,
        );
        let viewport_width = self
            .ai_entity
            .read(cx)
            .chat_ui()
            .overlay_window_size
            .map(|size| size.0)
            .unwrap_or(self.tokens.metrics.window_min_width);
        // External settings reloads use the same responsive limits as pointer
        // resizing, so persisted pixel widths cannot bypass the live viewport.
        self.sidebar_width = crate::workspace::sidebar::clamp_responsive_sidebar_width(
            settings.sidebar_ui.width as f32,
            viewport_width,
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        let ai_sidebar_width = crate::workspace::sidebar::clamp_responsive_sidebar_width(
            settings.sidebar_ui.ai_sidebar_width as f32,
            viewport_width,
            AI_SIDEBAR_ABSOLUTE_MIN_WIDTH,
            AI_SIDEBAR_ABSOLUTE_MAX_WIDTH,
        );
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_chat_sidebar_width(ai_sidebar_width);
        });
        self.sidebar_motion.settle(if self.sidebar_rendered {
            self.sidebar_panel_width()
        } else {
            0.0
        });
        self.context_sidebar_motion
            .settle(if self.context_sidebar_rendered {
                ai_sidebar_width
            } else {
                0.0
            });
        self.apply_terminal_preferences_to_panes(cx);
        // Toolbar chips are pane render state outside the preference snapshot;
        // a locale switch changes their labels, so re-push them here.
        self.sync_plugin_toolbar_state(cx);
        // Tauri's IDE reads Settings.ide live from settingsStore. Native IDE
        // surfaces keep their own GPUI owners, so push typography/wrap/autosave
        // changes into each open surface after the settings store changes.
        self.apply_ide_runtime_settings_to_surfaces(cx);
        self.sync_terminal_command_sender_appearance(cx);
        self.sync_search_editor_appearance(cx);
        self.sync_active_terminal_metadata_context(cx);
    }

    pub(super) fn apply_terminal_preferences_to_panes(&mut self, cx: &mut Context<Self>) {
        let panes = self
            .tab_host
            .read(cx)
            .panes()
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(pane_id, cx);
            let retained_overrides = pane.read(cx).preference_overrides_snapshot();
            let session_highlight_rule_set_id = pane
                .read(cx)
                .session_highlight_rule_set_id()
                .map(str::to_string);
            let refreshed_session_highlight_override =
                session_highlight_rule_set_id.and_then(|id| {
                    let terminal = &self.settings_store.settings().terminal;
                    let rules = if id == GLOBAL_HIGHLIGHT_RULE_SET_ID {
                        terminal.effective_highlight_rules()
                    } else {
                        &terminal.highlight_rule_set(&id)?.rules
                    };
                    Some(TerminalHighlightRuleSetOverride {
                        id,
                        rules: terminal_highlight_rules(rules),
                    })
                });
            let local_shell_id = retained_overrides.local_shell_id.clone();
            let session_id = self.tabs(cx).iter().find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.session_id_for_pane(pane_id))
            });
            let ssh_node_id = session_id.and_then(|session_id| {
                self.workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session_id)
            });
            let refreshed_overrides = local_shell_id
                .as_deref()
                .map(|shell_id| {
                    self.terminal_preference_overrides_for_local_shell(Some(&ShellInfo::new(
                        shell_id, shell_id, shell_id,
                    )))
                })
                .or_else(|| {
                    ssh_node_id
                        .as_ref()
                        .map(|node_id| self.terminal_preference_overrides_for_ssh_node(node_id))
                })
                .or_else(|| {
                    let semantic_scheme_id = retained_overrides.semantic_scheme_id.clone();
                    let highlight_rule_set_id = retained_overrides.highlight_rule_set_id.clone();
                    if semantic_scheme_id.is_none() && highlight_rule_set_id.is_none() {
                        return None;
                    }
                    let mut overrides = retained_overrides.clone();
                    let refreshed = terminal_preference_overrides(
                        ConnectionTerminalOptions {
                            semantic_scheme: semantic_scheme_id,
                            highlight_rule_set: highlight_rule_set_id,
                            ..ConnectionTerminalOptions::default()
                        },
                        &self.settings_store.settings().terminal,
                    );
                    overrides.semantic_scheme = refreshed.semantic_scheme;
                    overrides.highlight_rules = refreshed.highlight_rules;
                    Some(overrides)
                });
            let _ = pane.update(cx, |pane, cx| {
                if let Some(overrides) = refreshed_overrides {
                    pane.set_preference_overrides(overrides, preferences.clone(), cx);
                } else {
                    pane.set_preferences(preferences.clone(), cx);
                }
                if pane.session_highlight_rule_set_id().is_some() {
                    pane.set_session_highlight_override(
                        refreshed_session_highlight_override,
                        preferences,
                        cx,
                    );
                }
            });
        }
    }

    pub(in crate::workspace) fn emit_native_plugin_settings_events(
        &mut self,
        previous_settings: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        let previous_theme =
            ThemeTarget::Application.resolved_id(previous_settings, self.system_dark);
        let theme = ThemeTarget::Application.resolved_id(settings, self.system_dark);
        if previous_theme != theme {
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
                serde_json::json!({
                    "theme": crate::workspace::plugin_lifecycle::native_plugin_theme_snapshot(
                        theme
                    ),
                }),
                cx,
            );
        }

        if previous_settings.general.language != settings.general.language {
            let language = settings.general.language.as_str();
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
                serde_json::json!({ "language": language }),
                cx,
            );
        }

        let previous_value =
            serde_json::to_value(previous_settings).unwrap_or_else(|_| serde_json::json!({}));
        let current_value =
            serde_json::to_value(settings).unwrap_or_else(|_| serde_json::json!({}));
        if previous_value != current_value {
            self.emit_native_plugin_app_settings_changed(current_value, cx);
        }
    }

    pub(super) fn emit_native_plugin_app_settings_changed(
        &mut self,
        settings: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        // Native plugins receive the same immutable settings snapshot as app.onSettingsChange.
        self.emit_native_plugin_event_to_subscribers(
            plugin_host::NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
            serde_json::json!({ "settings": settings }),
            cx,
        );
    }
}
