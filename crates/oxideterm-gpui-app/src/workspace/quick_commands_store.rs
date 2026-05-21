impl QuickCommandsState {
    pub(super) fn load(settings_path: &Path) -> Self {
        let path = settings_path
            .parent()
            .unwrap_or(settings_path)
            .join(QUICK_COMMANDS_FILENAME);
        let mut state = Self {
            path,
            categories: default_quick_command_categories(),
            commands: default_quick_commands(),
            active_category: "system".to_string(),
            query: String::new(),
            focused_input: None,
            highlighted_command: None,
            command_editor: None,
            category_editor: None,
            last_persist_error: None,
        };

        match load_snapshot_from_path(&state.path) {
            Ok(Some(snapshot)) => {
                state.categories = snapshot.categories;
                state.commands = snapshot.commands;
                state.ensure_active_category();
            }
            Ok(None) => {}
            Err(error) => {
                state.last_persist_error = Some(error);
            }
        }
        state
    }

    #[allow(dead_code)]
    pub(super) fn visible_commands(&self) -> Vec<QuickCommand> {
        self.visible_commands_for_targets(&[])
    }

    pub(super) fn visible_commands_for_targets(
        &self,
        target_fields: &[String],
    ) -> Vec<QuickCommand> {
        let query = self.query.trim().to_lowercase();
        self.commands
            .iter()
            .filter(|command| command.category == self.active_category)
            .filter(|command| {
                match_quick_command_host_pattern(command.host_pattern.as_deref(), target_fields)
            })
            .filter(|command| {
                query.is_empty()
                    || command.name.to_lowercase().contains(&query)
                    || command.command.to_lowercase().contains(&query)
                    || command
                        .description
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(super) fn upsert_command(&mut self, draft: QuickCommandDraft) {
        let now = now_ms();
        let existing = draft
            .id
            .as_ref()
            .and_then(|id| self.commands.iter().find(|command| &command.id == id));
        let command = QuickCommand {
            id: draft.id.unwrap_or_else(new_quick_command_id),
            name: draft.name.trim().to_string(),
            command: draft.command.trim().to_string(),
            category: if self.categories.iter().any(|c| c.id == draft.category) {
                draft.category
            } else {
                "custom".to_string()
            },
            description: trim_optional(&draft.description),
            host_pattern: trim_optional(&draft.host_pattern),
            created_at: existing.map(|command| command.created_at).unwrap_or(now),
            updated_at: now,
        };
        if command.name.is_empty() || command.command.is_empty() {
            return;
        }
        if self
            .commands
            .iter()
            .any(|candidate| candidate.id == command.id)
        {
            self.commands = self
                .commands
                .iter()
                .map(|candidate| {
                    if candidate.id == command.id {
                        command.clone()
                    } else {
                        candidate.clone()
                    }
                })
                .collect();
        } else {
            self.commands.push(command);
        }
        self.persist();
    }

    pub(super) fn delete_command(&mut self, id: &str) {
        self.commands.retain(|command| command.id != id);
        self.persist();
    }

    pub(super) fn upsert_category(&mut self, draft: QuickCommandCategoryDraft) -> String {
        let category = QuickCommandCategory {
            id: draft.id.unwrap_or_else(new_quick_category_id),
            name: draft.name.trim().to_string(),
            icon: draft.icon,
        };
        if category.name.is_empty() {
            return self.active_category.clone();
        }
        if self
            .categories
            .iter()
            .any(|candidate| candidate.id == category.id)
        {
            self.categories = self
                .categories
                .iter()
                .map(|candidate| {
                    if candidate.id == category.id {
                        category.clone()
                    } else {
                        candidate.clone()
                    }
                })
                .collect();
        } else if self.categories.len() < MAX_CATEGORIES {
            self.categories.push(category.clone());
        }
        self.active_category = category.id.clone();
        self.persist();
        category.id
    }

    pub(super) fn delete_category(&mut self, id: &str) -> bool {
        if default_quick_command_categories()
            .iter()
            .any(|category| category.id == id)
            || self.commands.iter().any(|command| command.category == id)
        {
            return false;
        }
        let before = self.categories.len();
        self.categories.retain(|category| category.id != id);
        if self.categories.len() == before {
            return false;
        }
        self.ensure_active_category();
        self.persist();
        true
    }

    #[allow(dead_code)]
    pub(super) fn reset_defaults(&mut self) {
        self.categories = default_quick_command_categories();
        self.commands = default_quick_commands();
        self.active_category = "system".to_string();
        self.highlighted_command = None;
        self.command_editor = None;
        self.category_editor = None;
        self.persist();
    }

    #[allow(dead_code)]
    pub(super) fn export_snapshot_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.snapshot()).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(super) fn apply_snapshot_json(
        &mut self,
        snapshot_json: &str,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandsImportResult {
        let parsed = serde_json::from_str::<QuickCommandsSnapshot>(snapshot_json)
            .map_err(|err| err.to_string())
            .and_then(sanitize_snapshot);
        match parsed {
            Ok(snapshot) => self.apply_snapshot(snapshot, strategy),
            Err(error) => QuickCommandsImportResult {
                imported: 0,
                skipped: 0,
                errors: vec![error],
            },
        }
    }

    #[allow(dead_code)]
    fn apply_snapshot(
        &mut self,
        snapshot: QuickCommandsSnapshot,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandsImportResult {
        let MergeResult {
            categories,
            commands,
            imported,
            skipped,
        } = merge_quick_commands_snapshot(&self.categories, &self.commands, snapshot, strategy);
        self.categories = categories;
        self.commands = commands;
        self.ensure_active_category();
        self.persist();
        QuickCommandsImportResult {
            imported,
            skipped,
            errors: Vec::new(),
        }
    }

    fn snapshot(&self) -> QuickCommandsSnapshot {
        QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.categories.clone(),
            commands: self.commands.clone(),
            updated_at: now_ms(),
        }
    }

    fn persist(&mut self) {
        let snapshot = self.snapshot();
        self.last_persist_error = save_snapshot_to_path(&self.path, &snapshot).err();
    }

    fn ensure_active_category(&mut self) {
        if !self
            .categories
            .iter()
            .any(|category| category.id == self.active_category)
        {
            self.active_category = self
                .categories
                .first()
                .map(|category| category.id.clone())
                .unwrap_or_else(|| "custom".to_string());
        }
    }
}

#[allow(dead_code)]
struct MergeResult {
    categories: Vec<QuickCommandCategory>,
    commands: Vec<QuickCommand>,
    imported: usize,
    skipped: usize,
}

#[allow(dead_code)]
fn merge_quick_commands_snapshot(
    current_categories: &[QuickCommandCategory],
    current_commands: &[QuickCommand],
    incoming: QuickCommandsSnapshot,
    strategy: QuickCommandImportStrategy,
) -> MergeResult {
    let now = now_ms();
    let mut imported = 0;
    let mut skipped = 0;
    let mut categories = current_categories.to_vec();
    let mut commands = current_commands.to_vec();
    let mut category_remap = HashMap::new();

    for imported_category in incoming.categories {
        let conflict = categories
            .iter()
            .find(|category| {
                category.id == imported_category.id
                    || category
                        .name
                        .trim()
                        .eq_ignore_ascii_case(imported_category.name.trim())
            })
            .cloned();
        let Some(conflict) = conflict else {
            category_remap.insert(imported_category.id.clone(), imported_category.id.clone());
            categories.push(imported_category);
            continue;
        };

        match strategy {
            QuickCommandImportStrategy::Skip => {
                category_remap.insert(imported_category.id, conflict.id);
                skipped += 1;
            }
            QuickCommandImportStrategy::Rename => {
                let renamed = QuickCommandCategory {
                    id: new_quick_category_id(),
                    name: unique_category_name(
                        &categories,
                        &format!("{} (Imported)", imported_category.name),
                    ),
                    icon: imported_category.icon,
                };
                category_remap.insert(imported_category.id, renamed.id.clone());
                categories.push(renamed);
                imported += 1;
            }
            QuickCommandImportStrategy::Replace | QuickCommandImportStrategy::Merge => {
                category_remap.insert(imported_category.id, conflict.id.clone());
                categories = categories
                    .into_iter()
                    .map(|category| {
                        if category.id == conflict.id {
                            QuickCommandCategory {
                                id: conflict.id.clone(),
                                name: imported_category.name.clone(),
                                icon: imported_category.icon,
                            }
                        } else {
                            category
                        }
                    })
                    .collect();
                imported += 1;
            }
        }
    }

    let category_ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<HashSet<_>>();
    for imported_command in incoming.commands {
        let category = category_remap
            .get(&imported_command.category)
            .cloned()
            .unwrap_or_else(|| imported_command.category.clone());
        let category = if category_ids.contains(&category) {
            category
        } else {
            "custom".to_string()
        };
        let imported_command = QuickCommand {
            category,
            ..imported_command
        };
        let conflict = commands
            .iter()
            .find(|command| {
                command.id == imported_command.id
                    || (command.category == imported_command.category
                        && command
                            .name
                            .trim()
                            .eq_ignore_ascii_case(imported_command.name.trim()))
            })
            .cloned();
        let Some(conflict) = conflict else {
            commands.push(imported_command);
            imported += 1;
            continue;
        };

        match strategy {
            QuickCommandImportStrategy::Skip => skipped += 1,
            QuickCommandImportStrategy::Rename => {
                commands.push(QuickCommand {
                    id: new_quick_command_id(),
                    name: unique_command_name(
                        &commands,
                        &imported_command.category,
                        &format!("{} (Imported)", imported_command.name),
                    ),
                    ..imported_command
                });
                imported += 1;
            }
            QuickCommandImportStrategy::Merge => {
                commands = commands
                    .into_iter()
                    .map(|command| {
                        if command.id == conflict.id {
                            QuickCommand {
                                id: conflict.id.clone(),
                                created_at: conflict.created_at,
                                updated_at: now,
                                ..imported_command.clone()
                            }
                        } else {
                            command
                        }
                    })
                    .collect();
                imported += 1;
            }
            QuickCommandImportStrategy::Replace => {
                commands = commands
                    .into_iter()
                    .map(|command| {
                        if command.id == conflict.id {
                            QuickCommand {
                                id: conflict.id.clone(),
                                updated_at: now,
                                ..imported_command.clone()
                            }
                        } else {
                            command
                        }
                    })
                    .collect();
                imported += 1;
            }
        }
    }

    MergeResult {
        categories,
        commands,
        imported,
        skipped,
    }
}

fn load_snapshot_from_path(path: &Path) -> Result<Option<QuickCommandsSnapshot>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to stat Quick Commands file: {error}")),
    };
    if metadata.len() > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands file exceeds size limit".to_string());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read Quick Commands file: {error}"))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let snapshot = serde_json::from_str::<QuickCommandsSnapshot>(&contents)
        .map_err(|error| format!("Failed to parse Quick Commands file: {error}"))?;
    sanitize_snapshot(snapshot).map(Some)
}

fn save_snapshot_to_path(path: &Path, snapshot: &QuickCommandsSnapshot) -> Result<(), String> {
    let snapshot = sanitize_snapshot(snapshot.clone())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create Quick Commands directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| format!("Failed to serialize Quick Commands: {error}"))?;
    if json.len() as u64 > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands snapshot exceeds size limit".to_string());
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .map_err(|error| format!("Failed to write Quick Commands temp file: {error}"))?;
    fs::rename(&temp_path, path)
        .map_err(|error| format!("Failed to replace Quick Commands file: {error}"))?;
    Ok(())
}

fn sanitize_snapshot(snapshot: QuickCommandsSnapshot) -> Result<QuickCommandsSnapshot, String> {
    if snapshot.version != QUICK_COMMANDS_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Quick Commands schema version {}",
            snapshot.version
        ));
    }
    if snapshot.categories.len() > MAX_CATEGORIES {
        return Err(format!(
            "Quick Commands category count exceeds limit {MAX_CATEGORIES}"
        ));
    }
    if snapshot.commands.len() > MAX_COMMANDS {
        return Err(format!(
            "Quick Commands command count exceeds limit {MAX_COMMANDS}"
        ));
    }
    let categories = snapshot
        .categories
        .into_iter()
        .map(sanitize_category)
        .collect::<Result<Vec<_>, _>>()?;
    let category_ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<HashSet<_>>();
    let commands = snapshot
        .commands
        .into_iter()
        .map(|command| sanitize_command(command, &category_ids))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(QuickCommandsSnapshot {
        version: QUICK_COMMANDS_SCHEMA_VERSION,
        categories,
        commands,
        updated_at: snapshot.updated_at,
    })
}

fn sanitize_category(category: QuickCommandCategory) -> Result<QuickCommandCategory, String> {
    Ok(QuickCommandCategory {
        id: bounded_required(category.id, "category.id", MAX_ID_LEN)?,
        name: bounded_required(category.name, "category.name", MAX_NAME_LEN)?,
        icon: category.icon,
    })
}

fn sanitize_command(
    command: QuickCommand,
    category_ids: &HashSet<String>,
) -> Result<QuickCommand, String> {
    let category = bounded_required(command.category, "command.category", MAX_ID_LEN)?;
    Ok(QuickCommand {
        id: bounded_required(command.id, "command.id", MAX_ID_LEN)?,
        name: bounded_required(command.name, "command.name", MAX_NAME_LEN)?,
        command: bounded_required(command.command, "command.command", MAX_COMMAND_LEN)?,
        category: if category_ids.contains(&category) {
            category
        } else {
            "custom".to_string()
        },
        description: bounded_optional(
            command.description,
            "command.description",
            MAX_DESCRIPTION_LEN,
        )?,
        host_pattern: bounded_optional(
            command.host_pattern,
            "command.hostPattern",
            MAX_HOST_PATTERN_LEN,
        )?,
        created_at: command.created_at,
        updated_at: command.updated_at,
    })
}

fn bounded_required(value: String, field: &str, max_len: usize) -> Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("Quick Commands field {field} cannot be empty"));
    }
    if trimmed.len() > max_len {
        return Err(format!(
            "Quick Commands field {field} exceeds limit {max_len}"
        ));
    }
    Ok(trimmed)
}

fn bounded_optional(
    value: Option<String>,
    field: &str,
    max_len: usize,
) -> Result<Option<String>, String> {
    match value.map(|item| item.trim().to_string()) {
        Some(item) if item.is_empty() => Ok(None),
        Some(item) if item.len() > max_len => Err(format!(
            "Quick Commands field {field} exceeds limit {max_len}"
        )),
        Some(item) => Ok(Some(item)),
        None => Ok(None),
    }
}

pub(super) fn default_quick_command_categories() -> Vec<QuickCommandCategory> {
    vec![
        quick_category("system", "System", QuickCommandIcon::Server),
        quick_category("network", "Network", QuickCommandIcon::Terminal),
        quick_category("files", "Files", QuickCommandIcon::Folder),
        quick_category("docker", "Docker", QuickCommandIcon::Docker),
        quick_category("custom", "Custom", QuickCommandIcon::Zap),
    ]
}

pub(super) fn default_quick_commands() -> Vec<QuickCommand> {
    vec![
        quick_command(
            "qc-pwd",
            "Print Working Directory",
            "pwd",
            "files",
            "Show the current directory.",
        ),
        quick_command(
            "qc-ls-la",
            "List Files",
            "ls -la",
            "files",
            "List files with details.",
        ),
        quick_command(
            "qc-df-h",
            "Disk Usage",
            "df -h",
            "system",
            "Show mounted filesystem usage.",
        ),
        quick_command(
            "qc-free-h",
            "Memory Usage",
            "free -h",
            "system",
            "Show memory usage.",
        ),
        quick_command(
            "qc-uptime",
            "Uptime",
            "uptime",
            "system",
            "Show uptime and load average.",
        ),
        quick_command(
            "qc-whoami",
            "Current User",
            "whoami",
            "system",
            "Show the current user.",
        ),
        quick_command(
            "qc-ip-addr",
            "IP Addresses",
            "ip addr",
            "network",
            "Show network interface addresses.",
        ),
        quick_command(
            "qc-ifconfig",
            "Interface Config",
            "ifconfig",
            "network",
            "Show network interfaces on systems without iproute2.",
        ),
        quick_command(
            "qc-docker-ps",
            "Docker Containers",
            "docker ps",
            "docker",
            "List running containers.",
        ),
        quick_command(
            "qc-git-status",
            "Git Status",
            "git status",
            "files",
            "Show repository status.",
        ),
        quick_command(
            "qc-journal-errors",
            "Recent Journal Errors",
            "journalctl -xe --no-pager",
            "system",
            "Show recent system journal errors.",
        ),
    ]
}

fn quick_category(id: &str, name: &str, icon: QuickCommandIcon) -> QuickCommandCategory {
    QuickCommandCategory {
        id: id.to_string(),
        name: name.to_string(),
        icon,
    }
}

fn quick_command(
    id: &str,
    name: &str,
    command: &str,
    category: &str,
    description: &str,
) -> QuickCommand {
    QuickCommand {
        id: id.to_string(),
        name: name.to_string(),
        command: command.to_string(),
        category: category.to_string(),
        description: Some(description.to_string()),
        host_pattern: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn trim_optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn new_quick_command_id() -> String {
    format!(
        "qc-{}-{}",
        now_ms(),
        QUICK_COMMAND_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn new_quick_category_id() -> String {
    format!(
        "qcg-{}-{}",
        now_ms(),
        QUICK_COMMAND_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[allow(dead_code)]
fn unique_category_name(categories: &[QuickCommandCategory], desired_name: &str) -> String {
    let existing = categories
        .iter()
        .map(|category| category.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

#[allow(dead_code)]
fn unique_command_name(commands: &[QuickCommand], category: &str, desired_name: &str) -> String {
    let existing = commands
        .iter()
        .filter(|command| command.category == category)
        .map(|command| command.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

#[allow(dead_code)]
fn unique_name(desired_name: &str, existing_lower_names: &HashSet<String>) -> String {
    if !existing_lower_names.contains(&desired_name.trim().to_lowercase()) {
        return desired_name.to_string();
    }
    for index in 2..1000 {
        let candidate = format!("{desired_name} ({index})");
        if !existing_lower_names.contains(&candidate.trim().to_lowercase()) {
            return candidate;
        }
    }
    format!("{desired_name} ({})", now_ms())
}

#[cfg(test)]
mod quick_command_tests {
    use super::*;

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxideterm-quick-commands-{name}-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn upsert_command_persists_to_quick_commands_json() {
        let settings_path = temp_settings_path("persist");
        let mut state = QuickCommandsState::load(&settings_path);
        state.upsert_command(QuickCommandDraft {
            id: None,
            name: "List root".to_string(),
            command: "ls /".to_string(),
            category: "files".to_string(),
            description: "root listing".to_string(),
            host_pattern: String::new(),
        });

        let reloaded = QuickCommandsState::load(&settings_path);
        assert!(reloaded.commands.iter().any(|command| {
            command.name == "List root"
                && command.command == "ls /"
                && command.description.as_deref() == Some("root listing")
        }));
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn default_categories_cannot_be_deleted_while_custom_empty_categories_can() {
        let settings_path = temp_settings_path("delete-category");
        let mut state = QuickCommandsState::load(&settings_path);
        assert!(!state.delete_category("system"));
        let custom = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Ops".to_string(),
            icon: QuickCommandIcon::Zap,
        });
        assert!(state.delete_category(&custom));
        assert!(
            !state
                .categories
                .iter()
                .any(|category| category.id == custom)
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn upsert_category_allows_multiple_user_custom_groups() {
        let settings_path = temp_settings_path("multiple-custom-groups");
        let mut state = QuickCommandsState::load(&settings_path);

        let first = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Custom".to_string(),
            icon: QuickCommandIcon::Zap,
        });
        let second = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Custom".to_string(),
            icon: QuickCommandIcon::Zap,
        });

        assert_ne!(first, second);
        assert_ne!(first, "custom");
        assert_ne!(second, "custom");
        assert_eq!(state.active_category, second);
        assert_eq!(
            state
                .categories
                .iter()
                .filter(|category| category.name == "Custom")
                .count(),
            3
        );

        let reloaded = QuickCommandsState::load(&settings_path);
        assert!(
            reloaded
                .categories
                .iter()
                .any(|category| category.id == first)
        );
        assert!(
            reloaded
                .categories
                .iter()
                .any(|category| category.id == second)
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn import_snapshot_rename_preserves_conflicting_existing_command() {
        let settings_path = temp_settings_path("import-rename");
        let mut state = QuickCommandsState::load(&settings_path);
        let snapshot = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: vec![QuickCommandCategory {
                id: "files".to_string(),
                name: "Files".to_string(),
                icon: QuickCommandIcon::Folder,
            }],
            commands: vec![QuickCommand {
                id: "qc-ls-la".to_string(),
                name: "List Files".to_string(),
                command: "exa -la".to_string(),
                category: "files".to_string(),
                description: None,
                host_pattern: None,
                created_at: 1,
                updated_at: 1,
            }],
            updated_at: 1,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let result = state.apply_snapshot_json(&json, QuickCommandImportStrategy::Rename);

        assert_eq!(result.errors, Vec::<String>::new());
        assert!(result.imported > 0);
        assert!(
            state
                .commands
                .iter()
                .any(|command| command.command == "ls -la")
        );
        assert!(
            state
                .commands
                .iter()
                .any(|command| command.command == "exa -la")
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }
}

pub(in crate::workspace) fn match_quick_command_host_pattern(
    pattern: Option<&str>,
    target_fields: &[String],
) -> bool {
    let Some(pattern) = pattern.map(str::trim).filter(|pattern| !pattern.is_empty()) else {
        return true;
    };
    let pattern = pattern.to_lowercase();
    target_fields.iter().any(|field| {
        let field = field.to_lowercase();
        wildcard_match(&pattern, &field)
    })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut cursor = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*') || parts.last().is_none_or(|last| value.ends_with(last))
}
