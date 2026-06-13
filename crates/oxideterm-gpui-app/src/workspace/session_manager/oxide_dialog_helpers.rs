fn oxide_settings_section_label(section: &str, i18n: &oxideterm_i18n::I18n) -> String {
    // Keep .oxide section names on the same translation keys as the Tauri
    // import/export modals so sectioned settings previews do not leak Chinese.
    match section {
        "general" => i18n.t("settings_view.general.title"),
        "terminalAppearance" => i18n.t("export.app_settings_section_terminal_appearance"),
        "terminalBehavior" => i18n.t("export.app_settings_section_terminal_behavior"),
        "appearance" => i18n.t("settings_view.appearance.title"),
        "connections" => i18n.t("settings_view.connections.title"),
        "fileAndEditor" => i18n.t("export.app_settings_section_file_editor"),
        "ai" => i18n.t("settings_view.tabs.ai"),
        "localTerminal" => i18n.t("settings_view.local_terminal.title"),
        "legacy" => i18n.t("modals.import.app_settings_legacy_title"),
        _ => section.to_string(),
    }
}

fn oxide_export_connection_count(dialog: &OxideExportDialogState) -> usize {
    let mut ids = dialog.selected_ids.clone();
    if dialog.include_forwards {
        for forward in &dialog.available_forwards {
            if dialog.selected_forward_ids.contains(&forward.id) {
                if let Some(owner_id) = &forward.owner_connection_id {
                    ids.insert(owner_id.clone());
                }
            }
        }
    }
    ids.len()
}

fn oxide_forward_summary(forward: &PersistedForward) -> String {
    let direction = match forward.forward_type {
        ForwardType::Local => "L",
        ForwardType::Remote => "R",
        ForwardType::Dynamic => "D",
    };
    format!(
        "{} {}:{} -> {}:{}",
        direction,
        forward.rule.bind_address,
        forward.rule.bind_port,
        forward.rule.target_host,
        forward.rule.target_port
    )
}

fn oxide_forward_description_or_summary(forward: &PersistedForward) -> String {
    let description = forward.rule.description.trim();
    if description.is_empty() {
        oxide_forward_summary(forward)
    } else {
        description.to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OxidePasswordStrength {
    Weak,
    Fair,
    Strong,
}

fn oxide_password_strength(password: &str) -> OxidePasswordStrength {
    if password.len() < 8 {
        return OxidePasswordStrength::Weak;
    }

    let has_upper = password.chars().any(char::is_uppercase);
    let has_lower = password.chars().any(char::is_lowercase);
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    let has_special = password.chars().any(|ch| !ch.is_ascii_alphanumeric());
    let classes = [has_upper, has_lower, has_digit, has_special]
        .into_iter()
        .filter(|class| *class)
        .count();

    if password.len() >= 12 && classes >= 3 {
        OxidePasswordStrength::Strong
    } else {
        OxidePasswordStrength::Fair
    }
}

fn oxide_password_strength_label(
    strength: OxidePasswordStrength,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    match strength {
        OxidePasswordStrength::Weak => i18n.t("export.password_strength_weak"),
        OxidePasswordStrength::Fair => i18n.t("export.password_strength_fair"),
        OxidePasswordStrength::Strong => i18n.t("export.password_strength_strong"),
    }
}

fn oxide_export_progress_label(
    stage: &str,
    embed_keys: bool,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    let key = match stage {
        "collecting_connections" if embed_keys => "export.stage_reading_keys",
        "serializing_file" => "export.stage_writing",
        "done" => "export.stage_done",
        _ => "export.stage_encrypting",
    };
    i18n.t(key)
}

fn oxide_import_progress_label(
    stage: &str,
    total: usize,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    let key = match stage {
        "parsing_file" => "modals.import.stage_parsing",
        "deriving_key" | "decrypting_payload" | "deserializing_payload" | "verifying_checksum" => {
            "modals.import.stage_decrypting"
        }
        "collecting_existing" | "building_preview" | "analyzing_preview" if total == 8 => {
            "modals.import.stage_analyzing"
        }
        "filtering_selection" | "collecting_existing" | "preparing_connections" => {
            "modals.import.stage_preparing"
        }
        "applying_connections" => "modals.import.stage_applying",
        "saving_config" => "modals.import.stage_saving",
        "done" => "modals.import.stage_done",
        _ => "modals.import.stage_decrypting",
    };
    i18n.t(key)
}

fn oxide_format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn oxide_password_strength_bar_color(
    strength: OxidePasswordStrength,
    index: usize,
    border: u32,
    accent: u32,
) -> Rgba {
    let active = match strength {
        OxidePasswordStrength::Weak => index == 0,
        OxidePasswordStrength::Fair => index < 2,
        OxidePasswordStrength::Strong => true,
    };
    if !active {
        return rgb(border);
    }
    match strength {
        OxidePasswordStrength::Weak => rgb(OXIDE_YELLOW_500),
        OxidePasswordStrength::Fair => rgb(accent),
        OxidePasswordStrength::Strong => rgb(OXIDE_GREEN_500),
    }
}

fn oxide_password_strength_text_color(strength: OxidePasswordStrength, muted: u32) -> Rgba {
    match strength {
        OxidePasswordStrength::Weak => rgb(OXIDE_YELLOW_500),
        OxidePasswordStrength::Fair => rgb(muted),
        OxidePasswordStrength::Strong => rgb(OXIDE_GREEN_500),
    }
}

fn oxide_export_selected_plugin_setting_count(dialog: &OxideExportDialogState) -> usize {
    dialog
        .plugin_groups
        .iter()
        .filter(|(plugin_id, _)| dialog.selected_plugin_ids.contains(*plugin_id))
        .map(|(_, count)| *count)
        .sum()
}

fn oxide_export_has_selected_content(dialog: &OxideExportDialogState) -> bool {
    oxide_export_connection_count(dialog) > 0
        || (dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty())
        || dialog.include_quick_commands
        || dialog.include_serial_profiles
        || (dialog.include_plugin_settings && oxide_export_selected_plugin_setting_count(dialog) > 0)
        || dialog.include_portable_secrets
}

fn oxide_import_has_selected_content(dialog: &OxideImportDialogState) -> bool {
    let Some(preview) = dialog.preview.as_ref() else {
        return false;
    };
    !dialog.selected_names.is_empty()
        || (preview.has_app_settings
            && dialog.import_app_settings
            && !dialog.selected_app_settings_sections.is_empty())
        || (preview.has_quick_commands && dialog.import_quick_commands)
        || (preview.serial_profiles_count > 0 && dialog.import_serial_profiles)
        || (preview.plugin_settings_count > 0
            && dialog.import_plugin_settings
            && !dialog.selected_plugin_ids.is_empty())
        || (preview.total_forwards > 0 && dialog.import_forwards)
        || (preview.portable_secret_count > 0 && dialog.import_portable_secrets)
}

fn oxide_import_footer_actions(dialog: &OxideImportDialogState) -> Vec<OxideDialogFooterAction> {
    // Tauri dialog footers use normal DOM tab order. Model only rendered
    // footer buttons so preview/result stages do not expose hidden actions.
    if dialog.result.is_some() {
        vec![OxideDialogFooterAction::Primary]
    } else if dialog.preview.is_some() {
        vec![
            OxideDialogFooterAction::Secondary,
            OxideDialogFooterAction::Primary,
        ]
    } else {
        vec![
            OxideDialogFooterAction::Secondary,
            OxideDialogFooterAction::Cancel,
            OxideDialogFooterAction::Primary,
        ]
    }
}

const OXIDE_IMPORT_FOOTER_BODY_INPUTS: [SessionManagerInput; 1] =
    [SessionManagerInput::OxideImportPassword];
const OXIDE_EXPORT_FOOTER_BODY_INPUTS: [SessionManagerInput; 3] = [
    SessionManagerInput::OxideExportDescription,
    SessionManagerInput::OxideExportPassword,
    SessionManagerInput::OxideExportConfirmPassword,
];

fn oxide_import_footer_body_inputs(dialog: &OxideImportDialogState) -> &'static [SessionManagerInput] {
    // The decrypt password field is the only text input in the import stage
    // before preview/result. Keep its focus edge explicit so footer Tab order
    // does not leave the IME/input owner active behind a focused footer button.
    if dialog.file_data.is_some() && dialog.preview.is_none() && dialog.result.is_none() {
        &OXIDE_IMPORT_FOOTER_BODY_INPUTS
    } else {
        &[]
    }
}

fn oxide_export_footer_body_inputs(dialog: &OxideExportDialogState) -> &'static [SessionManagerInput] {
    // Tauri export modal body tab order reaches description before password
    // and confirm-password. Native tracks that order explicitly because GPUI
    // does not provide DOM tab stops for these custom-rendered inputs.
    if dialog.busy {
        &[]
    } else {
        &OXIDE_EXPORT_FOOTER_BODY_INPUTS
    }
}

fn import_preview_selectable_names(preview: &ImportPreview) -> HashSet<String> {
    let mut names = HashSet::new();
    names.extend(preview.unchanged.iter().cloned());
    names.extend(preview.will_rename.iter().map(|(original, _)| original.clone()));
    names.extend(preview.will_skip.iter().cloned());
    names.extend(preview.will_replace.iter().cloned());
    names.extend(preview.will_merge.iter().cloned());
    names
}

fn oxide_settings_field_label(field: &str, i18n: &oxideterm_i18n::I18n) -> String {
    // These mappings mirror Tauri's OxideImportModal field formatter.
    match field {
        "language" => i18n.t("settings_view.general.language"),
        "updateChannel" => i18n.t("settings_view.general.update_channel"),
        "theme" => i18n.t("settings_view.appearance.theme"),
        "fontFamily" => i18n.t("settings_view.terminal.font_family"),
        "customFontFamily" => i18n.t("settings_view.terminal.custom_font_stack"),
        "fontSize" => i18n.t("settings_view.terminal.font_size"),
        "lineHeight" => i18n.t("settings_view.terminal.line_height"),
        "cursorStyle" => i18n.t("settings_view.terminal.cursor_style"),
        "cursorBlink" => i18n.t("settings_view.terminal.cursor_blink"),
        "backgroundEnabled" => i18n.t("settings_view.terminal.bg_enabled"),
        "backgroundImage" => i18n.t("settings_view.terminal.bg_label"),
        "backgroundOpacity" => i18n.t("settings_view.terminal.bg_opacity"),
        "backgroundBlur" => i18n.t("settings_view.terminal.bg_blur"),
        "backgroundFit" => i18n.t("settings_view.terminal.bg_fit"),
        "backgroundEnabledTabs" => i18n.t("settings_view.terminal.bg_tabs"),
        "scrollback" => i18n.t("settings_view.terminal.scrollback"),
        "renderer" => i18n.t("settings_view.terminal.renderer"),
        "adaptiveRenderer" => i18n.t("settings_view.terminal.adaptive_renderer"),
        "showFpsOverlay" => i18n.t("settings_view.terminal.show_fps_overlay"),
        "pasteProtection" => i18n.t("settings_view.terminal.paste_protection"),
        "smartCopy" => i18n.t("settings_view.terminal.smart_copy"),
        "osc52Clipboard" => i18n.t("settings_view.terminal.osc52_clipboard"),
        "copyOnSelect" => i18n.t("settings_view.terminal.copy_on_select"),
        "middleClickPaste" => i18n.t("settings_view.terminal.middle_click_paste"),
        "selectionRequiresShift" => i18n.t("settings_view.terminal.selection_requires_shift"),
        "sidebarCollapsedDefault" => i18n.t("modals.settings.appearance.sidebar_collapse"),
        "uiDensity" => i18n.t("settings_view.appearance.density"),
        "borderRadius" => i18n.t("settings_view.appearance.border_radius"),
        "uiFontFamily" => i18n.t("settings_view.appearance.ui_font"),
        "animationSpeed" => i18n.t("settings_view.appearance.animation"),
        "frostedGlass" => i18n.t("settings_view.appearance.frosted_glass"),
        "connectionDefaults.username" => i18n.t("settings_view.connections.default_username"),
        "connectionDefaults.port" => i18n.t("settings_view.connections.default_port"),
        "reconnect.enabled" => i18n.t("settings_view.reconnect.enabled"),
        "reconnect.maxAttempts" => i18n.t("settings_view.reconnect.max_attempts"),
        "reconnect.baseDelayMs" => i18n.t("settings_view.reconnect.base_delay"),
        "reconnect.maxDelayMs" => i18n.t("settings_view.reconnect.max_delay"),
        "connectionPool.idleTimeoutSecs" => i18n.t("settings_view.connections.idle_timeout.label"),
        "sftp.maxConcurrentTransfers" => i18n.t("settings_view.sftp.concurrent"),
        "sftp.directoryParallelism" => i18n.t("settings_view.sftp.directory_parallelism"),
        "sftp.speedLimitEnabled" => i18n.t("settings_view.sftp.bandwidth"),
        "sftp.speedLimitKBps" => i18n.t("settings_view.sftp.speed_limit"),
        "sftp.conflictAction" => i18n.t("settings_view.sftp.conflict"),
        "ide.autoSave" => i18n.t("settings_view.ide.auto_save"),
        "ide.fontSize" => i18n.t("settings_view.ide.font_size"),
        "ide.lineHeight" => i18n.t("settings_view.ide.line_height"),
        "ide.agentMode" => i18n.t("settings_view.ide.agent_mode_label"),
        "ide.wordWrap" => i18n.t("settings_view.ide.word_wrap"),
        "defaultShellId" => i18n.t("settings_view.local_terminal.default_shell"),
        "recentShellIds" => i18n.t("modals.import.app_settings_field_recent_shells"),
        "defaultCwd" => i18n.t("settings_view.local_terminal.default_cwd"),
        "gitBashPath" => i18n.t("settings_view.local_terminal.git_bash_path"),
        "loadShellProfile" => i18n.t("settings_view.local_terminal.load_shell_profile"),
        "ohMyPoshEnabled" => i18n.t("settings_view.local_terminal.oh_my_posh_enable"),
        "ohMyPoshTheme" => i18n.t("settings_view.local_terminal.oh_my_posh_theme"),
        "customEnvVars" => i18n.t("settings_view.local_terminal.custom_env"),
        _ => field.to_string(),
    }
}
