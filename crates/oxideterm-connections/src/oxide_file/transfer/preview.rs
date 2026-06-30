pub fn preview_oxide_import(
    store: &ConnectionStore,
    bytes: &[u8],
    password: &str,
    strategy: ImportConflictStrategy,
) -> Result<ImportPreview, OxideFileError> {
    preview_oxide_import_with_options(
        store,
        bytes,
        password,
        OxideImportOptions {
            conflict_strategy: strategy,
            ..OxideImportOptions::default()
        },
    )
}

pub fn preview_oxide_import_with_options(
    store: &ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
) -> Result<ImportPreview, OxideFileError> {
    preview_oxide_import_inner(store, bytes, password, options, None)
}

pub fn preview_oxide_import_with_progress<F>(
    store: &ConnectionStore,
    bytes: &[u8],
    password: &str,
    strategy: ImportConflictStrategy,
    mut on_progress: F,
) -> Result<ImportPreview, OxideFileError>
where
    F: FnMut(&str, usize, usize),
{
    preview_oxide_import_inner(
        store,
        bytes,
        password,
        OxideImportOptions {
            conflict_strategy: strategy,
            ..OxideImportOptions::default()
        },
        Some(&mut on_progress),
    )
}

fn preview_oxide_import_inner(
    store: &ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
    mut on_progress: Option<&mut dyn FnMut(&str, usize, usize)>,
) -> Result<ImportPreview, OxideFileError> {
    const PREVIEW_IMPORT_TOTAL_STEPS: usize = 8;
    let mut current_step = 1usize;
    let mut report_progress = |stage: &str, current: usize| {
        if let Some(callback) = on_progress.as_deref_mut() {
            callback(stage, current, PREVIEW_IMPORT_TOTAL_STEPS);
        }
    };
    let file = OxideFile::from_bytes(bytes)?;
    report_progress("parsing_file", current_step);
    let payload = decrypt_oxide_file_with_progress(&file, password, |stage| {
        current_step += 1;
        report_progress(stage, current_step);
    })?;
    let EncryptedPayload {
        mut connections,
        app_settings_json,
        quick_commands_json,
        serial_profiles_json,
        raw_tcp_profiles_json,
        plugin_settings,
        portable_secrets,
        ..
    } = payload;
    connections = filter_selected_connections(connections, options.selected_names.as_ref());
    let _forward_selection =
        filter_selected_forward_ids(&mut connections, options.selected_forward_ids.as_ref());
    if !options.import_forwards {
        for connection in &mut connections {
            connection.forwards.clear();
        }
    }
    current_step += 1;
    report_progress("collecting_existing", current_step);
    let plans = plan_import(store, &connections, options.conflict_strategy);
    current_step += 1;
    report_progress("building_preview", current_step);
    let mut preview = ImportPreview {
        total_connections: connections.len(),
        has_embedded_keys: connections.iter().any(connection_has_embedded_key),
        total_forwards: connections.iter().map(|conn| conn.forwards.len()).sum(),
        serial_profiles_count: count_serial_profiles(serial_profiles_json.as_deref()),
        raw_tcp_profiles_count: count_raw_tcp_profiles(raw_tcp_profiles_json.as_deref()),
        plugin_settings_count: plugin_settings.len(),
        portable_secret_count: portable_secrets.len(),
        plugin_settings_by_plugin: plugin_settings_by_plugin(&plugin_settings),
        ..ImportPreview::default()
    };
    let (has_quick_commands, commands, categories) =
        count_quick_commands(quick_commands_json.as_deref());
    preview.has_app_settings = app_settings_json.is_some();
    if let Some(snapshot) = app_settings_json.as_deref() {
        let app_settings = preview_app_settings(snapshot);
        preview.app_settings_format = app_settings.format;
        preview.app_settings_keys = app_settings.keys;
        preview.app_settings_preview = app_settings.preview;
        preview.app_settings_section_ids = app_settings
            .sections
            .iter()
            .map(|section| section.id.clone())
            .collect();
        preview.app_settings_contains_local_terminal_env_vars = app_settings
            .sections
            .iter()
            .any(|section| section.contains_env_vars);
        preview.app_settings_sections = app_settings.sections;
    }
    preview.has_quick_commands = has_quick_commands;
    preview.quick_commands_count = commands;
    preview.quick_command_categories_count = categories;
    preview.forward_details = connections
        .iter()
        .flat_map(|conn| {
            conn.forwards.iter().map(|forward| ForwardDetail {
                owner_connection_name: conn.name.clone(),
                direction: forward.forward_type.clone(),
                description: format_forward_preview_description(forward),
            })
        })
        .collect();

    for (conn, action) in connections.iter().zip(plans) {
        let record_has_embedded_keys = connection_has_embedded_key(conn);
        let reason_code = preview_reason_code(&action).to_string();
        match action {
            PlannedImportAction::Import => {
                preview.unchanged.push(conn.name.clone());
                preview.records.push(import_preview_record(
                    conn,
                    "import",
                    reason_code,
                    None,
                    None,
                    record_has_embedded_keys,
                ));
            }
            PlannedImportAction::Rename(name) => {
                preview.will_rename.push((conn.name.clone(), name.clone()));
                preview.records.push(import_preview_record(
                    conn,
                    "rename",
                    reason_code,
                    Some(name),
                    None,
                    record_has_embedded_keys,
                ));
            }
            PlannedImportAction::Skip => {
                preview.will_skip.push(conn.name.clone());
                preview.records.push(import_preview_record(
                    conn,
                    "skip",
                    reason_code,
                    None,
                    None,
                    record_has_embedded_keys,
                ));
            }
            PlannedImportAction::Replace(existing_id) => {
                preview.will_replace.push(conn.name.clone());
                let target_name = store
                    .get(&existing_id)
                    .map(|existing| existing.name.clone());
                preview.records.push(import_preview_record(
                    conn,
                    "replace",
                    reason_code,
                    target_name,
                    Some(existing_id),
                    record_has_embedded_keys,
                ));
            }
            PlannedImportAction::Merge(existing_id) => {
                preview.will_merge.push(conn.name.clone());
                let target_name = store
                    .get(&existing_id)
                    .map(|existing| existing.name.clone());
                preview.records.push(import_preview_record(
                    conn,
                    "merge",
                    reason_code,
                    target_name,
                    Some(existing_id),
                    record_has_embedded_keys,
                ));
            }
        }
    }
    current_step += 1;
    report_progress("analyzing_preview", current_step);
    Ok(preview)
}
