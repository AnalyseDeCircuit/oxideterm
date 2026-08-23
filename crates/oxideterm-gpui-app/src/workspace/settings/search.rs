use super::*;

const SETTINGS_SEARCH_RESULT_LIMIT: usize = 40;
const SETTINGS_SEARCH_INPUT_HEIGHT: f32 = 36.0;

#[derive(Clone, Debug)]
struct SettingsSearchResult {
    tab: SettingsTab,
    terminal_page: Option<TerminalSettingsPage>,
    ai_page: Option<AiSettingsPage>,
    section_index: usize,
    label: String,
    breadcrumb: String,
    score: usize,
}

#[derive(Clone, Copy)]
struct SettingsSearchEntrySpec {
    tab: SettingsTab,
    terminal_page: Option<TerminalSettingsPage>,
    ai_page: Option<AiSettingsPage>,
    section_index: usize,
    title_key: &'static str,
    search_keys: &'static [&'static str],
}

// Keep each card index aligned with the section order rendered for its tab or subpage.
const fn settings_search_entry(
    tab: SettingsTab,
    section_index: usize,
    title_key: &'static str,
    search_keys: &'static [&'static str],
) -> SettingsSearchEntrySpec {
    SettingsSearchEntrySpec {
        tab,
        terminal_page: None,
        ai_page: None,
        section_index,
        title_key,
        search_keys,
    }
}

const fn terminal_search_entry(
    page: TerminalSettingsPage,
    section_index: usize,
    title_key: &'static str,
    search_keys: &'static [&'static str],
) -> SettingsSearchEntrySpec {
    SettingsSearchEntrySpec {
        tab: SettingsTab::Terminal,
        terminal_page: Some(page),
        ai_page: None,
        section_index,
        title_key,
        search_keys,
    }
}

const fn ai_search_entry(
    page: AiSettingsPage,
    section_index: usize,
    title_key: &'static str,
    search_keys: &'static [&'static str],
) -> SettingsSearchEntrySpec {
    SettingsSearchEntrySpec {
        tab: SettingsTab::Ai,
        terminal_page: None,
        ai_page: Some(page),
        section_index,
        title_key,
        search_keys,
    }
}

fn settings_search_specs() -> Vec<SettingsSearchEntrySpec> {
    let mut specs = vec![
        settings_search_entry(
            SettingsTab::General,
            0,
            "settings_view.general.language",
            &["settings_view.general.language_hint"],
        ),
        settings_search_entry(
            SettingsTab::General,
            1,
            "settings_view.general.data_directory",
            &["settings_view.general.data_directory_hint"],
        ),
        settings_search_entry(
            SettingsTab::General,
            2,
            "settings_view.general.cli_companion",
            &[
                "settings_view.general.cli_tool",
                "settings_view.general.cli_tool_hint",
            ],
        ),
        settings_search_entry(
            SettingsTab::General,
            3,
            "settings_view.general.startup",
            &[
                "settings_view.general.startup_hint",
                "settings_view.general.launch_at_login",
            ],
        ),
        settings_search_entry(
            SettingsTab::General,
            4,
            "settings_view.general.connection_uri_integration",
            &[
                "settings_view.general.connection_uri_integration_hint",
                "settings_view.general.external_connection_uris",
                "settings_view.general.external_connection_uris_hint",
            ],
        ),
        settings_search_entry(
            SettingsTab::Portable,
            0,
            "settings_view.general.portable_runtime",
            &[
                "settings_view.general.portable_runtime_hint",
                "settings_view.general.portable_biometric",
            ],
        ),
        settings_search_entry(
            SettingsTab::Portable,
            0,
            "settings_view.general.portable_migration",
            &[
                "settings_view.general.portable_migration_installed_hint",
                "settings_view.general.portable_migration_portable_hint",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Display,
            1,
            "settings_view.terminal.font",
            &[
                "settings_view.terminal.font_family",
                "settings_view.terminal.cjk_font_family",
                "settings_view.terminal.font_ligatures",
                "settings_view.terminal.font_size",
                "settings_view.terminal.line_height",
                "settings_view.terminal.smooth_scroll",
                "settings_view.terminal.encoding",
                "settings_view.terminal.show_performance_overlay",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Display,
            2,
            "settings_view.terminal.cursor",
            &[
                "settings_view.terminal.cursor_style",
                "settings_view.terminal.cursor_blink",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Display,
            3,
            "settings_view.terminal.command_marks",
            &[
                "settings_view.terminal.command_marks_hint",
                "settings_view.terminal.command_marks_hover_actions",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Display,
            4,
            "settings_view.terminal.buffer",
            &[
                "settings_view.terminal.scrollback",
                "settings_view.terminal.scrollback_hint",
                "settings_view.terminal.highlight_tab_on_new_output",
                "settings_view.terminal.highlight_tab_on_new_output_hint",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Input,
            1,
            "settings_view.terminal.input_safety",
            &[
                "settings_view.terminal.paste_protection",
                "settings_view.terminal.osc52_clipboard",
                "settings_view.terminal.smart_copy",
                "settings_view.terminal.copy_on_select",
                "settings_view.terminal.middle_click_paste",
                "settings_view.terminal.right_click_paste",
                "settings_view.terminal.open_links_with_modifier",
                "settings_view.terminal.detect_file_paths_as_links",
                "settings_view.terminal.selection_requires_shift",
                "settings_view.terminal.backspace_sequence",
                "settings_view.terminal.delete_sequence",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Local,
            1,
            "settings_view.local_terminal.shell",
            &[
                "settings_view.local_terminal.default_shell",
                "settings_view.local_terminal.default_cwd",
                "settings_view.local_terminal.git_bash_path",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Local,
            2,
            "settings_view.local_terminal.shell_profile",
            &[
                "settings_view.local_terminal.load_shell_profile",
                "settings_view.local_terminal.custom_env",
                "settings_view.local_terminal.oh_my_posh",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Local,
            3,
            "settings_view.local_terminal.privilege_credentials",
            &["settings_view.local_terminal.privilege_credentials_hint"],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Local,
            4,
            "settings_view.local_terminal.available_shells",
            &["settings_view.local_terminal.select_shell"],
        ),
        terminal_search_entry(
            TerminalSettingsPage::CommandBar,
            1,
            "settings_view.terminal.command_bar",
            &[
                "settings_view.terminal.command_bar_git_status",
                "settings_view.terminal.command_bar_project_tasks",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::CommandBar,
            2,
            "settings_view.terminal.command_bar_focus_handoff",
            &["settings_view.terminal.command_bar_focus_handoff_hint"],
        ),
        terminal_search_entry(
            TerminalSettingsPage::CommandBar,
            3,
            "settings_view.terminal.quick_commands",
            &[
                "settings_view.terminal.quick_bar",
                "settings_view.terminal.quick_commands_confirm",
                "settings_view.terminal.quick_commands_toast",
                "settings_view.terminal.command_specs",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Awareness,
            1,
            "settings_view.terminal.awareness_title",
            &[
                "settings_view.terminal.awareness_enabled",
                "settings_view.connections.shell_integration.mode_label",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Awareness,
            2,
            "settings_view.connections.shell_integration.title",
            &[
                "settings_view.connections.shell_integration.description",
                "settings_view.connections.shell_integration.status",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Awareness,
            3,
            "settings_view.terminal.triggers.title",
            &[
                "settings_view.terminal.triggers.description",
                "settings_view.terminal.triggers.shell_execution",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Transfer,
            1,
            "settings_view.terminal.in_band_transfer.title",
            &[
                "settings_view.terminal.in_band_transfer.enabled",
                "settings_view.terminal.in_band_transfer.allow_directory",
                "settings_view.terminal.in_band_transfer.max_chunk_bytes",
                "settings_view.terminal.in_band_transfer.max_file_count",
                "settings_view.terminal.in_band_transfer.max_total_bytes",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Logging,
            1,
            "settings_view.terminal.session_log_title",
            &[
                "settings_view.terminal.session_log_automatic",
                "settings_view.terminal.session_log_file_name_template",
                "settings_view.terminal.session_log_file_mode",
                "settings_view.terminal.session_log_content_template",
                "settings_view.terminal.session_log_control_sequences",
                "settings_view.terminal.session_log_retention_days",
                "settings_view.terminal.session_log_max_file_size",
                "settings_view.terminal.session_log_directory",
            ],
        ),
        terminal_search_entry(
            TerminalSettingsPage::Highlight,
            1,
            "settings_view.terminal.highlight_rules.title",
            &[
                "settings_view.terminal.highlight_rules.description",
                "settings_view.terminal.highlight_rules.rule_set",
                "settings_view.terminal.highlight_rules.rule_set_hint",
                "settings_view.terminal.highlight_rules.semantic_coloring",
                "settings_view.terminal.highlight_rules.semantic_coloring_hint",
                "settings_view.terminal.highlight_rules.semantic_scheme",
                "settings_view.terminal.highlight_rules.semantic_scheme_hint",
                "settings_view.terminal.highlight_rules.semantic_scheme_balanced",
                "settings_view.terminal.highlight_rules.semantic_scheme_conservative",
                "settings_view.terminal.highlight_rules.pattern",
                "settings_view.terminal.highlight_rules.foreground",
                "settings_view.terminal.highlight_rules.background",
                "settings_view.terminal.highlight_rules.match_scope",
                "settings_view.terminal.highlight_rules.preserve_background",
            ],
        ),
        settings_search_entry(
            SettingsTab::Appearance,
            0,
            "settings_view.appearance.theme",
            &[
                "settings_view.appearance.color_theme",
                "settings_view.custom_theme.create",
                "settings_view.appearance.theme_import",
            ],
        ),
        settings_search_entry(
            SettingsTab::Appearance,
            1,
            "settings_view.appearance.layout",
            &[
                "settings_view.appearance.density",
                "settings_view.appearance.border_radius",
                "settings_view.appearance.ui_font",
                "settings_view.appearance.ui_font_size",
                "settings_view.appearance.window_opacity",
                "settings_view.appearance.animation",
                "settings_view.appearance.render_profile",
                "settings_view.appearance.frosted_glass",
            ],
        ),
        settings_search_entry(
            SettingsTab::Appearance,
            2,
            "settings_view.appearance.app_icon",
            &["settings_view.appearance.app_icon_variant"],
        ),
        settings_search_entry(
            SettingsTab::Appearance,
            3,
            "settings_view.terminal.bg_title",
            &[
                "settings_view.terminal.bg_enabled",
                "settings_view.terminal.bg_scope",
                "settings_view.terminal.bg_opacity",
                "settings_view.terminal.bg_fit",
                "settings_view.terminal.bg_blur",
                "settings_view.terminal.bg_tabs",
            ],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            0,
            "settings_view.ssh_keys.title",
            &[
                "settings_view.ssh_keys.local_section",
                "settings_view.ssh_keys.managed_section",
                "settings_view.ssh_keys.import_file",
                "settings_view.ssh_keys.paste_key",
            ],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            1,
            "settings_view.connections.title",
            &[
                "settings_view.connections.default_username",
                "settings_view.connections.default_port",
            ],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            2,
            "settings_view.connections.idle_timeout.title",
            &[
                "settings_view.connections.idle_timeout.label",
                "settings_view.connections.idle_timeout.hint",
            ],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            3,
            "settings_view.reconnect.title",
            &["settings_view.reconnect.description"],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            4,
            "settings_view.connections.ssh_config.title",
            &[
                "settings_view.connections.ssh_config.auto_load",
                "settings_view.connections.ssh_config.auto_sync",
                "settings_view.connections.ssh_config.allow_proxy_command",
            ],
        ),
        settings_search_entry(
            SettingsTab::Connections,
            5,
            "settings_view.connections.importers.title",
            &[
                "settings_view.connections.importers.source",
                "settings_view.connections.importers.paths",
                "settings_view.connections.importers.target_group",
                "settings_view.connections.importers.duplicate",
            ],
        ),
        settings_search_entry(
            SettingsTab::Privilege,
            0,
            "settings_view.privilege_credentials.title",
            &["settings_view.privilege_credentials.description"],
        ),
        settings_search_entry(
            SettingsTab::Network,
            0,
            "settings_view.network.shared_proxy",
            &["settings_view.network.shared_proxy_hint"],
        ),
        settings_search_entry(
            SettingsTab::Network,
            1,
            "settings_view.network.routing",
            &["settings_view.network.routing_hint"],
        ),
        settings_search_entry(
            SettingsTab::Network,
            2,
            "settings_view.network.public_mcp",
            &["settings_view.network.public_mcp_hint"],
        ),
        settings_search_entry(
            SettingsTab::Sftp,
            0,
            "settings_view.sftp.protocol",
            &[
                "settings_view.sftp.concurrent",
                "settings_view.sftp.directory_parallelism",
            ],
        ),
        settings_search_entry(
            SettingsTab::Sftp,
            1,
            "settings_view.sftp.bandwidth",
            &[
                "settings_view.sftp.speed_limit",
                "settings_view.sftp.bandwidth_hint",
            ],
        ),
        settings_search_entry(
            SettingsTab::Sftp,
            2,
            "settings_view.sftp.conflict",
            &["settings_view.sftp.conflict_hint"],
        ),
        settings_search_entry(
            SettingsTab::Ide,
            0,
            "settings_view.ide.auto_save",
            &["settings_view.ide.auto_save_hint"],
        ),
        settings_search_entry(
            SettingsTab::Ide,
            1,
            "settings_view.ide.word_wrap",
            &["settings_view.ide.word_wrap_hint"],
        ),
        settings_search_entry(
            SettingsTab::Ide,
            2,
            "settings_view.ide.editor_typography",
            &[
                "settings_view.ide.font_size",
                "settings_view.ide.line_height",
            ],
        ),
        settings_search_entry(
            SettingsTab::Ide,
            3,
            "settings_view.ide.agent_title",
            &[
                "settings_view.ide.agent_mode_label",
                "settings_view.ide.agent_path_label",
            ],
        ),
        settings_search_entry(
            SettingsTab::Ide,
            4,
            "settings_view.ide.agent_transparency_title",
            &[
                "settings_view.ide.agent_privacy_label",
                "settings_view.ide.agent_lifecycle_label",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::General,
            1,
            "settings_view.ai.general",
            &["settings_view.ai.enable", "settings_view.ai.enable_hint"],
        ),
        ai_search_entry(
            AiSettingsPage::General,
            2,
            "settings_view.ai.privacy_notice",
            &["settings_view.ai.privacy_text"],
        ),
        ai_search_entry(
            AiSettingsPage::Providers,
            1,
            "settings_view.ai.provider_settings",
            &[
                "settings_view.ai.provider_settings_summary",
                "settings_view.ai.api_key",
                "settings_view.ai.base_url",
                "settings_view.ai.model",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Agents,
            1,
            "settings_view.ai.acp_agents",
            &[
                "settings_view.ai.acp_agents_summary",
                "settings_view.ai.acp_agent_command",
                "settings_view.ai.acp_agent_cwd",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Context,
            1,
            "settings_view.ai.context_controls",
            &[
                "settings_view.ai.max_context",
                "settings_view.ai.context_sources",
                "settings_view.ai.embedding_title",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Context,
            2,
            "settings_view.ai.system_prompt_title",
            &["settings_view.ai.system_prompt_hint"],
        ),
        ai_search_entry(
            AiSettingsPage::Context,
            3,
            "settings_view.ai.memory_title",
            &[
                "settings_view.ai.memory_hint",
                "settings_view.ai.memory_enabled",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Context,
            4,
            "settings_view.ai.model_context_windows",
            &[
                "settings_view.ai.model_context_windows_hint",
                "settings_view.ai.max_response_tokens",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Tools,
            1,
            "settings_view.ai.tool_use",
            &[
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_max_rounds",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Tools,
            2,
            "settings_view.ai.skills_title",
            &[
                "settings_view.ai.skills_hint",
                "settings_view.ai.skills_enabled",
            ],
        ),
        ai_search_entry(
            AiSettingsPage::Tools,
            3,
            "settings_view.mcp.title",
            &["settings_view.mcp.description"],
        ),
        settings_search_entry(
            SettingsTab::Knowledge,
            0,
            "settings_view.knowledge.collections",
            &[
                "settings_view.knowledge.create_description",
                "settings_view.knowledge.file_filter_documents",
                "settings_view.knowledge.configure_embeddings",
            ],
        ),
        settings_search_entry(
            SettingsTab::Keybindings,
            0,
            "settings_view.keybindings.title",
            &[
                "settings_view.keybindings.description",
                "settings_view.keybindings.search_placeholder",
            ],
        ),
        settings_search_entry(
            SettingsTab::Help,
            0,
            "settings_view.help.version_info",
            &[
                "settings_view.help.update_channel_hint",
                "settings_view.help.check_update",
                "settings_view.help.release_notes",
                "settings_view.help.channel_stable",
            ],
        ),
        settings_search_entry(
            SettingsTab::Help,
            1,
            "settings_view.help.diagnostics",
            &[
                "settings_view.help.debug_logs",
                "settings_view.help.memory_diagnostics_title",
            ],
        ),
        settings_search_entry(SettingsTab::Help, 2, "settings_view.help.tech_stack", &[]),
        settings_search_entry(
            SettingsTab::Help,
            3,
            "settings_view.help.resources",
            &[
                "settings_view.help.documentation",
                "settings_view.help.github",
                "settings_view.help.issues",
            ],
        ),
        settings_search_entry(
            SettingsTab::Help,
            4,
            "settings_view.help.safety_title",
            &[
                "settings_view.help.safety_privacy",
                "settings_view.help.safety_secrets",
                "settings_view.help.safety_ai",
            ],
        ),
        settings_search_entry(
            SettingsTab::Help,
            5,
            "settings_view.help.disclaimer",
            &[
                "settings_view.help.copyright",
                "settings_view.help.legal_notice_description",
                "settings_view.help.license",
            ],
        ),
    ];

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    specs.push(settings_search_entry(
        SettingsTab::General,
        5,
        "settings_view.general.window_behavior",
        &["settings_view.general.window_behavior_hint"],
    ));
    specs.push(settings_search_entry(
        SettingsTab::General,
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            6
        } else {
            5
        },
        "settings_view.general.app_lock_title",
        &[
            "settings_view.general.app_lock_description",
            "settings_view.general.app_lock_show_sidebar_icon",
        ],
    ));
    specs
}

fn settings_search_score(title: &str, haystack: &str, query: &str) -> Option<usize> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return None;
    }
    let normalized_title = title.to_lowercase();
    let normalized_haystack = haystack.to_lowercase();
    let terms = normalized_query.split_whitespace().collect::<Vec<_>>();
    if !terms.iter().all(|term| normalized_haystack.contains(term)) {
        return None;
    }

    // Prefer direct card-title matches while still allowing localized row labels and hints.
    let score = if normalized_title == normalized_query {
        0
    } else if normalized_title.starts_with(&normalized_query) {
        1
    } else if normalized_title
        .split_whitespace()
        .any(|word| word.starts_with(&normalized_query))
    {
        2
    } else if normalized_title.contains(&normalized_query) {
        3
    } else {
        5
    };
    Some(score)
}

fn settings_search_results(i18n: &I18n, query: &str) -> Vec<SettingsSearchResult> {
    let mut results = settings_search_specs()
        .into_iter()
        .filter_map(|spec| {
            let label = i18n.t(spec.title_key);
            let mut search_text =
                format!("{} {}", label, spec.title_key.replace(['.', '_', '-'], " "));
            for key in spec.search_keys {
                search_text.push(' ');
                search_text.push_str(&i18n.t(key));
                search_text.push(' ');
                search_text.push_str(&key.replace(['.', '_', '-'], " "));
            }
            let score = settings_search_score(&label, &search_text, query)?;
            let mut breadcrumb = i18n.t(spec.tab.label_key());
            if let Some(page) = spec.terminal_page {
                breadcrumb.push_str(" · ");
                breadcrumb.push_str(&i18n.t(page.label_key()));
            }
            if let Some(page) = spec.ai_page {
                breadcrumb.push_str(" · ");
                breadcrumb.push_str(&i18n.t(page.label_key()));
            }
            Some(SettingsSearchResult {
                tab: spec.tab,
                terminal_page: spec.terminal_page,
                ai_page: spec.ai_page,
                section_index: spec.section_index,
                label,
                breadcrumb,
                score,
            })
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.label.chars().count().cmp(&right.label.chars().count()))
            .then_with(|| left.tab.id().cmp(right.tab.id()))
            .then_with(|| left.section_index.cmp(&right.section_index))
            .then_with(|| left.label.cmp(&right.label))
    });
    results.truncate(SETTINGS_SEARCH_RESULT_LIMIT);
    results
}

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_settings_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.read(cx).settings_search_open() {
            self.close_settings_search(cx);
            return;
        }

        self.settings_workspace
            .update(cx, SettingsWorkspaceEntity::open_settings_search);
        self.focus_settings_input(SettingsInput::SettingsSearch, String::new(), cx);
        self.ime_marked_text = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_settings_search(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_settings_search(true, cx);
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn activate_first_settings_search_result(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let query = self
            .settings_workspace
            .read(cx)
            .settings_search_query()
            .to_string();
        let Some(result) = settings_search_results(&self.i18n, &query)
            .into_iter()
            .next()
        else {
            return false;
        };
        self.activate_settings_search_result(result, cx);
        true
    }

    fn activate_settings_search_result(
        &mut self,
        result: SettingsSearchResult,
        cx: &mut Context<Self>,
    ) {
        let tab = result.tab;
        // Knowledge displays a transient error card before its indexed content cards.
        let knowledge_error_offset = usize::from(
            tab == SettingsTab::Knowledge
                && self.settings_dynamic_section_counts(cx).knowledge_has_error,
        );
        let target_section_index = result.section_index + knowledge_error_offset;
        if result.terminal_page == Some(TerminalSettingsPage::Awareness)
            && result.section_index == 3
        {
            self.terminal_trigger_settings_pane = None;
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(tab, cx);
            if let Some(page) = result.terminal_page {
                settings.set_terminal_page(page, cx);
            }
            if let Some(page) = result.ai_page {
                settings.set_ai_page(page, cx);
            }
            settings.close_settings_search(true, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.ime_marked_text = None;
        if tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
            #[cfg(not(target_os = "macos"))]
            self.refresh_launch_at_login_status(cx);
        }
        if tab == SettingsTab::Portable {
            self.refresh_portable_settings_snapshot(true, cx);
        }
        self.sync_settings_section_list_state(cx);
        self.settings_section_list_state
            .scroll_to(gpui::ListOffset {
                item_ix: SETTINGS_SECTION_HEADER_ITEM_COUNT + target_section_index,
                offset_in_item: px(0.0),
            });
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_search_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_workspace.read(cx);
        let query = settings.settings_search_query();
        let focused =
            settings.settings_entity_focused_input() == Some(SettingsInput::SettingsSearch);
        let target = WorkspaceImeTarget::Settings(SettingsInput::SettingsSearch);
        let workspace = cx.entity();
        let search_input =
            text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: query,
                        placeholder: self.i18n.t("settings_view.search.placeholder"),
                        focused,
                        caret_visible: self.input_caret.visible(),
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target, cx),
                        marked_text: self.marked_text_for_target(target, cx),
                    },
                )
                .relative()
                .w_full()
                .h(px(SETTINGS_SEARCH_INPUT_HEIGHT))
                .pl(px(34.0))
                .pr(px(if query.is_empty() { 12.0 } else { 34.0 }))
                .cursor(CursorStyle::IBeam)
                .child(div().absolute().left(px(12.0)).top(px(10.0)).child(
                    Self::render_lucide_icon(
                        LucideIcon::Search,
                        15.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                ))
                .when(!query.is_empty(), |input| {
                    let clear_workspace = workspace.clone();
                    input.child(div().absolute().right(px(6.0)).top(px(4.0)).child(
                        self.workspace_tooltip_icon_button(
                            LucideIcon::X,
                            13.0,
                            rgb(self.tokens.ui.text_muted),
                            IconButtonOptions {
                                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                            },
                            self.i18n.t("settings_view.search.clear"),
                            "settings-search-clear",
                            true,
                            move |_event, _window, cx| {
                                let _ = clear_workspace.update(cx, |this, cx| {
                                    this.settings_workspace.update(cx, |settings, cx| {
                                        settings.clear_settings_search_query(cx);
                                    });
                                    this.clear_ime_selection();
                                    this.show_active_input_caret(cx);
                                });
                                cx.stop_propagation();
                            },
                            workspace.clone(),
                        ),
                    ))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.focus_settings_input(SettingsInput::SettingsSearch, String::new(), cx);
                        this.ime_marked_text = None;
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            );
        let results = settings_search_results(&self.i18n, query);
        let result_scroll = self.selectable_text_scroll_handle("settings-search-results-scroll");
        let mut result_list = div()
            .id("settings-search-results-scroll")
            .size_full()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(&result_scroll)
            .px_2()
            .pb_3()
            .flex()
            .flex_col()
            .gap(px(2.0));

        if query.trim().is_empty() || results.is_empty() {
            let message_key = if query.trim().is_empty() {
                "settings_view.search.hint"
            } else {
                "settings_view.search.no_results"
            };
            result_list = result_list.child(
                div()
                    .flex_1()
                    .min_h(px(120.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        20.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(self.i18n.t(message_key)),
            );
        } else {
            for result in results {
                let tab = result.tab;
                let action_result = result.clone();
                let row = entity_list_row(
                    &self.tokens,
                    EntityListRowOptions::new()
                        .compact()
                        .has_background_image(self.settings_background_active()),
                    Some(
                        Self::render_lucide_icon(
                            settings_tab_lucide(tab.icon()),
                            16.0,
                            rgb(self.tokens.ui.accent),
                        )
                        .into_any_element(),
                    ),
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(result.label)
                        .into_any_element(),
                    Some(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(result.breadcrumb)
                            .into_any_element(),
                    ),
                    Vec::new(),
                    Vec::new(),
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.activate_settings_search_result(action_result.clone(), cx);
                        cx.stop_propagation();
                    }),
                );
                result_list = result_list.child(row);
            }
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(div().flex_none().px_3().pb_3().child(search_input))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(result_list)
                    .child(selectable_vertical_scrollbar_layer(
                        "settings-search-results-scrollbar",
                        &result_scroll,
                    )),
            )
            .into_any_element()
    }
}
