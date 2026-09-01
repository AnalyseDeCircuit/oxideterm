use std::path::Path;

use super::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandAvailability,
    QuickCommandCategoryDraft, QuickCommandEditorDraft, QuickCommandImportResult,
    QuickCommandImportStrategy, QuickCommandsSnapshot, QuickCommandsState,
    default_quick_command_categories, default_quick_commands, now_ms,
};
use oxideterm_quick_commands::{
    QuickCommandDraft, delete_quick_command, delete_quick_command_category,
    ensure_active_quick_command_category, new_quick_command_id, upsert_quick_command,
    upsert_quick_command_category, visible_quick_commands, visible_quick_commands_for_management,
};

impl QuickCommandsState {
    pub(in crate::workspace) fn load(settings_path: &Path) -> Self {
        let mut state = Self {
            settings_path: settings_path.to_path_buf(),
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

        match oxideterm_quick_commands::load_snapshot(settings_path) {
            Ok(snapshot) => {
                state.categories = snapshot.categories;
                state.commands = snapshot.commands;
                state.ensure_active_category();
            }
            Err(error) => {
                state.last_persist_error = Some(error);
            }
        }
        state
    }

    pub(super) fn visible_commands_for_targets(
        &self,
        target_fields: &[String],
    ) -> Vec<QuickCommand> {
        visible_quick_commands(
            &self.commands,
            &self.active_category,
            &self.query,
            target_fields,
        )
    }

    pub(super) fn visible_commands_for_management(&self) -> Vec<QuickCommand> {
        visible_quick_commands_for_management(&self.commands, &self.active_category, &self.query)
    }

    pub(in crate::workspace) fn upsert_command(&mut self, draft: QuickCommandDraft) {
        let updated_at = now_ms();
        let mut commands = self.commands.clone();
        if !upsert_quick_command(&mut commands, &self.categories, draft, updated_at) {
            return;
        }
        // Plugin-originated candidates become visible only after full snapshot validation succeeds.
        let snapshot = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.categories.clone(),
            commands: commands.clone(),
            updated_at,
        };
        match oxideterm_quick_commands::save_snapshot(&self.settings_path, &snapshot) {
            Ok(()) => {
                self.commands = commands;
                self.last_persist_error = None;
            }
            Err(error) => self.last_persist_error = Some(error),
        }
    }

    pub(in crate::workspace) fn upsert_editor_command(
        &mut self,
        draft: QuickCommandEditorDraft,
    ) -> bool {
        let updated_at = now_ms();
        let command = QuickCommand {
            id: draft.id.unwrap_or_else(new_quick_command_id),
            name: draft.name.trim().to_string(),
            command: draft.command.trim().to_string(),
            category: if self
                .categories
                .iter()
                .any(|category| category.id == draft.category)
            {
                draft.category
            } else {
                "custom".to_string()
            },
            description: trimmed_optional(draft.description),
            parameters: draft
                .parameters
                .into_iter()
                .map(|parameter| super::QuickCommandParameter {
                    name: parameter.name.trim().to_string(),
                    label: parameter.label.trim().to_string(),
                    kind: parameter.kind,
                    default_value: (parameter.kind != super::QuickCommandParameterKind::Secret)
                        .then(|| trimmed_optional(parameter.default_value))
                        .flatten(),
                    choices: if parameter.kind == super::QuickCommandParameterKind::Secret {
                        Vec::new()
                    } else {
                        split_choices(&parameter.choices)
                    },
                    required: parameter.required,
                })
                .collect(),
            availability: QuickCommandAvailability {
                protocols: draft.protocols,
                host_patterns: split_host_patterns(&draft.host_patterns),
            },
            confirmation: draft.confirmation,
            sort_order: draft.sort_order,
            created_at: draft.created_at,
            updated_at,
        };
        if command.name.is_empty() || command.command.is_empty() {
            return false;
        }
        // Persist the candidate snapshot before exposing it to readers so a
        // validation or I/O failure cannot leave unsaved editor state active.
        let mut commands = self.commands.clone();
        if let Some(existing) = commands
            .iter_mut()
            .find(|existing| existing.id == command.id)
        {
            *existing = command;
        } else {
            commands.push(command);
        }
        let snapshot = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.categories.clone(),
            commands: commands.clone(),
            updated_at,
        };
        match oxideterm_quick_commands::save_snapshot(&self.settings_path, &snapshot) {
            Ok(()) => {
                self.commands = commands;
                self.last_persist_error = None;
                true
            }
            Err(error) => {
                self.last_persist_error = Some(error);
                false
            }
        }
    }

    pub(in crate::workspace) fn delete_command(&mut self, id: &str) {
        if delete_quick_command(&mut self.commands, id) {
            self.persist();
        }
    }

    pub(in crate::workspace) fn move_command(&mut self, id: &str, offset: isize) -> bool {
        if offset == 0 {
            return false;
        }
        let Some(category) = self
            .commands
            .iter()
            .find(|command| command.id == id)
            .map(|command| command.category.clone())
        else {
            return false;
        };
        let mut category_indices = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.category == category)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        category_indices.sort_by_key(|index| self.commands[*index].sort_order);
        let Some(current_position) = category_indices
            .iter()
            .position(|index| self.commands[*index].id == id)
        else {
            return false;
        };
        let target_position = current_position.saturating_add_signed(offset);
        if target_position >= category_indices.len() || target_position == current_position {
            return false;
        }
        category_indices.swap(current_position, target_position);

        let updated_at = now_ms();
        let mut commands = self.commands.clone();
        // Normalize only this category so duplicate imported sort keys cannot block a move.
        for (sort_order, index) in category_indices.into_iter().enumerate() {
            commands[index].sort_order = sort_order as i64;
            commands[index].updated_at = updated_at;
        }
        commands.sort_by_key(|command| command.sort_order);
        let snapshot = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.categories.clone(),
            commands: commands.clone(),
            updated_at,
        };
        match oxideterm_quick_commands::save_snapshot(&self.settings_path, &snapshot) {
            Ok(()) => {
                self.commands = commands;
                if let Some(editor) = self.command_editor.as_mut()
                    && let Some(editor_id) = editor.id.as_deref()
                    && let Some(command) =
                        self.commands.iter().find(|command| command.id == editor_id)
                {
                    editor.sort_order = command.sort_order;
                }
                self.last_persist_error = None;
                true
            }
            Err(error) => {
                self.last_persist_error = Some(error);
                false
            }
        }
    }

    pub(super) fn upsert_category(&mut self, draft: QuickCommandCategoryDraft) -> String {
        self.active_category =
            upsert_quick_command_category(&mut self.categories, draft, &self.active_category);
        self.persist();
        self.active_category.clone()
    }

    pub(super) fn delete_category(&mut self, id: &str) -> bool {
        if !delete_quick_command_category(&mut self.categories, &mut self.commands, id) {
            return false;
        }
        self.ensure_active_category();
        self.persist();
        true
    }

    pub(in crate::workspace) fn export_snapshot_json(&self) -> Result<String, String> {
        oxideterm_quick_commands::export_snapshot_json(&self.settings_path)
    }

    pub(in crate::workspace) fn apply_snapshot_json(
        &mut self,
        snapshot_json: &str,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandImportResult {
        let result = oxideterm_quick_commands::apply_snapshot_json(
            &self.settings_path,
            snapshot_json,
            strategy,
        );
        if result.errors.is_empty() {
            self.reload_from_store();
        }
        result
    }

    pub(in crate::workspace) fn reload_from_store(&mut self) {
        match oxideterm_quick_commands::load_snapshot(&self.settings_path) {
            Ok(snapshot) => {
                self.categories = snapshot.categories;
                self.commands = snapshot.commands;
                self.ensure_active_category();
                self.highlighted_command = self
                    .highlighted_command
                    .take()
                    .filter(|id| self.commands.iter().any(|command| command.id == *id));
                self.last_persist_error = None;
            }
            Err(error) => self.last_persist_error = Some(error),
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
        self.last_persist_error =
            oxideterm_quick_commands::save_snapshot(&self.settings_path, &snapshot).err();
    }

    fn ensure_active_category(&mut self) {
        ensure_active_quick_command_category(&self.categories, &mut self.active_category);
    }
}

fn trimmed_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn split_host_patterns(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(str::to_string)
        .collect()
}

fn split_choices(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|choice| !choice.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod quick_command_tests {
    use super::{
        QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandCategoryDraft, QuickCommandDraft,
        QuickCommandImportStrategy, QuickCommandsSnapshot, QuickCommandsState,
        default_quick_command_categories, default_quick_commands, now_ms,
    };
    use crate::workspace::quick_commands::{QuickCommandCategory, QuickCommandIcon};
    use std::fs;
    use std::path::PathBuf;

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
            category: Some("files".to_string()),
            description: Some("root listing".to_string()),
            parameters: None,
            protocols: None,
            host_patterns: None,
            confirmation: None,
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
    fn moving_command_updates_visible_and_persisted_category_order() {
        let settings_path = temp_settings_path("move-command");
        let mut state = QuickCommandsState::load(&settings_path);
        let category = state.commands[0].category.clone();
        let category_commands = state
            .commands
            .iter()
            .filter(|command| command.category == category)
            .map(|command| command.id.clone())
            .collect::<Vec<_>>();
        assert!(category_commands.len() >= 2);
        let moved_id = category_commands[1].clone();

        assert!(state.move_command(&moved_id, -1));
        let reloaded = QuickCommandsState::load(&settings_path);
        let reloaded_order = reloaded
            .commands
            .iter()
            .filter(|command| command.category == category)
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(reloaded_order.first().copied(), Some(moved_id.as_str()));
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn deleting_custom_category_moves_its_commands_to_the_default_group() {
        let settings_path = temp_settings_path("delete-category");
        let mut state = QuickCommandsState::load(&settings_path);
        assert!(!state.delete_category("system"));
        let custom = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Ops".to_string(),
            icon: QuickCommandIcon::Zap,
        });
        state.upsert_command(QuickCommandDraft {
            id: None,
            name: "Restart service".to_string(),
            command: "systemctl restart example".to_string(),
            category: Some(custom.clone()),
            description: None,
            parameters: None,
            protocols: None,
            host_patterns: None,
            confirmation: None,
        });
        assert!(state.delete_category(&custom));
        let reloaded = QuickCommandsState::load(&settings_path);
        assert!(
            !reloaded
                .categories
                .iter()
                .any(|category| category.id == custom)
        );
        assert!(
            reloaded.commands.iter().any(|command| {
                command.name == "Restart service" && command.category == "custom"
            })
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
                sort_order: 0,
            }],
            commands: vec![QuickCommand {
                id: "qc-ls-la".to_string(),
                name: "List Files".to_string(),
                command: "exa -la".to_string(),
                category: "files".to_string(),
                description: None,
                parameters: Vec::new(),
                availability: oxideterm_quick_commands::QuickCommandAvailability::default(),
                confirmation: oxideterm_quick_commands::QuickCommandConfirmationPolicy::Inherit,
                sort_order: 0,
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

    #[test]
    fn import_snapshot_rename_does_not_duplicate_builtin_roundtrip_records() {
        let settings_path = temp_settings_path("import-rename-roundtrip");
        let mut state = QuickCommandsState::load(&settings_path);
        let json = state.export_snapshot_json().unwrap();

        let result = state.apply_snapshot_json(&json, QuickCommandImportStrategy::Rename);

        assert_eq!(result.errors, Vec::<String>::new());
        assert_eq!(result.imported, 0);
        assert_eq!(
            state.categories.len(),
            default_quick_command_categories().len()
        );
        assert_eq!(state.commands.len(), default_quick_commands().len());
        assert_eq!(
            state
                .categories
                .iter()
                .filter(|category| category.id == "system")
                .count(),
            1
        );

        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn reload_from_store_observes_external_structured_sync_write() {
        let settings_path = temp_settings_path("external-sync");
        let mut state = QuickCommandsState::load(&settings_path);
        let mut snapshot = oxideterm_quick_commands::load_snapshot(&settings_path).unwrap();
        snapshot.commands.push(QuickCommand {
            id: "qc-synced".to_string(),
            name: "Synced command".to_string(),
            command: "echo synced".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: Vec::new(),
            availability: oxideterm_quick_commands::QuickCommandAvailability::default(),
            confirmation: oxideterm_quick_commands::QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        });
        oxideterm_quick_commands::save_snapshot(&settings_path, &snapshot).unwrap();

        state.reload_from_store();

        assert!(
            state
                .commands
                .iter()
                .any(|command| command.id == "qc-synced")
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }
}
