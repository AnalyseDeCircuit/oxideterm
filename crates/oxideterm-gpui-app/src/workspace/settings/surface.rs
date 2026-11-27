use super::*;

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
        self.settings_workspace
            .update(cx, |settings, _| settings.set_slider_drag(None));
        self.clear_ime_selection();
        self.sync_settings_section_list_state(cx);
        self.settings_workspace
            .read(cx)
            .section_list
            .state()
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
        if self.onboarding_disclaimer_locked() {
            return;
        }
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
        self.settings_workspace
            .update(cx, |settings, _| settings.set_slider_drag(None));
        self.clear_ime_selection();
        self.sync_settings_section_list_state(cx);
        // Target the importer row directly so callers do not merely land at
        // the top of a long Connections settings page.
        self.settings_workspace
            .read(cx)
            .section_list
            .scroll_to_section(
                SettingsTab::Connections,
                SettingsSectionId::ConnectionsImporters,
                self.settings_dynamic_section_counts(cx),
            );
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
        self.settings_workspace
            .update(cx, |settings, _| settings.set_slider_drag(None));
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
        self.blur_highlight_input(
            oxideterm_gpui_settings_view::highlight_page::is_highlight_input,
            cx,
        );
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
        self.settings_workspace
            .update(cx, |settings, _| settings.set_slider_drag(None));
        if close_active_settings_tab {
            if let Some(tab_id) = self.active_content_tab_id(cx) {
                self.close_tab_by_id(tab_id, window, cx);
            }
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn refresh_settings_tab_services(
        &mut self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) {
        match tab {
            SettingsTab::General => {
                self.refresh_cli_companion_status(cx);
                #[cfg(not(target_os = "macos"))]
                self.refresh_launch_at_login_status(cx);
            }
            SettingsTab::Portable => self.refresh_portable_settings_snapshot(true, cx),
            _ => {}
        }
    }

    pub(in crate::workspace) fn render_settings_surface(&self) -> AnyElement {
        self.settings_views.surface.clone().into_any_element()
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
        let state = self.settings_workspace.read(cx);
        let settings = self.settings_store.settings();
        let tab = state.route_snapshot().active_tab;
        let (clients, approvals) = if tab == SettingsTab::Network {
            (self.public_mcp.clients(), self.public_mcp.approvals())
        } else {
            (Vec::new(), Vec::new())
        };
        oxideterm_gpui_settings_view::surface::SettingsSectionLayout {
            state,
            settings,
            connections: &self.connection_store,
            app_lock_configured: self.app_lock.configured,
            shell_count: self.local_shells.len(),
            mcp_clients: &clients,
            mcp_approvals: &approvals,
            mcp_credential_ready: self.public_mcp.revealed_credential().is_some(),
            mcp_startup_error: self.public_mcp.startup_error().is_some(),
        }
        .signatures(
            self.settings_dynamic_section_counts(cx),
            |tab, section, hasher| {
                self.hash_runtime_settings_layout(tab, section, hasher, cx);
            },
        )
    }

    fn hash_runtime_settings_layout(
        &self,
        tab: SettingsTab,
        section: Option<SettingsSectionId>,
        hasher: &mut DefaultHasher,
        cx: &App,
    ) {
        match tab {
            SettingsTab::Ai => {
                oxideterm_gpui_settings_view::ai::hash_section_layout(
                    section,
                    self.settings_workspace.read(cx),
                    self.settings_store.settings(),
                    self.settings_views.ai_providers.read(cx),
                    &self.ai.skills.read(),
                    hasher,
                );
            }
            SettingsTab::Knowledge => {
                let ai = self.knowledge_page(cx).read(cx);
                ai.knowledge_selected_collection_id().hash(hasher);
                ai.knowledge_document_page_index().hash(hasher);
                ai.knowledge_stats_cache_key().hash(hasher);
                ai.knowledge_embedding_config_expanded().hash(hasher);
                ai.knowledge_error().is_some().hash(hasher);
                ai.knowledge_import_progress().hash(hasher);
                ai.knowledge_embedding_progress().hash(hasher);
                ai.knowledge_reindex_progress().hash(hasher);
            }
            SettingsTab::Keybindings => {
                for entry in &self
                    .plugin_entity
                    .read(cx)
                    .registry()
                    .contributions()
                    .runtime_keybindings
                {
                    entry.plugin_id.hash(hasher);
                    entry.normalized_keybinding.hash(hasher);
                    entry.label.hash(hasher);
                }
            }
            _ => {}
        }
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
            knowledge_has_error: self.knowledge_page(cx).read(cx).knowledge_error().is_some(),
            knowledge_has_selected_collection,
        }
    }

    pub(in crate::workspace) fn visible_keybinding_scope_count(&self, cx: &App) -> usize {
        oxideterm_gpui_settings_view::keybinding_page::visible_scope_count(
            &self.keybinding_definitions(cx),
            self.settings_workspace.read(cx),
            |definition| self.keybinding_label(definition),
        )
    }

    pub(in crate::workspace) fn knowledge_has_selected_collection(&self, cx: &App) -> bool {
        let rag_store = self.ai.state.read(cx).rag_store();
        let collections = oxideterm_ai::rag_list_collections(&rag_store, None).unwrap_or_default();
        self.knowledge_page(cx)
            .read(cx)
            .knowledge_selected_collection_id()
            .filter(|id| collections.iter().any(|collection| collection.id == *id))
            .or_else(|| collections.first().map(|collection| collection.id.as_str()))
            .is_some()
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
        self.settings_workspace
            .update(cx, |state, _| state.acknowledge_external_store_state());
        self.refresh_ai_settings_views(&previous_settings, &settings, cx);
        self.emit_native_plugin_settings_events(&previous_settings, &settings, cx);
        self.sync_tab_titles(cx);
        self.notify_onboarding_view(cx);
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
                self.connection_store
                    .reload_after_external_change()
                    .map_err(|error| {
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
        self.reconcile_discovery_sources(cx);
        // External sync mutates persisted stores outside the GPUI controls.
        // Re-apply the same runtime side effects used by edit_settings instead
        // of relying on stale in-memory settings or browser-style stores.
        self.apply_loaded_settings_to_runtime(&previous_settings, &settings, cx);
        self.refresh_ai_settings_views(&previous_settings, &settings, cx);
        self.refresh_ai_skill_registry(cx);
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

    fn refresh_ai_settings_views(
        &self,
        previous: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        if settings.ai != previous.ai {
            self.settings_workspace.update(cx, |_, cx| cx.notify());
            self.settings_views.ai_mcp.update(cx, |_, cx| cx.notify());
        }
    }
}
