use super::*;
use crate::workspace::root::init::terminal_highlight_rules;
use crate::workspace::root::init::terminal_preference_overrides;

const SETTINGS_CONNECTION_IMPORTERS_SECTION_INDEX: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SettingsNavSelectionMotion {
    duration: Duration,
    spatial: bool,
}

fn settings_nav_item_index(groups: &[Vec<SettingsTab>], tab: SettingsTab) -> Option<usize> {
    let mut item_index = 0;
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            item_index += 1;
        }
        for candidate in group {
            if *candidate == tab {
                return Some(item_index);
            }
            item_index += 1;
        }
    }
    None
}

fn settings_nav_selection_motion(tokens: &ThemeTokens) -> Option<SettingsNavSelectionMotion> {
    oxideterm_gpui_ui::segmented_control_motion(tokens).map(|motion| SettingsNavSelectionMotion {
        duration: motion.duration,
        spatial: motion.spatial,
    })
}

fn settings_nav_vertical_offset(
    scroll_handle: &ScrollHandle,
    source_index: usize,
    target_index: usize,
) -> Option<f32> {
    let source_bounds = scroll_handle.bounds_for_item(source_index)?;
    let target_bounds = scroll_handle.bounds_for_item(target_index)?;
    Some(f32::from(source_bounds.origin.y - target_bounds.origin.y))
}

impl WorkspaceApp {
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
        self.settings_section_list_state
            .scroll_to(gpui::ListOffset {
                item_ix: SETTINGS_SECTION_HEADER_ITEM_COUNT
                    + SETTINGS_CONNECTION_IMPORTERS_SECTION_INDEX,
                offset_in_item: px(0.0),
            });
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn close_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let close_active_settings_tab = self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.terminal_trigger_settings_pane = None;
        self.terminal_trigger_shell_confirmation_pending = false;
        self.clear_terminal_trigger_input_focus();
        self.terminal_triggers.cancel_edit();
        self.close_settings_select();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_navigation_editor(cx);
            settings.close_settings_search(true, cx);
        });
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        if close_active_settings_tab {
            self.close_active_tab(window, cx);
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_settings_background = self.settings_background_active();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(if has_settings_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_settings_nav(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .relative()
                    .child(self.render_settings_section_list_scroll(cx)),
            )
            .when_some(
                self.render_settings_select_overlay(cx),
                |surface, overlay| surface.child(overlay),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_section_list_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_settings_section_list_state(cx);
        let state = self.settings_section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.settings_section_list_spec(cx);
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        let transition_id = format!("settings-page-{active_tab:?}");
        // All settings pages now share the same variable-height section list.
        // This matches the browser/TanStack virtualizer direction and avoids
        // keeping a full flex tree mounted just because a tab is inside Settings.
        let list = div()
            .id("settings-content-scroll")
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                this.pause_settings_caret_blink_during_scroll(cx);
                // Tauri only closes an open select on page scroll. When no select is
                // visible, keep wheel scrolling free of state writes so large settings
                // pages do not rebuild just to maintain stale overlay anchors.
                if this.open_settings_select.is_some() {
                    this.close_settings_select();
                    this.clear_settings_select_anchors();
                    cx.notify();
                }
            }))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_settings_section_list_item(index, cx)
                    })
                },
            ));
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            SharedString::from(transition_id),
            list,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }

    pub(in crate::workspace) fn render_settings_section_list_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        if active_tab == SettingsTab::Ai {
            return self.render_settings_ai_section_item(index, cx);
        }

        let section_index = index.saturating_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT);
        let child = if index == 0 {
            self.render_settings_virtual_header(active_tab, cx)
        } else {
            self.render_settings_tab_section(active_tab, section_index, cx)
        };

        self.wrap_settings_section_list_item(index, child, cx)
    }

    pub(in crate::workspace) fn render_settings_ai_section_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = if index == 0 {
            self.render_settings_virtual_header(SettingsTab::Ai, cx)
        } else {
            self.render_settings_ai_page_section(index - 1, cx)
        };

        self.wrap_settings_section_list_item(index, item, cx)
    }

    pub(in crate::workspace) fn render_settings_ai_page_section(
        &mut self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section_index == 0 {
            return self.ai_page_switcher(cx);
        }

        let page_section_index = section_index - 1;
        let ai_page = self.settings_workspace.read(cx).route_snapshot().ai_page;
        match (ai_page, page_section_index) {
            (AiSettingsPage::General, 0) => {
                let settings = self.settings_store.settings();
                self.ai_general_settings_card(settings, cx)
            }
            (AiSettingsPage::General, 1) => self.ai_privacy_settings_card(),
            (AiSettingsPage::Providers, 0) => {
                let provider_views = self.ai_provider_views_for_settings_render(cx);
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_provider_settings_section(&provider_views, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Agents, 0) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_acp_agents_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Context, 0) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_context_controls_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Context, 1) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_system_prompt_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Context, 2) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_memory_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Context, 3) => {
                let settings = self.settings_store.settings();
                let provider_views = ai_provider_views(settings);
                self.ai_disabled_settings_card(
                    self.ai_model_context_windows_section(settings, &provider_views, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Tools, 0) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_tool_use_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Tools, 1) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_skills_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            (AiSettingsPage::Tools, 2) => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_mcp_servers_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn wrap_settings_section_list_item(
        &self,
        index: usize,
        child: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let outer_max_width = self.settings_content_outer_max_width();
        let mut inner = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(outer_max_width))
            .px(px(padding))
            .pb(px(gap));
        if index == 0 {
            inner = inner.pt(px(padding));
        }
        if index + 1 == self.settings_section_list_item_count(cx) {
            inner = inner.pb(px(padding));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .justify_center()
            .child(inner.child(child))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_content_outer_max_width(&self) -> f32 {
        // Native keeps Tauri's padded `mx-auto` settings shell, but uses a wider
        // semantic cap so large desktop windows do not leave every page pinned
        // to the original browser `max-w-4xl` column.
        self.tokens.metrics.settings_content_wide_max_width
            + self.tokens.metrics.settings_content_padding * 2.0
    }

    pub(in crate::workspace) fn render_settings_virtual_header(
        &self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(self.render_settings_page_header(tab, cx))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_views_for_settings_render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<AiProviderView> {
        let provider_views = ai_provider_views(self.settings_store.settings());
        self.ensure_ai_provider_key_statuses_for_views(&provider_views, cx);
        provider_views
    }

    pub(in crate::workspace) fn sync_settings_section_list_state(&mut self, cx: &App) {
        let spec = self.settings_section_list_spec(cx);
        let identity = self.settings_section_list_identity(cx);
        let signatures = self.settings_section_list_signatures(cx);
        sync_tauri_variable_list_state_by_signatures(
            &self.settings_section_list_state,
            &mut self.settings_section_list_cache.borrow_mut(),
            &identity,
            &signatures,
            spec,
        );
    }

    pub(in crate::workspace) fn settings_section_list_spec(
        &self,
        cx: &App,
    ) -> TauriVirtualListSpec {
        if self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::Ai {
            TauriVirtualListSpec::new(
                px(AI_SETTINGS_SECTION_ESTIMATED_HEIGHT),
                SETTINGS_SECTION_LIST_OVERSCAN,
            )
        } else {
            TauriVirtualListSpec::new(
                px(SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT),
                SETTINGS_SECTION_LIST_OVERSCAN,
            )
        }
    }

    pub(in crate::workspace) fn settings_section_list_identity(&self, cx: &App) -> String {
        // Nested settings pages own distinct row sets. Keybinding filtering is
        // handled by per-row signatures so its toolbar can retain animation state.
        let route = self.settings_workspace.read(cx).route_snapshot();
        settings_model_section_list_identity(route.active_tab, route.terminal_page, route.ai_page)
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

        match route.active_tab {
            SettingsTab::General => {
                let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
                launch_at_login.enabled.hash(&mut hasher);
                launch_at_login.pending.hash(&mut hasher);
                launch_at_login.error.hash(&mut hasher);
                settings.general.minimize_to_tray_on_close.hash(&mut hasher);
                settings
                    .general
                    .external_connection_uris_enabled
                    .hash(&mut hasher);
                let cli = self.settings_workspace.read(cx).cli_companion_snapshot();
                cli.loading.hash(&mut hasher);
                cli.error.is_some().hash(&mut hasher);
                cli.status.hash(&mut hasher);
                let app_lock_section_index =
                    if cfg!(any(target_os = "windows", target_os = "macos")) {
                        6
                    } else {
                        5
                    };
                if index
                    == oxideterm_settings_model::SETTINGS_SECTION_HEADER_ITEM_COUNT
                        + app_lock_section_index
                {
                    // Only the application-lock card changes height when its
                    // configured action set switches between one and two buttons.
                    self.app_lock.configured.hash(&mut hasher);
                    settings.sidebar_ui.show_app_lock_icon.hash(&mut hasher);
                }
            }
            SettingsTab::Terminal => {
                format!("{:?}", route.terminal_page).hash(&mut hasher);
                if settings_terminal_focus_handoff_list_item(route.terminal_page, index) {
                    // Selected chips can change width and wrap this card, but
                    // they must not invalidate measurements for every terminal row.
                    settings
                        .terminal
                        .command_bar
                        .focus_handoff_commands
                        .hash(&mut hasher);
                }
                if route.terminal_page == TerminalSettingsPage::Local {
                    settings.local_terminal.oh_my_posh_enabled.hash(&mut hasher);
                    settings.local_terminal.default_shell_id.hash(&mut hasher);
                    self.local_shells.len().hash(&mut hasher);
                }
            }
            SettingsTab::Sftp => {
                settings.sftp.speed_limit_enabled.hash(&mut hasher);
            }
            SettingsTab::Appearance => {
                // App icon selection only changes paint state. Keeping it out
                // of the height signature prevents scroll anchoring from
                // jumping when the icon picker updates its selected badge.
            }
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
            SettingsTab::Help => {
                settings.general.update_channel.hash(&mut hasher);
            }
            SettingsTab::Connections => {
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
                if settings_connection_importers_list_item(index) {
                    // Importer state only changes the final importer card. Invalidating
                    // earlier measured rows makes GPUI move the current scroll anchor.
                    self.settings_workspace
                        .read(cx)
                        .connection_import_list_signature()
                        .hash(&mut hasher);
                }
            }
            SettingsTab::Privilege => {
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
            SettingsTab::Portable => {
                let portable = self.settings_workspace.read(cx).portable_status_snapshot();
                portable.refresh_pending.hash(&mut hasher);
                portable.error.is_some().hash(&mut hasher);
                portable.exportable_secret_count.hash(&mut hasher);
                if let Some(status) = portable.status.as_ref() {
                    status.is_portable.hash(&mut hasher);
                    format!("{:?}", status.status).hash(&mut hasher);
                    status.is_unlocked.hash(&mut hasher);
                    status.auto_unlock_enabled.hash(&mut hasher);
                }
            }
            SettingsTab::Ai => {
                format!("{:?}", route.ai_page).hash(&mut hasher);
                // Hash expansion state only into the virtual row whose height
                // can change. The compact prompt and memory cards stay stable.
                match (route.ai_page, index) {
                    (AiSettingsPage::Providers, 2) => {
                        settings.ai.providers.len().hash(&mut hasher);
                        self.ai_entity
                            .read(cx)
                            .hash_settings_provider_layout(&mut hasher);
                    }
                    (AiSettingsPage::Agents, 2) => {
                        settings.ai.acp_agents.len().hash(&mut hasher);
                    }
                    (AiSettingsPage::Context, 5) => {
                        settings.ai.providers.len().hash(&mut hasher);
                        self.ai_entity
                            .read(cx)
                            .hash_settings_context_layout(&mut hasher);
                    }
                    (AiSettingsPage::Tools, 2) => {
                        self.ai_entity
                            .read(cx)
                            .settings_section_expanded(AiSettingsViewSection::ToolUse)
                            .hash(&mut hasher);
                    }
                    (AiSettingsPage::Tools, 3) => {
                        let registry = self.skill_registry.read();
                        for skill in registry.records() {
                            skill.id.hash(&mut hasher);
                            skill.enabled.hash(&mut hasher);
                            skill.content_hash.hash(&mut hasher);
                        }
                        registry.diagnostics().len().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            SettingsTab::Knowledge => {
                let ai = self.ai_entity.read(cx);
                ai.knowledge_selected_collection_id().hash(&mut hasher);
                ai.knowledge_error().is_some().hash(&mut hasher);
                ai.knowledge_import_progress().hash(&mut hasher);
                ai.knowledge_embedding_progress().hash(&mut hasher);
                ai.knowledge_reindex_progress().hash(&mut hasher);
            }
            SettingsTab::Keybindings => {
                // The toolbar owns the moving scope indicator. Keep row zero
                // mounted while filtered table rows are replaced underneath it.
                if index > 0 {
                    let keybinding_state = self.settings_workspace.read(cx);
                    format!("{:?}", keybinding_state.keybinding_scope_filter()).hash(&mut hasher);
                    keybinding_state
                        .keybinding_search_query()
                        .trim()
                        .hash(&mut hasher);
                }
                settings.keybindings.overrides.len().hash(&mut hasher);
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
        ]
        .into_iter()
        .filter(|scope| {
            crate::keybindings::ACTION_DEFINITIONS
                .iter()
                .filter(|definition| definition.scope == *scope)
                .filter(|definition| {
                    settings_keybinding_scope_matches(scope_filter, definition.scope)
                })
                .any(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.i18n.t(&definition.label_key()).to_lowercase();
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
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Section virtualization only pays off if item rendering is lazy.
        // Dispatch by section index instead of constructing the old full
        // settings Vec and discarding every non-visible card.
        match tab {
            SettingsTab::General => self.settings_general_section(section_index, cx),
            SettingsTab::Portable => self.settings_portable_section(section_index, cx),
            SettingsTab::Terminal => self.settings_terminal_section(section_index, cx),
            SettingsTab::Appearance => self.settings_appearance_section(section_index, cx),
            SettingsTab::Connections => self.settings_connections_section(section_index, cx),
            SettingsTab::Privilege => {
                self.settings_privilege_credentials_section(section_index, cx)
            }
            SettingsTab::Network => self.settings_network_section(section_index, cx),
            SettingsTab::Sftp => self.settings_sftp_section(section_index, cx),
            SettingsTab::Ide => self.settings_ide_section(section_index, cx),
            SettingsTab::Ai => div().into_any_element(),
            SettingsTab::Knowledge => self.settings_knowledge_section(section_index, cx),
            SettingsTab::Keybindings => self.settings_keybindings_section(section_index, cx),
            SettingsTab::Help => self.settings_help_section(section_index, cx),
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

    pub(in crate::workspace) fn render_settings_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let settings_search_open = self.settings_workspace.read(cx).settings_search_open();
        let settings_nav_scroll = self.selectable_text_scroll_handle("settings-nav-scroll");
        let settings_nav_width = self.tokens.metrics.settings_nav_width;
        let navigation_layout = SettingsNavigationLayout::from_persisted_groups(
            &self.settings_store.settings().settings_navigation.groups,
        );
        let navigation_groups = navigation_layout.groups();
        let mut nav = div()
            .w(px(settings_nav_width))
            .min_w(px(settings_nav_width))
            .h_full()
            .flex_none()
            // Mirrors Tauri's `min-h-0` settings sidebar contract: the title
            // stays fixed and the tab list owns vertical overflow instead of
            // forcing the sidebar to grow with every added settings category.
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .pb_4()
            .bg(self.settings_panel_background(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border));

        nav = nav.child(
            div()
                .flex_none()
                .h(px(48.0))
                .px(px(20.0))
                .mb(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(theme.border))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text_heading))
                .child(self.i18n.t("settings_view.title"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            self.workspace_tooltip_icon_button(
                                LucideIcon::Search,
                                15.0,
                                rgb(if settings_search_open {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }),
                                IconButtonOptions {
                                    background: settings_search_open
                                        .then(|| self.settings_panel_background(theme.bg_active)),
                                    hover_background: Some(rgb(theme.bg_hover)),
                                    ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                },
                                self.i18n.t("settings_view.search.open"),
                                "settings-search",
                                true,
                                cx.listener(|this, _event, window, cx| {
                                    this.toggle_settings_search(window, cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ),
                        )
                        .child(self.workspace_tooltip_icon_button(
                            LucideIcon::ListTree,
                            15.0,
                            rgb(theme.text_muted),
                            IconButtonOptions {
                                hover_background: Some(rgb(theme.bg_hover)),
                                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                            },
                            self.i18n.t("settings_view.navigation_editor.open"),
                            "settings-navigation-editor",
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.open_settings_navigation_editor(cx);
                                cx.stop_propagation();
                            }),
                            cx.entity(),
                        )),
                ),
        );

        if settings_search_open {
            return nav
                .child(self.render_settings_search_panel(cx))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-nav-scroll")
            .size_full()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(&settings_nav_scroll)
            .px_3()
            .flex()
            .flex_col();

        for (group_index, group) in navigation_groups.iter().enumerate() {
            if group_index > 0 {
                list = list.child(
                    div()
                        .flex_none()
                        .py_2()
                        .child(separator(&self.tokens, SeparatorOrientation::Horizontal)),
                );
            }
            for tab in group {
                list = list.child(self.render_settings_nav_item(
                    *tab,
                    navigation_groups,
                    &settings_nav_scroll,
                    cx,
                ));
            }
        }

        nav.child(div().flex_1().min_h(px(0.0)).relative().child(list).child(
            selectable_vertical_scrollbar_layer("settings-nav-scrollbar", &settings_nav_scroll),
        ))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_nav_item(
        &self,
        tab: SettingsTab,
        navigation_groups: &[Vec<SettingsTab>],
        settings_nav_scroll: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.settings_workspace.read(cx).route_snapshot().active_tab == tab;
        let nav_item_index = settings_nav_item_index(navigation_groups, tab);
        let navigation_groups_for_click = navigation_groups.to_vec();
        let selection_transition = active.then_some(()).and_then(|()| {
            self.segmented_control_user_transition(
                selection_motion::SETTINGS_NAVIGATION_ID,
                nav_item_index?,
            )
        });
        let transition_scroll_handle = settings_nav_scroll.clone();
        let selection_surface = active.then(|| {
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(self.settings_panel_background(theme.bg_panel));
            let surface = oxideterm_gpui_ui::theme_card_surface_shadow(surface, &self.tokens);

            let Some((generation, vertical_offset_y)) = selection_transition else {
                return surface.into_any_element();
            };
            let Some(motion) = settings_nav_selection_motion(&self.tokens) else {
                return surface.into_any_element();
            };

            let animation_id = (
                gpui::ElementId::from(selection_motion::SETTINGS_NAVIGATION_ID),
                format!("selection-{generation}"),
            );
            if motion.spatial
                && let Some(vertical_offset_y) = vertical_offset_y
            {
                // The measured item bounds include real group separators and
                // flex growth, so the indicator crosses groups continuously.
                return surface
                    .with_animation(
                        animation_id,
                        Animation::new(motion.duration)
                            .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                        move |surface, progress| {
                            let offset =
                                oxideterm_gpui_ui::motion::lerp(vertical_offset_y, 0.0, progress);
                            // Moving both edges preserves the absolute
                            // indicator's height throughout the transition.
                            surface.top(px(offset)).bottom(px(-offset))
                        },
                    )
                    .into_any_element();
            }

            surface
                .with_animation(
                    animation_id,
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                    |surface, progress| surface.opacity(progress),
                )
                .into_any_element()
        });
        div()
            // Keep selection and hover surfaces stable when the settings
            // window has spare vertical space.
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .mb(px(4.0))
            .px_3()
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(self.tokens.radii.md))
            .bg(rgba(0x00000000))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(if active {
                theme.text_heading
            } else {
                theme.text
            }))
            .cursor_pointer()
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(rgba((theme.bg_hover << 8) | 0x80))
                }
            })
            .when_some(selection_surface, |item, surface| item.child(surface))
            .child(div().flex_none().child(Self::render_lucide_icon(
                settings_tab_lucide(tab.icon()),
                18.0,
                rgb(theme.accent),
            )))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t(tab.label_key())),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let active_tab = this.settings_workspace.read(cx).route_snapshot().active_tab;
                    if active_tab != tab
                        && let Some(source_index) =
                            settings_nav_item_index(&navigation_groups_for_click, active_tab)
                        && let Some(target_index) =
                            settings_nav_item_index(&navigation_groups_for_click, tab)
                    {
                        let vertical_offset_y = settings_nav_vertical_offset(
                            &transition_scroll_handle,
                            source_index,
                            target_index,
                        );
                        this.begin_user_segmented_control_transition_with_vertical_offset(
                            selection_motion::SETTINGS_NAVIGATION_ID,
                            target_index,
                            vertical_offset_y,
                            cx,
                        );
                    }
                    this.settings_workspace
                        .update(cx, |settings, cx| settings.set_active_tab(tab, cx));
                    this.close_settings_select();
                    this.focused_settings_input = None;
                    this.settings_slider_drag = None;
                    this.clear_ime_selection();
                    if tab == SettingsTab::General {
                        this.refresh_cli_companion_status(cx);
                        #[cfg(not(target_os = "macos"))]
                        this.refresh_launch_at_login_status(cx);
                    }
                    if tab == SettingsTab::Portable {
                        this.refresh_portable_settings_snapshot(true, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_page_header(
        &self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(tab.title_key());
        let description = self.i18n.t(tab.description_key());
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    // Settings headers are page chrome, so they stay outside the
                    // selectable document while preserving the full-width CJK layout.
                    .line_height(px(30.0))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "settings-page-title",
                        tab.title_key(),
                        title,
                        self.tokens.ui.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px((self.tokens.metrics.ui_text_base + 6.0).max(20.0)))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "settings-page-description",
                        tab.description_key(),
                        description,
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .when(tab == SettingsTab::Keybindings, |header| {
                let note = self.i18n.t("settings_view.keybindings.intl_keyboard_note");
                header.child(
                    div()
                        .mt(px(2.0))
                        .w_full()
                        .min_w(px(0.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xb3))
                        .line_height(px((self.tokens.metrics.ui_text_xs + 4.0).max(16.0)))
                        .child(self.render_selectable_text_scoped(
                            "settings-keybindings-note",
                            "keybindings",
                            note,
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
            })
            .into_any_element()
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
        let previous_settings = self.settings_store.settings().clone();
        let settings_path = self.settings_store.path().to_path_buf();
        let connection_path = self.connection_store.path().to_path_buf();
        let next_settings = SettingsStore::load_from_path(settings_path)
            .map_err(|error| format!("Failed to reload settings after external sync: {error}"))?;
        let next_connections = ConnectionStore::load(connection_path).map_err(|error| {
            format!("Failed to reload connections after external sync: {error}")
        })?;
        let settings = next_settings.settings().clone();
        self.settings_store = next_settings;
        self.connection_store = next_connections;
        self.sync_ssh_config_sync_service();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
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
        oxideterm_desktop_presence::set_keep_running_on_close(
            settings.general.minimize_to_tray_on_close,
        );
        self.tokens = tokens_from_settings(&settings);
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
        if let Some(group_id) = self.terminal.read(cx).selected_broadcast_group_id() {
            if settings
                .terminal
                .broadcast_groups
                .iter()
                .any(|group| group.id == group_id)
            {
                let targets = self.resolve_terminal_broadcast_group(group_id, cx);
                self.terminal.update(cx, |terminal, _cx| {
                    terminal.refresh_selected_broadcast_group(group_id, &targets);
                });
            } else {
                self.terminal.update(cx, |terminal, _cx| {
                    terminal.clear_selected_broadcast_group();
                });
            }
        }
        self.ssh_registry.set_idle_timeout(Some(Duration::from_secs(
            settings.connection_pool.idle_timeout_secs as u64,
        )));
        self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.configure_reconnect(
                settings.reconnect.enabled,
                reconnect_timing_from_settings(&settings),
                reconnect_max_attempts_from_settings(&settings),
                cx,
            );
            runtime.configure_remote_shell_integration(
                settings.terminal.remote_shell_integration_mode,
                settings.terminal.command_bar.current_directory_awareness,
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
        // Tauri's IDE reads Settings.ide live from settingsStore. Native IDE
        // surfaces keep their own GPUI owners, so push typography/wrap/autosave
        // changes into each open surface after the settings store changes.
        self.apply_ide_runtime_settings_to_surfaces(cx);
        self.sync_terminal_command_sender_appearance(cx);
        self.sync_active_terminal_metadata_context(cx);
    }

    pub(in crate::workspace) fn emit_native_plugin_settings_events(
        &mut self,
        previous_settings: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        if previous_settings.terminal.theme != settings.terminal.theme {
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
                serde_json::json!({
                    "theme": crate::workspace::plugin_lifecycle::native_plugin_theme_snapshot(
                        &settings.terminal.theme
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
            // Tauri exposes app.onSettingsChange as an application-level
            // snapshot callback. Native sends the same immutable snapshot over
            // the plugin event channel after persistence succeeds.
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
                serde_json::json!({ "settings": current_value }),
                cx,
            );
        }
    }
}

fn settings_connection_importers_list_item(list_index: usize) -> bool {
    list_index
        .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
        .is_some_and(|section_index| section_index == SETTINGS_CONNECTION_IMPORTERS_SECTION_INDEX)
}

const SETTINGS_TERMINAL_FOCUS_HANDOFF_SECTION_INDEX: usize = 1;

fn settings_terminal_focus_handoff_list_item(
    terminal_page: TerminalSettingsPage,
    list_index: usize,
) -> bool {
    terminal_page == TerminalSettingsPage::CommandBar
        && list_index
            .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
            .is_some_and(|section_index| {
                section_index == SETTINGS_TERMINAL_FOCUS_HANDOFF_SECTION_INDEX
            })
}
