use super::super::*;
use super::init::{terminal_highlight_rules, terminal_preference_overrides};
use oxideterm_network_proxy::install_application_proxy_policy_from_settings;
use oxideterm_settings::PersistedSettings;

impl WorkspaceApp {
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
        self.theme_transition = None;
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
        self.ide_workspace.update(cx, |ide, _cx| {
            ide.set_automation_mode(crate::workspace::ide::node_agent_mode_from_settings(
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
            .ai.state
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
        self.ai.state.update(cx, |ai, _cx| {
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

    pub(in crate::workspace) fn apply_terminal_preferences_to_panes(
        &mut self,
        cx: &mut Context<Self>,
    ) {
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
}
