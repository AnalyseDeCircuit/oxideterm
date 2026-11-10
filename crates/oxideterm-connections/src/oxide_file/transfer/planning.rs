/// Groups are part of a connection's identity: the same name in two groups is
/// two connections. An archive's source id still follows one moved between groups.
fn find_import_conflict<'a>(
    store: &'a ConnectionStore,
    conn: &EncryptedConnection,
) -> Option<&'a SavedConnection> {
    let group = canonical_group_name(conn.group.as_deref());
    store
        .connections()
        .iter()
        .find(|existing| {
            existing.name == conn.name && canonical_group_name(existing.group.as_deref()) == group
        })
        .or_else(|| {
            conn.source_connection_id
                .as_deref()
                .and_then(|id| store.get(id))
                .filter(|existing| existing.name == conn.name)
        })
}

fn plan_rename<'a>(
    conn: &'a EncryptedConnection,
    reserved_names: &mut HashSet<String>,
    planned_identities: &mut HashSet<(String, Option<&'a str>)>,
) -> PlannedImportAction {
    let name = unique_copy_name(&conn.name, reserved_names);
    planned_identities.insert((name.clone(), canonical_group_name(conn.group.as_deref())));
    PlannedImportAction::Rename(name)
}

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
    let mut planned_identities = HashSet::new();
    let mut replaced_ids = HashSet::new();

    connections
        .iter()
        .map(|conn| {
            let identity = (conn.name.clone(), canonical_group_name(conn.group.as_deref()));
            let Some(existing_id) = find_import_conflict(store, conn).map(|c| c.id.clone()) else {
                if !planned_identities.insert(identity) {
                    return plan_rename(conn, &mut reserved_names, &mut planned_identities);
                }
                reserved_names.insert(conn.name.clone());
                return PlannedImportAction::Import;
            };

            match strategy {
                ImportConflictStrategy::Rename => {
                    plan_rename(conn, &mut reserved_names, &mut planned_identities)
                }
                ImportConflictStrategy::Skip => PlannedImportAction::Skip,
                ImportConflictStrategy::Replace | ImportConflictStrategy::Merge => {
                    if planned_identities.contains(&identity)
                        || !replaced_ids.insert(existing_id.clone())
                    {
                        return plan_rename(conn, &mut reserved_names, &mut planned_identities);
                    }
                    // A moved connection occupies its destination before later archive entries
                    // are planned, even though the store is not updated until import commits.
                    planned_identities.insert(identity);
                    if strategy == ImportConflictStrategy::Replace {
                        PlannedImportAction::Replace(existing_id)
                    } else {
                        PlannedImportAction::Merge(existing_id)
                    }
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
