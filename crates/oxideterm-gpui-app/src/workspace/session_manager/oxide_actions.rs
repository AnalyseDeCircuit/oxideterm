use super::*;

impl WorkspaceApp {
    fn oxide_transfer(&self) -> OxideTransfer<'_> {
        OxideTransfer {
            manager: &self.connections.session_manager,
            store: &self.connection_store,
            i18n: &self.i18n,
        }
    }
    pub(in crate::workspace) fn open_oxide_import_portable_migration_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.connections
            .session_manager
            .update(cx, |manager, cx| manager.show_oxide_import_dialog(true, cx));
    }

    pub(in crate::workspace) fn active_session_manager_input(
        &self,
        cx: &App,
    ) -> Option<SessionManagerInput> {
        let input = self.connections.session_manager.read(cx).focused_input?;
        let session_manager_tab_active = self
            .keyboard_content_tab(cx)
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::SessionManager);
        // The sidebar form has no tab to own keys, so the surfaces the root
        // mounts there gate their fields directly: the search disclosure
        // mirrors the active-sessions search gating, and the group manager
        // dialog is a modal whose draft field must stay active independently
        // of the tab underneath, like the Oxide import/export dialogs.
        let sidebar_search_active = input == SessionManagerInput::Search
            && self.session_manager_search_open
            && !self.session_manager_sidebar_sort_open
            && self.session_manager_sidebar_visible();
        let session_manager = self.connections.session_manager.read(cx);
        let group_dialog_active = input == SessionManagerInput::GroupName
            && session_manager.show_group_manager
            && self.session_manager_sidebar_visible();
        (sidebar_search_active
            || group_dialog_active
            || session_manager_input_is_active(
                input,
                session_manager_tab_active,
                session_manager.oxide_import_dialog.as_ref(),
                session_manager.oxide_export_dialog.as_ref(),
            ))
        .then_some(input)
    }

    pub(in crate::workspace) fn focused_oxide_dialog_input(
        &self,
        cx: &App,
    ) -> Option<SessionManagerInput> {
        let session_manager = self.connections.session_manager.read(cx);
        let input = session_manager.focused_input?;
        session_manager_input_is_active(
            input,
            false,
            session_manager.oxide_import_dialog.as_ref(),
            session_manager.oxide_export_dialog.as_ref(),
        )
        .then_some(input)
    }

    pub(in crate::workspace) fn open_oxide_export_portable_migration_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.open_oxide_export_dialog_with_portable_mode(true, cx);
    }

    pub(super) fn open_oxide_export_dialog_with_portable_mode(
        &mut self,
        portable_migration: bool,
        cx: &mut Context<Self>,
    ) {
        let plugin_settings =
            oxideterm_cloud_sync::plugin_settings::load_plugin_settings(self.settings_store.path())
                .unwrap_or_default();
        let mut dialog = oxide_export_draft(
            &self.connection_store,
            self.forwarding_service.registry().list_all_saved_forwards(),
            plugin_settings
                .iter()
                .filter_map(|setting| plugin_id_from_setting_storage_key(&setting.storage_key)),
            self.settings_store.path(),
            portable_migration,
        );
        dialog.preflight = self.oxide_export_preflight_for_dialog(&dialog, cx);
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.show_oxide_export_dialog(dialog, cx)
        });
    }

    pub(in crate::workspace) fn handle_oxide_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.dispatch_oxide_key(event, OxideKeyScope::Active, cx)
    }

    pub(in crate::workspace) fn handle_oxide_import_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.dispatch_oxide_key(event, OxideKeyScope::Import, cx)
    }

    pub(in crate::workspace) fn handle_oxide_export_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.dispatch_oxide_key(event, OxideKeyScope::Export, cx)
    }

    fn dispatch_oxide_key(
        &mut self,
        event: &KeyDownEvent,
        scope: OxideKeyScope,
        cx: &mut Context<Self>,
    ) -> bool {
        let (handled, request) = self
            .connections
            .session_manager
            .update(cx, |state, cx| state.handle_oxide_key(event, scope, cx));
        if let Some(request) = request {
            self.dispatch_connections_request(request, None, cx);
        }
        handled
    }

    pub(in crate::workspace) fn select_oxide_import_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.i18n.t("modals.import.select_file"))),
        });
        let selection = async move {
            match receiver.await {
                Ok(Ok(Some(paths))) => Some(
                    paths
                        .into_iter()
                        .next()
                        .ok_or_else(|| "未选择文件".to_string())
                        .and_then(|path| {
                            fs::read(&path)
                                .map(|bytes| (path, Arc::<[u8]>::from(bytes)))
                                .map_err(|error| error.to_string())
                        }),
                ),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => Some(Err(error.to_string())),
                Err(error) => Some(Err(error.to_string())),
            }
        };
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                session_manager.start_import_file_picker(selection, cx);
            });
    }

    pub(in crate::workspace) fn preview_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        self.oxide_transfer().preview_oxide_import_dialog(cx);
    }

    pub(in crate::workspace) fn apply_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        self.oxide_transfer().apply_oxide_import_dialog(cx);
    }

    pub(in crate::workspace) fn handle_session_manager_workspace_event(
        &mut self,
        event: &SessionManagerWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SessionManagerWorkspaceEvent::OxideEffectsReady(effects) => {
                for effect in effects.take() {
                    self.handle_oxide_workspace_effect(effect, cx);
                }
            }
            SessionManagerWorkspaceEvent::RefreshOxideExportPreflight => {
                self.refresh_oxide_export_preflight(cx);
            }
            SessionManagerWorkspaceEvent::ModalBoundaryChanged => cx.notify(),
        }
    }

    fn handle_oxide_workspace_effect(
        &mut self,
        effect: OxideWorkspaceEffect,
        cx: &mut Context<Self>,
    ) {
        match effect {
            OxideWorkspaceEffect::ApplyImport {
                generation,
                core,
                options,
                mut audit,
                child_context,
            } => {
                use oxideterm_audit::{AuditContext, AuditEvidence, AuditOutcome};
                let still_current = self
                    .connections
                    .session_manager
                    .read(cx)
                    .oxide_import_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.operation_generation == generation);
                if !still_current {
                    audit.finish(AuditOutcome::Partial, AuditEvidence::Protocol, None, None);
                    return;
                }
                let (applied, errors) =
                    AuditContext::with_sync_request(child_context.as_ref(), || {
                        self.finish_oxide_import_core_result(core, options, cx)
                    });
                audit.summary(&format!("mode=oxide,applied={applied},errors={errors}"));
                let outcome = if errors > 0 {
                    AuditOutcome::Partial
                } else if applied == 0 {
                    AuditOutcome::Unchanged
                } else {
                    AuditOutcome::Succeeded
                };
                audit.finish(outcome, AuditEvidence::Protocol, None, None);
            }
            OxideWorkspaceEffect::SaveExport {
                generation,
                bytes,
                exported_count,
            } => {
                if self
                    .connections
                    .session_manager
                    .read(cx)
                    .oxide_export_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.operation_generation == generation)
                {
                    self.prompt_save_oxide_export(bytes, exported_count, cx);
                }
            }
        }
    }

    pub(super) fn finish_oxide_import_core_result(
        &mut self,
        core: OxideCoreImportResult,
        options: OxideClientStateImportOptions,
        cx: &mut Context<Self>,
    ) -> (usize, usize) {
        self.connection_store = core.store;
        let mut envelope = core.envelope;

        let imported_forwards = self.apply_oxide_import_forward_records(&mut envelope);
        envelope.imported_forwards = imported_forwards;

        let (imported_quick_commands, skipped_quick_commands, quick_commands_errors) = self
            .apply_oxide_import_quick_commands(
                envelope.quick_commands_json.as_deref(),
                options.import_quick_commands,
                options.quick_command_strategy,
                cx,
            );

        let (imported_plugin_settings, plugin_settings_failed) = match self
            .try_apply_oxide_import_plugin_settings(
                &envelope.plugin_settings,
                options.import_plugin_settings,
                options.selected_plugin_ids.as_ref(),
            ) {
            Ok(count) => (count, false),
            Err(_) => (0, true),
        };
        let skipped_plugin_settings =
            !options.import_plugin_settings && !envelope.plugin_settings.is_empty();

        let (imported_app_settings, skipped_app_settings) = self.apply_oxide_import_app_settings(
            envelope.app_settings_json.as_deref(),
            options.import_app_settings,
            options.selected_app_settings_sections.as_ref(),
            cx,
        );

        self.apply_oxide_import_portable_secrets(&mut envelope, cx);
        self.queue_cloud_sync_dirty_refresh(cx);

        let result = OxideClientStateImportResult {
            envelope,
            imported_app_settings,
            skipped_app_settings,
            imported_quick_commands,
            skipped_quick_commands,
            quick_commands_errors,
            imported_plugin_settings,
            skipped_plugin_settings,
        };
        let applied = result.envelope.imported
            + result.envelope.merged
            + result.envelope.replaced
            + result.envelope.renamed
            + result.envelope.imported_forwards
            + result.envelope.imported_serial_profiles
            + result.envelope.imported_telnet_profiles
            + result.envelope.imported_mosh_profiles
            + result.envelope.imported_standalone_sftp_profiles
            + result.envelope.imported_remote_desktop_profiles
            + result.envelope.restored_managed_keys
            + result.envelope.restored_profile_credentials
            + result.envelope.cleared_profile_credentials
            + result.imported_quick_commands
            + result.imported_plugin_settings
            + usize::from(result.imported_app_settings)
            + result.envelope.imported_portable_secrets;
        let errors = result.envelope.errors.len()
            + result.quick_commands_errors.len()
            + usize::from(plugin_settings_failed)
            + usize::from(options.import_app_settings && result.skipped_app_settings);
        self.present_oxide_import_result(result, cx);
        (applied, errors)
    }

    pub(super) fn present_oxide_import_result(
        &mut self,
        result: OxideClientStateImportResult,
        cx: &mut Context<Self>,
    ) {
        self.oxide_transfer()
            .present_oxide_import_result(result, cx);
    }

    pub(in crate::workspace) fn oxide_export_portable_secret_count(
        &self,
        dialog: &OxideExportDialogState,
        cx: &App,
    ) -> usize {
        if !dialog.include_portable_secrets {
            return 0;
        }
        oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
            .into_iter()
            .filter(|provider| {
                self.ai.state
                    .read(cx)
                    .key_store()
                    .has_provider_key(&provider.id)
            })
            .count()
    }

    pub(in crate::workspace) fn refresh_oxide_export_preflight(&mut self, cx: &mut Context<Self>) {
        let Some(preflight) = ({
            let session_manager = self.connections.session_manager.read(cx);
            session_manager
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| self.oxide_export_preflight_for_dialog(dialog, cx))
        }) else {
            return;
        };
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                    dialog.preflight = preflight;
                    cx.notify();
                }
            });
    }

    pub(super) fn oxide_export_preflight_for_dialog(
        &self,
        dialog: &OxideExportDialogState,
        cx: &App,
    ) -> Option<ExportPreflightResult> {
        let has_preflight_content =
            !oxide_export_target_ids(dialog).is_empty() || dialog.include_portable_secrets;
        has_preflight_content.then(|| {
            oxide_export_preflight(
                &self.connection_store,
                dialog,
                self.oxide_export_portable_secret_count(dialog, cx),
            )
        })
    }

    pub(in crate::workspace) fn export_oxide_dialog(&mut self, cx: &mut Context<Self>) {
        let validation = {
            let session_manager = self.connections.session_manager.read(cx);
            let Some(dialog) = session_manager.oxide_export_dialog.as_ref() else {
                return;
            };
            if let Some(error_key) = oxide_export_validation_error(dialog) {
                Err(self.i18n.t(error_key))
            } else {
                let selected_ids = oxide_export_target_ids(dialog)
                    .into_iter()
                    .collect::<Vec<_>>();
                let preflight = oxide_export_preflight(
                    &self.connection_store,
                    dialog,
                    self.oxide_export_portable_secret_count(dialog, cx),
                );
                self.build_oxide_export_options(dialog, cx)
                    .map(|options| (selected_ids, preflight, options))
            }
        };
        let (selected_ids, preflight, options) = match validation {
            Ok(request) => request,
            Err(error) => {
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                            dialog.error = Some(error);
                            cx.notify();
                        }
                    });
                return;
            }
        };
        self.oxide_transfer()
            .start_export(selected_ids, preflight, options, cx);
    }

    pub(super) fn prompt_save_oxide_export(
        &mut self,
        bytes: Vec<u8>,
        exported_count: usize,
        cx: &mut Context<Self>,
    ) {
        let mut audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Configuration,
            "configuration_export",
            None,
            Some("oxide_file"),
        );
        audit.summary(&format!("connections={exported_count}"));
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!(
            "oxideterm-export-{}.oxide",
            Utc::now().format("%Y%m%d-%H%M%S")
        );
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        let save = async move {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => Some(
                    fs::write(&path, bytes)
                        .map(|_| path)
                        .map_err(|error| error.to_string()),
                ),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => Some(Err(error.to_string())),
                Err(error) => Some(Err(error.to_string())),
            };
            audit.finish(
                match &result {
                    Some(Ok(_)) => oxideterm_audit::AuditOutcome::Succeeded,
                    Some(Err(_)) => oxideterm_audit::AuditOutcome::Failed,
                    None => oxideterm_audit::AuditOutcome::Cancelled,
                },
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            result
        };
        let settings_path = self.settings_store.path().to_path_buf();
        let success_template = self.i18n.t("export.success");
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                session_manager.start_export_file_picker(
                    save,
                    settings_path,
                    success_template,
                    exported_count,
                    cx,
                );
            });
    }

    pub(super) fn build_oxide_export_options(
        &self,
        dialog: &OxideExportDialogState,
        cx: &App,
    ) -> Result<OxideExportOptions, String> {
        let plugin_settings = if dialog.include_plugin_settings {
            oxideterm_cloud_sync::plugin_settings::load_plugin_settings(self.settings_store.path())?
                .into_iter()
                .filter(|setting| {
                    plugin_id_from_setting_storage_key(&setting.storage_key)
                        .is_some_and(|plugin_id| dialog.selected_plugin_ids.contains(&plugin_id))
                })
                .collect()
        } else {
            Vec::new()
        };
        let portable_secrets = if dialog.include_portable_secrets {
            let provider_ids =
                oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
                    .into_iter()
                    .map(|provider| provider.id)
                    .filter(|provider_id| {
                        self.ai.state
                            .read(cx)
                            .key_store()
                            .has_provider_key(provider_id)
                    })
                    .collect::<Vec<_>>();
            self.ai.state
                .read(cx)
                .key_store()
                .get_provider_keys(&provider_ids)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(
                    |(id, secret)| oxideterm_connections::oxide_file::EncryptedPortableSecret {
                        kind: "ai_provider_key".to_string(),
                        id,
                        secret,
                    },
                )
                .collect()
        } else {
            Vec::new()
        };
        oxide_export_options(
            dialog,
            &self.connection_store,
            self.settings_store.settings(),
            self.settings_store.path(),
            plugin_settings,
            portable_secrets,
        )
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn apply_oxide_import_forward_records(
        &mut self,
        envelope: &mut ImportResultEnvelope,
    ) -> usize {
        if envelope.forward_records.is_empty() {
            return 0;
        }

        let records = envelope
            .forward_records
            .iter()
            .map(owned_forward_import_record)
            .collect::<Vec<_>>();
        let replace_owner_ids = envelope
            .forward_replace_owner_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let merge_owner_ids = envelope
            .forward_merge_owner_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        match self
            .forwarding_service
            .registry()
            .apply_owned_forward_import_records(&records, &replace_owner_ids, &merge_owner_ids)
        {
            Ok(count) => count,
            Err(error) => {
                envelope
                    .errors
                    .push(format!("Failed to save imported forwards: {error}"));
                0
            }
        }
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn apply_oxide_import_quick_commands(
        &mut self,
        quick_commands_json: Option<&str>,
        should_import: bool,
        strategy: QuickCommandImportStrategy,
        cx: &mut Context<Self>,
    ) -> (usize, bool, Vec<String>) {
        let Some(snapshot) = quick_commands_json else {
            return (0, false, Vec::new());
        };
        if !should_import {
            return (0, true, Vec::new());
        }

        let result = self.terminal.update(cx, |terminal, _cx| {
            terminal
                .quick_commands
                .store
                .apply_snapshot_json(snapshot, strategy)
        });
        (result.imported, !result.errors.is_empty(), result.errors)
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn apply_oxide_import_plugin_settings(
        &mut self,
        plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
        should_import: bool,
        selected_plugin_ids: Option<&HashSet<String>>,
    ) -> usize {
        self.try_apply_oxide_import_plugin_settings(
            plugin_settings,
            should_import,
            selected_plugin_ids,
        )
        .unwrap_or(0)
    }

    pub(in crate::workspace) fn try_apply_oxide_import_plugin_settings(
        &mut self,
        plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
        should_import: bool,
        selected_plugin_ids: Option<&HashSet<String>>,
    ) -> Result<usize, String> {
        if !should_import || plugin_settings.is_empty() {
            return Ok(0);
        }

        let filtered = plugin_settings
            .iter()
            .filter(|entry| {
                selected_plugin_ids.is_none_or(|ids| {
                    plugin_id_from_setting_storage_key(&entry.storage_key)
                        .is_some_and(|plugin_id| ids.contains(&plugin_id))
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        oxideterm_cloud_sync::plugin_settings::upsert_plugin_settings(
            self.settings_store.path(),
            &filtered,
        )
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn apply_oxide_import_app_settings(
        &mut self,
        app_settings_json: Option<&str>,
        should_import: bool,
        selected_sections: Option<&HashSet<String>>,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        let Some(snapshot) = app_settings_json else {
            return (false, false);
        };
        if !should_import {
            return (false, true);
        }

        match merge_oxide_settings_snapshot(
            self.settings_store.settings(),
            snapshot,
            selected_sections,
        ) {
            Ok(merged) => {
                let previous = self.settings_store.settings().clone();
                match self.settings_store.replace_and_save(merged) {
                    Ok(saved) => {
                        self.apply_loaded_settings_to_runtime(&previous, &saved.settings, cx);
                        self.settings_workspace.update(cx, |settings, _cx| {
                            settings.acknowledge_external_store_state()
                        });
                        self.emit_native_plugin_settings_events(&previous, &saved.settings, cx);
                        self.sync_tab_titles(cx);
                        cx.notify();
                        (true, false)
                    }
                    Err(_) => {
                        let status = self.i18n.t("modals.import.skipped_app_settings");
                        self.connections
                            .session_manager
                            .update(cx, |session_manager, cx| {
                                session_manager.status.replace(status);
                                cx.notify();
                            });
                        (false, true)
                    }
                }
            }
            Err(error) => {
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager
                            .status
                            .replace(format!("应用设置导入失败: {error}"));
                        cx.notify();
                    });
                (false, true)
            }
        }
    }

    #[allow(dead_code)]
    pub(in crate::workspace) fn apply_oxide_import_portable_secrets(
        &mut self,
        envelope: &mut ImportResultEnvelope,
        cx: &mut Context<Self>,
    ) {
        let total = envelope.portable_secrets.len();
        if total == 0 {
            return;
        }

        let mut imported = 0usize;
        for secret in envelope.portable_secrets.drain(..) {
            if secret.kind != "ai_provider_key" || secret.id.trim().is_empty() {
                envelope.errors.push(format!(
                    "Unsupported portable secret kind '{}' for id '{}'",
                    secret.kind, secret.id
                ));
                continue;
            }

            match self
                .ai.state
                .read(cx)
                .key_store()
                .store_provider_key(&secret.id, secret.secret)
            {
                Ok(()) => imported += 1,
                Err(error) => envelope.errors.push(format!(
                    "Failed to import portable secret '{}': {error}",
                    secret.id
                )),
            }
        }

        envelope.imported_portable_secrets = imported;
        envelope.skipped_portable_secrets = total.saturating_sub(imported);
    }
}

pub(super) fn owned_forward_import_record(record: &OxideForwardRecord) -> OwnedForwardImportRecord {
    OwnedForwardImportRecord {
        owner_connection_id: record.connection_id.clone(),
        forward_type: record.forward_type.clone(),
        bind_address: record.bind_address.clone(),
        bind_port: record.bind_port,
        target_host: record.target_host.clone(),
        target_port: record.target_port,
        description: record.description.clone(),
        auto_start: record.auto_start,
        tunnel: record
            .tunnel
            .as_ref()
            .map(|profile| oxideterm_forwarding::SshTunnelProfile {
                name: profile.name.clone(),
                group: profile.group.clone(),
                connect_on_startup: profile.connect_on_startup,
            }),
    }
}

pub(super) fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";

    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    if plugin_id.is_empty() || setting_id.is_empty() {
        return None;
    }
    Some(plugin_id.to_string())
}
