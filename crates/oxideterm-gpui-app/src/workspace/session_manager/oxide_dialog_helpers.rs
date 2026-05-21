fn oxide_settings_section_label(section: &str) -> &'static str {
    match section {
        "general" => "常规",
        "terminalAppearance" => "终端外观",
        "terminalBehavior" => "终端行为",
        "appearance" => "外观",
        "connections" => "连接",
        "fileAndEditor" => "文件与编辑器",
        "ai" => "OxideSens",
        "localTerminal" => "本地终端",
        _ => "设置",
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

fn oxide_password_strength_label(strength: OxidePasswordStrength) -> &'static str {
    match strength {
        OxidePasswordStrength::Weak => "密码较弱，建议使用 12+ 位并混合大小写字母、数字和符号",
        OxidePasswordStrength::Fair => "中等",
        OxidePasswordStrength::Strong => "强",
    }
}

fn oxide_export_progress_label(stage: &str, embed_keys: bool) -> String {
    match stage {
        "collecting_connections" if embed_keys => "读取密钥...",
        "serializing_file" => "写入文件...",
        "done" => "完成！",
        _ => "加密中...",
    }
    .to_string()
}

fn oxide_import_progress_label(stage: &str, total: usize) -> String {
    match stage {
        "parsing_file" => "正在解析文件...",
        "deriving_key" | "decrypting_payload" | "deserializing_payload" | "verifying_checksum" => {
            "正在解密..."
        }
        "collecting_existing" | "building_preview" | "analyzing_preview" if total == 8 => {
            "正在分析导入预览..."
        }
        "filtering_selection" | "collecting_existing" | "preparing_connections" => {
            "正在准备导入..."
        }
        "applying_connections" => "正在应用更改...",
        "saving_config" => "正在保存导入的数据...",
        "done" => "完成!",
        _ => "正在解密...",
    }
    .to_string()
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

fn next_oxide_footer_action(
    actions: &[OxideDialogFooterAction],
    current: Option<OxideDialogFooterAction>,
    forward: bool,
) -> Option<OxideDialogFooterAction> {
    let Some(first) = actions.first().copied() else {
        return None;
    };
    let current = current.unwrap_or(first);
    let index = actions
        .iter()
        .position(|candidate| *candidate == current)
        .unwrap_or(0);
    if forward {
        actions.get(index + 1).copied().or(Some(first))
    } else {
        index
            .checked_sub(1)
            .and_then(|previous| actions.get(previous).copied())
            .or_else(|| actions.last().copied())
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

fn oxide_settings_field_label(field: &str) -> String {
    match field {
        "language" => "语言",
        "updateChannel" => "更新通道",
        "theme" => "主题",
        "fontFamily" => "字体",
        "customFontFamily" => "自定义字体",
        "fontSize" => "字号",
        "lineHeight" => "行高",
        "cursorStyle" => "光标样式",
        "cursorBlink" => "光标闪烁",
        "backgroundEnabled" => "背景图",
        "backgroundImage" => "背景图片",
        "backgroundOpacity" => "背景透明度",
        "backgroundBlur" => "背景模糊",
        "backgroundFit" => "背景适配",
        "backgroundEnabledTabs" => "背景启用范围",
        "scrollback" => "回滚行数",
        "renderer" => "渲染器",
        "adaptiveRenderer" => "自适应渲染",
        "showFpsOverlay" => "FPS 浮层",
        "pasteProtection" => "粘贴保护",
        "smartCopy" => "智能复制",
        "osc52Clipboard" => "OSC52 剪贴板",
        "copyOnSelect" => "选中即复制",
        "middleClickPaste" => "中键粘贴",
        "selectionRequiresShift" => "Shift 选择",
        "sidebarCollapsedDefault" => "侧边栏默认折叠",
        "uiDensity" => "UI 密度",
        "borderRadius" => "圆角",
        "uiFontFamily" => "界面字体",
        "animationSpeed" => "动画速度",
        "frostedGlass" => "磨砂玻璃",
        "connectionDefaults.username" => "默认用户名",
        "connectionDefaults.port" => "默认端口",
        "reconnect.enabled" => "自动重连",
        "reconnect.maxAttempts" => "最大重试次数",
        "reconnect.baseDelayMs" => "基础延迟",
        "reconnect.maxDelayMs" => "最大延迟",
        "connectionPool.idleTimeoutSecs" => "空闲超时",
        "sftp.maxConcurrentTransfers" => "并发传输",
        "sftp.speedLimitEnabled" => "限速",
        "sftp.speedLimitKBps" => "速度限制",
        "sftp.conflictAction" => "冲突策略",
        "ide.autoSave" => "自动保存",
        "ide.lineHeight" => "编辑器行高",
        "ide.agentMode" => "Agent 模式",
        "ide.wordWrap" => "自动换行",
        "defaultShellId" => "默认 Shell",
        "recentShellIds" => "最近 Shell",
        "defaultCwd" => "默认目录",
        "gitBashPath" => "Git Bash 路径",
        "loadShellProfile" => "加载 Shell profile",
        "ohMyPoshEnabled" => "Oh My Posh",
        "ohMyPoshTheme" => "Oh My Posh 主题",
        "customEnvVars" => "环境变量",
        _ => field,
    }
    .to_string()
}
