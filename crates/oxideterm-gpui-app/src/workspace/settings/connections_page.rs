use super::*;

pub(in crate::workspace) const CONNECTION_IDLE_TIMEOUT_CONTROL_WIDTH: f32 = 320.0; // Keep the standalone idle-timeout select from expanding into side panels.
pub(in crate::workspace) const CONNECTION_RESPONSIVE_FIELD_BASIS: f32 = 240.0; // Preferred field width before a settings row redistributes space.
pub(in crate::workspace) const CONNECTION_IMPORT_SOURCE_BASIS: f32 = 220.0; // Match the desktop source column while allowing narrow panes to wrap.
pub(in crate::workspace) const CONNECTION_IMPORT_PATH_BASIS: f32 = 420.0; // Give localized path actions room before they move below the source picker.
pub(in crate::workspace) const CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS: f32 = 420.0; // Keep preview controls together until the toolbar wraps.
pub(in crate::workspace) const CONNECTION_IMPORT_AUTH_WIDTH: f32 = 120.0; // Match the compact trailing authentication column from Tauri.
pub(in crate::workspace) const SSH_KEY_HEADER_TEXT_BASIS: f32 = 320.0; // Let long localized descriptions wrap before key actions.
pub(in crate::workspace) const SSH_CONFIG_IMPORT_DIALOG_WIDTH: f32 = 720.0;
pub(in crate::workspace) const SSH_CONFIG_IMPORT_DIALOG_HEIGHT: f32 = 560.0;

struct ManagedKeyFileImportRequest {
    path: String,
    name: Option<String>,
    passphrase: Option<SecretString>,
}

struct ManagedKeyPasteImportRequest {
    // The private key moves from the Entity into the store boundary exactly once.
    private_key: SecretString,
    name: Option<String>,
    passphrase: Option<SecretString>,
}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn ssh_config_import_snapshot(&self) -> SshConfigImportSnapshot {
        SshConfigImportSnapshot {
            open: self.ssh_config_import_dialog_open,
            selected_hosts: self.ssh_config_selected_hosts.clone(),
            status: self.connection_import_status.clone(),
            presence: self.ssh_config_import_dialog_presence,
        }
    }

    pub(in crate::workspace) fn ssh_config_import_dialog_open(&self) -> bool {
        self.ssh_config_import_dialog_open
    }

    pub(in crate::workspace) fn open_ssh_config_import_dialog(&mut self, cx: &mut Context<Self>) {
        // Each visit starts from the current scanned host set instead of
        // carrying selections or status from another import surface.
        self.ssh_config_import_dialog_exit_task = None;
        self.ssh_config_selected_hosts.clear();
        self.connection_import_status = None;
        self.ssh_config_import_dialog_presence.reopen();
        self.ssh_config_import_dialog_open = true;
        cx.notify();
    }

    pub(in crate::workspace) fn close_ssh_config_import_dialog(
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
            gpui::Timer::after(delay).await;
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

    pub(in crate::workspace) fn toggle_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        if !self.ssh_config_selected_hosts.insert(alias.clone()) {
            self.ssh_config_selected_hosts.remove(&alias);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn clear_ssh_config_host_selection(&mut self, cx: &mut Context<Self>) {
        self.ssh_config_selected_hosts.clear();
        cx.notify();
    }

    pub(in crate::workspace) fn set_selected_ssh_config_hosts(
        &mut self,
        hosts: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.ssh_config_selected_hosts = hosts;
        cx.notify();
    }

    pub(in crate::workspace) fn selected_ssh_config_hosts(&self) -> Vec<String> {
        self.ssh_config_selected_hosts.iter().cloned().collect()
    }

    pub(in crate::workspace) fn set_connection_import_status(
        &mut self,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.connection_import_status = status;
        cx.notify();
    }

    pub(in crate::workspace) fn connection_import_snapshot(&self) -> ConnectionImportSnapshot {
        ConnectionImportSnapshot {
            source: self.connection_import_source,
            paths: self.connection_import_paths.clone(),
            preview: self.connection_import_preview.clone(),
            selected_draft_ids: self.selected_connection_import_drafts.clone(),
            duplicate_strategy: self.connection_import_duplicate_strategy,
            status: self.connection_import_status.clone(),
        }
    }

    pub(in crate::workspace) fn connection_import_source(&self) -> ConnectionImportSource {
        self.connection_import_source
    }

    pub(in crate::workspace) fn connection_import_duplicate_strategy(
        &self,
    ) -> ConnectionImportDuplicateStrategy {
        self.connection_import_duplicate_strategy
    }

    pub(in crate::workspace) fn connection_import_list_signature(
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

    pub(in crate::workspace) fn set_connection_import_source(
        &mut self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) {
        if self.connection_import_source == source {
            return;
        }
        self.connection_import_source = source;
        self.connection_import_paths.clear();
        self.clear_connection_import_preview();
        self.connection_import_status = None;
        cx.notify();
    }

    pub(in crate::workspace) fn set_connection_import_duplicate_strategy(
        &mut self,
        strategy: ConnectionImportDuplicateStrategy,
        cx: &mut Context<Self>,
    ) {
        if self.connection_import_duplicate_strategy != strategy {
            self.connection_import_duplicate_strategy = strategy;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn start_connection_import_path_picker(
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

    pub(in crate::workspace) fn connection_import_preview_request(
        &self,
    ) -> Option<(ConnectionImportSource, Vec<String>)> {
        (!self.connection_import_paths.is_empty()).then(|| {
            (
                self.connection_import_source,
                self.connection_import_paths.clone(),
            )
        })
    }

    pub(in crate::workspace) fn apply_connection_import_preview(
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

    pub(in crate::workspace) fn toggle_connection_import_draft(
        &mut self,
        draft_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .selected_connection_import_drafts
            .insert(draft_id.clone())
        {
            self.selected_connection_import_drafts.remove(&draft_id);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_all_connection_import_drafts(
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

    pub(in crate::workspace) fn connection_import_apply_request(
        &self,
    ) -> Option<ConnectionImportApplyRequest> {
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
            target_group: non_empty_trimmed(&self.connection_import_target_group),
        })
    }

    fn clear_connection_import_preview(&mut self) {
        self.connection_import_preview = None;
        self.selected_connection_import_drafts.clear();
    }

    pub(in crate::workspace) fn managed_key_status(&self) -> Option<&str> {
        self.managed_key_status.as_deref()
    }

    pub(in crate::workspace) fn clear_managed_key_status(&mut self, cx: &mut Context<Self>) {
        if self.managed_key_status.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn managed_key_dialog_snapshot(
        &self,
    ) -> Option<ManagedKeyDialogSnapshot> {
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

    pub(in crate::workspace) fn open_managed_key_import_file_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::ImportFile);
        cx.notify();
    }

    pub(in crate::workspace) fn open_managed_key_paste_dialog(&mut self, cx: &mut Context<Self>) {
        self.managed_key_dialog_exit_task = None;
        self.clear_managed_key_dialog_drafts();
        self.managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.managed_key_dialog = Some(SettingsManagedKeyDialog::Paste);
        cx.notify();
    }

    pub(in crate::workspace) fn open_managed_key_rename_dialog(
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

    pub(in crate::workspace) fn open_managed_key_delete_dialog(
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

    pub(in crate::workspace) fn close_managed_key_dialog(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.managed_key_dialog_presence.begin_exit() else {
            return;
        };
        self.managed_key_dialog_exit_task = None;
        if delay.is_zero() {
            self.finish_managed_key_dialog_exit(generation, cx);
            return;
        }
        self.managed_key_dialog_exit_task = Some(cx.spawn(async move |settings, cx| {
            gpui::Timer::after(delay).await;
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

    pub(in crate::workspace) fn set_managed_key_import_file(
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

    pub(in crate::workspace) fn start_managed_key_file_picker(
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

    fn take_managed_key_file_import_request(&mut self) -> Option<ManagedKeyFileImportRequest> {
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

    fn take_managed_key_paste_import_request(&mut self) -> Option<ManagedKeyPasteImportRequest> {
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

    fn take_managed_key_rename_request(&mut self) -> Option<(String, String)> {
        let SettingsManagedKeyDialog::Rename { key_id } = self.managed_key_dialog.as_ref()? else {
            return None;
        };
        let name = self.managed_key_rename_name.trim();
        if name.is_empty() {
            return None;
        }
        Some((key_id.clone(), name.to_string()))
    }

    fn managed_key_delete_id(&self) -> Option<String> {
        let SettingsManagedKeyDialog::Delete { key, .. } = self.managed_key_dialog.as_ref()? else {
            return None;
        };
        Some(key.id.clone())
    }

    pub(in crate::workspace) fn finish_managed_key_action(
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

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_connections_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_ssh_section(0, cx),
            1 => self.settings_card(
                "settings_view.connections.title",
                "settings_view.connections.description",
                vec![self.connection_defaults_section(settings, cx)],
            ),
            2 => self.connection_section(
                "settings_view.connections.idle_timeout.title",
                "settings_view.connections.idle_timeout.description",
                vec![self.connection_idle_timeout_control(settings, cx)],
            ),
            3 => self.settings_reconnect_section(0, cx),
            4 => self.ssh_config_import_section(cx),
            5 => self.connection_importers_section(cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn settings_ssh_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section_index != 0 {
            return div().into_any_element();
        }
        let keys = list_available_ssh_keys();
        let managed_keys = self.connection_store.managed_ssh_keys();
        let mut local_list = div().w_full().min_w_0().flex().flex_col();
        if keys.is_empty() {
            local_list = local_list.child(self.ssh_keys_empty_state());
        } else {
            let key_count = keys.len();
            for (index, key) in keys.into_iter().enumerate() {
                local_list = local_list.child(self.ssh_key_row(key));
                if index + 1 < key_count {
                    local_list = local_list.child(self.card_separator());
                }
            }
        }
        let managed_key_count = managed_keys.len();
        let mut managed_list = div().w_full().min_w_0().flex().flex_col();
        for (index, key) in managed_keys.into_iter().enumerate() {
            managed_list = managed_list.child(self.managed_ssh_key_row(key, cx));
            if index + 1 < managed_key_count {
                managed_list = managed_list.child(self.card_separator());
            }
        }
        let content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(32.0))
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.local_section",
                "settings_view.ssh_keys.local_description",
                None,
            ))
            .child(local_list)
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.managed_section",
                "settings_view.ssh_keys.managed_description",
                Some(self.managed_ssh_key_toolbar(cx)),
            ))
            .when_some(
                self.settings_workspace
                    .read(cx)
                    .managed_key_status()
                    .map(str::to_string),
                |section, status| section.child(self.connection_status_row(status)),
            )
            .child(if self.connection_store.managed_ssh_keys().is_empty() {
                self.managed_ssh_keys_empty_state()
            } else {
                managed_list.into_any_element()
            });

        self.settings_card(
            "settings_view.ssh_keys.title",
            "settings_view.ssh_keys.description",
            vec![content.into_any_element()],
        )
    }

    pub(in crate::workspace) fn connection_defaults_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .gap(px(32.0))
            .child(self.connection_labeled_input(
                "settings_view.connections.default_username",
                SettingsInput::ConnectionDefaultUsername,
                settings.connection_defaults.username.clone(),
                settings.connection_defaults.username.clone(),
                cx,
            ))
            .child(self.connection_labeled_input(
                "settings_view.connections.default_port",
                SettingsInput::ConnectionDefaultPort,
                settings.connection_defaults.port.to_string(),
                "22".to_string(),
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_labeled_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .max_w_full()
            .flex_1()
            .flex_basis(px(CONNECTION_RESPONSIVE_FIELD_BASIS))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control_fill(input, value, placeholder, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_idle_timeout_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::ConnectionIdleTimeout;
        let value =
            connection_idle_timeout_label(settings.connection_pool.idle_timeout_secs, &self.i18n);
        let control = self.settings_select_control(
            select_id,
            value,
            false,
            Some(CONNECTION_IDLE_TIMEOUT_CONTROL_WIDTH),
            cx,
        );

        self.setting_row(
            "settings_view.connections.idle_timeout.label",
            "settings_view.connections.idle_timeout.hint",
            control,
            cx,
        )
    }

    pub(in crate::workspace) fn ssh_config_import_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auto_load_hosts = self.settings_store.settings().ssh_config.auto_load_hosts;
        let auto_sync_hosts = self.settings_store.settings().ssh_config.auto_sync_hosts;
        let allow_proxy_command = self
            .settings_store
            .settings()
            .ssh_config
            .allow_proxy_command;
        self.connection_section(
            "settings_view.connections.ssh_config.title",
            "settings_view.connections.ssh_config.description",
            vec![
                self.setting_row(
                    "settings_view.connections.ssh_config.auto_load",
                    "settings_view.connections.ssh_config.auto_load_hint",
                    checkbox(&self.tokens, String::new(), auto_load_hosts)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.edit_settings(
                                    |settings| {
                                        set_ssh_config_auto_load_hosts(settings, !auto_load_hosts)
                                    },
                                    cx,
                                );
                                this.refresh_session_manager_ssh_config_hosts(cx);
                                if this.command_palette.read(cx).is_open() {
                                    this.load_command_palette_ssh_config_hosts(cx);
                                }
                            }),
                        )
                        .into_any_element(),
                    cx,
                ),
                self.setting_row(
                    "settings_view.connections.ssh_config.auto_sync",
                    "settings_view.connections.ssh_config.auto_sync_hint",
                    checkbox(&self.tokens, String::new(), auto_sync_hosts)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.edit_settings(
                                    |settings| {
                                        set_ssh_config_auto_sync_hosts(settings, !auto_sync_hosts)
                                    },
                                    cx,
                                );
                                this.sync_ssh_config_sync_service();
                            }),
                        )
                        .into_any_element(),
                    cx,
                ),
                self.setting_row(
                    "settings_view.connections.ssh_config.allow_proxy_command",
                    "settings_view.connections.ssh_config.allow_proxy_command_hint",
                    checkbox(&self.tokens, String::new(), allow_proxy_command)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.edit_settings(
                                    |settings| {
                                        set_ssh_config_allow_proxy_command(
                                            settings,
                                            !allow_proxy_command,
                                        )
                                    },
                                    cx,
                                );
                            }),
                        )
                        .into_any_element(),
                    cx,
                ),
                div()
                    .flex()
                    .justify_start()
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.connections.ssh_config.open"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::FolderInput,
                            16.0,
                            rgb(self.tokens.ui.text),
                        )),
                        self.connection_import_secondary_button_options(false),
                        cx.listener(|this, _event, _window, cx| {
                            this.open_settings_ssh_config_import_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .into_any_element(),
            ],
        )
    }

    pub(in crate::workspace) fn sync_ssh_config_sync_service(&mut self) {
        let enabled = self.settings_store.settings().ssh_config.auto_sync_hosts;
        if enabled == self.ssh_config_sync_service.is_some() {
            return;
        }
        self.ssh_config_sync_service = enabled.then(|| {
            oxideterm_connections::SshConfigSyncService::start(
                self.connection_store.path().to_path_buf(),
                oxideterm_connections::default_ssh_config_path(),
                Duration::from_secs(10),
            )
        });
    }

    pub(in crate::workspace) fn render_settings_ssh_config_import_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self
            .settings_workspace
            .read(cx)
            .ssh_config_import_snapshot();
        if !dialog.open {
            return None;
        }

        let ssh_hosts = self.settings_ssh_config_hosts();
        let importable_count = ssh_hosts
            .iter()
            .filter(|host| !host.already_imported)
            .count();
        let selected_count = dialog.selected_hosts.len();
        let all_selected = importable_count > 0 && selected_count == importable_count;
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_settings_ssh_config_import_dialog(cx);
                cx.stop_propagation();
            }),
        );

        let mut list = div()
            .id("settings-ssh-config-dialog-scroll")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("settings-ssh-config-dialog-scroll"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0));
        if ssh_hosts.is_empty() {
            list = list.child(self.ssh_config_empty_state());
        } else {
            for host in ssh_hosts {
                let selected = dialog.selected_hosts.contains(&host.alias);
                list = list.child(self.ssh_config_host_row(host, selected, cx));
            }
        }

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .px(px(24.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .when(importable_count > 0, |body| {
                body.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(self.ssh_config_toggle_all_button(
                            all_selected,
                            importable_count,
                            cx,
                        ))
                        .when(selected_count > 0, |toolbar| {
                            toolbar.child(self.ssh_config_batch_import_button(selected_count, cx))
                        }),
                )
            })
            .when_some(dialog.status, |body, status| {
                body.child(self.connection_status_row(status))
            })
            .child(list);

        let form = dialog_content(&self.tokens)
            .w(px(SSH_CONFIG_IMPORT_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .h(px(SSH_CONFIG_IMPORT_DIALOG_HEIGHT))
            .max_h(relative(0.88))
            .flex()
            .flex_col()
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.connections.ssh_config.title"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.connections.ssh_config.description"),
                    )),
            )
            .child(body)
            .child(
                dialog_footer(&self.tokens).child(self.standard_footer_action_button(
                    self.i18n.t("settings_view.connections.ssh_config.close"),
                    ButtonVariant::Secondary,
                    ConfirmDialogAction::Cancel,
                    false,
                    |this, _event, _window, cx| {
                        this.close_settings_ssh_config_import_dialog(cx);
                    },
                    cx,
                )),
            );

        Some(settings_dialog_transition(
            &self.tokens,
            "ssh-config-import-dialog-form",
            backdrop,
            form,
            dialog.presence.phase(),
        ))
    }

    pub(in crate::workspace) fn settings_ssh_config_hosts(&self) -> Vec<SshConfigHost> {
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        list_ssh_config_hosts(&existing_names).unwrap_or_default()
    }

    pub(in crate::workspace) fn open_settings_ssh_config_import_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_ssh_config_import_dialog(cx);
        });
        self.reset_standard_confirm_focus();
    }

    pub(in crate::workspace) fn close_settings_ssh_config_import_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_ssh_config_import_dialog(delay, cx);
        });
    }

    pub(in crate::workspace) fn connection_section(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        let mut card_rows = vec![
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t(description_key))
                .into_any_element(),
        ];
        card_rows.extend(rows);

        self.settings_card(title_key, description_key, card_rows)
    }

    pub(in crate::workspace) fn ssh_config_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.ssh_config.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.ssh_config.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent_secondary))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
            .child(label)
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_all_settings_ssh_config_hosts(all_selected, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_batch_import_button(
        &self,
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.ssh_config.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        // This is the same compact outline action chrome as other migrated
        // settings toolbars; keep it on the workspace action wrapper so click
        // dispatch shares the disabled/loading guard with other Buttons.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(28.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.import_selected_settings_ssh_hosts(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_host_row(
        &self,
        host: SshConfigHost,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let alias = host.alias.clone();
        let disabled = host.already_imported;
        let detail = format!(
            "{}@{}:{}",
            host.user.as_deref().unwrap_or_default(),
            host.hostname.as_deref().unwrap_or(alias.as_str()),
            host.port.unwrap_or(22)
        );

        div()
            .w_full()
            .mb(px(4.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0x00000000))
            .bg(rgba(0x00000000))
            .p(px(12.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .hover(|style| {
                if disabled {
                    style
                } else {
                    style
                        .bg(rgb(self.tokens.ui.bg_hover))
                        .border_color(rgb(self.tokens.ui.border))
                }
            })
            .child(
                div()
                    .cursor_pointer()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.ssh_config_checkbox(selected))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(host.alias.clone()),
                                    )
                                    .when(host.already_imported, |row| {
                                        row.child(self.ssh_config_imported_badge())
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    )
                    .when(!disabled, |left| {
                        let alias = alias.clone();
                        left.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.toggle_settings_ssh_config_host(alias.clone(), cx);
                                cx.stop_propagation();
                            }),
                        )
                    }),
            )
            .child(self.ssh_config_import_button(host.alias, disabled, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_import_button(
        &self,
        alias: String,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Host-row import uses the shared outline action path but preserves
        // Tauri's pill shape with a post-primitive radius override.
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.connections.ssh_config.import"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(34.0),
                padding_x: Some(14.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.import_settings_ssh_host(alias.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .rounded_full()
        .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_checkbox(&self, checked: bool) -> AnyElement {
        div()
            .size(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(if checked {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(if checked {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.accent_text))
            .child(if checked { "✓" } else { "" })
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_imported_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.accent_secondary))
            .child(
                self.i18n
                    .t("settings_view.connections.ssh_config.already_imported"),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .max_w(px(672.0))
            .h(px(256.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.connections.ssh_config.no_hosts"))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_importers_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut importer = self
            .settings_workspace
            .read(cx)
            .connection_import_snapshot();
        let mut rows = vec![self.connection_import_input_row(importer.source, &importer.paths, cx)];

        if let Some(preview) = importer.preview.take() {
            rows.push(self.connection_import_preview_toolbar(
                &preview,
                &importer.selected_draft_ids,
                importer.duplicate_strategy,
                cx,
            ));
            rows.push(self.connection_import_preview_list(
                preview,
                &importer.selected_draft_ids,
                cx,
            ));
        }
        if let Some(status) = importer.status {
            rows.push(self.connection_status_row(status.to_string()));
        }

        self.connection_section(
            "settings_view.connections.importers.title",
            "settings_view.connections.importers.description",
            rows,
        )
    }

    pub(in crate::workspace) fn connection_import_input_row(
        &self,
        source: ConnectionImportSource,
        paths: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(16.0))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_SOURCE_BASIS))
                    .child(self.connection_import_source_picker(source, cx)),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PATH_BASIS))
                    .grid()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.connections.importers.paths")),
                    )
                    .child(self.connection_import_path_toolbar(source, !paths.is_empty(), cx))
                    .child(self.connection_import_path_summary(paths)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_source_picker(
        &self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = connection_import_source_label(source, &self.i18n);
        div()
            .w_full()
            .min_w_0()
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.connections.importers.source")),
            )
            .child(self.settings_select_control(
                SettingsSelect::ConnectionImportSource,
                selected_label,
                false,
                None,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_path_toolbar(
        &self,
        source: ConnectionImportSource,
        has_paths: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .when(connection_import_supports_files(source), |row| {
                row.child(self.connection_import_pick_files_button(cx))
            })
            .when(connection_import_supports_directory(source), |row| {
                row.child(self.connection_import_pick_directory_button(cx))
            })
            .child(self.connection_import_preview_button(has_paths, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_pick_files_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_files"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            cx.listener(|this, _event, _window, cx| {
                this.pick_connection_import_paths(false, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_pick_directory_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_directory"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            cx.listener(|this, _event, _window, cx| {
                this.pick_connection_import_paths(true, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_button(
        &self,
        has_paths: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.connections.importers.preview"),
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                16.0,
                rgb(if has_paths {
                    self.tokens.ui.bg
                } else {
                    self.tokens.ui.text_muted
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    radius: ButtonRadius::Md,
                    disabled: !has_paths,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.preview_settings_connection_import(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_secondary_button_options(
        &self,
        disabled: bool,
    ) -> ToolbarButtonOptions {
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Default,
                radius: ButtonRadius::Md,
                disabled,
            },
            background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
            border: Some(rgb(self.tokens.ui.border)),
            text_color: Some(rgb(self.tokens.ui.text)),
            hover_background: Some(rgb(self.tokens.ui.bg_hover)),
            height: Some(36.0),
            padding_x: Some(12.0),
            font_size: Some(self.tokens.metrics.ui_text_sm),
            ..ToolbarButtonOptions::default()
        }
    }

    pub(in crate::workspace) fn connection_import_path_summary(
        &self,
        paths: &[String],
    ) -> AnyElement {
        let summary = if paths.is_empty() {
            self.i18n.t("settings_view.connections.importers.no_paths")
        } else {
            paths.join(" · ")
        };
        div()
            .w_full()
            .min_w_0()
            .truncate()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(summary)
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_toolbar(
        &self,
        preview: &ConnectionImportPreview,
        selected_draft_ids: &HashSet<String>,
        duplicate_strategy: ConnectionImportDuplicateStrategy,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let importable = preview
            .drafts
            .iter()
            .filter(|draft| draft.importable)
            .count();
        let all_selected = importable > 0
            && preview
                .drafts
                .iter()
                .filter(|draft| draft.importable)
                .all(|draft| selected_draft_ids.contains(&draft.id));
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(self.connection_import_toggle_all_button(all_selected, importable, cx))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.connection_import_duplicate_strategy_picker(duplicate_strategy, cx))
                    .child(
                        self.settings_text_input_control(
                            SettingsInput::ConnectionImportTargetGroup,
                            String::new(),
                            self.i18n
                                .t("settings_view.connections.importers.target_group"),
                            192.0,
                            cx,
                        )
                        .into_any_element(),
                    )
                    .child(self.connection_import_apply_button(selected_draft_ids.len(), cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.importers.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.importers.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
            .child(label)
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_all_settings_connection_import_drafts(all_selected, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_duplicate_strategy_picker(
        &self,
        strategy: ConnectionImportDuplicateStrategy,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = connection_import_duplicate_strategy_label(strategy, &self.i18n);
        // Tauri renders duplicate strategy as a compact SelectTrigger (w-36 h-8)
        // in the import preview toolbar, not as adjacent action buttons.
        self.settings_select_control_with_trigger_style(
            SettingsSelect::ConnectionImportDuplicateStrategy,
            selected_label,
            false,
            Some(144.0),
            |trigger| {
                trigger
                    .h(px(32.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
            },
            cx,
        )
    }

    pub(in crate::workspace) fn connection_import_apply_button(
        &self,
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.importers.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::Upload,
                16.0,
                rgb(if selected_count == 0 {
                    self.tokens.ui.text_muted
                } else {
                    self.tokens.ui.bg
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: selected_count == 0,
                },
                height: Some(32.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.apply_settings_connection_import(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_list(
        &self,
        preview: ConnectionImportPreview,
        selected_draft_ids: &HashSet<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if preview.drafts.is_empty() {
            return div()
                .w_full()
                .h(px(288.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.connections.importers.no_drafts"))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-connection-import-scroll")
            .w_full()
            .h(px(288.0))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("settings-connection-import-scroll"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel));
        for draft in preview.drafts {
            let selected = selected_draft_ids.contains(&draft.id);
            list = list.child(self.connection_import_preview_row(draft, selected, cx));
        }
        list.into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_row(
        &self,
        draft: oxideterm_connections::ImportedConnectionDraft,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = !draft.importable;
        let detail = format!("{}@{}:{}", draft.username, draft.host, draft.port);
        let origin_detail = [
            draft.group.clone(),
            Some(connection_import_source_label(draft.source, &self.i18n)),
            Some(draft.source_path.clone()),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let warnings = draft
            .warnings
            .iter()
            .chain(draft.unsupported_fields.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let draft_id = draft.id.clone();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap(px(8.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
            .p(px(12.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .child(
                div()
                    .w(px(28.0))
                    .flex_none()
                    .child(self.ssh_config_checkbox(selected)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(draft.name),
                            )
                            .when(draft.duplicate, |row| {
                                row.child(self.connection_import_duplicate_badge())
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(detail),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(origin_detail),
                    )
                    .when(!warnings.is_empty(), |column| {
                        column.child(
                            div()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.warning))
                                .child(warnings),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(CONNECTION_IMPORT_AUTH_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(imported_auth_label(draft.auth_type, &self.i18n)),
            )
            .when(!disabled, |row| {
                row.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_settings_connection_import_draft(draft_id.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_duplicate_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.i18n.t("settings_view.connections.importers.duplicate"))
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_key_row(
        &self,
        key: oxideterm_connections::SshKeyInfo,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Key,
                                18.0,
                                rgb(theme.accent),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{} · {}", key.key_type, key.path)),
                            ),
                    ),
            )
            .when(key.has_passphrase, |row| {
                row.child(div().flex_none().child(self.text_badge(
                    self.i18n.t("settings_view.ssh_keys.encrypted"),
                    theme.warning,
                )))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_key_section_header(
        &self,
        title_key: &str,
        description_key: &str,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SSH_KEY_HEADER_TEXT_BASIS))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(description_key)),
                    ),
            )
            .when_some(actions, |header, actions| header.child(actions))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_key_toolbar(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(self.managed_key_action_button(
                LucideIcon::FileLock,
                "settings_view.ssh_keys.import_file",
                ButtonVariant::Outline,
                cx,
                |this, _event, _window, cx| {
                    this.open_managed_key_import_file_dialog(cx);
                    cx.stop_propagation();
                },
            ))
            .child(self.managed_key_action_button(
                LucideIcon::ShieldCheck,
                "settings_view.ssh_keys.paste_key",
                ButtonVariant::Outline,
                cx,
                |this, _event, _window, cx| {
                    this.open_managed_key_paste_dialog(cx);
                    cx.stop_propagation();
                },
            ))
            .child(self.managed_key_action_button(
                LucideIcon::RefreshCw,
                "settings_view.ssh_keys.refresh",
                ButtonVariant::Ghost,
                cx,
                |this, _event, _window, cx| {
                    this.settings_workspace.update(cx, |settings, cx| {
                        settings.clear_managed_key_status(cx);
                    });
                    cx.stop_propagation();
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_action_button(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        variant: ButtonVariant,
        cx: &mut Context<Self>,
        handler: impl Fn(
            &mut WorkspaceApp,
            &gpui::MouseDownEvent,
            &mut Window,
            &mut Context<WorkspaceApp>,
        ) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(28.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(handler),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_key_row(
        &self,
        key: ManagedSshKeyInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let usage = self
            .connection_store
            .managed_ssh_key_usage(&key.id)
            .map(|usage| usage.count)
            .unwrap_or(0);
        let detail = format!(
            "{} · {} · {}",
            self.managed_key_origin_label(&key.origin),
            if key.requires_passphrase {
                self.i18n.t("settings_view.ssh_keys.passphrase_required")
            } else {
                self.i18n
                    .t("settings_view.ssh_keys.passphrase_not_required")
            },
            self.i18n
                .t("settings_view.ssh_keys.used_by")
                .replace("{{count}}", &usage.to_string())
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ShieldCheck,
                                18.0,
                                rgb(theme.accent),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(key.fingerprint.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.workspace_icon_action_button(
                        LucideIcon::Pencil,
                        14.0,
                        rgb(theme.text),
                        IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md),
                        {
                            let key_id = key.id.clone();
                            let key_name = key.name.clone();
                            move |this, _event, _window, cx| {
                                this.open_managed_key_rename_dialog(
                                    key_id.clone(),
                                    key_name.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    ))
                    .child(self.workspace_icon_action_button(
                        LucideIcon::Trash2,
                        14.0,
                        rgb(theme.error),
                        IconButtonOptions {
                            hover_background: Some(rgba((theme.error << 8) | 0x14)),
                            ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
                        },
                        {
                            let key = key;
                            move |this, _event, _window, cx| {
                                this.open_managed_key_delete_dialog(key.clone(), cx);
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_managed_keys"))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_origin_label(
        &self,
        origin: &ManagedSshKeyOrigin,
    ) -> String {
        match origin {
            ManagedSshKeyOrigin::ImportedFile => {
                self.i18n.t("settings_view.ssh_keys.origin_imported_file")
            }
            ManagedSshKeyOrigin::PastedText => {
                self.i18n.t("settings_view.ssh_keys.origin_pasted_text")
            }
            ManagedSshKeyOrigin::OxideImport => {
                self.i18n.t("settings_view.ssh_keys.origin_oxide_import")
            }
        }
    }

    pub(in crate::workspace) fn render_settings_managed_key_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match self
            .settings_workspace
            .read(cx)
            .managed_key_dialog_snapshot()?
        {
            ManagedKeyDialogSnapshot::ImportFile {
                file_path,
                file_name,
                presence,
            } => Some(self.render_settings_managed_key_import_file_dialog(
                file_path, file_name, presence, cx,
            )),
            ManagedKeyDialogSnapshot::Paste {
                name,
                private_key_present,
                presence,
            } => Some(self.render_settings_managed_key_paste_dialog(
                name,
                private_key_present,
                presence,
                cx,
            )),
            ManagedKeyDialogSnapshot::Rename { name, presence } => {
                Some(self.render_settings_managed_key_rename_dialog(name, presence, cx))
            }
            ManagedKeyDialogSnapshot::Delete {
                key,
                usage,
                presence,
            } => Some(self.render_settings_managed_key_delete_dialog(key, usage, presence, cx)),
        }
    }

    pub(in crate::workspace) fn render_settings_managed_key_import_file_dialog(
        &self,
        file_path: String,
        file_name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_import = !file_path.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.import_file.title",
            "modals.managed_key.import_file.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.import_file.path",
                    SettingsInput::ManagedKeyFilePath,
                    file_path,
                    "~/.ssh/id_ed25519".to_string(),
                    420.0,
                    cx,
                ),
                div()
                    .flex()
                    .justify_start()
                    .child(self.managed_key_dialog_button(
                        self.i18n.t("modals.managed_key.import_file.browse_title"),
                        ButtonVariant::Outline,
                        false,
                        |this, _event, _window, cx| {
                            this.pick_managed_key_import_file(cx);
                        },
                        cx,
                    ))
                    .into_any_element(),
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyFileName,
                    file_name,
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyFilePassphrase,
                    String::new(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            |this, _event, _window, cx| {
                this.import_managed_key_from_file(cx);
            },
            presence.phase(),
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_paste_dialog(
        &self,
        name: String,
        private_key_present: bool,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_import = private_key_present;
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.paste.title",
            "modals.managed_key.paste.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyPasteName,
                    name,
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_private_key_textarea(cx),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyPastePassphrase,
                    String::new(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            |this, _event, _window, cx| {
                this.import_managed_key_from_paste(cx);
            },
            presence.phase(),
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_rename_dialog(
        &self,
        name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_save = !name.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.rename_title",
            "settings_view.ssh_keys.managed_description",
            vec![self.settings_managed_key_input_field(
                "settings_view.ssh_keys.rename_name",
                SettingsInput::ManagedKeyRenameName,
                name,
                "Managed SSH Key".to_string(),
                420.0,
                cx,
            )],
            self.i18n.t("settings_view.ssh_keys.rename"),
            can_save,
            |this, _event, _window, cx| {
                this.rename_managed_key(cx);
            },
            presence.phase(),
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_delete_dialog(
        &self,
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_delete = usage.count == 0;
        let mut description = if can_delete {
            self.i18n
                .t("settings_view.ssh_keys.delete_unused_description")
                .replace("{{name}}", &key.name)
        } else {
            self.i18n
                .t("settings_view.ssh_keys.delete_blocked_description")
                .replace("{{count}}", &usage.count.to_string())
        };
        if !usage.items.is_empty() {
            let used_by = usage
                .items
                .iter()
                .map(|item| format!("{} ({})", item.connection_name, item.location))
                .collect::<Vec<_>>()
                .join(", ");
            description.push_str("\n");
            description.push_str(&used_by);
        }
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.delete_title",
            "",
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(description)
                    .into_any_element(),
            ],
            self.i18n.t("settings_view.ssh_keys.delete"),
            can_delete,
            |this, _event, _window, cx| {
                this.delete_managed_key(cx);
            },
            presence.phase(),
            cx,
        )
    }

    pub(in crate::workspace) fn settings_managed_key_dialog_frame(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
        confirm_label: String,
        can_confirm: bool,
        confirm: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        phase: oxideterm_gpui_ui::motion::ExitPhase,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_managed_key_dialog(cx);
                cx.stop_propagation();
            }),
        );
        let form = dialog_content(&self.tokens)
            .w(px(520.0))
            .max_w(relative(0.92))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, self.i18n.t(title_key)))
                    .when(!description_key.is_empty(), |header| {
                        header.child(dialog_description(
                            &self.tokens,
                            self.i18n.t(description_key),
                        ))
                    }),
            )
            .child(
                div()
                    .px(px(24.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(rows),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        false,
                        |this, _event, _window, cx| {
                            this.close_managed_key_dialog(cx);
                        },
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        confirm_label,
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_confirm,
                        confirm,
                        cx,
                    )),
            );
        settings_dialog_transition(
            &self.tokens,
            "managed-key-dialog-form",
            backdrop,
            form,
            phase,
        )
    }

    pub(in crate::workspace) fn settings_managed_key_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_secret_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_secret_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_private_key_textarea(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::ManagedKeyPastePrivateKey;
        let settings = self.settings_workspace.read(cx);
        // Keep the private key in the Entity; the multiline renderer derives
        // only zeroizing line buffers for GPUI's owned visual text.
        let value = settings
            .settings_entity_input_value(input)
            .expect("managed private-key input is owned by the Settings Entity");
        let focused = settings.settings_entity_focused_input() == Some(input);
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(160.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        if value.is_empty() {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                "-----BEGIN OPENSSH PRIVATE KEY-----",
                true,
                line_height,
                cx,
            );
        } else {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                value,
                false,
                line_height,
                cx,
            );
        }
        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("modals.managed_key.paste.private_key")),
            )
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_hint(&self, label_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_dialog_button(
        &self,
        label: String,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(in crate::workspace) fn open_managed_key_import_file_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_managed_key_import_file_dialog(cx);
        });
    }

    pub(in crate::workspace) fn open_managed_key_paste_dialog(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_managed_key_paste_dialog(cx);
        });
    }

    pub(in crate::workspace) fn open_managed_key_rename_dialog(
        &mut self,
        key_id: String,
        key_name: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_managed_key_rename_dialog(key_id, key_name, cx);
        });
    }

    pub(in crate::workspace) fn open_managed_key_delete_dialog(
        &mut self,
        key: ManagedSshKeyInfo,
        cx: &mut Context<Self>,
    ) {
        match self.connection_store.managed_ssh_key_usage(&key.id) {
            Ok(usage) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.open_managed_key_delete_dialog(key, usage, cx);
                });
            }
            Err(error) => self.set_managed_key_action_error(error, cx),
        }
    }

    pub(in crate::workspace) fn close_managed_key_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_managed_key_dialog(delay, cx);
        });
    }

    pub(in crate::workspace) fn pick_managed_key_import_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(gpui::SharedString::from(
                self.i18n.t("modals.managed_key.import_file.browse_title"),
            )),
        });
        let selected_file = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            let Some(path) = paths.into_iter().next() else {
                return None;
            };
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Managed SSH Key")
                .to_string();
            Some((path.display().to_string(), file_name))
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_managed_key_file_picker(selected_file, cx);
        });
    }

    pub(in crate::workspace) fn import_managed_key_from_file(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.settings_workspace.update(cx, |settings, _cx| {
            settings.take_managed_key_file_import_request()
        }) else {
            return;
        };
        match self.connection_store.create_managed_ssh_key_from_file(
            &request.path,
            request.name,
            request.passphrase,
        ) {
            Ok(info) => {
                let status = self
                    .i18n
                    .t("settings_view.ssh_keys.import_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error, cx),
        }
    }

    pub(in crate::workspace) fn import_managed_key_from_paste(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.settings_workspace.update(cx, |settings, _cx| {
            settings.take_managed_key_paste_import_request()
        }) else {
            return;
        };
        // The Entity moves the private key into this one-shot request without a plaintext clone.
        match self.connection_store.create_managed_ssh_key_from_text(
            request.private_key,
            request.name,
            request.passphrase,
        ) {
            Ok(info) => {
                let status = self
                    .i18n
                    .t("settings_view.ssh_keys.import_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error, cx),
        }
    }

    pub(in crate::workspace) fn rename_managed_key(&mut self, cx: &mut Context<Self>) {
        let Some((key_id, name)) = self.settings_workspace.update(cx, |settings, _cx| {
            settings.take_managed_key_rename_request()
        }) else {
            return;
        };
        match self.connection_store.rename_managed_ssh_key(&key_id, name) {
            Ok(info) => {
                let status = self
                    .i18n
                    .t("settings_view.ssh_keys.rename_success")
                    .replace("{{name}}", &info.name);
                self.finish_managed_key_action(status, true, cx);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error, cx),
        }
    }

    pub(in crate::workspace) fn delete_managed_key(&mut self, cx: &mut Context<Self>) {
        let Some(key_id) = self.settings_workspace.read(cx).managed_key_delete_id() else {
            return;
        };
        match self.connection_store.delete_managed_ssh_key(&key_id, false) {
            Ok(result) => {
                let status = self
                    .i18n
                    .t("settings_view.ssh_keys.delete_success")
                    .replace("{{count}}", &result.deleted.to_string());
                self.finish_managed_key_action(status, true, cx);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error, cx),
        }
    }

    fn finish_managed_key_action(&mut self, status: String, success: bool, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.finish_managed_key_action(status, success, cx);
        });
    }

    pub(in crate::workspace) fn set_managed_key_action_error(
        &mut self,
        error: impl std::fmt::Display,
        cx: &mut Context<Self>,
    ) {
        let status = self
            .i18n
            .t("settings_view.ssh_keys.action_failed")
            .replace("{{error}}", &error.to_string());
        self.finish_managed_key_action(status, false, cx);
    }

    pub(in crate::workspace) fn ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_keys"))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_status_row(&self, status: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
            .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.info))
            .child(status)
            .into_any_element()
    }

    pub(in crate::workspace) fn toggle_settings_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.toggle_ssh_config_host(alias, cx);
        });
    }

    pub(in crate::workspace) fn toggle_all_settings_ssh_config_hosts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        if all_selected {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.clear_ssh_config_host_selection(cx);
            });
        } else {
            let existing_names = self
                .connection_store
                .connections()
                .iter()
                .map(|conn| conn.name.clone())
                .collect::<HashSet<_>>();
            if let Ok(hosts) = list_ssh_config_hosts(&existing_names) {
                let selected_hosts = hosts
                    .into_iter()
                    .filter(|host| !host.already_imported)
                    .map(|host| host.alias)
                    .collect();
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.set_selected_ssh_config_hosts(selected_hosts, cx);
                });
            }
        }
    }

    pub(in crate::workspace) fn import_settings_ssh_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        let (completed, status) = match oxideterm_connections::import_ssh_config_alias(
            &mut self.connection_store,
            &alias,
        ) {
            Ok(true) => {
                let status = self
                    .i18n
                    .t("settings_view.errors.import_success")
                    .replace("{{name}}", &alias);
                self.queue_cloud_sync_dirty_refresh(cx);
                (true, status)
            }
            Ok(false) => (
                false,
                self.i18n
                    .t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", "1"),
            ),
            Err(error) => (
                false,
                self.i18n
                    .t("settings_view.errors.import_failed")
                    .replace("{{error}}", &error.to_string()),
            ),
        };
        self.settings_workspace.update(cx, |settings, cx| {
            if completed {
                settings.ssh_config_selected_hosts.remove(&alias);
            }
            settings.connection_import_status = Some(status);
            cx.notify();
        });
    }

    pub(in crate::workspace) fn import_selected_settings_ssh_hosts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let aliases = self.settings_workspace.read(cx).selected_ssh_config_hosts();
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        let mut completed_aliases = Vec::new();

        for alias in aliases {
            match oxideterm_connections::import_ssh_config_alias(&mut self.connection_store, &alias)
            {
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
                self.i18n
                    .t("settings_view.connections.ssh_config.batch_import_success")
                    .replace("{{count}}", &imported.to_string()),
            );
        }
        if skipped > 0 {
            parts.push(
                self.i18n
                    .t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", &skipped.to_string()),
            );
        }
        if !errors.is_empty() {
            parts.push(errors.join(", "));
        }
        let status = (!parts.is_empty()).then(|| parts.join("; "));
        self.settings_workspace.update(cx, |settings, cx| {
            for alias in &completed_aliases {
                settings.ssh_config_selected_hosts.remove(alias);
            }
            settings.connection_import_status = status;
            cx.notify();
        });
        if imported > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(in crate::workspace) fn set_connection_import_source(
        &mut self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_connection_import_source(source, cx);
        });
    }

    pub(in crate::workspace) fn pick_connection_import_paths(
        &mut self,
        directories: bool,
        cx: &mut Context<Self>,
    ) {
        let source = self.settings_workspace.read(cx).connection_import_source();
        let multiple = !directories && source != ConnectionImportSource::Termius;
        let prompt_key = if directories {
            "settings_view.connections.importers.choose_directory"
        } else {
            "settings_view.connections.importers.choose_files"
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple,
            prompt: Some(SharedString::from(self.i18n.t(prompt_key))),
        });
        let selected_paths = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            let selected = paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            (!selected.is_empty()).then_some(selected)
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_connection_import_path_picker(selected_paths, cx);
        });
    }

    pub(in crate::workspace) fn preview_settings_connection_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some((source, paths)) = self
            .settings_workspace
            .read(cx)
            .connection_import_preview_request()
        else {
            return;
        };
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        match preview_connection_import(source, &paths, &existing_names) {
            Ok(preview) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.apply_connection_import_preview(Ok(preview), cx);
                });
            }
            Err(error) => {
                let status = self
                    .i18n
                    .t("settings_view.connections.importers.preview_failed")
                    .replace("{{error}}", &error.to_string());
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.apply_connection_import_preview(Err(status), cx);
                });
            }
        }
    }

    pub(in crate::workspace) fn toggle_settings_connection_import_draft(
        &mut self,
        draft_id: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.toggle_connection_import_draft(draft_id, cx);
        });
    }

    pub(in crate::workspace) fn toggle_all_settings_connection_import_drafts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.toggle_all_connection_import_drafts(all_selected, cx);
        });
    }

    pub(in crate::workspace) fn apply_settings_connection_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .settings_workspace
            .read(cx)
            .connection_import_apply_request()
        else {
            return;
        };
        match apply_connection_import(&mut self.connection_store, request) {
            Ok(result) => {
                let mut parts = Vec::new();
                if result.imported > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.imported_count")
                            .replace("{{count}}", &result.imported.to_string()),
                    );
                }
                if result.skipped > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.skipped_count")
                            .replace("{{count}}", &result.skipped.to_string()),
                    );
                }
                if result.renamed > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.renamed_count")
                            .replace("{{count}}", &result.renamed.to_string()),
                    );
                }
                if !result.errors.is_empty() {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.error_count")
                            .replace("{{count}}", &result.errors.len().to_string()),
                    );
                }
                let status = if parts.is_empty() {
                    self.i18n
                        .t("settings_view.connections.importers.no_changes")
                } else {
                    parts.join(" · ")
                };
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.set_connection_import_status(Some(status), cx);
                });
                if result.imported > 0 {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
                self.preview_settings_connection_import(cx);
            }
            Err(error) => {
                let status = self
                    .i18n
                    .t("settings_view.connections.importers.apply_failed")
                    .replace("{{error}}", &error.to_string());
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.set_connection_import_status(Some(status), cx);
                });
            }
        }
    }
}

pub(in crate::workspace) fn connection_idle_timeout_options(i18n: &I18n) -> Vec<(i64, String)> {
    vec![
        (300, i18n.t("settings_view.connections.idle_timeout.5min")),
        (900, i18n.t("settings_view.connections.idle_timeout.15min")),
        (1800, i18n.t("settings_view.connections.idle_timeout.30min")),
        (3600, i18n.t("settings_view.connections.idle_timeout.1hr")),
        (0, i18n.t("settings_view.connections.idle_timeout.never")),
    ]
}

pub(in crate::workspace) fn connection_idle_timeout_label(seconds: i64, i18n: &I18n) -> String {
    connection_idle_timeout_options(i18n)
        .into_iter()
        .find_map(|(value, label)| (value == seconds).then_some(label))
        .unwrap_or_else(|| seconds.to_string())
}

pub(in crate::workspace) fn connection_import_source_options() -> &'static [ConnectionImportSource]
{
    &[
        ConnectionImportSource::SecureCrt,
        ConnectionImportSource::Xshell,
        ConnectionImportSource::Termius,
        ConnectionImportSource::MobaXterm,
        ConnectionImportSource::WindTerm,
        ConnectionImportSource::Electerm,
        ConnectionImportSource::FinalShell,
    ]
}

pub(in crate::workspace) fn connection_import_source_label(
    source: ConnectionImportSource,
    i18n: &I18n,
) -> String {
    match source {
        ConnectionImportSource::SecureCrt => {
            i18n.t("settings_view.connections.importers.sources.securecrt")
        }
        ConnectionImportSource::Xshell => {
            i18n.t("settings_view.connections.importers.sources.xshell")
        }
        ConnectionImportSource::Termius => {
            i18n.t("settings_view.connections.importers.sources.termius")
        }
        ConnectionImportSource::MobaXterm => {
            i18n.t("settings_view.connections.importers.sources.mobaxterm")
        }
        ConnectionImportSource::WindTerm => {
            i18n.t("settings_view.connections.importers.sources.windterm")
        }
        ConnectionImportSource::Electerm => {
            i18n.t("settings_view.connections.importers.sources.electerm")
        }
        ConnectionImportSource::FinalShell => {
            i18n.t("settings_view.connections.importers.sources.finalshell")
        }
    }
}

fn connection_import_supports_files(source: ConnectionImportSource) -> bool {
    source != ConnectionImportSource::FinalShell
}

fn connection_import_supports_directory(source: ConnectionImportSource) -> bool {
    matches!(
        source,
        ConnectionImportSource::SecureCrt
            | ConnectionImportSource::Xshell
            | ConnectionImportSource::FinalShell
    )
}

pub(in crate::workspace) fn connection_import_duplicate_strategy_label(
    strategy: ConnectionImportDuplicateStrategy,
    i18n: &I18n,
) -> String {
    match strategy {
        ConnectionImportDuplicateStrategy::Skip => {
            i18n.t("settings_view.connections.importers.duplicate_skip")
        }
        ConnectionImportDuplicateStrategy::Rename => {
            i18n.t("settings_view.connections.importers.duplicate_rename")
        }
    }
}

pub(in crate::workspace) fn imported_auth_label(
    auth_type: ImportedConnectionAuthType,
    _i18n: &I18n,
) -> String {
    match auth_type {
        ImportedConnectionAuthType::Password => "password",
        ImportedConnectionAuthType::Key => "key",
        ImportedConnectionAuthType::Certificate => "certificate",
        ImportedConnectionAuthType::Agent => "agent",
    }
    .to_string()
}

pub(in crate::workspace) fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod settings_connection_entity_tests {
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
    fn ssh_config_dialog_owns_selection_status_and_exit_lifecycle(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings
                .ssh_config_selected_hosts
                .insert("stale-host".into());
            settings.connection_import_status = Some("stale-status".into());

            settings.open_ssh_config_import_dialog(cx);

            assert!(settings.ssh_config_import_dialog_open);
            assert!(settings.ssh_config_selected_hosts.is_empty());
            assert!(settings.connection_import_status.is_none());

            settings.toggle_ssh_config_host("host-a".into(), cx);
            assert!(settings.ssh_config_selected_hosts.contains("host-a"));

            settings.close_ssh_config_import_dialog(std::time::Duration::ZERO, cx);
            assert!(!settings.ssh_config_import_dialog_open);
            assert!(settings.ssh_config_import_dialog_exit_task.is_none());
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
    }
}
