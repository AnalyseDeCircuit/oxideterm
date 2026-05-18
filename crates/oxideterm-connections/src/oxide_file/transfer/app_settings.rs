struct AppSettingsPreviewParts {
    format: Option<String>,
    keys: Vec<String>,
    preview: HashMap<String, String>,
    sections: Vec<AppSettingsSectionPreview>,
}

fn preview_app_settings(snapshot_json: &str) -> AppSettingsPreviewParts {
    let Ok(value) = serde_json::from_str::<Value>(snapshot_json) else {
        return AppSettingsPreviewParts {
            format: None,
            keys: Vec::new(),
            preview: HashMap::new(),
            sections: Vec::new(),
        };
    };
    let Some(map) = value.as_object() else {
        return AppSettingsPreviewParts {
            format: None,
            keys: Vec::new(),
            preview: HashMap::new(),
            sections: Vec::new(),
        };
    };

    if map.get("format").and_then(Value::as_str) == Some("oxide-settings-sections-v1") {
        let Some(settings) = map.get("settings").and_then(Value::as_object) else {
            return AppSettingsPreviewParts {
                format: Some("sectioned".to_string()),
                keys: Vec::new(),
                preview: HashMap::new(),
                sections: Vec::new(),
            };
        };
        let section_ids = map
            .get("sectionIds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut keys = settings.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        return AppSettingsPreviewParts {
            format: Some("sectioned".to_string()),
            keys,
            preview: HashMap::new(),
            sections: build_sectioned_app_settings_sections(settings, &section_ids),
        };
    }

    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    let mut preview = HashMap::new();
    for key in &keys {
        if let Some(value) = map.get(key)
            && let Ok(serialized) = serde_json::to_string(value)
        {
            preview.insert(key.clone(), serialized);
        }
    }
    AppSettingsPreviewParts {
        format: Some("legacy".to_string()),
        keys: keys.clone(),
        preview,
        sections: vec![AppSettingsSectionPreview {
            id: "legacy".to_string(),
            field_keys: keys,
            field_values: HashMap::new(),
            contains_env_vars: false,
        }],
    }
}

fn build_sectioned_app_settings_sections(
    settings: &serde_json::Map<String, Value>,
    section_ids: &[String],
) -> Vec<AppSettingsSectionPreview> {
    let mut sections = Vec::new();
    for section_id in section_ids {
        let mut field_values = HashMap::new();
        let mut contains_env_vars = false;
        match section_id.as_str() {
            "general" => add_preview_fields(
                settings.get("general").and_then(Value::as_object),
                &["language", "updateChannel"],
                None,
                &mut field_values,
            ),
            "terminalAppearance" => add_preview_fields(
                settings.get("terminal").and_then(Value::as_object),
                &[
                    "theme",
                    "fontFamily",
                    "customFontFamily",
                    "fontSize",
                    "lineHeight",
                    "cursorStyle",
                    "cursorBlink",
                    "backgroundEnabled",
                    "backgroundImage",
                    "backgroundOpacity",
                    "backgroundBlur",
                    "backgroundFit",
                    "backgroundEnabledTabs",
                ],
                None,
                &mut field_values,
            ),
            "terminalBehavior" => add_preview_fields(
                settings.get("terminal").and_then(Value::as_object),
                &[
                    "scrollback",
                    "renderer",
                    "adaptiveRenderer",
                    "showFpsOverlay",
                    "pasteProtection",
                    "smartCopy",
                    "osc52Clipboard",
                    "copyOnSelect",
                    "middleClickPaste",
                    "selectionRequiresShift",
                ],
                None,
                &mut field_values,
            ),
            "appearance" => add_preview_fields(
                settings.get("appearance").and_then(Value::as_object),
                &[
                    "sidebarCollapsedDefault",
                    "uiDensity",
                    "borderRadius",
                    "uiFontFamily",
                    "animationSpeed",
                    "frostedGlass",
                ],
                None,
                &mut field_values,
            ),
            "connections" => {
                add_preview_fields(
                    settings
                        .get("connectionDefaults")
                        .and_then(Value::as_object),
                    &["username", "port"],
                    Some("connectionDefaults"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("reconnect").and_then(Value::as_object),
                    &["enabled", "maxAttempts", "baseDelayMs", "maxDelayMs"],
                    Some("reconnect"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("connectionPool").and_then(Value::as_object),
                    &["idleTimeoutSecs"],
                    Some("connectionPool"),
                    &mut field_values,
                );
            }
            "fileAndEditor" => {
                add_preview_fields(
                    settings.get("sftp").and_then(Value::as_object),
                    &[
                        "maxConcurrentTransfers",
                        "speedLimitEnabled",
                        "speedLimitKBps",
                        "conflictAction",
                    ],
                    Some("sftp"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("ide").and_then(Value::as_object),
                    &[
                        "autoSave",
                        "fontSize",
                        "lineHeight",
                        "agentMode",
                        "wordWrap",
                    ],
                    Some("ide"),
                    &mut field_values,
                );
            }
            "localTerminal" => {
                let local_terminal = settings.get("localTerminal").and_then(Value::as_object);
                add_preview_fields(
                    local_terminal,
                    &[
                        "defaultShellId",
                        "recentShellIds",
                        "defaultCwd",
                        "gitBashPath",
                        "loadShellProfile",
                        "ohMyPoshEnabled",
                        "ohMyPoshTheme",
                    ],
                    None,
                    &mut field_values,
                );
                contains_env_vars = add_env_var_preview(
                    local_terminal.and_then(|object| object.get("customEnvVars")),
                    &mut field_values,
                );
            }
            _ => {}
        }
        if let Some(section) = build_section_preview(section_id, field_values, contains_env_vars) {
            sections.push(section);
        }
    }
    sections
}

fn add_preview_fields(
    source: Option<&serde_json::Map<String, Value>>,
    keys: &[&str],
    prefix: Option<&str>,
    target: &mut HashMap<String, String>,
) {
    let Some(source) = source else {
        return;
    };
    for key in keys {
        if let Some(value) = source.get(*key) {
            let target_key = prefix
                .map(|prefix| format!("{prefix}.{key}"))
                .unwrap_or_else(|| (*key).to_string());
            target.insert(target_key, stringify_preview_value(value));
        }
    }
}

fn add_env_var_preview(value: Option<&Value>, target: &mut HashMap<String, String>) -> bool {
    let Some(map) = value.and_then(Value::as_object) else {
        return false;
    };
    let mut names = map.keys().cloned().collect::<Vec<_>>();
    names.sort();
    target.insert(
        "customEnvVars".to_string(),
        if names.is_empty() {
            "0".to_string()
        } else {
            names.join(", ")
        },
    );
    true
}

fn build_section_preview(
    id: &str,
    field_values: HashMap<String, String>,
    contains_env_vars: bool,
) -> Option<AppSettingsSectionPreview> {
    if field_values.is_empty() && !contains_env_vars {
        return None;
    }
    let mut field_keys = field_values.keys().cloned().collect::<Vec<_>>();
    field_keys.sort();
    Some(AppSettingsSectionPreview {
        id: id.to_string(),
        field_keys,
        field_values,
        contains_env_vars,
    })
}

fn stringify_preview_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(values) => values
            .iter()
            .map(stringify_preview_value)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()),
    }
}
