use super::*;
use oxideterm_gpui_settings_view::components::SettingsWindowRequest;
use oxideterm_gpui_settings_view::connections_page::{
    ConnectionDialog, ConnectionPage, ConnectionRequest,
};

pub(in crate::workspace) use oxideterm_gpui_settings_view::connections_page::{
    connection_import_duplicate_strategy_label, connection_import_source_label,
    connection_import_source_options,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_settings_managed_key_dialog(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .managed_key_dialog_open()
            .then(|| self.managed_key_dialog_view.clone().into_any_element())
    }

    pub(in crate::workspace) fn render_settings_ssh_config_import_dialog(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .ssh_config_import_dialog_open()
            .then(|| self.ssh_import_dialog_view.clone().into_any_element())
    }

    pub(in crate::workspace) fn mount_connection_dialog(
        state: &Entity<SettingsWorkspaceEntity>,
        managed_key: bool,
        cx: &mut Context<Self>,
    ) -> Entity<ConnectionDialog> {
        let owner = cx.entity().downgrade();
        let body = Rc::new(move |cx: &App| {
            let owner = owner.upgrade()?;
            owner.read(cx).with_connection_page(owner, cx, |page, cx| {
                if managed_key {
                    page.render_settings_managed_key_dialog(cx)
                } else {
                    page.render_settings_ssh_config_import_dialog(cx)
                }
            })
        });
        cx.new(|cx| ConnectionDialog::new(state, managed_key, body, cx))
    }

    fn with_connection_page<R>(
        &self,
        owner: Entity<Self>,
        cx: &App,
        render: impl FnOnce(ConnectionPage<'_>, &App) -> R,
    ) -> R {
        let weak = owner.downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = weak.update(cx, |app, cx| {
                app.handle_connection_page_request(request, cx)
            });
        });
        let scroll = |key: &str| self.selectable_text_scroll_handle(key);
        let private_key =
            |cx: &App| self.settings_managed_key_private_key_textarea(owner.clone(), cx);
        self.with_settings_controls_for(owner.clone(), cx, |controls, cx| {
            render(
                ConnectionPage {
                    controls,
                    state: &self.settings_workspace,
                    store: &self.connection_store,
                    dispatch,
                    scroll: &scroll,
                    private_key: &private_key,
                    footer_focus: self.standard_confirm_focus(),
                },
                cx,
            )
        })
    }

    fn handle_connection_page_request(
        &mut self,
        request: ConnectionRequest,
        cx: &mut Context<Self>,
    ) {
        match request {
            ConnectionRequest::LoadHosts(enabled) => {
                self.edit_settings(
                    |settings| set_ssh_config_auto_load_hosts(settings, enabled),
                    cx,
                );
                self.refresh_session_manager_ssh_config_hosts(cx);
                if self.command_palette.read(cx).is_open() {
                    self.load_command_palette_ssh_config_hosts(cx);
                }
            }
            ConnectionRequest::SyncHosts(enabled) => {
                self.edit_settings(
                    |settings| set_ssh_config_auto_sync_hosts(settings, enabled),
                    cx,
                );
                self.sync_ssh_config_sync_service();
            }
            ConnectionRequest::ProxyCommand(enabled) => {
                self.edit_settings(
                    |settings| set_ssh_config_allow_proxy_command(settings, enabled),
                    cx,
                );
            }
            ConnectionRequest::OpenSsh => self.open_settings_ssh_config_import_dialog(cx),
            ConnectionRequest::CloseSsh => self.close_settings_ssh_config_import_dialog(cx),
            ConnectionRequest::ImportSelectedSsh => self.import_selected_settings_ssh_hosts(cx),
            ConnectionRequest::ImportSsh(alias) => self.import_settings_ssh_host(alias, cx),
            ConnectionRequest::PickImport(directory) => {
                self.pick_connection_import_paths(directory, cx)
            }
            ConnectionRequest::PreviewImport => self.preview_settings_connection_import(cx),
            ConnectionRequest::ApplyImport => self.apply_settings_connection_import(cx),
            ConnectionRequest::OpenKeyFile => self.open_managed_key_import_file_dialog(cx),
            ConnectionRequest::OpenKeyPaste => self.open_managed_key_paste_dialog(cx),
            ConnectionRequest::OpenKeyRename(id, name) => {
                self.open_managed_key_rename_dialog(id, name, cx)
            }
            ConnectionRequest::OpenKeyDelete(key) => self.open_managed_key_delete_dialog(key, cx),
            ConnectionRequest::PickKey => self.pick_managed_key_import_file(cx),
            ConnectionRequest::ImportKeyFile => self.import_managed_key_from_file(cx),
            ConnectionRequest::ImportKeyPaste => self.import_managed_key_from_paste(cx),
            ConnectionRequest::RenameKey => self.rename_managed_key(cx),
            ConnectionRequest::DeleteKey => self.delete_managed_key(cx),
            ConnectionRequest::CloseKey => self.close_managed_key_dialog(cx),
            ConnectionRequest::ClearFooterFocus => self.clear_standard_confirm_focus(),
        }
    }

    pub(in crate::workspace) fn settings_connections_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section {
            SettingsSectionId::ConnectionsReconnect => self.settings_reconnect_section(0, cx),
            SettingsSectionId::ConnectionsKnownHosts => self.settings_known_hosts_section(cx),
            _ => self.render_connections_page_section(section, cx.entity(), cx),
        }
    }

    pub(super) fn render_connections_page_section(
        &self,
        section: SettingsSectionId,
        owner: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        self.with_connection_page(owner, cx, |page, cx| match section {
            SettingsSectionId::ConnectionsDefaults => page.settings_card(
                "settings_view.connections.title",
                "settings_view.connections.description",
                vec![page.connection_defaults_section(page.settings, cx)],
            ),
            SettingsSectionId::ConnectionsIdleTimeout => page.settings_card(
                "settings_view.connections.idle_timeout.title",
                "settings_view.connections.idle_timeout.description",
                vec![page.connection_idle_timeout_control(page.settings, cx)],
            ),
            SettingsSectionId::ConnectionsSshKeys => page.settings_ssh_section(0, cx),
            SettingsSectionId::ConnectionsSshConfigImport => page.ssh_config_import_section(cx),
            SettingsSectionId::ConnectionsImporters => page.connection_importers_section(cx),
            _ => div().into_any_element(),
        })
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

    pub(in crate::workspace) fn settings_managed_key_private_key_textarea(
        &self,
        workspace: Entity<Self>,
        cx: &App,
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
            .text_color(rgb(theme.text));

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
        let dispatch = Self::settings_window_dispatch_for(workspace.downgrade());
        let select = dispatch.clone();
        let textarea = textarea
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                select(
                    SettingsWindowRequest::BeginInputSelection(input, event.clone()),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                dispatch(
                    SettingsWindowRequest::DragInputSelection(event.clone()),
                    window,
                    cx,
                )
            });
        let anchors = self.text_input_anchors.clone();
        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _, _| {
            anchors.update(anchor)
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
            .child(self.settings_multiline_input_viewport(target, control.into_any_element()))
            .into_any_element()
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

#[cfg(test)]
mod settings_connection_entity_tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn managed_key_long_paste_keeps_import_visible(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_KEY_LAYOUT_TEST_DIR";
        if std::env::var_os(fixture_key).is_none() {
            // Isolate settings and key storage from the maintainer's profile.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .args([cx.test_function_name().unwrap(), "--nocapture"])
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let mut store = SettingsStore::load_default().unwrap();
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().onboarding_disclaimer_accepted = true;
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.save().unwrap();
        crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(&default_settings_path())
            .unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            crate::workspace::window_shell::WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        let input = SettingsInput::ManagedKeyPastePrivateKey;
        let text = format!("{}\n", "synthetic-text-".repeat(12)).repeat(50);
        workspace.update_in(cx, |this, window, cx| {
            this.open_settings(window, cx);
            this.open_managed_key_paste_dialog(cx);
            this.settings_workspace.update(cx, |settings, cx| {
                settings.focus_settings_entity_input(input, cx);
            });
            window.focus(&this.focus_handle, cx);
        });
        cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone())));
        workspace.update(cx, |this, cx| {
            assert!(this.paste_active_text_input(cx));
            assert_eq!(
                this.settings_workspace
                    .read(cx)
                    .settings_entity_input_value(input),
                Some(text.as_str())
            );
        });
        cx.run_until_parked();
        for height in [600.0, 420.0] {
            cx.simulate_resize(gpui::size(px(800.0), px(height)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let button = cx.debug_bounds("managed-key-confirm").unwrap();
            assert!(
                button.top() >= px(0.0) && button.bottom() <= px(height),
                "{button:?}"
            );
            let anchor = workspace.read_with(cx, |this, _| {
                this.text_input_anchors
                    .get(WorkspaceImeTarget::Settings(input).anchor_id())
                    .unwrap()
                    .bounds
            });
            // A physical line has exactly one line box even when its text is wide.
            assert!(anchor.size.height >= px(50.0 * input.textarea_line_height()));
            let scroll = workspace.read_with(cx, |this, _| {
                this.selectable_text_scroll_handle(format!(
                    "multiline-input-{}",
                    WorkspaceImeTarget::Settings(input).anchor_id().0
                ))
            });
            assert!(
                scroll.max_offset().x > px(0.0),
                "long lines must be horizontally reachable"
            );
            assert!(
                scroll.max_offset().y > px(0.0),
                "later lines must be vertically reachable"
            );
            scroll.set_offset(point(px(-80.0), px(-100.0)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let scrolled = workspace.read_with(cx, |this, _| {
                this.text_input_anchors
                    .get(WorkspaceImeTarget::Settings(input).anchor_id())
                    .unwrap()
                    .bounds
            });
            assert_eq!(scrolled.origin, anchor.origin + scroll.offset());
            scroll.set_offset(point(px(0.0), px(0.0)));
        }
        let button = cx.debug_bounds("managed-key-confirm").unwrap();
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        workspace.read_with(cx, |this, cx| {
            // Clicking consumed the draft through the actual import handler.
            assert_eq!(
                this.settings_workspace
                    .read(cx)
                    .settings_entity_input_value(input),
                Some("")
            );
        });
    }
}
