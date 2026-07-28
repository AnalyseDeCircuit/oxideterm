use super::*;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(in crate::workspace) struct OxideClientStateImportOptions {
    pub(in crate::workspace) oxide_options: OxideImportOptions,
    pub(in crate::workspace) import_quick_commands: bool,
    pub(in crate::workspace) quick_command_strategy: QuickCommandImportStrategy,
    pub(in crate::workspace) import_plugin_settings: bool,
    pub(in crate::workspace) selected_plugin_ids: Option<HashSet<String>>,
    pub(in crate::workspace) import_app_settings: bool,
    pub(in crate::workspace) selected_app_settings_sections: Option<HashSet<String>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(in crate::workspace) struct OxideClientStateImportResult {
    pub(in crate::workspace) envelope: ImportResultEnvelope,
    pub(in crate::workspace) imported_app_settings: bool,
    pub(in crate::workspace) skipped_app_settings: bool,
    pub(in crate::workspace) imported_quick_commands: usize,
    pub(in crate::workspace) skipped_quick_commands: bool,
    pub(in crate::workspace) quick_commands_errors: Vec<String>,
    pub(in crate::workspace) imported_plugin_settings: usize,
    pub(in crate::workspace) skipped_plugin_settings: bool,
}

pub(super) struct OxideCoreImportResult {
    store: ConnectionStore,
    envelope: ImportResultEnvelope,
}

pub(super) enum OxideWorkerDelivery {
    PreviewProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    PreviewDone {
        generation: u64,
        result: Result<ImportPreview, OxideFileError>,
        password: zeroize::Zeroizing<String>,
    },
    ImportProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    ImportDone {
        generation: u64,
        result: Result<OxideCoreImportResult, OxideFileError>,
        options: OxideClientStateImportOptions,
        password: zeroize::Zeroizing<String>,
    },
    ExportProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    ExportDone {
        generation: u64,
        result: Result<Vec<u8>, OxideFileError>,
        password: zeroize::Zeroizing<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum OxideWorkerKey {
    Preview(u64),
    Import(u64),
    Export(u64),
}

pub(super) enum OxideWorkspaceEffect {
    PreviewDone {
        generation: u64,
        result: Result<ImportPreview, OxideFileError>,
        password: zeroize::Zeroizing<String>,
    },
    ImportDone {
        generation: u64,
        result: Result<OxideCoreImportResult, OxideFileError>,
        options: OxideClientStateImportOptions,
        password: zeroize::Zeroizing<String>,
    },
    ExportDone {
        generation: u64,
        result: Result<Vec<u8>, OxideFileError>,
        password: zeroize::Zeroizing<String>,
    },
}

pub(in crate::workspace) struct OxideWorkspaceEffects {
    effects: RefCell<VecDeque<OxideWorkspaceEffect>>,
}

impl OxideWorkspaceEffects {
    fn new(effects: VecDeque<OxideWorkspaceEffect>) -> Self {
        Self {
            effects: RefCell::new(effects),
        }
    }

    pub(super) fn take(&self) -> VecDeque<OxideWorkspaceEffect> {
        std::mem::take(&mut *self.effects.borrow_mut())
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OxideClientStateSnapshot {
    #[serde(default)]
    last_export_timestamp: Option<i64>,
}

impl SessionManagerState {
    pub(super) fn initialize_oxide_delivery(&mut self, cx: &mut Context<Self>) {
        let (sender, receiver) = delivery::ActiveDeliverySender::channel();
        let wake = sender.wake();
        let release_wake = wake.clone();
        cx.on_release(move |_, _| {
            // Import/export workers may outlive the window, but their waiter
            // must stop with the Entity that owns the delivery receiver.
            release_wake.stop();
        })
        .detach();
        let task_wake = wake.clone();
        let delivery_task = cx.spawn(async move |session_manager, cx| {
            loop {
                task_wake.wait().await;
                let should_drain = task_wake.take();
                let stopped = task_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = session_manager
                        .update(cx, |session_manager, cx| {
                            session_manager.drain_oxide_deliveries(cx)
                        })
                        .unwrap_or(false);
                    if backlog_remaining {
                        task_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        });
        self.oxide_worker_tx = Some(sender);
        self.oxide_worker_rx = Some(receiver);
        self._oxide_delivery_task = Some(delivery_task);
    }

    fn oxide_worker_sender(&self) -> Option<delivery::ActiveDeliverySender<OxideWorkerDelivery>> {
        self.oxide_worker_tx.clone()
    }

    fn drain_oxide_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.oxide_worker_rx.as_ref() else {
            return false;
        };
        let batch = delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut effects = VecDeque::new();
        let changed = !batch.items.is_empty();
        for message in batch.items {
            match message {
                OxideWorkerDelivery::PreviewProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_import_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::ImportProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_import_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::ExportProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_export_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::PreviewDone {
                    generation,
                    result,
                    password,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Preview(generation));
                    if self
                        .oxide_import_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.operation_generation == generation)
                    {
                        effects.push_back(OxideWorkspaceEffect::PreviewDone {
                            generation,
                            result,
                            password,
                        });
                    }
                }
                OxideWorkerDelivery::ImportDone {
                    generation,
                    result,
                    options,
                    password,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Import(generation));
                    if self
                        .oxide_import_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.operation_generation == generation)
                    {
                        effects.push_back(OxideWorkspaceEffect::ImportDone {
                            generation,
                            result,
                            options,
                            password,
                        });
                    }
                }
                OxideWorkerDelivery::ExportDone {
                    generation,
                    result,
                    password,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Export(generation));
                    if self
                        .oxide_export_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.operation_generation == generation)
                    {
                        effects.push_back(OxideWorkspaceEffect::ExportDone {
                            generation,
                            result,
                            password,
                        });
                    }
                }
            }
        }
        if !effects.is_empty() {
            cx.emit(SessionManagerWorkspaceEvent::OxideEffectsReady(
                OxideWorkspaceEffects::new(effects),
            ));
        }
        if changed {
            cx.notify();
        }
        batch.outcome.backlog_remaining
    }

    fn retain_oxide_worker(&mut self, key: OxideWorkerKey, worker: std::thread::JoinHandle<()>) {
        if let Some(previous) = self.oxide_worker_threads.insert(key, worker) {
            // A generation key cannot be reused while its operation is active.
            // Reap a completed predecessor defensively without retaining it.
            let _ = previous.join();
        }
    }

    fn reap_oxide_worker(&mut self, key: OxideWorkerKey) {
        if let Some(worker) = self.oxide_worker_threads.remove(&key) {
            // Completion is the worker's final send, so this join only closes
            // the tiny gap between channel delivery and thread return.
            let _ = worker.join();
        }
    }

    fn begin_import_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.oxide_import_dialog.as_mut() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_import_dialog_exit(generation, cx);
            return true;
        }
        self.import_dialog_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_import_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_import_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        self.oxide_import_dialog = None;
        self.import_dialog_exit_task = None;
        cx.notify();
        true
    }

    fn begin_export_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.oxide_export_dialog.as_mut() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_export_dialog_exit(generation, cx);
            return true;
        }
        self.export_dialog_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_export_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_export_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .oxide_export_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        self.oxide_export_dialog = None;
        self.export_dialog_exit_task = None;
        cx.notify();
        true
    }

    fn start_import_file_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<Result<(PathBuf, Arc<[u8]>), String>>>
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.import_file_picker_task.is_some() {
            return;
        }
        self.import_file_picker_task = Some(cx.spawn(async move |session_manager, cx| {
            let result = selection.await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.import_file_picker_task = None;
                let Some(result) = result else {
                    return;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return;
                };
                match result {
                    Ok((path, bytes)) => match OxideFile::from_bytes(bytes.as_ref()) {
                        Ok(file) => {
                            let metadata = file.metadata;
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
        }));
    }

    fn start_export_file_picker(
        &mut self,
        save: impl std::future::Future<Output = Option<Result<PathBuf, String>>> + 'static,
        settings_path: PathBuf,
        success_template: String,
        exported_count: usize,
        cx: &mut Context<Self>,
    ) {
        if self.export_file_picker_task.is_some() {
            return;
        }
        self.export_file_picker_task = Some(cx.spawn(async move |session_manager, cx| {
            let result = save.await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.export_file_picker_task = None;
                let Some(result) = result else {
                    if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                        dialog.busy = false;
                        dialog.progress_stage = None;
                    }
                    cx.notify();
                    return;
                };
                match result {
                    Ok(path) => {
                        let _ = persist_oxide_last_export_timestamp(&settings_path);
                        let summary = success_template
                            .replace("{{count}}", &exported_count.to_string())
                            .replace("{{path}}", path.to_string_lossy().as_ref());
                        session_manager.status = Some(summary);
                        session_manager.oxide_export_dialog = None;
                        session_manager.focused_input = None;
                    }
                    Err(error) => {
                        if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            dialog.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn schedule_import_auto_close(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self
            .oxide_import_dialog
            .as_ref()
            .map(|dialog| dialog.operation_generation)
        else {
            return;
        };
        self.dialog_auto_close_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                let should_close =
                    session_manager
                        .oxide_import_dialog
                        .as_ref()
                        .is_some_and(|dialog| {
                            dialog.operation_generation == generation
                                && dialog.error.is_none()
                                && dialog.result_summary.is_some()
                        });
                if should_close {
                    session_manager.oxide_import_dialog = None;
                    session_manager.focused_input = None;
                }
                session_manager.dialog_auto_close_task = None;
                cx.notify();
            });
        }));
    }
}

impl WorkspaceApp {
    pub(super) fn begin_oxide_import_dialog_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.begin_import_dialog_exit(delay, cx)
        })
    }

    pub(super) fn begin_oxide_export_dialog_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.begin_export_dialog_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn open_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.oxide_import_dialog = Some(OxideImportDialogState::default());
            session_manager.focused_input = None;
            session_manager.status = None;
            cx.notify();
        });
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
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.oxide_import_dialog = Some(dialog);
            session_manager.focused_input = None;
            session_manager.status = None;
            cx.notify();
        });
    }

    pub(in crate::workspace) fn open_oxide_export_dialog(&mut self, cx: &mut Context<Self>) {
        self.open_oxide_export_dialog_with_portable_mode(false, cx);
    }

    pub(in crate::workspace) fn active_session_manager_input(
        &self,
        cx: &App,
    ) -> Option<SessionManagerInput> {
        let input = self.session_manager.read(cx).focused_input?;
        let session_manager_tab_active = self
            .active_tab()
            .is_some_and(|tab| tab.kind == oxideterm_workspace::TabKind::SessionManager);
        let settings = self.settings_store.settings();
        let saved_connections_sidebar_active = !self.sidebar_collapsed
            && !settings.sidebar_ui.zen_mode
            && self.effective_sidebar_panel_section() == SidebarSection::Connections;
        let session_manager = self.session_manager.read(cx);
        session_manager_input_is_active(
            input,
            session_manager_tab_active,
            saved_connections_sidebar_active,
            session_manager.oxide_import_dialog.as_ref(),
            session_manager.oxide_export_dialog.as_ref(),
        )
        .then_some(input)
    }

    pub(in crate::workspace) fn focused_oxide_dialog_input(
        &self,
        cx: &App,
    ) -> Option<SessionManagerInput> {
        let session_manager = self.session_manager.read(cx);
        let input = session_manager.focused_input?;
        session_manager_input_is_active(
            input,
            false,
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
        let mut dialog = OxideExportDialogState::default();
        dialog.connection_rows = self
            .connection_store
            .connections()
            .iter()
            .map(OxideExportConnectionRow::from)
            .collect::<Vec<_>>()
            .into();
        dialog.include_portable_secrets = portable_migration;
        dialog.embed_keys = portable_migration;
        dialog.include_managed_key_passphrases = portable_migration;
        dialog.available_forwards = self.exportable_saved_forwards();
        dialog.forward_group_rows = oxide_export_forward_group_rows(
            self.connection_store.connections(),
            &dialog.available_forwards,
        );
        dialog.last_export_timestamp = load_oxide_last_export_timestamp(self.settings_store.path());
        dialog.selected_forward_ids = dialog
            .available_forwards
            .iter()
            .map(|forward| forward.id.clone())
            .collect();
        let plugin_settings =
            oxideterm_cloud_sync::plugin_settings::load_plugin_settings(self.settings_store.path())
                .unwrap_or_default();
        for setting in plugin_settings {
            if let Some(plugin_id) = plugin_id_from_setting_storage_key(&setting.storage_key) {
                *dialog.plugin_groups.entry(plugin_id).or_insert(0) += 1;
            }
        }
        dialog.selected_plugin_ids = dialog.plugin_groups.keys().cloned().collect();
        dialog.preflight = self.oxide_export_preflight_for_dialog(&dialog);
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.oxide_export_dialog = Some(dialog);
            session_manager.focused_input = None;
            session_manager.status = None;
            cx.notify();
        });
    }

    pub(in crate::workspace) fn handle_oxide_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }

        let (has_import_dialog, import_exiting, has_export_dialog, export_exiting) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.oxide_import_dialog.is_some(),
                session_manager
                    .oxide_import_dialog
                    .as_ref()
                    .is_some_and(|dialog| {
                        dialog.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
                    }),
                session_manager.oxide_export_dialog.is_some(),
                session_manager
                    .oxide_export_dialog
                    .as_ref()
                    .is_some_and(|dialog| {
                        dialog.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
                    }),
            )
        };
        if has_import_dialog {
            if import_exiting {
                return true;
            }
            return self.handle_oxide_import_footer_key(event, cx);
        }
        if has_export_dialog {
            if export_exiting {
                return true;
            }
            return self.handle_oxide_export_footer_key(event, cx);
        }
        false
    }

    pub(in crate::workspace) fn handle_oxide_import_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let exiting = self
            .session_manager
            .read(cx)
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| {
                dialog.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
            });
        if exiting {
            true
        } else {
            self.handle_oxide_import_footer_key(event, cx)
        }
    }

    pub(in crate::workspace) fn handle_oxide_export_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let exiting = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .is_some_and(|dialog| {
                dialog.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
            });
        if exiting {
            true
        } else {
            self.handle_oxide_export_footer_key(event, cx)
        }
    }

    pub(super) fn handle_oxide_import_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((actions, body_inputs, current_input, focused_footer_action)) = ({
            let session_manager = self.session_manager.read(cx);
            session_manager
                .oxide_import_dialog
                .as_ref()
                .and_then(|dialog| {
                    if dialog.busy {
                        return None;
                    }
                    let actions = oxide_import_footer_actions(dialog);
                    (!actions.is_empty()).then(|| {
                        let body_inputs = oxide_import_footer_body_inputs(dialog);
                        let current_input = session_manager
                            .focused_input
                            .filter(|focused| body_inputs.contains(focused));
                        (
                            actions,
                            body_inputs,
                            current_input,
                            dialog.focused_footer_action,
                        )
                    })
                })
        }) else {
            return false;
        };

        match browser_behavior::modal_footer_body_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &actions,
            focused_footer_action,
            body_inputs,
            current_input,
            actions[0],
            None,
        ) {
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Cancel) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.oxide_import_dialog = None;
                    session_manager.focused_input = None;
                    cx.notify();
                });
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusInput(input)) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.focused_input = Some(input);
                    if let Some(dialog) = session_manager.oxide_import_dialog.as_mut() {
                        dialog.focused_footer_action = None;
                    }
                    cx.notify();
                });
                self.ime_marked_text = None;
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusFooter(action)) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    if let Some(dialog) = session_manager.oxide_import_dialog.as_mut() {
                        dialog.focused_footer_action = Some(action);
                    }
                    session_manager.focused_input = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Activate(action)) => {
                self.activate_oxide_import_footer_action(action, cx);
                true
            }
            None => false,
        }
    }

    pub(super) fn activate_oxide_import_footer_action(
        &mut self,
        action: OxideDialogFooterAction,
        cx: &mut Context<Self>,
    ) {
        let Some((preview_ready, result_ready, has_selected_content, has_password)) = ({
            let session_manager = self.session_manager.read(cx);
            session_manager.oxide_import_dialog.as_ref().map(|dialog| {
                (
                    dialog.preview.is_some(),
                    dialog.result.is_some(),
                    oxide_import_has_selected_content(dialog),
                    !dialog.password.is_empty(),
                )
            })
        }) else {
            return;
        };
        match action {
            OxideDialogFooterAction::Cancel => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.oxide_import_dialog = None;
                    session_manager.focused_input = None;
                    cx.notify();
                });
            }
            OxideDialogFooterAction::Secondary if preview_ready => {
                self.session_manager.update(cx, |session_manager, cx| {
                    if let Some(dialog) = session_manager.oxide_import_dialog.as_mut() {
                        dialog.preview = None;
                        dialog.result_summary = None;
                        dialog.focused_footer_action = Some(OxideDialogFooterAction::Secondary);
                    }
                    cx.notify();
                });
            }
            OxideDialogFooterAction::Secondary => self.select_oxide_import_file(cx),
            OxideDialogFooterAction::Primary if result_ready => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.oxide_import_dialog = None;
                    session_manager.focused_input = None;
                    cx.notify();
                });
            }
            OxideDialogFooterAction::Primary if preview_ready => {
                if has_selected_content {
                    self.apply_oxide_import_dialog(cx);
                } else {
                    cx.notify();
                }
            }
            OxideDialogFooterAction::Primary => {
                if has_password {
                    self.preview_oxide_import_dialog(cx);
                } else {
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn handle_oxide_export_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let actions = [
            OxideDialogFooterAction::Cancel,
            OxideDialogFooterAction::Primary,
        ];
        let Some((body_inputs, current_input, focused_footer_action, has_selected_content)) = ({
            let session_manager = self.session_manager.read(cx);
            session_manager
                .oxide_export_dialog
                .as_ref()
                .and_then(|dialog| {
                    (!dialog.busy).then(|| {
                        let body_inputs = oxide_export_footer_body_inputs(dialog);
                        let current_input = session_manager
                            .focused_input
                            .filter(|focused| body_inputs.contains(focused));
                        (
                            body_inputs,
                            current_input,
                            dialog.focused_footer_action,
                            oxide_export_has_selected_content(dialog),
                        )
                    })
                })
        }) else {
            return false;
        };
        match browser_behavior::modal_footer_body_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &actions,
            focused_footer_action,
            body_inputs,
            current_input,
            OxideDialogFooterAction::Cancel,
            None,
        ) {
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Cancel) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.oxide_export_dialog = None;
                    session_manager.focused_input = None;
                    cx.notify();
                });
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusInput(input)) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.focused_input = Some(input);
                    if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                        dialog.focused_footer_action = None;
                    }
                    cx.notify();
                });
                self.ime_marked_text = None;
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::FocusFooter(action)) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                        dialog.focused_footer_action = Some(action);
                    }
                    session_manager.focused_input = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                true
            }
            Some(browser_behavior::ModalFooterBodyInputKeyAction::Activate(action)) => {
                match action {
                    OxideDialogFooterAction::Cancel => {
                        self.session_manager.update(cx, |session_manager, cx| {
                            session_manager.oxide_export_dialog = None;
                            session_manager.focused_input = None;
                            cx.notify();
                        });
                    }
                    OxideDialogFooterAction::Primary => {
                        if has_selected_content {
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

    pub(super) fn exportable_saved_forwards(&self) -> Vec<PersistedForward> {
        let connection_ids = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .collect::<HashSet<_>>();
        let mut forwards_by_key = HashMap::<String, PersistedForward>::new();

        for forward in self.forwarding_service.registry().list_all_saved_forwards() {
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

    pub(super) fn select_oxide_import_file(&mut self, cx: &mut Context<Self>) {
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
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.start_import_file_picker(selection, cx);
        });
    }

    pub(super) fn preview_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        let missing_file_error = self.i18n.t("modals.import.select_file");
        let missing_password_error = self.i18n.t("modals.import.error_enter_password");
        let Some((bytes, password, conflict_strategy, generation, sender)) =
            self.session_manager.update(cx, |session_manager, cx| {
                let Some(sender) = session_manager.oxide_worker_sender() else {
                    return None;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return None;
                };
                let Some(bytes) = dialog.file_data.clone() else {
                    dialog.error = Some(missing_file_error);
                    cx.notify();
                    return None;
                };
                if dialog.password.is_empty() {
                    dialog.error = Some(missing_password_error);
                    cx.notify();
                    return None;
                }
                // Move the sole password owner into the worker. The completion
                // delivery returns this same zeroizing allocation to the dialog.
                let password = std::mem::take(&mut dialog.password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 8));
                dialog.error = None;
                cx.notify();
                Some((
                    bytes,
                    password,
                    dialog.conflict_strategy,
                    dialog.operation_generation,
                    sender,
                ))
            })
        else {
            return;
        };
        let store = self.connection_store.clone();
        let worker = std::thread::spawn(move || {
            let result = preview_oxide_import_with_progress(
                &store,
                bytes.as_ref(),
                &password,
                conflict_strategy,
                |stage, current, total| {
                    let _ = sender.send(OxideWorkerDelivery::PreviewProgress {
                        generation,
                        progress: OxideTransferProgress::new(stage, current, total),
                    });
                },
            );
            let _ = sender.send(OxideWorkerDelivery::PreviewDone {
                generation,
                result,
                password,
            });
        });
        self.session_manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Preview(generation), worker);
        });
    }

    pub(super) fn apply_oxide_import_dialog(&mut self, cx: &mut Context<Self>) {
        let missing_file_error = self.i18n.t("modals.import.select_file");
        let missing_password_error = self.i18n.t("modals.import.error_enter_password");
        let Some((bytes, password, options, generation, sender)) =
            self.session_manager.update(cx, |session_manager, cx| {
                let Some(sender) = session_manager.oxide_worker_sender() else {
                    return None;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return None;
                };
                let Some(bytes) = dialog.file_data.clone() else {
                    dialog.error = Some(missing_file_error);
                    cx.notify();
                    return None;
                };
                if dialog.password.is_empty() {
                    dialog.error = Some(missing_password_error);
                    cx.notify();
                    return None;
                }
                let options = OxideClientStateImportOptions {
                    oxide_options: OxideImportOptions {
                        selected_names: Some(dialog.selected_names.iter().cloned().collect()),
                        selected_forward_ids: None,
                        conflict_strategy: dialog.conflict_strategy,
                        import_forwards: dialog.import_forwards,
                        import_serial_profiles: dialog.import_serial_profiles,
                        import_portable_secrets: dialog.import_portable_secrets,
                        restore_managed_keys: dialog.restore_managed_keys,
                        restore_managed_key_passphrases: dialog.restore_managed_key_passphrases,
                        ..OxideImportOptions::default()
                    },
                    import_quick_commands: dialog.import_quick_commands,
                    quick_command_strategy: quick_command_strategy_from_oxide(
                        dialog.conflict_strategy,
                    ),
                    import_plugin_settings: dialog.import_plugin_settings,
                    selected_plugin_ids: Some(dialog.selected_plugin_ids.clone()),
                    import_app_settings: dialog.import_app_settings
                        && !dialog.selected_app_settings_sections.is_empty(),
                    selected_app_settings_sections: Some(
                        dialog.selected_app_settings_sections.clone(),
                    ),
                };
                // The worker becomes the only password owner until completion.
                let password = std::mem::take(&mut dialog.password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 10));
                dialog.error = None;
                cx.notify();
                Some((
                    bytes,
                    password,
                    options,
                    dialog.operation_generation,
                    sender,
                ))
            })
        else {
            return;
        };
        let mut store = self.connection_store.clone();
        let oxide_options = options.oxide_options.clone();
        let worker = std::thread::spawn(move || {
            let result = apply_oxide_import_with_options_with_progress(
                &mut store,
                bytes.as_ref(),
                &password,
                oxide_options,
                |stage, current, total| {
                    let _ = sender.send(OxideWorkerDelivery::ImportProgress {
                        generation,
                        progress: OxideTransferProgress::new(stage, current, total),
                    });
                },
            )
            .map(|envelope| OxideCoreImportResult { store, envelope });
            let _ = sender.send(OxideWorkerDelivery::ImportDone {
                generation,
                result,
                options,
                password,
            });
        });
        self.session_manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Import(generation), worker);
        });
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
        }
    }

    fn handle_oxide_workspace_effect(
        &mut self,
        effect: OxideWorkspaceEffect,
        cx: &mut Context<Self>,
    ) {
        match effect {
            OxideWorkspaceEffect::PreviewDone {
                generation,
                result,
                password,
            } => {
                let result = result.map_err(|error| oxide_file_error_message(error, &self.i18n));
                self.session_manager.update(cx, |session_manager, cx| {
                    let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                        return;
                    };
                    if dialog.operation_generation != generation {
                        return;
                    }
                    // Restore the exact UI draft owner returned by the worker.
                    dialog.password = password;
                    dialog.busy = false;
                    dialog.progress_stage = None;
                    match result {
                        Ok(preview) => {
                            dialog.selected_names = preview
                                .records
                                .iter()
                                .map(|record| record.name.clone())
                                .collect();
                            if !preview.app_settings_section_ids.is_empty() {
                                dialog.selected_app_settings_sections =
                                    preview.app_settings_section_ids.iter().cloned().collect();
                            }
                            dialog.import_app_settings = preview.has_app_settings;
                            dialog.import_quick_commands = preview.has_quick_commands;
                            dialog.import_serial_profiles = preview.serial_profiles_count > 0;
                            dialog.import_plugin_settings = preview.plugin_settings_count > 0;
                            dialog.import_forwards = preview.total_forwards > 0;
                            dialog.import_portable_secrets = false;
                            dialog.selected_plugin_ids =
                                preview.plugin_settings_by_plugin.keys().cloned().collect();
                            dialog.expanded_app_settings_sections.clear();
                            dialog.result = None;
                            dialog.result_summary = None;
                            dialog.preview = Some(Arc::new(preview));
                            dialog.error = None;
                        }
                        Err(error) => dialog.error = Some(error),
                    }
                    cx.notify();
                });
            }
            OxideWorkspaceEffect::ImportDone {
                generation,
                result,
                options,
                password,
            } => {
                let still_current = self
                    .session_manager
                    .read(cx)
                    .oxide_import_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.operation_generation == generation);
                if !still_current {
                    return;
                }
                match result {
                    Ok(core) => {
                        // A successful import no longer needs the decryption password.
                        drop(password);
                        self.finish_oxide_import_core_result(core, options, cx);
                    }
                    Err(error) => {
                        let error = oxide_file_error_message(error, &self.i18n);
                        self.session_manager.update(cx, |session_manager, cx| {
                            let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                                return;
                            };
                            if dialog.operation_generation != generation {
                                return;
                            }
                            dialog.password = password;
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            dialog.error = Some(error);
                            cx.notify();
                        });
                    }
                }
            }
            OxideWorkspaceEffect::ExportDone {
                generation,
                result,
                password,
            } => match result {
                Ok(bytes) => {
                    drop(password);
                    let exported_count = self.session_manager.update(cx, |session_manager, cx| {
                        let Some(dialog) = session_manager.oxide_export_dialog.as_mut() else {
                            return None;
                        };
                        if dialog.operation_generation != generation {
                            return None;
                        }
                        let exported_count = oxide_export_connection_count(dialog);
                        dialog.progress_stage = Some(OxideTransferProgress::new("writing", 1, 1));
                        cx.notify();
                        Some(exported_count)
                    });
                    if let Some(exported_count) = exported_count {
                        self.prompt_save_oxide_export(bytes, exported_count, cx);
                    }
                }
                Err(error) => {
                    let error = oxide_file_error_message(error, &self.i18n);
                    self.session_manager.update(cx, |session_manager, cx| {
                        let Some(dialog) = session_manager.oxide_export_dialog.as_mut() else {
                            return;
                        };
                        if dialog.operation_generation != generation {
                            return;
                        }
                        dialog.password = password;
                        dialog.busy = false;
                        dialog.progress_stage = None;
                        dialog.error = Some(error);
                        cx.notify();
                    });
                }
            },
        }
    }

    pub(super) fn finish_oxide_import_core_result(
        &mut self,
        core: OxideCoreImportResult,
        options: OxideClientStateImportOptions,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn present_oxide_import_result(
        &mut self,
        result: OxideClientStateImportResult,
        cx: &mut Context<Self>,
    ) {
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
            parts.push(format!(
                "{} 条快捷命令",
                result_view.imported_quick_commands
            ));
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
        let result_error = (!result_view.errors.is_empty()).then(|| result_view.errors.join("; "));
        let result_summary = parts.join(" · ");
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                return;
            };
            dialog.busy = false;
            dialog.progress_stage = None;
            dialog.error = result_error;
            dialog.result_summary = Some(result_summary.clone());
            dialog.result = Some(Arc::new(result_view));
            session_manager.status = Some(result_summary);
            if auto_close_import_dialog {
                session_manager.schedule_import_auto_close(Duration::from_secs(2), cx);
            }
            cx.notify();
        });
    }

    pub(super) fn oxide_export_connection_ids(
        &self,
        dialog: &OxideExportDialogState,
    ) -> HashSet<String> {
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

    pub(super) fn oxide_export_has_content(&self, dialog: &OxideExportDialogState) -> bool {
        !self.oxide_export_connection_ids(dialog).is_empty()
            || (dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty())
            || dialog.include_quick_commands
            || dialog.include_serial_profiles
            || dialog.include_remote_desktop_profiles
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
            .filter(|provider| self.ai.models.key_store.has_provider_key(&provider.id))
            .count()
    }

    pub(super) fn oxide_export_preflight(
        &self,
        dialog: &OxideExportDialogState,
    ) -> ExportPreflightResult {
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

    pub(super) fn refresh_oxide_export_preflight(&mut self, cx: &mut Context<Self>) {
        let Some(preflight) = ({
            let session_manager = self.session_manager.read(cx);
            session_manager
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| self.oxide_export_preflight_for_dialog(dialog))
        }) else {
            return;
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                dialog.preflight = preflight;
                cx.notify();
            }
        });
    }

    pub(super) fn oxide_export_preflight_for_dialog(
        &self,
        dialog: &OxideExportDialogState,
    ) -> Option<ExportPreflightResult> {
        let has_preflight_content =
            !self.oxide_export_connection_ids(dialog).is_empty() || dialog.include_portable_secrets;
        has_preflight_content.then(|| self.oxide_export_preflight(dialog))
    }

    pub(super) fn export_oxide_dialog(&mut self, cx: &mut Context<Self>) {
        let validation = {
            let session_manager = self.session_manager.read(cx);
            let Some(dialog) = session_manager.oxide_export_dialog.as_ref() else {
                return;
            };
            if !self.oxide_export_has_content(dialog) {
                Err(self.i18n.t("export.error_select_something"))
            } else if dialog.password.len() < 6 {
                Err(self.i18n.t("export.error_password_too_short"))
            } else if dialog.password != dialog.confirm_password {
                Err(self.i18n.t("export.error_password_mismatch"))
            } else if dialog
                .preflight
                .as_ref()
                .is_some_and(|preflight| !preflight.can_export)
            {
                Err(self.i18n.t("export.error_managed_keys_required"))
            } else {
                let selected_ids = self
                    .oxide_export_connection_ids(dialog)
                    .into_iter()
                    .collect::<Vec<_>>();
                let preflight = self.oxide_export_preflight(dialog);
                self.build_oxide_export_options(dialog)
                    .map(|options| (selected_ids, preflight, options))
            }
        };
        let (selected_ids, preflight, options) = match validation {
            Ok(request) => request,
            Err(error) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                        dialog.error = Some(error);
                        cx.notify();
                    }
                });
                return;
            }
        };
        let Some((password, generation, sender)) =
            self.session_manager.update(cx, |session_manager, cx| {
                let sender = session_manager.oxide_worker_sender()?;
                let dialog = session_manager.oxide_export_dialog.as_mut()?;
                // Both drafts are zeroizing owners. Move them out directly so
                // no plaintext password copy survives the submission boundary.
                let password = std::mem::take(&mut dialog.password);
                let confirm_password = std::mem::take(&mut dialog.confirm_password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage =
                    Some(OxideTransferProgress::new("collecting_connections", 0, 1));
                dialog.error = None;
                dialog.preflight = Some(preflight);
                drop(confirm_password);
                cx.notify();
                Some((password, dialog.operation_generation, sender))
            })
        else {
            return;
        };
        let store = self.connection_store.clone();
        let worker = std::thread::spawn(move || {
            let result = export_connections_to_oxide_with_progress(
                &store,
                &selected_ids,
                &password,
                options,
                |stage, current, total| {
                    let _ = sender.send(OxideWorkerDelivery::ExportProgress {
                        generation,
                        progress: OxideTransferProgress::new(stage, current, total),
                    });
                },
            );
            let _ = sender.send(OxideWorkerDelivery::ExportDone {
                generation,
                result,
                password,
            });
        });
        self.session_manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Export(generation), worker);
        });
    }

    pub(super) fn prompt_save_oxide_export(
        &mut self,
        bytes: Vec<u8>,
        exported_count: usize,
        cx: &mut Context<Self>,
    ) {
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
            match receiver.await {
                Ok(Ok(Some(path))) => Some(
                    fs::write(&path, bytes)
                        .map(|_| path)
                        .map_err(|error| error.to_string()),
                ),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => Some(Err(error.to_string())),
                Err(error) => Some(Err(error.to_string())),
            }
        };
        let settings_path = self.settings_store.path().to_path_buf();
        let success_template = self.i18n.t("export.success");
        self.session_manager.update(cx, |session_manager, cx| {
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
            Some(oxideterm_quick_commands::export_snapshot_json(
                self.settings_store.path(),
            )?)
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
        let remote_desktop_profiles_json = if dialog.include_remote_desktop_profiles {
            Some(
                serde_json::to_string_pretty(
                    &self
                        .connection_store
                        .export_remote_desktop_profiles_snapshot()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
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
            let provider_ids =
                oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
                    .into_iter()
                    .map(|provider| provider.id)
                    .filter(|provider_id| self.ai.models.key_store.has_provider_key(provider_id))
                    .collect::<Vec<_>>();
            self.ai
                .models
                .key_store
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
            remote_desktop_profiles_json,
            plugin_settings,
            portable_secrets,
            forwards,
            ..OxideExportOptions::default()
        })
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
        oxideterm_cloud_sync::plugin_settings::upsert_plugin_settings(
            self.settings_store.path(),
            &filtered,
        )
        .unwrap_or(0)
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
                self.edit_settings(|settings| *settings = merged, cx);
                (true, false)
            }
            Err(error) => {
                self.session_manager.update(cx, |session_manager, cx| {
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
                .ai
                .models
                .key_store
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

pub(super) fn oxide_forward_export_identity(forward: &PersistedForward) -> String {
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

pub(super) fn quick_command_strategy_from_oxide(
    strategy: ImportConflictStrategy,
) -> QuickCommandImportStrategy {
    match strategy {
        ImportConflictStrategy::Rename => QuickCommandImportStrategy::Rename,
        ImportConflictStrategy::Skip => QuickCommandImportStrategy::Skip,
        ImportConflictStrategy::Replace => QuickCommandImportStrategy::Replace,
        ImportConflictStrategy::Merge => QuickCommandImportStrategy::Merge,
    }
}

pub(super) fn oxide_file_error_message(
    error: OxideFileError,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    match error {
        OxideFileError::DecryptionFailed => i18n.t("modals.import.error_password"),
        OxideFileError::ChecksumMismatch => i18n.t("modals.import.error_tampered"),
        OxideFileError::PasswordTooShort => i18n.t("export.error_password_too_short"),
        other => other.to_string(),
    }
}

pub(super) fn persist_oxide_last_export_timestamp(
    settings_path: &std::path::Path,
) -> Result<(), String> {
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

pub(super) fn load_oxide_last_export_timestamp(settings_path: &std::path::Path) -> Option<i64> {
    let path = oxide_client_state_path(settings_path);
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<OxideClientStateSnapshot>(&contents)
        .ok()
        .and_then(|snapshot| snapshot.last_export_timestamp)
}

pub(super) fn oxide_client_state_path(settings_path: &std::path::Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join("oxide-client-state.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn oxide_delivery_updates_progress_without_a_workspace(cx: &mut TestAppContext) {
        let session_manager = cx.new(SessionManagerState::new);
        let sender = session_manager.read_with(cx, |session_manager, _cx| {
            session_manager
                .oxide_worker_sender()
                .expect("oxide worker sender")
        });
        session_manager.update(cx, |session_manager, _cx| {
            let mut dialog = OxideImportDialogState::default();
            dialog.busy = true;
            dialog.operation_generation = 7;
            session_manager.oxide_import_dialog = Some(dialog);
        });

        sender
            .send(OxideWorkerDelivery::PreviewProgress {
                generation: 7,
                progress: OxideTransferProgress::new("decrypting", 2, 4),
            })
            .expect("preview progress delivery");

        // The Entity drains progress even when no WorkspaceApp or page is mounted.
        cx.run_until_parked();

        session_manager.read_with(cx, |session_manager, _cx| {
            let progress = session_manager
                .oxide_import_dialog
                .as_ref()
                .and_then(|dialog| dialog.progress_stage.as_ref())
                .expect("oxide progress");
            assert_eq!(progress.current, 2);
            assert_eq!(progress.total, 4);
        });
    }

    #[test]
    fn oxide_effect_batch_preserves_secret_owner() {
        let password = zeroize::Zeroizing::new("delivery-secret".to_string());
        let password_allocation = password.as_ptr();
        let effects =
            OxideWorkspaceEffects::new(VecDeque::from([OxideWorkspaceEffect::PreviewDone {
                generation: 7,
                result: Err(OxideFileError::DecryptionFailed),
                password,
            }]));
        match effects.take().pop_front().expect("oxide effect") {
            OxideWorkspaceEffect::PreviewDone { password, .. } => {
                // Moving the zeroizing owner through the worker channel must not
                // allocate a second plaintext password buffer.
                assert_eq!(password.as_ptr(), password_allocation);
            }
            _ => panic!("unexpected oxide effect"),
        }
    }
}
