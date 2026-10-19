fn plan_plugin_import(
    store: &ConnectionStore,
    profiles: &[crate::PluginConnectionProfile],
    strategy: ImportConflictStrategy,
) -> Result<Vec<PlannedImportAction>, OxideFileError> {
    let mut ids = HashSet::new();
    let mut names = store
        .plugin_profiles()
        .iter()
        .map(|p| p.name.clone())
        .collect::<HashSet<_>>();
    let mut replaced = HashSet::new();
    profiles
        .iter()
        .map(|profile| {
            profile.validate()?;
            if !ids.insert(&profile.id) {
                return Err(OxideFileError::InvalidFormat(
                    "Duplicate plugin connection identity".into(),
                ));
            }
            let existing = store
                .plugin_profiles()
                .iter()
                .find(|p| {
                    p.plugin_id == profile.plugin_id
                        && p.type_id == profile.type_id
                        && p.id == profile.id
                })
                .or_else(|| {
                    store.plugin_profiles().iter().find(|p| {
                        p.plugin_id == profile.plugin_id
                            && p.type_id == profile.type_id
                            && p.name == profile.name
                    })
                });
            Ok(match existing {
                Some(existing) => match strategy {
                    ImportConflictStrategy::Skip => PlannedImportAction::Skip,
                    ImportConflictStrategy::Replace if replaced.insert(existing.id.clone()) => {
                        PlannedImportAction::Replace(existing.id.clone())
                    }
                    ImportConflictStrategy::Merge if replaced.insert(existing.id.clone()) => {
                        PlannedImportAction::Merge(existing.id.clone())
                    }
                    _ => PlannedImportAction::Rename(unique_copy_name(&profile.name, &mut names)),
                },
                None if !names.insert(profile.name.clone()) => {
                    PlannedImportAction::Rename(unique_copy_name(&profile.name, &mut names))
                }
                None => PlannedImportAction::Import,
            })
        })
        .collect()
}

fn preview_plugin_connections(
    preview: &mut ImportPreview,
    store: &ConnectionStore,
    profiles: &[crate::PluginConnectionProfile],
    strategy: ImportConflictStrategy,
) -> Result<(), OxideFileError> {
    let plans = plan_plugin_import(store, profiles, strategy)?;
    preview.total_connections += profiles.len();
    for (profile, plan) in profiles.iter().zip(plans) {
        let reason_code = preview_reason_code(&plan).into();
        let (action, target_name, target_connection_id) = match plan {
            PlannedImportAction::Import => {
                preview.unchanged.push(profile.name.clone());
                ("import", None, None)
            }
            PlannedImportAction::Rename(name) => {
                preview
                    .will_rename
                    .push((profile.name.clone(), name.clone()));
                ("rename", Some(name), None)
            }
            PlannedImportAction::Skip => {
                preview.will_skip.push(profile.name.clone());
                ("skip", None, None)
            }
            PlannedImportAction::Replace(id) => {
                preview.will_replace.push(profile.name.clone());
                ("replace", None, Some(id))
            }
            PlannedImportAction::Merge(id) => {
                preview.will_merge.push(profile.name.clone());
                ("merge", None, Some(id))
            }
        };
        preview.records.push(ImportPreviewRecord {
            resource: "connection".into(),
            name: profile.name.clone(),
            action: action.into(),
            reason_code,
            target_name,
            target_connection_id,
            forward_count: 0,
            has_embedded_keys: false,
        });
    }
    Ok(())
}

fn stage_plugin_connections(
    store: &mut ConnectionStore,
    profiles: Vec<crate::PluginConnectionProfile>,
    plans: Vec<PlannedImportAction>,
    result: &mut ImportResultEnvelope,
) -> Result<HashMap<String, String>, OxideFileError> {
    let mut imported_ids = HashMap::new();
    for (mut profile, plan) in profiles.into_iter().zip(plans) {
        let source_id = profile.id.clone();
        match plan {
            PlannedImportAction::Skip => {
                result.skipped += 1;
                continue;
            }
            PlannedImportAction::Import => {
                profile.id = Uuid::new_v4().to_string();
                result.imported += 1;
            }
            PlannedImportAction::Rename(name) => {
                result.renames.push((profile.name.clone(), name.clone()));
                profile.name = name;
                profile.id = Uuid::new_v4().to_string();
                result.renamed += 1;
                result.imported += 1;
            }
            PlannedImportAction::Replace(id) => {
                profile.id = id;
                result.replaced += 1;
                result.imported += 1;
            }
            PlannedImportAction::Merge(id) => {
                let old = store
                    .plugin_profiles()
                    .iter()
                    .find(|p| p.id == id)
                    .expect("planned existing profile");
                for (key, value) in &old.config {
                    profile.config.insert(key.clone(), value.clone());
                }
                for key in old.credential_refs.keys() {
                    profile.credential_refs.entry(key.clone()).or_insert(None);
                }
                profile.group = old.group.clone().or(profile.group);
                profile.id = id;
                result.imported += 1;
                result.merged += 1;
            }
        }
        imported_ids.insert(source_id, profile.id.clone());
        store.stage_imported_plugin_profile(profile)?;
    }
    Ok(imported_ids)
}
