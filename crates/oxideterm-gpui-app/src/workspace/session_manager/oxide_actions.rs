#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) struct OxideClientStateImportOptions {
    pub(super) oxide_options: OxideImportOptions,
    pub(super) import_quick_commands: bool,
    pub(super) quick_command_strategy: QuickCommandImportStrategy,
    pub(super) import_plugin_settings: bool,
    pub(super) selected_plugin_ids: Option<HashSet<String>>,
    pub(super) import_app_settings: bool,
    pub(super) selected_app_settings_sections: Option<HashSet<String>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) struct OxideClientStateImportResult {
    pub(super) envelope: ImportResultEnvelope,
    pub(super) imported_app_settings: bool,
    pub(super) skipped_app_settings: bool,
    pub(super) imported_quick_commands: usize,
    pub(super) skipped_quick_commands: bool,
    pub(super) quick_commands_errors: Vec<String>,
    pub(super) imported_plugin_settings: usize,
    pub(super) skipped_plugin_settings: bool,
}

struct OxideCoreImportResult {
    store: ConnectionStore,
    envelope: ImportResultEnvelope,
}

enum OxidePreviewWorkerMessage {
    Progress(OxideTransferProgress),
    Done(Result<ImportPreview, String>),
}

enum OxideImportWorkerMessage {
    Progress(OxideTransferProgress),
    Done(Result<OxideCoreImportResult, String>),
}

enum OxideExportWorkerMessage {
    Progress(OxideTransferProgress),
    Done(Result<Vec<u8>, String>),
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OxideClientStateSnapshot {
    #[serde(default)]
    last_export_timestamp: Option<i64>,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        self.session_manager.oxide_import_dialog = Some(OxideImportDialogState::default());
        self.session_manager.focused_input = None;
        self.session_manager.status = None;
        cx.notify();
    }

    pub(in crate::workspace) fn open_oxide_import_portable_migration_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let dialog = OxideImportDialogState {
            import_portable_secrets: true,
            restore_managed_key_passphrases: true,
            ..OxideImportDialogState::default()
        };
        self.session_manager.oxide_import_dialog = Some(dialog);
        self.session_manager.focused_input = None;
        self.session_manager.status = None;
        cx.notify();
    }

    pub(in crate::workspace) fn open_oxide_export_dialog(&mut self, cx: &mut Context<Self>) {
        self.open_oxide_export_dialog_with_portable_mode(false, cx);
    }

    pub(in crate::workspace) fn open_oxide_export_portable_migration_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.open_oxide_export_dialog_with_portable_mode(true, cx);
    }

    fn open_oxide_export_dialog_with_portable_mode(
        &mut self,
        portable_migration: bool,
        cx: &mut Context<Self>,
    ) {
        let mut dialog = OxideExportDialogState::default();
        dialog.include_portable_secrets = portable_migration;
        dialog.embed_keys = portable_migration;
        dialog.include_managed_key_passphrases = portable_migration;
        dialog.available_forwards = self.exportable_saved_forwards();
        dialog.last_export_timestamp = load_oxide_last_export_timestamp(self.settings_store.path());
        dialog.selected_forward_ids = dialog
            .available_forwards
            .iter()
            .map(|forward| forward.id.clone())
            .collect();
        let plugin_settings =
            crate::workspace::plugin_settings_store::load_plugin_settings(self.settings_store.path())
                .unwrap_or_default();
        for setting in plugin_settings {
            if let Some(plugin_id) = plugin_id_from_setting_storage_key(&setting.storage_key) {
                *dialog.plugin_groups.entry(plugin_id).or_insert(0) += 1;
            }
        }
        dialog.selected_plugin_ids = dialog.plugin_groups.keys().cloned().collect();
        dialog.preflight = self.oxide_export_preflight_for_dialog(&dialog);
        self.session_manager.oxide_export_dialog = Some(dialog);
        self.session_manager.focused_input = None;
        self.session_manager.status = None;
        cx.notify();
    }

    pub(super) fn handle_oxide_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }

        if self.session_manager.oxide_import_dialog.is_some() {
            return self.handle_oxide_import_footer_key(event, cx);
        }
        if self.session_manager.oxide_export_dialog.is_some() {
            return self.handle_oxide_export_footer_key(event, cx);
        }
        false
    }

    fn handle_oxide_import_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_ref() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let actions = oxide_import_footer_actions(dialog);
        if actions.is_empty() {
            return false;
        }
        let body_inputs = oxide_import_footer_body_inputs(dialog);
        let current_input = self
            .session_manager
            .focused_input
            .filter(|focused| body_inputs.contains(focused));

        match browser_behavior::modal_footer_body_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &actions,
            dialog.focused_footer_action,
            body_inputs,
            current_input,
            actions[0],
            None,
        ) {
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Cancel) => {
                self.session_manager.oxide_import_dialog = None;
                self.session_manager.focused_input = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusInput(input)) => {
                self.session_manager.focused_input = Some(input);
                if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                    dialog.focused_footer_action = None;
                }
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusFooter(action)) => {
                if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                    dialog.focused_footer_action = Some(action);
                }
                self.session_manager.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Activate(action)) => {
                self.activate_oxide_import_footer_action(action, cx);
                true
            }
            None => false,
        }
    }

    fn activate_oxide_import_footer_action(
        &mut self,
        action: OxideDialogFooterAction,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_ref() else {
            return;
        };
        match action {
            OxideDialogFooterAction::Cancel => {
                self.session_manager.oxide_import_dialog = None;
                self.session_manager.focused_input = None;
                cx.notify();
            }
            OxideDialogFooterAction::Secondary if dialog.preview.is_some() => {
                if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                    dialog.preview = None;
                    dialog.result_summary = None;
                    dialog.focused_footer_action = Some(OxideDialogFooterAction::Secondary);
                }
                cx.notify();
            }
            OxideDialogFooterAction::Secondary => self.select_oxide_import_file(cx),
            OxideDialogFooterAction::Primary if dialog.result.is_some() => {
                self.session_manager.oxide_import_dialog = None;
                self.session_manager.focused_input = None;
                cx.notify();
            }
            OxideDialogFooterAction::Primary if dialog.preview.is_some() => {
                if oxide_import_has_selected_content(dialog) {
                    self.apply_oxide_import_dialog(cx);
                } else {
                    cx.notify();
                }
            }
            OxideDialogFooterAction::Primary => {
                if !dialog.password.is_empty() {
                    self.preview_oxide_import_dialog(cx);
                } else {
                    cx.notify();
                }
            }
        }
    }

    fn handle_oxide_export_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.session_manager.oxide_export_dialog.as_ref() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let actions = [OxideDialogFooterAction::Cancel, OxideDialogFooterAction::Primary];
        let body_inputs = oxide_export_footer_body_inputs(dialog);
        let current_input = self
            .session_manager
            .focused_input
            .filter(|focused| body_inputs.contains(focused));
        match browser_behavior::modal_footer_body_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &actions,
            dialog.focused_footer_action,
            body_inputs,
            current_input,
            OxideDialogFooterAction::Cancel,
            None,
        ) {
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Cancel) => {
                self.session_manager.oxide_export_dialog = None;
                self.session_manager.focused_input = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusInput(input)) => {
                self.session_manager.focused_input = Some(input);
                if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                    dialog.focused_footer_action = None;
                }
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusFooter(action)) => {
                if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                    dialog.focused_footer_action = Some(action);
                }
                self.session_manager.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Activate(action)) => {
                match action {
                    OxideDialogFooterAction::Cancel => {
                        self.session_manager.oxide_export_dialog = None;
                        self.session_manager.focused_input = None;
                        cx.notify();
                    }
                    OxideDialogFooterAction::Primary => {
                        if oxide_export_has_selected_content(dialog) {
                            self.export_oxide_dialog(cx);
                        } else {
                            cx.notify();
                        }
                    }
                    OxideDialogFooterAction::Secondary => cx.notify(),
                }
                true
            }
            None => false,
        }
    }

    fn exportable_saved_forwards(&self) -> Vec<PersistedForward> {
        let connection_ids = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .collect::<HashSet<_>>();
        let mut forwards_by_key = HashMap::<String, PersistedForward>::new();

        for forward in self.forwarding_registry.list_all_saved_forwards() {
            let Some(owner_id) = forward.owner_connection_id.as_ref() else {
                continue;
            };
            if !connection_ids.contains(owner_id) {
                continue;
            }

            let key = oxide_forward_export_identity(&forward);
            match forwards_by_key.get(&key) {
                Some(existing) if existing.sync_updated_at() >= forward.sync_updated_at() => {}
                _ => {
                    forwards_by_key.insert(key, forward);
                }
            }
        }

        let mut forwards = forwards_by_key.into_values().collect::<Vec<_>>();
        forwards.sort_by_key(|forward| {
            (
                forward.owner_connection_id.clone().unwrap_or_default(),
                forward.created_at,
            )
        });
        forwards
    }

    fn select_oxide_import_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.i18n.t("modals.import.select_file"))),
        });
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(paths))) => paths
                    .into_iter()
                    .next()
                    .ok_or_else(|| "未选择文件".to_string())
                    .and_then(|path| {
                        fs::read(&path)
                            .map(|bytes| (path, bytes))
                            .map_err(|error| error.to_string())
                    }),
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = weak.update(cx, |this, cx| {
            let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() else {
                return;
            };
                match result {
                    Ok((path, bytes)) => match OxideFile::from_bytes(&bytes) {
                        Ok(file) => {
                            let metadata = file.metadata.clone();
                            dialog.file_path = Some(path);
                            dialog.file_data = Some(bytes);
                            dialog.metadata_summary = Some(format!(
                                "{} 个连接 · {}",
                                metadata.num_connections,
                                metadata
                                    .exported_at
                                    .with_timezone(&Local)
                                    .format("%Y-%m-%d %H:%M")
                            ));
                            dialog.selected_names =
                                metadata.connection_names.iter().cloned().collect();
                            dialog.expanded_app_settings_sections.clear();
                            dialog.metadata = Some(metadata);
                            dialog.preview = None;
                            dialog.error = None;
                            dialog.result_summary = None;
                            dialog.result = None;
                        }
                        Err(error) => {
                            dialog.metadata = None;
                            dialog.error = Some(error.to_string());
                        }
                    },
                    Err(error) => dialog.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn preview_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() else {
            return;
        };
        let Some(bytes) = dialog.file_data.clone() else {
            dialog.error = Some(self.i18n.t("modals.import.select_file"));
            cx.notify();
            return;
        };
        if dialog.password.is_empty() {
            dialog.error = Some(self.i18n.t("modals.import.error_enter_password"));
            cx.notify();
            return;
        }
        dialog.busy = true;
        dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
        let generation = dialog.operation_generation;
        dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 8));
        dialog.error = None;
        cx.notify();

        let store = self.connection_store.clone();
        // Dialog input is a UI String draft; worker-owned copies are zeroized
        // when the import preview thread exits.
        let password = zeroize::Zeroizing::new(dialog.password.clone());
        let conflict_strategy = dialog.conflict_strategy;
        let (tx, rx) = std::sync::mpsc::channel();
        let i18n = self.i18n.clone();
        std::thread::spawn(move || {
            let result = preview_oxide_import_with_progress(
                &store,
                &bytes,
                &password,
                conflict_strategy,
                |stage, current, total| {
                    let _ = tx.send(OxidePreviewWorkerMessage::Progress(
                        OxideTransferProgress::new(stage, current, total),
                    ));
                },
            )
            .map_err(|error| oxide_file_error_message(error, &i18n));
            let _ = tx.send(OxidePreviewWorkerMessage::Done(result));
        });
        self.poll_oxide_import_preview_worker(generation, rx, cx);
    }

    fn apply_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_ref() else {
            return;
        };
        let Some(bytes) = dialog.file_data.clone() else {
            if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("modals.import.select_file"));
            }
            cx.notify();
            return;
        };
        if dialog.password.is_empty() {
            if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("modals.import.error_enter_password"));
            }
            cx.notify();
            return;
        }
        // Dialog input is a UI String draft; worker-owned copies are zeroized
        // when the import thread exits.
        let password = zeroize::Zeroizing::new(dialog.password.clone());
        let options = OxideClientStateImportOptions {
            oxide_options: OxideImportOptions {
                selected_names: Some(dialog.selected_names.iter().cloned().collect()),
                selected_forward_ids: None,
                conflict_strategy: dialog.conflict_strategy,
                import_forwards: dialog.import_forwards,
                import_serial_profiles: dialog.import_serial_profiles,
                import_raw_tcp_profiles: dialog.import_raw_tcp_profiles,
                import_portable_secrets: dialog.import_portable_secrets,
                restore_managed_keys: dialog.restore_managed_keys,
                restore_managed_key_passphrases: dialog.restore_managed_key_passphrases,
                ..OxideImportOptions::default()
            },
            import_quick_commands: dialog.import_quick_commands,
            quick_command_strategy: quick_command_strategy_from_oxide(dialog.conflict_strategy),
            import_plugin_settings: dialog.import_plugin_settings,
            selected_plugin_ids: Some(dialog.selected_plugin_ids.clone()),
            import_app_settings: dialog.import_app_settings
                && !dialog.selected_app_settings_sections.is_empty(),
            selected_app_settings_sections: Some(dialog.selected_app_settings_sections.clone()),
        };
        if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
            dialog.busy = true;
            dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
            dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 10));
            dialog.error = None;
        }
        let generation = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .map(|dialog| dialog.operation_generation)
            .unwrap_or(0);
        cx.notify();

        let mut store = self.connection_store.clone();
        let oxide_options = options.oxide_options.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let i18n = self.i18n.clone();
        std::thread::spawn(move || {
            let result = apply_oxide_import_with_options_with_progress(
                &mut store,
                &bytes,
                &password,
                oxide_options,
                |stage, current, total| {
                    let _ = tx.send(OxideImportWorkerMessage::Progress(
                        OxideTransferProgress::new(stage, current, total),
                    ));
                },
            )
            .map(|envelope| OxideCoreImportResult { store, envelope })
            .map_err(|error| oxide_file_error_message(error, &i18n));
            let _ = tx.send(OxideImportWorkerMessage::Done(result));
        });
        self.poll_oxide_import_worker(generation, options, rx, cx);
    }

    fn poll_oxide_import_preview_worker(
        &mut self,
        generation: u64,
        rx: std::sync::mpsc::Receiver<OxidePreviewWorkerMessage>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            loop {
                match rx.try_recv() {
                    Ok(OxidePreviewWorkerMessage::Progress(progress)) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.progress_stage = Some(progress);
                                cx.notify();
                            }
                        });
                    }
                    Ok(OxidePreviewWorkerMessage::Done(result)) => {
                        let _ = weak.update(cx, |this, cx| {
                            let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                            else {
                                return;
                            };
                            if dialog.operation_generation != generation {
                                return;
                            }
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            match result {
                                Ok(preview) => {
                                    dialog.selected_names =
                                        preview.records.iter().map(|r| r.name.clone()).collect();
                                    if !preview.app_settings_section_ids.is_empty() {
                                        dialog.selected_app_settings_sections = preview
                                            .app_settings_section_ids
                                            .iter()
                                            .cloned()
                                            .collect();
                                    }
                                    dialog.import_app_settings = preview.has_app_settings;
                                    dialog.import_quick_commands = preview.has_quick_commands;
                                    dialog.import_serial_profiles =
                                        preview.serial_profiles_count > 0;
                                    dialog.import_raw_tcp_profiles =
                                        preview.raw_tcp_profiles_count > 0;
                                    dialog.import_plugin_settings =
                                        preview.plugin_settings_count > 0;
                                    dialog.import_forwards = preview.total_forwards > 0;
                                    dialog.import_portable_secrets = false;
                                    dialog.selected_plugin_ids =
                                        preview.plugin_settings_by_plugin.keys().cloned().collect();
                                    dialog.expanded_app_settings_sections.clear();
                                    dialog.result = None;
                                    dialog.result_summary = None;
                                    dialog.preview = Some(preview);
                                    dialog.error = None;
                                }
                                Err(error) => dialog.error = Some(error),
                            }
                            cx.notify();
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(33)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.busy = false;
                                dialog.progress_stage = None;
                                dialog.error = Some("导入预览任务已中断".to_string());
                                cx.notify();
                            }
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn poll_oxide_import_worker(
        &mut self,
        generation: u64,
        options: OxideClientStateImportOptions,
        rx: std::sync::mpsc::Receiver<OxideImportWorkerMessage>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            loop {
                match rx.try_recv() {
                    Ok(OxideImportWorkerMessage::Progress(progress)) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.progress_stage = Some(progress);
                                cx.notify();
                            }
                        });
                    }
                    Ok(OxideImportWorkerMessage::Done(result)) => {
                        let _ = weak.update(cx, |this, cx| {
                            let still_current = this
                                .session_manager
                                .oxide_import_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.operation_generation == generation);
                            if !still_current {
                                return;
                            }
                            match result {
                                Ok(core) => {
                                    this.finish_oxide_import_core_result(core, options, cx);
                                }
                                Err(error) => {
                                    if let Some(dialog) =
                                        this.session_manager.oxide_import_dialog.as_mut()
                                    {
                                        dialog.busy = false;
                                        dialog.progress_stage = None;
                                        dialog.error = Some(error);
                                    }
                                }
                            }
                            cx.notify();
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(33)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.busy = false;
                                dialog.progress_stage = None;
                                dialog.error = Some("导入任务已中断".to_string());
                                cx.notify();
                            }
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn finish_oxide_import_core_result(
        &mut self,
        core: OxideCoreImportResult,
        options: OxideClientStateImportOptions,
        cx: &mut Context<Self>,
    ) {
        self.connection_store = core.store;
        let mut envelope = core.envelope;

        let imported_forwards = self.apply_oxide_import_forward_records(&mut envelope);
        envelope.imported_forwards = imported_forwards;

        let (imported_quick_commands, skipped_quick_commands, quick_commands_errors) =
            self.apply_oxide_import_quick_commands(
                envelope.quick_commands_json.as_deref(),
                options.import_quick_commands,
                options.quick_command_strategy,
            );

        let imported_plugin_settings = self.apply_oxide_import_plugin_settings(
            &envelope.plugin_settings,
            options.import_plugin_settings,
            options.selected_plugin_ids.as_ref(),
        );
        let skipped_plugin_settings =
            !options.import_plugin_settings && !envelope.plugin_settings.is_empty();

        let (imported_app_settings, skipped_app_settings) = self.apply_oxide_import_app_settings(
            envelope.app_settings_json.as_deref(),
            options.import_app_settings,
            options.selected_app_settings_sections.as_ref(),
            cx,
        );

        self.apply_oxide_import_portable_secrets(&mut envelope);
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
        self.present_oxide_import_result(result, cx);
    }

    fn present_oxide_import_result(
        &mut self,
        result: OxideClientStateImportResult,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() else {
            return;
        };
        dialog.busy = false;
        dialog.progress_stage = None;

        let result_view = OxideImportResultView {
            imported: result.envelope.imported,
            skipped: result.envelope.skipped,
            merged: result.envelope.merged,
            replaced: result.envelope.replaced,
            renamed: result.envelope.renamed,
            renames: result.envelope.renames.clone(),
            errors: result.envelope.errors.clone(),
            imported_forwards: result.envelope.imported_forwards,
            skipped_forwards: result.envelope.skipped_forwards,
            imported_app_settings: result.imported_app_settings,
            skipped_app_settings: result.skipped_app_settings,
            imported_quick_commands: result.imported_quick_commands,
            skipped_quick_commands: result.skipped_quick_commands,
            imported_serial_profiles: result.envelope.imported_serial_profiles,
            skipped_serial_profiles: result.envelope.skipped_serial_profiles,
            imported_raw_tcp_profiles: result.envelope.imported_raw_tcp_profiles,
            skipped_raw_tcp_profiles: result.envelope.skipped_raw_tcp_profiles,
            quick_commands_errors: result.quick_commands_errors.clone(),
            imported_plugin_settings: result.imported_plugin_settings,
            skipped_plugin_settings: result.skipped_plugin_settings,
            imported_portable_secrets: result.envelope.imported_portable_secrets,
            skipped_portable_secrets: result.envelope.skipped_portable_secrets,
        };

        let mut parts = vec![format!("✓ 导入成功: {} 个连接", result_view.imported)];
        if result_view.imported_forwards > 0 {
            parts.push(format!("{} 个端口转发", result_view.imported_forwards));
        }
        if result_view.imported_app_settings {
            parts.push("应用设置".to_string());
        }
        if result_view.imported_quick_commands > 0 {
            parts.push(format!("{} 条快捷命令", result_view.imported_quick_commands));
        }
        if result_view.imported_serial_profiles > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_serial_profiles")
                    .replace(
                        "{{count}}",
                        &result_view.imported_serial_profiles.to_string(),
                    ),
            );
        }
        if result_view.imported_raw_tcp_profiles > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_raw_tcp_profiles")
                    .replace(
                        "{{count}}",
                        &result_view.imported_raw_tcp_profiles.to_string(),
                    ),
            );
        }
        if result_view.imported_plugin_settings > 0 {
            parts.push(format!(
                "已恢复 {} 项插件偏好设置。",
                result_view.imported_plugin_settings
            ));
        }
        if result_view.imported_portable_secrets > 0 {
            parts.push(format!(
                "已恢复 {} 项便携秘密项。",
                result_view.imported_portable_secrets
            ));
        }
        let auto_close_import_dialog = result_view.errors.is_empty();
        if !result_view.errors.is_empty() {
            dialog.error = Some(result_view.errors.join("; "));
        }
        dialog.result_summary = Some(parts.join(" · "));
        dialog.result = Some(result_view);
        self.session_manager.status = dialog.result_summary.clone();

        if auto_close_import_dialog {
            cx.spawn(async move |weak, cx| {
                Timer::after(Duration::from_secs(2)).await;
                let _ = weak.update(cx, |this, cx| {
                    if let Some(dialog) = this.session_manager.oxide_import_dialog.as_ref()
                        && dialog.error.is_none()
                        && dialog.result_summary.is_some()
                    {
                        this.session_manager.oxide_import_dialog = None;
                        this.session_manager.focused_input = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    fn oxide_export_connection_ids(&self, dialog: &OxideExportDialogState) -> HashSet<String> {
        let mut ids = dialog.selected_ids.clone();
        if dialog.include_forwards {
            for forward in &dialog.available_forwards {
                if dialog.selected_forward_ids.contains(&forward.id) {
                    if let Some(owner_id) = &forward.owner_connection_id {
                        ids.insert(owner_id.clone());
                    }
                }
            }
        }
        ids
    }

    fn oxide_export_has_content(&self, dialog: &OxideExportDialogState) -> bool {
        !self.oxide_export_connection_ids(dialog).is_empty()
            || (dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty())
            || dialog.include_quick_commands
            || dialog.include_serial_profiles
            || (dialog.include_plugin_settings && !dialog.selected_plugin_ids.is_empty())
            || dialog.include_portable_secrets
    }

    pub(in crate::workspace) fn oxide_export_portable_secret_count(
        &self,
        dialog: &OxideExportDialogState,
    ) -> usize {
        if !dialog.include_portable_secrets {
            return 0;
        }
        oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
            .into_iter()
            .filter(|provider| self.ai_key_store.has_provider_key(&provider.id))
            .count()
    }

    fn oxide_export_preflight(&self, dialog: &OxideExportDialogState) -> ExportPreflightResult {
        let selected_ids = self
            .oxide_export_connection_ids(dialog)
            .into_iter()
            .collect::<Vec<_>>();
        preflight_export(
            &self.connection_store,
            &selected_ids,
            dialog.embed_keys,
            dialog.include_managed_keys,
            self.oxide_export_portable_secret_count(dialog),
        )
    }

    fn refresh_oxide_export_preflight(&mut self) {
        let Some(dialog) = self.session_manager.oxide_export_dialog.as_ref() else {
            return;
        };
        let preflight = self.oxide_export_preflight_for_dialog(dialog);
        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
            dialog.preflight = preflight;
        }
    }

    fn oxide_export_preflight_for_dialog(
        &self,
        dialog: &OxideExportDialogState,
    ) -> Option<ExportPreflightResult> {
        let has_preflight_content =
            !self.oxide_export_connection_ids(dialog).is_empty() || dialog.include_portable_secrets;
        has_preflight_content.then(|| self.oxide_export_preflight(dialog))
    }

    fn export_oxide_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.session_manager.oxide_export_dialog.as_ref() else {
            return;
        };
        if !self.oxide_export_has_content(dialog) {
            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("export.error_select_something"));
            }
            cx.notify();
            return;
        }
        if dialog.password.len() < 6 || dialog.password != dialog.confirm_password {
            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                dialog.error = if dialog.password.len() < 6 {
                    Some(self.i18n.t("export.error_password_too_short"))
                } else {
                    Some(self.i18n.t("export.error_password_mismatch"))
                };
            }
            cx.notify();
            return;
        }
        if dialog
            .preflight
            .as_ref()
            .is_some_and(|preflight| !preflight.can_export)
        {
            if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("export.error_managed_keys_required"));
            }
            cx.notify();
            return;
        }
        let selected_ids = self
            .oxide_export_connection_ids(dialog)
            .into_iter()
            .collect::<Vec<_>>();
        // Dialog input is a UI String draft; worker-owned copies are zeroized
        // when the export thread exits.
        let password = zeroize::Zeroizing::new(dialog.password.clone());
        let preflight = self.oxide_export_preflight(dialog);
        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
            dialog.busy = true;
            dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
            dialog.progress_stage = Some(OxideTransferProgress::new("collecting_connections", 0, 1));
            dialog.error = None;
            dialog.preflight = Some(preflight);
        }
        let generation = self
            .session_manager
            .oxide_export_dialog
            .as_ref()
            .map(|dialog| dialog.operation_generation)
            .unwrap_or(0);
        let result = self
            .session_manager
            .oxide_export_dialog
            .as_ref()
            .ok_or_else(|| self.i18n.t("export.error_export_failed"))
            .and_then(|dialog| self.build_oxide_export_options(dialog));
        match result {
            Ok(options) => {
                cx.notify();
                let store = self.connection_store.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let i18n = self.i18n.clone();
                std::thread::spawn(move || {
                    let result = export_connections_to_oxide_with_progress(
                        &store,
                        &selected_ids,
                        &password,
                        options,
                        |stage, current, total| {
                            let _ = tx.send(OxideExportWorkerMessage::Progress(
                                OxideTransferProgress::new(stage, current, total),
                            ));
                        },
                    )
                    .map_err(|error| oxide_file_error_message(error, &i18n));
                    let _ = tx.send(OxideExportWorkerMessage::Done(result));
                });
                self.poll_oxide_export_worker(generation, rx, cx);
            }
            Err(error) => {
                if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                    dialog.busy = false;
                    dialog.progress_stage = None;
                    dialog.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    fn poll_oxide_export_worker(
        &mut self,
        generation: u64,
        rx: std::sync::mpsc::Receiver<OxideExportWorkerMessage>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            loop {
                match rx.try_recv() {
                    Ok(OxideExportWorkerMessage::Progress(progress)) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.progress_stage = Some(progress);
                                cx.notify();
                            }
                        });
                    }
                    Ok(OxideExportWorkerMessage::Done(result)) => {
                        let _ = weak.update(cx, |this, cx| {
                            let still_current = this
                                .session_manager
                                .oxide_export_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.operation_generation == generation);
                            if !still_current {
                                return;
                            }
                            match result {
                                Ok(bytes) => {
                                    let exported_count = this
                                        .session_manager
                                        .oxide_export_dialog
                                        .as_ref()
                                        .map(oxide_export_connection_count)
                                        .unwrap_or(0);
                                    if let Some(dialog) =
                                        this.session_manager.oxide_export_dialog.as_mut()
                                    {
                                        dialog.progress_stage =
                                            Some(OxideTransferProgress::new("writing", 1, 1));
                                    }
                                    this.prompt_save_oxide_export(bytes, exported_count, cx);
                                }
                                Err(error) => {
                                    if let Some(dialog) =
                                        this.session_manager.oxide_export_dialog.as_mut()
                                    {
                                        dialog.busy = false;
                                        dialog.progress_stage = None;
                                        dialog.error = Some(error);
                                    }
                                }
                            }
                            cx.notify();
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(33)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut()
                                && dialog.busy
                                && dialog.operation_generation == generation
                            {
                                dialog.busy = false;
                                dialog.progress_stage = None;
                                dialog.error = Some(this.i18n.t("export.error_export_failed"));
                                cx.notify();
                            }
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn prompt_save_oxide_export(
        &mut self,
        bytes: Vec<u8>,
        exported_count: usize,
        cx: &mut Context<Self>,
    ) {
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!("oxideterm-export-{}.oxide", Utc::now().format("%Y%m%d-%H%M%S"));
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => fs::write(&path, bytes)
                    .map(|_| path)
                    .map_err(|error| error.to_string()),
                Ok(Ok(None)) => {
                    let _ = weak.update(cx, |this, cx| {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.busy = false;
                            dialog.progress_stage = None;
                        }
                        cx.notify();
                    });
                    return;
                }
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(path) => {
                        let _ = persist_oxide_last_export_timestamp(this.settings_store.path());
                        let summary = this
                            .i18n
                            .t("export.success")
                            .replace("{{count}}", &exported_count.to_string())
                            .replace("{{path}}", path.to_string_lossy().as_ref());
                        this.session_manager.status = Some(summary);
                        this.session_manager.oxide_export_dialog = None;
                        this.session_manager.focused_input = None;
                    }
                    Err(error) => {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            dialog.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn build_oxide_export_options(
        &self,
        dialog: &OxideExportDialogState,
    ) -> Result<OxideExportOptions, String> {
        let app_settings_json = if dialog.include_app_settings {
            Some(
                export_oxide_settings_snapshot_json(
                    self.settings_store.settings(),
                    Some(&dialog.selected_app_settings_sections),
                    dialog.include_local_terminal_env_vars,
                )
                .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let quick_commands_json = if dialog.include_quick_commands {
            Some(self.quick_commands.export_snapshot_json()?)
        } else {
            None
        };
        let serial_profiles_json = if dialog.include_serial_profiles {
            Some(
                serde_json::to_string_pretty(
                    &self
                        .connection_store
                        .export_serial_profiles_snapshot()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let raw_tcp_profiles_json = if dialog.include_raw_tcp_profiles {
            Some(
                serde_json::to_string_pretty(
                    &self
                        .connection_store
                        .export_raw_tcp_profiles_snapshot()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let plugin_settings = if dialog.include_plugin_settings {
            crate::workspace::plugin_settings_store::load_plugin_settings(self.settings_store.path())?
                .into_iter()
                .filter(|setting| {
                    plugin_id_from_setting_storage_key(&setting.storage_key)
                        .is_some_and(|plugin_id| dialog.selected_plugin_ids.contains(&plugin_id))
                })
                .collect()
        } else {
            Vec::new()
        };
        let selected_ids = self.oxide_export_connection_ids(dialog);
        let forwards = if dialog.include_forwards {
            dialog
                .available_forwards
                .iter()
                .cloned()
                .into_iter()
                .filter_map(|forward| {
                    let owner_id = forward.owner_connection_id?;
                    (selected_ids.contains(&owner_id)
                        && dialog.selected_forward_ids.contains(&forward.id))
                    .then(|| OxideForwardRecord {
                        id: Some(forward.id),
                        connection_id: owner_id,
                        forward_type: match forward.forward_type {
                            ForwardType::Local => "local".to_string(),
                            ForwardType::Remote => "remote".to_string(),
                            ForwardType::Dynamic => "dynamic".to_string(),
                        },
                        bind_address: forward.rule.bind_address,
                        bind_port: forward.rule.bind_port,
                        target_host: forward.rule.target_host,
                        target_port: forward.rule.target_port,
                        description: Some(forward.rule.description),
                        auto_start: forward.auto_start,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let portable_secrets = if dialog.include_portable_secrets {
            let provider_ids = oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
                .into_iter()
                .map(|provider| provider.id)
                .filter(|provider_id| self.ai_key_store.has_provider_key(provider_id))
                .collect::<Vec<_>>();
            self.ai_key_store
                .get_provider_keys(&provider_ids)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|(id, secret)| oxideterm_connections::oxide_file::EncryptedPortableSecret {
                    kind: "ai_provider_key".to_string(),
                    id,
                    secret,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(OxideExportOptions {
            description: (!dialog.description.trim().is_empty())
                .then(|| dialog.description.trim().to_string()),
            embed_keys: dialog.embed_keys,
            include_passwords: dialog.include_passwords,
            include_key_passphrases: dialog.include_key_passphrases,
            include_managed_keys: dialog.include_managed_keys,
            include_managed_key_passphrases: dialog.include_managed_key_passphrases,
            app_settings_json,
            quick_commands_json,
            serial_profiles_json,
            raw_tcp_profiles_json,
            plugin_settings,
            portable_secrets,
            forwards,
            ..OxideExportOptions::default()
        })
    }

    #[allow(dead_code)]
    fn import_oxide_with_client_state(
        &mut self,
        bytes: &[u8],
        password: &str,
        options: OxideClientStateImportOptions,
        cx: &mut Context<Self>,
        on_progress: &mut dyn FnMut(&str, usize, usize),
    ) -> Result<OxideClientStateImportResult, String> {
        let mut envelope = apply_oxide_import_with_options_with_progress(
            &mut self.connection_store,
            bytes,
            password,
            options.oxide_options,
            |stage, current, total| on_progress(stage, current, total),
        )
        .map_err(|error| oxide_file_error_message(error, &self.i18n))?;

        let imported_forwards = self.apply_oxide_import_forward_records(&mut envelope);
        envelope.imported_forwards = imported_forwards;

        let (imported_quick_commands, skipped_quick_commands, quick_commands_errors) =
            self.apply_oxide_import_quick_commands(
                envelope.quick_commands_json.as_deref(),
                options.import_quick_commands,
                options.quick_command_strategy,
            );

        let imported_plugin_settings = self.apply_oxide_import_plugin_settings(
            &envelope.plugin_settings,
            options.import_plugin_settings,
            options.selected_plugin_ids.as_ref(),
        );
        let skipped_plugin_settings =
            !options.import_plugin_settings && !envelope.plugin_settings.is_empty();

        let (imported_app_settings, skipped_app_settings) = self.apply_oxide_import_app_settings(
            envelope.app_settings_json.as_deref(),
            options.import_app_settings,
            options.selected_app_settings_sections.as_ref(),
            cx,
        );

        self.apply_oxide_import_portable_secrets(&mut envelope);
        self.queue_cloud_sync_dirty_refresh(cx);

        Ok(OxideClientStateImportResult {
            envelope,
            imported_app_settings,
            skipped_app_settings,
            imported_quick_commands,
            skipped_quick_commands,
            quick_commands_errors,
            imported_plugin_settings,
            skipped_plugin_settings,
        })
    }

    #[allow(dead_code)]
    pub(super) fn apply_oxide_import_forward_records(
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

        match self.forwarding_registry.apply_owned_forward_import_records(
            &records,
            &replace_owner_ids,
            &merge_owner_ids,
        ) {
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
    pub(super) fn apply_oxide_import_quick_commands(
        &mut self,
        quick_commands_json: Option<&str>,
        should_import: bool,
        strategy: QuickCommandImportStrategy,
    ) -> (usize, bool, Vec<String>) {
        let Some(snapshot) = quick_commands_json else {
            return (0, false, Vec::new());
        };
        if !should_import {
            return (0, true, Vec::new());
        }

        let result = self.quick_commands.apply_snapshot_json(snapshot, strategy);
        (
            result.imported,
            !result.errors.is_empty(),
            result.errors,
        )
    }

    #[allow(dead_code)]
    pub(super) fn apply_oxide_import_plugin_settings(
        &mut self,
        plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
        should_import: bool,
        selected_plugin_ids: Option<&HashSet<String>>,
    ) -> usize {
        if !should_import || plugin_settings.is_empty() {
            return 0;
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
        crate::workspace::plugin_settings_store::upsert_plugin_settings(
            self.settings_store.path(),
            &filtered,
        )
        .unwrap_or(0)
    }

    #[allow(dead_code)]
    pub(super) fn apply_oxide_import_app_settings(
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
                self.edit_settings(|settings| *settings = merged, cx);
                (true, false)
            }
            Err(error) => {
                self.session_manager
                    .status
                    .replace(format!("应用设置导入失败: {error}"));
                (false, true)
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn apply_oxide_import_portable_secrets(
        &mut self,
        envelope: &mut ImportResultEnvelope,
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

            match self.ai_key_store.store_provider_key(&secret.id, secret.secret) {
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

fn owned_forward_import_record(record: &OxideForwardRecord) -> OwnedForwardImportRecord {
    OwnedForwardImportRecord {
        owner_connection_id: record.connection_id.clone(),
        forward_type: record.forward_type.clone(),
        bind_address: record.bind_address.clone(),
        bind_port: record.bind_port,
        target_host: record.target_host.clone(),
        target_port: record.target_port,
        description: record.description.clone(),
        auto_start: record.auto_start,
    }
}

fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
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

fn oxide_forward_export_identity(forward: &PersistedForward) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        forward.owner_connection_id.as_deref().unwrap_or_default(),
        match forward.forward_type {
            ForwardType::Local => "local",
            ForwardType::Remote => "remote",
            ForwardType::Dynamic => "dynamic",
        },
        forward.rule.bind_address,
        forward.rule.bind_port,
        forward.rule.target_host,
        forward.rule.target_port
    )
}

fn quick_command_strategy_from_oxide(
    strategy: ImportConflictStrategy,
) -> QuickCommandImportStrategy {
    match strategy {
        ImportConflictStrategy::Rename => QuickCommandImportStrategy::Rename,
        ImportConflictStrategy::Skip => QuickCommandImportStrategy::Skip,
        ImportConflictStrategy::Replace => QuickCommandImportStrategy::Replace,
        ImportConflictStrategy::Merge => QuickCommandImportStrategy::Merge,
    }
}

fn oxide_file_error_message(error: OxideFileError, i18n: &oxideterm_i18n::I18n) -> String {
    match error {
        OxideFileError::DecryptionFailed => i18n.t("modals.import.error_password"),
        OxideFileError::ChecksumMismatch => i18n.t("modals.import.error_tampered"),
        OxideFileError::PasswordTooShort => i18n.t("export.error_password_too_short"),
        other => other.to_string(),
    }
}

fn persist_oxide_last_export_timestamp(settings_path: &std::path::Path) -> Result<(), String> {
    let path = oxide_client_state_path(settings_path);
    let mut snapshot = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<OxideClientStateSnapshot>(&contents).ok())
            .unwrap_or_default()
    } else {
        OxideClientStateSnapshot::default()
    };
    snapshot.last_export_timestamp = Some(Utc::now().timestamp_millis());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn load_oxide_last_export_timestamp(settings_path: &std::path::Path) -> Option<i64> {
    let path = oxide_client_state_path(settings_path);
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<OxideClientStateSnapshot>(&contents)
        .ok()
        .and_then(|snapshot| snapshot.last_export_timestamp)
}

fn oxide_client_state_path(settings_path: &std::path::Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join("oxide-client-state.json")
}
