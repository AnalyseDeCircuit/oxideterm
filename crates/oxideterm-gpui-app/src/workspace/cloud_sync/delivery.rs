use super::*;

impl WorkspaceApp {
    pub(super) fn with_cloud_sync_runtime<R>(
        &mut self,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut CloudSyncRuntime<'_>, &mut App) -> R,
    ) -> R {
        let ai_keys = self.ai.state.read(cx).key_store().clone();
        let mut runtime = CloudSyncRuntime {
            cloud_sync: self.cloud_sync.clone(),
            connection_store: &mut self.connection_store,
            settings_store: &mut self.settings_store,
            forwarding_registry: self.forwarding_service.registry(),
            forwarding_runtime: self.forwarding_runtime.handle(),
            i18n: &self.i18n,
            tokens: &self.tokens,
            ai_keys,
            file_dialog: local_file::cloud_sync_file_dialog,
            effects: Vec::new(),
        };
        let result = run(&mut runtime, cx);
        for effect in runtime.effects {
            match effect {
                CloudSyncHostEffect::Notice {
                    title,
                    description,
                    kind,
                } => {
                    self.push_workspace_notice(
                        TerminalNotice {
                            title,
                            description,
                            status_text: None,
                            progress: None,
                            variant: match kind {
                                CloudSyncNoticeKind::Default => TerminalNoticeVariant::Default,
                                CloudSyncNoticeKind::Success => TerminalNoticeVariant::Success,
                                CloudSyncNoticeKind::Error => TerminalNoticeVariant::Error,
                            },
                        },
                        cx,
                    );
                }
                CloudSyncHostEffect::RefreshOwners(previous) => {
                    self.refresh_causal_sync_owners(&previous, cx)
                }
                CloudSyncHostEffect::ApplyPreview(result) => {
                    self.finish_cloud_sync_apply_preview(*result, cx)
                }
                CloudSyncHostEffect::AnimateTab(tab) => {
                    self.begin_user_segmented_control_transition(
                        selection_motion::CLOUD_SYNC_SWITCHER_ID,
                        cloud_sync_tab_index(tab),
                        cx,
                    );
                }
                CloudSyncHostEffect::OpenUrl(url) => cx.open_url(&url),
            }
        }
        result
    }

    pub(in crate::workspace) fn handle_cloud_sync_workspace_event(
        &mut self,
        event: &CloudSyncWorkspaceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CloudSyncWorkspaceEvent::DeliveriesReady => {
                let (deliveries, disconnected) =
                    self.cloud_sync.update(cx, |sync, _| sync.take_deliveries());
                // Finish each import before the next delivery, including the disconnect
                // that can start a follow-up upload.
                for delivery in deliveries {
                    self.with_cloud_sync_runtime(cx, |sync, cx| {
                        sync.handle_cloud_sync_delivery(delivery, cx)
                    });
                }
                self.with_cloud_sync_runtime(cx, |sync, cx| {
                    sync.finish_delivery_batch(disconnected, cx)
                });
            }
            CloudSyncWorkspaceEvent::UiIntent(intent) => {
                self.handle_cloud_sync_window_intent(intent, window, cx)
            }
            _ => self.with_cloud_sync_runtime(cx, |sync, cx| sync.handle_event(event, cx)),
        }
    }

    fn handle_cloud_sync_window_intent(
        &mut self,
        intent: &CloudSyncUiIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match intent {
            CloudSyncUiIntent::BeginInputSelection {
                input,
                event,
                source_window,
            } => {
                if *source_window == window.window_handle() {
                    let current_value = self.current_settings_input_value(*input, cx);
                    // Pointer selection requires an active IME owner. Cloud Sync
                    // fields move their draft into the root adapter at focus time.
                    self.focus_settings_input(*input, current_value, cx);
                    window.focus(&self.focus_handle, cx);
                    self.begin_ime_selection_from_mouse_down(
                        WorkspaceImeTarget::Settings(*input),
                        event,
                        window,
                        cx,
                    );
                }
            }
            CloudSyncUiIntent::UpdateInputSelection {
                event,
                source_window,
            } => {
                if *source_window == window.window_handle() {
                    self.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }
            }
            CloudSyncUiIntent::UpdateInputAnchor {
                anchor,
                source_window,
            } => {
                if *source_window == window.window_handle() {
                    self.update_text_input_anchor(*anchor, cx);
                }
            }
            CloudSyncUiIntent::UpdateSelectAnchor {
                anchor,
                source_window,
            } => {
                if *source_window == window.window_handle() {
                    self.update_select_anchor(*anchor, cx);
                }
            }

            _ => {
                let entity = self.cloud_sync.read(cx);
                let leaving_config = matches!(intent, CloudSyncUiIntent::SelectTab { tab } if *tab != CloudSyncTab::Configure)
                    && entity.view.active_tab == CloudSyncTab::Configure;
                let persists = leaving_config
                    || matches!(
                        intent,
                        CloudSyncUiIntent::StartGithubOauth
                            | CloudSyncUiIntent::StartMicrosoftOauth
                            | CloudSyncUiIntent::StartGoogleOauth
                            | CloudSyncUiIntent::StartUploadPreview
                            | CloudSyncUiIntent::PullPreview
                            | CloudSyncUiIntent::ViewRemote
                            | CloudSyncUiIntent::SaveConfiguration
                    )
                    || (entity.view.local_file_mode
                        && matches!(intent, CloudSyncUiIntent::StartUpload));
                if persists && !entity.operation_in_flight() {
                    self.apply_focused_cloud_sync_input_draft(cx);
                }
                self.with_cloud_sync_runtime(cx, |sync, cx| {
                    sync.handle_event(&CloudSyncWorkspaceEvent::UiIntent(intent.clone()), cx)
                });
            }
        }
    }

    pub(in crate::workspace) fn bootstrap_cloud_sync_controller(&mut self, cx: &mut Context<Self>) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.bootstrap_cloud_sync_controller(cx));
    }

    pub(in crate::workspace) fn queue_cloud_sync_dirty_refresh(&mut self, cx: &mut Context<Self>) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.queue_cloud_sync_dirty_refresh(cx));
    }

    pub(in crate::workspace) fn reschedule_cloud_sync_auto_upload(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.cloud_sync
            .update(cx, |sync, cx| sync.reschedule_auto_upload(cx));
    }

    pub(in crate::workspace) fn refresh_cloud_sync_local_dirty_state(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.refresh_cloud_sync_local_dirty_state(cx));
    }

    pub(in crate::workspace) fn save_cloud_sync_state(&mut self, cx: &mut Context<Self>) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.save_cloud_sync_state(cx));
    }

    pub(in crate::workspace) fn start_cloud_sync_check_with_options(
        &mut self,
        skip_if_busy: bool,
        cx: &mut Context<Self>,
    ) {
        self.with_cloud_sync_runtime(cx, |sync, cx| {
            sync.start_cloud_sync_check_with_options(skip_if_busy, cx)
        });
    }

    pub(in crate::workspace) fn start_cloud_sync_upload_with_options(
        &mut self,
        force: bool,
        automatic: bool,
        skip_if_busy: bool,
        cx: &mut Context<Self>,
    ) {
        if !automatic
            && self.cloud_sync.read(cx).view.local_file_mode
            && !self.cloud_sync.read(cx).operation_in_flight()
        {
            self.apply_focused_cloud_sync_input_draft(cx);
        }
        self.with_cloud_sync_runtime(cx, |sync, cx| {
            sync.start_cloud_sync_upload_with_options(force, automatic, skip_if_busy, cx)
        });
    }

    pub(in crate::workspace) fn start_cloud_sync_upload_preview(&mut self, cx: &mut Context<Self>) {
        if !self.cloud_sync.read(cx).operation_in_flight() {
            self.apply_focused_cloud_sync_input_draft(cx);
        }
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.start_cloud_sync_upload_preview(cx));
    }

    pub(in crate::workspace) fn start_cloud_sync_pull_preview(&mut self, cx: &mut Context<Self>) {
        if !self.cloud_sync.read(cx).operation_in_flight() {
            self.apply_focused_cloud_sync_input_draft(cx);
        }
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.start_cloud_sync_pull_preview(cx));
    }

    pub(in crate::workspace) fn start_cloud_sync_pull_preview_with_options(
        &mut self,
        persist: bool,
        cx: &mut Context<Self>,
    ) {
        if persist && !self.cloud_sync.read(cx).operation_in_flight() {
            self.apply_focused_cloud_sync_input_draft(cx);
        }
        self.with_cloud_sync_runtime(cx, |sync, cx| {
            sync.start_cloud_sync_pull_preview_with_options(persist, cx)
        });
    }

    pub(in crate::workspace) fn start_cloud_sync_apply_preview(&mut self, cx: &mut Context<Self>) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.start_cloud_sync_apply_preview(cx));
    }

    pub(in crate::workspace) fn finish_cloud_sync_error(
        &mut self,
        action: &str,
        error: String,
        cx: &mut Context<Self>,
    ) {
        self.with_cloud_sync_runtime(cx, |sync, cx| {
            sync.finish_cloud_sync_error(action, error, cx)
        });
    }

    pub(in crate::workspace) fn handle_cloud_sync_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use oxideterm_gpui_ui::confirm::ModalFooterKeyAction;
        match self
            .cloud_sync_confirm
            .update(cx, |portal, cx| portal.handle_key(event, cx))
        {
            Some(ModalFooterKeyAction::Cancel) => {
                self.apply_cloud_sync_confirm_action(ConfirmDialogAction::Cancel, cx);
            }
            Some(ModalFooterKeyAction::Activate(action)) => {
                self.apply_cloud_sync_confirm_action(action, cx);
            }
            Some(ModalFooterKeyAction::Focus(_)) => {}
            None => return false,
        }
        true
    }

    pub(super) fn apply_cloud_sync_confirm_action(
        &mut self,
        action: ConfirmDialogAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            ConfirmDialogAction::Cancel => self.cancel_cloud_sync_confirm(cx),
            ConfirmDialogAction::Confirm => self.confirm_cloud_sync_confirm(cx),
        }
    }

    fn cancel_cloud_sync_confirm(&mut self, cx: &mut Context<Self>) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.cancel_cloud_sync_confirm(cx));
    }

    fn confirm_cloud_sync_confirm(&mut self, cx: &mut Context<Self>) {
        let sync = self.cloud_sync.read(cx);
        let persists = matches!(
            sync.view.confirm,
            Some(CloudSyncConfirm::RestoreBackup { .. })
        ) || (sync.view.local_file_mode
            && matches!(sync.view.confirm, Some(CloudSyncConfirm::ForceUpload)));
        if persists && !sync.operation_in_flight() {
            self.apply_focused_cloud_sync_input_draft(cx);
        }
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.confirm_cloud_sync_confirm(cx));
    }

    pub(in crate::workspace) fn refresh_causal_sync_owners(
        &mut self,
        previous: &oxideterm_settings::PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings().clone();
        self.apply_loaded_settings_to_runtime(previous, &settings, cx);
        self.ai.state.read(cx).key_store().clear_cache();
        self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.store.reload_from_store()
        });
        self.settings_workspace.update(cx, |settings, _| {
            settings.acknowledge_external_store_state()
        });
        self.sync_ssh_config_sync_service();
        self.reconcile_discovery_sources(cx);
        self.refresh_ai_skill_registry(cx);
        self.emit_native_plugin_settings_events(previous, &settings, cx);
        self.sync_tab_titles(cx);
        self.invalidate_cloud_sync_snapshot_caches(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn finish_causal_sync(
        &mut self,
        outcome: &oxideterm_cloud_sync::operation::SyncOutcome,
        cx: &mut Context<Self>,
    ) {
        self.with_cloud_sync_runtime(cx, |sync, cx| sync.finish_causal_sync(outcome, cx));
    }

    fn finish_cloud_sync_apply_preview(
        &mut self,
        mut ui_outcome: CloudSyncApplyUiOutcome,
        cx: &mut Context<Self>,
    ) {
        let legacy_import_audit = ui_outcome.legacy_import_audit.take();
        let previous_network = self.settings_store.settings().network.clone();
        self.connection_store = ui_outcome.connection_store;
        self.settings_store = ui_outcome.settings_store;
        if previous_network != self.settings_store.settings().network {
            oxideterm_network_proxy::install_application_proxy_policy_from_settings(
                self.settings_store.settings(),
                &self.connection_store,
            );
        }
        match ui_outcome.outcome {
            CloudSyncApplyOutcome::Structured(mut outcome) => {
                self.terminal.update(cx, |terminal, _| {
                    terminal.quick_commands.store.reload_from_store()
                });
                if let Some(envelope) = outcome.sensitive_credentials_envelope.as_mut() {
                    self.apply_oxide_import_portable_secrets(envelope, cx);
                }
                self.with_cloud_sync_runtime(cx, |sync, cx| {
                    sync.finish_structured_cloud_sync_apply(outcome, cx)
                });
            }
            CloudSyncApplyOutcome::Legacy {
                preview,
                source,
                selection,
                outcome,
            } => {
                self.apply_legacy_cloud_sync_stores(
                    preview,
                    source,
                    selection,
                    outcome,
                    legacy_import_audit,
                    cx,
                );
            }
        }
    }

    fn apply_legacy_cloud_sync_stores(
        &mut self,
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
        selection: CloudSyncPreviewSelection,
        outcome: ApplyLegacyPreviewOutcome,
        audit: Option<PendingLegacyImportAudit>,
        cx: &mut Context<Self>,
    ) {
        let plan = cloud_sync_legacy_apply_plan(&preview, &source, &selection);
        let cloud_options = &plan.import_options;
        let (sidecars, plugin_settings_failed) = self.import_oxide_sidecars(
            outcome.envelope,
            crate::workspace::session_manager::OxideSidecarOptions {
                forwards: cloud_options.oxide_options.import_forwards,
                quick_commands: selection
                    .import_quick_commands
                    .then_some(QuickCommandImportStrategy::Merge),
                plugin_settings: cloud_options.import_plugin_settings,
                selected_plugin_ids: cloud_options.selected_plugin_ids.as_ref(),
                app_settings: cloud_options.import_app_settings,
                selected_app_settings_sections: cloud_options
                    .selected_app_settings_sections
                    .as_ref(),
                portable_secrets: cloud_options.oxide_options.import_portable_secrets,
            },
            cx,
        );
        let stats = CloudSyncLegacyImportStats {
            quick_commands: sidecars.imported_quick_commands,
            plugin_settings: sidecars.imported_plugin_settings,
            app_settings: sidecars.imported_app_settings,
            quick_command_errors: sidecars.quick_commands_errors.len(),
            plugin_settings_failed,
            app_settings_failed: sidecars.skipped_app_settings,
        };
        self.with_cloud_sync_runtime(cx, |sync, cx| {
            sync.finish_legacy_cloud_sync_apply(
                preview,
                source,
                selection,
                plan,
                sidecars.envelope,
                stats,
                audit,
                cx,
            )
        });
    }
}
