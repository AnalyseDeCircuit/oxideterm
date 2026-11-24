use super::*;
use oxideterm_connections::{
    ConnectionImportApplyRequest, ManagedSshKeyInfo, ManagedSshKeyUsage, SecretString,
};

pub struct ManagedKeyFileImportRequest {
    pub path: String,
    pub name: Option<String>,
    pub passphrase: Option<SecretString>,
}

pub struct ManagedKeyPasteImportRequest {
    // The private key moves from the Entity into the store boundary exactly once.
    pub private_key: SecretString,
    pub name: Option<String>,
    pub passphrase: Option<SecretString>,
}

impl SettingsWorkspaceEntity {
    pub fn ssh_config_import_snapshot(&self) -> SshConfigImportSnapshot {
        SshConfigImportSnapshot {
            open: self.ssh_config_import_dialog_open,
            selected_hosts: self.ssh_config_selected_hosts.clone(),
            status: self.connection_import_status.clone(),
            presence: self.ssh_config_import_dialog_presence,
        }
    }

    pub fn ssh_config_import_dialog_open(&self) -> bool {
        self.ssh_config_import_dialog_open
    }

    pub fn ssh_config_import_dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.ssh_config_import_dialog_presence.phase()
    }

    pub fn open_ssh_config_import_dialog(&mut self, cx: &mut Context<Self>) {
        // Each visit starts from the current scanned host set instead of
        // carrying selections or status from another import surface.
        self.ssh_config_import_dialog_exit_task = None;
        self.ssh_config_selected_hosts.clear();
        self.connection_import_status = None;
        self.ssh_config_import_dialog_presence.reopen();
        self.ssh_config_import_dialog_open = true;
        cx.notify();
    }

    pub fn close_ssh_config_import_dialog(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.ssh_config_import_dialog_presence.begin_exit() else {
            return;
        };
        self.ssh_config_import_dialog_exit_task = None;
        if delay.is_zero() {
            self.finish_ssh_config_import_dialog_exit(generation, cx);
            return;
        }
        self.ssh_config_import_dialog_exit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_ssh_config_import_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
    }

    fn finish_ssh_config_import_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.ssh_config_import_dialog_exit_task = None;
        if self
            .ssh_config_import_dialog_presence
            .finish_exit(generation)
        {
            self.ssh_config_import_dialog_open = false;
            self.ssh_config_import_dialog_presence.reopen();
            cx.notify();
        }
    }

    pub fn toggle_ssh_config_host(&mut self, alias: String, cx: &mut Context<Self>) {
        if !self.ssh_config_selected_hosts.insert(alias.clone()) {
            self.ssh_config_selected_hosts.remove(&alias);
        }
        cx.notify();
    }

    pub fn clear_ssh_config_host_selection(&mut self, cx: &mut Context<Self>) {
        self.ssh_config_selected_hosts.clear();
        cx.notify();
    }

    pub fn set_selected_ssh_config_hosts(
        &mut self,
        hosts: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.ssh_config_selected_hosts = hosts;
        cx.notify();
    }

    pub fn selected_ssh_config_hosts(&self) -> Vec<String> {
        self.ssh_config_selected_hosts.iter().cloned().collect()
    }

    pub fn set_connection_import_status(&mut self, status: Option<String>, cx: &mut Context<Self>) {
        self.connection_import_status = status;
        cx.notify();
    }

    pub fn connection_import_snapshot(&self) -> ConnectionImportSnapshot {
        ConnectionImportSnapshot {
            source: self.connection_import_source,
            paths: self.connection_import_paths.clone(),
            preview: self.connection_import_preview.clone(),
            selected_draft_ids: self.selected_connection_import_drafts.clone(),
            duplicate_strategy: self.connection_import_duplicate_strategy,
            status: self.connection_import_status.clone(),
        }
    }

    pub fn connection_import_source(&self) -> ConnectionImportSource {
        self.connection_import_source
    }

    pub fn connection_import_duplicate_strategy(&self) -> ConnectionImportDuplicateStrategy {
        self.connection_import_duplicate_strategy
    }

    pub fn connection_import_list_signature(
        &self,
    ) -> (
        bool,
        &'static str,
        usize,
        Option<usize>,
        usize,
        &'static str,
    ) {
        (
            self.connection_import_status.is_some(),
            self.connection_import_source.tag(),
            self.connection_import_paths.len(),
            self.connection_import_preview
                .as_ref()
                .map(|preview| preview.drafts.len()),
            self.selected_connection_import_drafts.len(),
            self.connection_import_duplicate_strategy.tag(),
        )
    }

    pub fn set_connection_import_source(
        &mut self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) {
        if self.connection_import_source == source {
            return;
        }
        // A pending picker belongs to the source that opened it.
        self.connection_import_path_picker_task = None;
        self.connection_import_source = source;
        self.connection_import_paths.clear();
        self.clear_connection_import_preview();
        self.connection_import_status = None;
        cx.notify();
    }

    pub fn set_connection_import_duplicate_strategy(
        &mut self,
        strategy: ConnectionImportDuplicateStrategy,
        cx: &mut Context<Self>,
    ) {
        if self.connection_import_duplicate_strategy != strategy {
            self.connection_import_duplicate_strategy = strategy;
            cx.notify();
        }
    }

    pub fn start_connection_import_path_picker(
        &mut self,
        selected_paths: impl std::future::Future<Output = Option<Vec<String>>> + 'static,
        cx: &mut Context<Self>,
    ) {
        // Retaining the task keeps picker completion owned by the settings surface.
        self.connection_import_path_picker_task = Some(cx.spawn(async move |settings, cx| {
            let selected_paths = selected_paths.await.filter(|paths| !paths.is_empty());
            let _ = settings.update(cx, |settings, cx| {
                settings.connection_import_path_picker_task = None;
                if let Some(paths) = selected_paths {
                    settings.connection_import_paths = paths;
                    settings.clear_connection_import_preview();
                    settings.connection_import_status = None;
                    cx.notify();
                }
            });
        }));
    }

    pub fn connection_import_preview_request(
        &self,
    ) -> Option<(ConnectionImportSource, Vec<String>)> {
        (!self.connection_import_paths.is_empty()).then(|| {
            (
                self.connection_import_source,
                self.connection_import_paths.clone(),
            )
        })
    }

    pub fn apply_connection_import_preview(
        &mut self,
        result: Result<ConnectionImportPreview, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(preview) => {
                self.selected_connection_import_drafts = preview
                    .drafts
                    .iter()
                    .filter(|draft| draft.importable && !draft.duplicate)
                    .map(|draft| draft.id.clone())
                    .collect();
                self.connection_import_preview = Some(preview);
                self.connection_import_status = None;
            }
            Err(status) => self.connection_import_status = Some(status),
        }
        cx.notify();
    }

    pub fn toggle_connection_import_draft(&mut self, draft_id: String, cx: &mut Context<Self>) {
        if !self
            .selected_connection_import_drafts
            .insert(draft_id.clone())
        {
            self.selected_connection_import_drafts.remove(&draft_id);
        }
        cx.notify();
    }

    pub fn toggle_all_connection_import_drafts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        if all_selected {
            self.selected_connection_import_drafts.clear();
        } else if let Some(preview) = self.connection_import_preview.as_ref() {
            self.selected_connection_import_drafts = preview
                .drafts
                .iter()
                .filter(|draft| draft.importable)
                .map(|draft| draft.id.clone())
                .collect();
        }
        cx.notify();
    }

    pub fn connection_import_apply_request(&self) -> Option<ConnectionImportApplyRequest> {
        if self.selected_connection_import_drafts.is_empty()
            || self.connection_import_paths.is_empty()
        {
            return None;
        }
        Some(ConnectionImportApplyRequest {
            source: self.connection_import_source,
            paths: self.connection_import_paths.clone(),
            selected_draft_ids: self
                .selected_connection_import_drafts
                .iter()
                .cloned()
                .collect(),
            duplicate_strategy: self.connection_import_duplicate_strategy,
            target_group: optional_trimmed_string(&self.connection_import_target_group),
        })
    }

    fn clear_connection_import_preview(&mut self) {
        self.connection_import_preview = None;
        self.selected_connection_import_drafts.clear();
    }

    pub fn managed_key_status(&self) -> Option<&str> {
        self.managed_key_status.as_deref()
    }

    pub fn clear_managed_key_status(&mut self, cx: &mut Context<Self>) {
        if self.managed_key_status.take().is_some() {
            cx.notify();
        }
    }

    pub fn managed_key_dialog_snapshot(&self) -> Option<ManagedKeyDialogSnapshot> {
        let presence = self.managed_key_dialog_presence;
        match self.managed_key_dialog.as_ref()? {
            SettingsManagedKeyDialog::ImportFile => Some(ManagedKeyDialogSnapshot::ImportFile {
                file_path: self.managed_key_file_path.clone(),
                file_name: self.managed_key_file_name.clone(),
                presence,
            }),
            SettingsManagedKeyDialog::Paste => Some(ManagedKeyDialogSnapshot::Paste {
                name: self.managed_key_paste_name.clone(),
                // The view needs only validation state; plaintext remains in the Entity.
                private_key_present: !self.managed_key_paste_private_key.trim().is_empty(),
                presence,
            }),
            SettingsManagedKeyDialog::Rename { .. } => Some(ManagedKeyDialogSnapshot::Rename {
                name: self.managed_key_rename_name.clone(),
                presence,
            }),
            SettingsManagedKeyDialog::Delete { key, usage } => {
                Some(ManagedKeyDialogSnapshot::Delete {
                    key: key.clone(),
                    usage: usage.clone(),
                    presence,
                })
            }
        }
    }

    pub fn managed_key_dialog_open(&self) -> bool {
        self.managed_key_dialog.is_some()
    }

    pub fn managed_key_dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.managed_key_dialog_presence.phase()
    }

    pub fn open_managed_key_import_file_dialog(&mut self, cx: &mut Context<Self>) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::ImportFile);
        cx.notify();
    }

    pub fn open_managed_key_paste_dialog(&mut self, cx: &mut Context<Self>) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::Paste);
        cx.notify();
    }

    pub fn open_managed_key_rename_dialog(
        &mut self,
        key_id: String,
        key_name: String,
        cx: &mut Context<Self>,
    ) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_rename_name = key_name;
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::Rename { key_id });
        cx.notify();
    }

    pub fn open_managed_key_delete_dialog(
        &mut self,
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
        cx: &mut Context<Self>,
    ) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::Delete { key, usage });
        cx.notify();
    }

    pub fn close_managed_key_dialog(&mut self, delay: std::time::Duration, cx: &mut Context<Self>) {
        let Some(generation) = self.managed_key_dialog_presence.begin_exit() else {
            return;
        };
        self.managed_key_dialog_exit_task = None;
        if delay.is_zero() {
            self.finish_managed_key_dialog_exit(generation, cx);
            return;
        }
        self.managed_key_dialog_exit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_managed_key_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
    }

    fn finish_managed_key_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.managed_key_dialog_exit_task = None;
        if self.managed_key_dialog_presence.finish_exit(generation) {
            self.managed_key_dialog = None;
            self.clear_managed_key_dialog_drafts();
            self.managed_key_dialog_presence.reopen();
            cx.notify();
        }
    }

    fn clear_managed_key_dialog_drafts(&mut self) {
        self.managed_key_file_picker_task = None;
        self.managed_key_file_path.clear();
        self.managed_key_file_name.clear();
        zeroize::Zeroize::zeroize(&mut *self.managed_key_file_passphrase);
        self.managed_key_paste_name.clear();
        zeroize::Zeroize::zeroize(&mut *self.managed_key_paste_private_key);
        zeroize::Zeroize::zeroize(&mut *self.managed_key_paste_passphrase);
        self.managed_key_rename_name.clear();
        if self
            .settings_focused_input
            .is_some_and(is_managed_key_input)
        {
            self.settings_focused_input = None;
        }
    }

    pub fn set_managed_key_import_file(
        &mut self,
        path: String,
        default_name: String,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.managed_key_dialog,
            Some(SettingsManagedKeyDialog::ImportFile)
        ) {
            return;
        }
        self.managed_key_file_path = path;
        if self.managed_key_file_name.trim().is_empty() {
            self.managed_key_file_name = default_name;
        }
        self.managed_key_status = None;
        cx.notify();
    }

    pub fn start_managed_key_file_picker(
        &mut self,
        selected_file: impl std::future::Future<Output = Option<(String, String)>> + 'static,
        cx: &mut Context<Self>,
    ) {
        // The picker completion must not retain or write through WorkspaceApp.
        self.managed_key_file_picker_task = Some(cx.spawn(async move |settings, cx| {
            let selected_file = selected_file.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.managed_key_file_picker_task = None;
                if let Some((path, default_name)) = selected_file {
                    settings.set_managed_key_import_file(path, default_name, cx);
                }
            });
        }));
    }

    pub fn take_managed_key_file_import_request(&mut self) -> Option<ManagedKeyFileImportRequest> {
        if !matches!(
            self.managed_key_dialog,
            Some(SettingsManagedKeyDialog::ImportFile)
        ) {
            return None;
        }
        let path = self.managed_key_file_path.trim();
        if path.is_empty() {
            return None;
        }
        let request = ManagedKeyFileImportRequest {
            path: path.to_string(),
            name: optional_trimmed_string(&self.managed_key_file_name),
            passphrase: take_optional_managed_key_secret(&mut self.managed_key_file_passphrase),
        };
        self.settings_focused_input = None;
        Some(request)
    }

    pub fn take_managed_key_paste_import_request(
        &mut self,
    ) -> Option<ManagedKeyPasteImportRequest> {
        if !matches!(
            self.managed_key_dialog,
            Some(SettingsManagedKeyDialog::Paste)
        ) || self.managed_key_paste_private_key.trim().is_empty()
        {
            return None;
        }
        let private_key = std::mem::replace(
            &mut self.managed_key_paste_private_key,
            zeroize::Zeroizing::new(String::new()),
        );
        let request = ManagedKeyPasteImportRequest {
            private_key: SecretString::from(private_key),
            name: optional_trimmed_string(&self.managed_key_paste_name),
            passphrase: take_optional_managed_key_secret(&mut self.managed_key_paste_passphrase),
        };
        self.settings_focused_input = None;
        Some(request)
    }

    pub fn take_managed_key_rename_request(&mut self) -> Option<(String, String)> {
        let SettingsManagedKeyDialog::Rename { key_id } = self.managed_key_dialog.as_ref()? else {
            return None;
        };
        let name = self.managed_key_rename_name.trim();
        if name.is_empty() {
            return None;
        }
        Some((key_id.clone(), name.to_string()))
    }

    pub fn managed_key_delete_id(&self) -> Option<String> {
        let SettingsManagedKeyDialog::Delete { key, .. } = self.managed_key_dialog.as_ref()? else {
            return None;
        };
        Some(key.id.clone())
    }

    pub fn finish_managed_key_action(
        &mut self,
        status: String,
        success: bool,
        cx: &mut Context<Self>,
    ) {
        self.managed_key_status = Some(status);
        if success {
            self.managed_key_dialog = None;
            self.managed_key_dialog_exit_task = None;
            self.clear_managed_key_dialog_drafts();
            self.managed_key_dialog_presence.reopen();
        }
        cx.notify();
    }
}

fn is_managed_key_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName
    )
}

fn optional_trimmed_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn take_optional_managed_key_secret(
    value: &mut zeroize::Zeroizing<String>,
) -> Option<SecretString> {
    if value.trim().is_empty() {
        zeroize::Zeroize::zeroize(&mut **value);
        return None;
    }
    let owned = std::mem::replace(value, zeroize::Zeroizing::new(String::new()));
    let trimmed = owned.trim();
    if trimmed.len() == owned.len() {
        return Some(SecretString::from(owned));
    }
    Some(SecretString::from(zeroize::Zeroizing::new(
        trimmed.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn managed_key_paste_request_moves_secrets_out_of_entity(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.open_managed_key_paste_dialog(cx);
            assert!(
                settings.focus_settings_entity_input(SettingsInput::ManagedKeyPastePrivateKey, cx)
            );
            assert!(settings.replace_settings_entity_input(
                SettingsInput::ManagedKeyPastePrivateKey,
                None,
                "private-key-material",
                cx,
            ));
            assert!(
                settings.focus_settings_entity_input(SettingsInput::ManagedKeyPastePassphrase, cx)
            );
            assert!(settings.replace_settings_entity_input(
                SettingsInput::ManagedKeyPastePassphrase,
                None,
                " key-passphrase ",
                cx,
            ));
            let private_key_allocation = settings.managed_key_paste_private_key.as_ptr();

            // Submission moves each secret into the one-shot store request.
            let request = settings
                .take_managed_key_paste_import_request()
                .expect("paste request");
            assert_eq!(request.private_key.expose_secret(), "private-key-material");
            assert_eq!(
                request.private_key.expose_secret().as_ptr(),
                private_key_allocation
            );
            assert_eq!(
                request.passphrase.as_ref().map(SecretString::expose_secret),
                Some("key-passphrase")
            );
            assert!(settings.managed_key_paste_private_key.is_empty());
            assert!(settings.managed_key_paste_passphrase.is_empty());
            assert_eq!(settings.settings_entity_focused_input(), None);
        });
    }

    #[gpui::test]
    fn closing_managed_key_dialog_clears_entity_owned_drafts(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.open_managed_key_import_file_dialog(cx);
            assert!(
                settings.focus_settings_entity_input(SettingsInput::ManagedKeyFilePassphrase, cx)
            );
            assert!(settings.replace_settings_entity_input(
                SettingsInput::ManagedKeyFilePassphrase,
                None,
                "secret-passphrase",
                cx,
            ));

            settings.close_managed_key_dialog(std::time::Duration::ZERO, cx);

            assert!(settings.managed_key_dialog.is_none());
            assert!(settings.managed_key_file_passphrase.is_empty());
            assert_eq!(settings.settings_entity_focused_input(), None);
            assert!(settings.managed_key_dialog_exit_task.is_none());
        });
    }

    #[gpui::test]
    fn managed_key_file_picker_completion_and_cancellation_are_entity_owned(
        cx: &mut TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.open_managed_key_import_file_dialog(cx);
            settings.start_managed_key_file_picker(
                std::future::ready(Some(("/tmp/id_ed25519".into(), "id_ed25519".into()))),
                cx,
            );
            assert!(settings.managed_key_file_picker_task.is_some());
        });

        cx.run_until_parked();

        settings.update(cx, |settings, cx| {
            assert_eq!(settings.managed_key_file_path, "/tmp/id_ed25519");
            assert_eq!(settings.managed_key_file_name, "id_ed25519");
            assert!(settings.managed_key_file_picker_task.is_none());

            settings.start_managed_key_file_picker(std::future::pending(), cx);
            assert!(settings.managed_key_file_picker_task.is_some());
            settings.close_managed_key_dialog(std::time::Duration::ZERO, cx);
            assert!(settings.managed_key_file_picker_task.is_none());
        });
    }

    #[gpui::test]
    fn connection_import_state_input_and_picker_completion_are_entity_owned(
        cx: &mut TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.set_connection_import_source(ConnectionImportSource::Xshell, cx);
            settings.set_connection_import_duplicate_strategy(
                ConnectionImportDuplicateStrategy::Rename,
                cx,
            );
            assert!(
                settings
                    .focus_settings_entity_input(SettingsInput::ConnectionImportTargetGroup, cx,)
            );
            assert!(settings.replace_settings_entity_input(
                SettingsInput::ConnectionImportTargetGroup,
                None,
                "imported",
                cx,
            ));
            settings.start_connection_import_path_picker(
                std::future::ready(Some(vec!["/tmp/connections.ini".into()])),
                cx,
            );
            assert!(settings.connection_import_path_picker_task.is_some());
        });

        cx.run_until_parked();

        cx.read(|cx| {
            let settings = settings.read(cx);
            assert_eq!(
                settings.connection_import_source,
                ConnectionImportSource::Xshell
            );
            assert_eq!(
                settings.connection_import_duplicate_strategy,
                ConnectionImportDuplicateStrategy::Rename
            );
            assert_eq!(settings.connection_import_target_group, "imported");
            assert_eq!(
                settings.connection_import_paths,
                ["/tmp/connections.ini".to_string()]
            );
            assert!(settings.connection_import_path_picker_task.is_none());
        });

        settings.update(cx, |settings, cx| {
            settings.set_connection_import_source(ConnectionImportSource::XTerminal, cx);
            settings.start_connection_import_path_picker(
                std::future::ready(Some(vec!["/tmp/xterminal-export.txt".into()])),
                cx,
            );
            settings.set_connection_import_source(ConnectionImportSource::Xshell, cx);
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert!(settings.read(cx).connection_import_paths.is_empty());
        });
    }
}
