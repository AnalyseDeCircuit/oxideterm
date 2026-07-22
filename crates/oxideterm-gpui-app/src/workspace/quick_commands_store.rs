use std::path::Path;

use super::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandCategoryDraft, QuickCommandDraft,
    QuickCommandImportResult, QuickCommandImportStrategy, QuickCommandsSnapshot,
    QuickCommandsState, default_quick_command_categories, default_quick_commands, now_ms,
};
use oxideterm_quick_commands::{
    delete_quick_command, delete_quick_command_category, ensure_active_quick_command_category,
    upsert_quick_command, upsert_quick_command_category, visible_quick_commands,
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

    pub(in crate::workspace) fn upsert_command(&mut self, draft: QuickCommandDraft) {
        if upsert_quick_command(&mut self.commands, &self.categories, draft, now_ms()) {
            self.persist();
        }
    }

    pub(in crate::workspace) fn delete_command(&mut self, id: &str) {
        if delete_quick_command(&mut self.commands, id) {
            self.persist();
        }
    }

    pub(super) fn upsert_category(&mut self, draft: QuickCommandCategoryDraft) -> String {
        self.active_category =
            upsert_quick_command_category(&mut self.categories, draft, &self.active_category);
        self.persist();
        self.active_category.clone()
    }

    pub(super) fn delete_category(&mut self, id: &str) -> bool {
        if !delete_quick_command_category(&mut self.categories, &self.commands, id) {
            return false;
        }
        self.ensure_active_category();
        self.persist();
        true
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn export_snapshot_json(&self) -> Result<String, String> {
        oxideterm_quick_commands::export_snapshot_json(&self.settings_path)
    }

    #[allow(dead_code)]
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
            host_pattern: None,
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
