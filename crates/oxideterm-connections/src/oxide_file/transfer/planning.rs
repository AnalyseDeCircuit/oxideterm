fn plan_import(
    store: &ConnectionStore,
    connections: &[EncryptedConnection],
    strategy: ImportConflictStrategy,
) -> Vec<PlannedImportAction> {
    let mut reserved_names: HashSet<String> = store
        .connections()
        .iter()
        .map(|conn| conn.name.clone())
        .collect();
    let mut first_existing_by_name: HashMap<String, String> = HashMap::new();
    for conn in store.connections() {
        first_existing_by_name
            .entry(conn.name.clone())
            .or_insert_with(|| conn.id.clone());
    }
    let mut replaced_names = HashSet::new();

    connections
        .iter()
        .map(|conn| {
            let Some(existing_id) = first_existing_by_name.get(&conn.name).cloned() else {
                if reserved_names.contains(&conn.name) {
                    return PlannedImportAction::Rename(unique_copy_name(
                        &conn.name,
                        &mut reserved_names,
                    ));
                }
                reserved_names.insert(conn.name.clone());
                return PlannedImportAction::Import;
            };

            match strategy {
                ImportConflictStrategy::Rename => {
                    let name = unique_copy_name(&conn.name, &mut reserved_names);
                    PlannedImportAction::Rename(name)
                }
                ImportConflictStrategy::Skip => PlannedImportAction::Skip,
                ImportConflictStrategy::Replace if replaced_names.insert(conn.name.clone()) => {
                    PlannedImportAction::Replace(existing_id)
                }
                ImportConflictStrategy::Merge if replaced_names.insert(conn.name.clone()) => {
                    PlannedImportAction::Merge(existing_id)
                }
                ImportConflictStrategy::Replace | ImportConflictStrategy::Merge => {
                    let name = unique_copy_name(&conn.name, &mut reserved_names);
                    PlannedImportAction::Rename(name)
                }
            }
        })
        .collect()
}

fn preview_reason_code(action: &PlannedImportAction) -> &'static str {
    match action {
        PlannedImportAction::Import => "new-connection",
        PlannedImportAction::Rename(_) => "name-conflict",
        PlannedImportAction::Skip => "name-conflict-skipped",
        PlannedImportAction::Replace(_) => "replace-existing",
        PlannedImportAction::Merge(_) => "merge-existing",
    }
}

fn import_preview_record(
    conn: &EncryptedConnection,
    action: &str,
    reason_code: String,
    target_name: Option<String>,
    target_connection_id: Option<String>,
    has_embedded_keys: bool,
) -> ImportPreviewRecord {
    ImportPreviewRecord {
        resource: "connection".to_string(),
        name: conn.name.clone(),
        action: action.to_string(),
        reason_code,
        target_name,
        target_connection_id,
        forward_count: conn.forwards.len(),
        has_embedded_keys,
    }
}

fn format_forward_preview_description(forward: &EncryptedForward) -> String {
    let summary = match forward.forward_type.as_str() {
        "local" => format!(
            "L:{} -> {}:{}",
            forward.bind_port, forward.target_host, forward.target_port
        ),
        "remote" => format!(
            "R:{} -> {}:{}",
            forward.bind_port, forward.target_host, forward.target_port
        ),
        "dynamic" => format!("D:{} -> SOCKS", forward.bind_port),
        other => format!(
            "{}:{} -> {}:{}",
            other, forward.bind_port, forward.target_host, forward.target_port
        ),
    };

    match forward.description.as_deref().map(str::trim) {
        Some("") | None => summary,
        Some(description) => format!("{description} ({summary})"),
    }
}

fn unique_copy_name(original: &str, occupied: &mut HashSet<String>) -> String {
    for index in 1..=1000 {
        let candidate = if index == 1 {
            format!("{original} (Copy)")
        } else {
            format!("{original} (Copy {index})")
        };
        if occupied.insert(candidate.clone()) {
            return candidate;
        }
    }
    let fallback = format!("{original} ({})", Uuid::new_v4());
    occupied.insert(fallback.clone());
    fallback
}
