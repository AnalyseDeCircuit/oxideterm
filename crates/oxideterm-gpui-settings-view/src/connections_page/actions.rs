use gpui::Context;
use oxideterm_connections::{
    ConnectionStore, ManagedSshKeyInfo, apply_connection_import, preview_connection_import,
};
use oxideterm_i18n::I18n;
use std::collections::HashSet;

use super::ConnectionAction;
use crate::entity::SettingsWorkspaceEntity;

impl SettingsWorkspaceEntity {
    /// Applies a settings page action and reports whether saved connections changed.
    pub fn apply_connection_action(
        &mut self,
        request: ConnectionAction,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        match request {
            ConnectionAction::ImportSelectedSsh => {
                return self.import_selected_settings_ssh_hosts(store, i18n, cx);
            }
            ConnectionAction::ImportSsh(alias) => {
                return self.import_settings_ssh_host(alias, store, i18n, cx);
            }
            ConnectionAction::PreviewImport => {
                self.preview_settings_connection_import(store, i18n, cx)
            }
            ConnectionAction::ApplyImport => {
                return self.apply_settings_connection_import(store, i18n, cx);
            }
            ConnectionAction::OpenKeyFile => self.open_managed_key_import_file_dialog(cx),
            ConnectionAction::OpenKeyPaste => self.open_managed_key_paste_dialog(cx),
            ConnectionAction::OpenKeyRename(id, name) => {
                self.open_managed_key_rename_dialog(id, name, cx)
            }
            ConnectionAction::OpenKeyDelete(key) => {
                self.prepare_managed_key_delete(key, store, i18n, cx)
            }
            ConnectionAction::ImportKeyFile => {
                return self.import_managed_key_from_file(store, i18n, cx);
            }
            ConnectionAction::ImportKeyPaste => {
                return self.import_managed_key_from_paste(store, i18n, cx);
            }
            ConnectionAction::RenameKey => return self.rename_managed_key(store, i18n, cx),
            ConnectionAction::DeleteKey => return self.delete_managed_key(store, i18n, cx),
        }
        false
    }

    fn prepare_managed_key_delete(
        &mut self,
        key: ManagedSshKeyInfo,
        store: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        match store.managed_ssh_key_usage(&key.id) {
            Ok(usage) => {
                self.open_managed_key_delete_dialog(key, usage, cx);
            }
            Err(error) => self.set_managed_key_action_error(error, i18n, cx),
        }
    }

    fn import_managed_key_from_file(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request) = self.take_managed_key_file_import_request() else {
            return false;
        };
        match store.create_managed_ssh_key_from_file(
            &request.path,
            request.name,
            request.passphrase,
        ) {
            Ok(info) => {
                let status = i18n
                    .t("settings_view.ssh_keys.import_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                true
            }
            Err(error) => {
                self.set_managed_key_action_error(error, i18n, cx);
                false
            }
        }
    }

    fn import_managed_key_from_paste(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request) = self.take_managed_key_paste_import_request() else {
            return false;
        };
        // The Entity moves the private key into this one-shot request without a plaintext clone.
        match store.create_managed_ssh_key_from_text(
            request.private_key,
            request.name,
            request.passphrase,
        ) {
            Ok(info) => {
                let status = i18n
                    .t("settings_view.ssh_keys.import_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                true
            }
            Err(error) => {
                self.set_managed_key_action_error(error, i18n, cx);
                false
            }
        }
    }

    fn rename_managed_key(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((key_id, name)) = self.take_managed_key_rename_request() else {
            return false;
        };
        match store.rename_managed_ssh_key(&key_id, name) {
            Ok(info) => {
                let status = i18n
                    .t("settings_view.ssh_keys.rename_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                true
            }
            Err(error) => {
                self.set_managed_key_action_error(error, i18n, cx);
                false
            }
        }
    }

    fn delete_managed_key(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(key_id) = self.managed_key_delete_id() else {
            return false;
        };
        match store.delete_managed_ssh_key(&key_id, false) {
            Ok(result) => {
                let status = i18n
                    .t("settings_view.ssh_keys.delete_success")
                    .replace("{{count}}", &result.deleted.to_string());
                self.finish_managed_key_action(status, true, cx);
                true
            }
            Err(error) => {
                self.set_managed_key_action_error(error, i18n, cx);
                false
            }
        }
    }

    fn set_managed_key_action_error(
        &mut self,
        error: impl std::fmt::Display,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        let status = i18n
            .t("settings_view.ssh_keys.action_failed")
            .replace("{{error}}", &error.to_string());
        self.finish_managed_key_action(status, false, cx);
    }

    fn import_settings_ssh_host(
        &mut self,
        alias: String,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let (completed, status) =
            match oxideterm_connections::import_ssh_config_alias(store, &alias) {
                Ok(true) => {
                    let status = i18n
                        .t("settings_view.errors.import_success")
                        .replace("{{name}}", &alias);
                    (true, status)
                }
                Ok(false) => (
                    false,
                    i18n.t("settings_view.connections.ssh_config.batch_import_skipped")
                        .replace("{{count}}", "1"),
                ),
                Err(error) => (
                    false,
                    i18n.t("settings_view.errors.import_failed")
                        .replace("{{error}}", &error.to_string()),
                ),
            };
        if completed {
            self.ssh_config_selected_hosts.remove(&alias);
        }
        self.connection_import_status = Some(status);
        cx.notify();

        completed
    }

    fn import_selected_settings_ssh_hosts(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let aliases = self.selected_ssh_config_hosts();
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        let mut completed_aliases = Vec::new();

        for alias in aliases {
            match oxideterm_connections::import_ssh_config_alias(store, &alias) {
                Ok(true) => {
                    imported += 1;
                    completed_aliases.push(alias);
                }
                Ok(false) => {
                    skipped += 1;
                    completed_aliases.push(alias);
                }
                Err(error) => errors.push(format!("{alias}: {error}")),
            }
        }

        let mut parts = Vec::new();
        if imported > 0 {
            parts.push(
                i18n.t("settings_view.connections.ssh_config.batch_import_success")
                    .replace("{{count}}", &imported.to_string()),
            );
        }
        if skipped > 0 {
            parts.push(
                i18n.t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", &skipped.to_string()),
            );
        }
        if !errors.is_empty() {
            parts.push(errors.join(", "));
        }
        let status = (!parts.is_empty()).then(|| parts.join("; "));
        for alias in &completed_aliases {
            self.ssh_config_selected_hosts.remove(alias);
        }
        self.connection_import_status = status;
        cx.notify();
        imported > 0
    }

    fn preview_settings_connection_import(
        &mut self,
        store: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        let Some((source, paths)) = self.connection_import_preview_request() else {
            return;
        };
        let existing_names = store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        match preview_connection_import(source, &paths, &existing_names) {
            Ok(preview) => {
                self.apply_connection_import_preview(Ok(preview), cx);
            }
            Err(error) => {
                let status = i18n
                    .t("settings_view.connections.importers.preview_failed")
                    .replace("{{error}}", &error.to_string());
                self.apply_connection_import_preview(Err(status), cx);
            }
        }
    }

    fn apply_settings_connection_import(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request) = self.connection_import_apply_request() else {
            return false;
        };
        match apply_connection_import(store, request) {
            Ok(result) => {
                let mut parts = Vec::new();
                if result.imported > 0 {
                    parts.push(
                        i18n.t("settings_view.connections.importers.imported_count")
                            .replace("{{count}}", &result.imported.to_string()),
                    );
                }
                if result.skipped > 0 {
                    parts.push(
                        i18n.t("settings_view.connections.importers.skipped_count")
                            .replace("{{count}}", &result.skipped.to_string()),
                    );
                }
                if result.renamed > 0 {
                    parts.push(
                        i18n.t("settings_view.connections.importers.renamed_count")
                            .replace("{{count}}", &result.renamed.to_string()),
                    );
                }
                if !result.errors.is_empty() {
                    parts.push(
                        i18n.t("settings_view.connections.importers.error_count")
                            .replace("{{count}}", &result.errors.len().to_string()),
                    );
                }
                let status = if parts.is_empty() {
                    i18n.t("settings_view.connections.importers.no_changes")
                } else {
                    parts.join(" · ")
                };
                self.set_connection_import_status(Some(status), cx);
                self.preview_settings_connection_import(store, i18n, cx);
                result.imported > 0
            }
            Err(error) => {
                let status = i18n
                    .t("settings_view.connections.importers.apply_failed")
                    .replace("{{error}}", &error.to_string());
                self.set_connection_import_status(Some(status), cx);
                false
            }
        }
    }
}
