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
        self.connections.session_manager.read(cx).active_input(
            self.keyboard_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::SessionManager),
            self.session_manager_sidebar_visible(),
        )
    }

    pub(in crate::workspace) fn focused_oxide_dialog_input(
        &self,
        cx: &App,
    ) -> Option<SessionManagerInput> {
        self.connections.session_manager.read(cx).active_input(false, false)
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
        self.oxide_transfer().open_export_dialog(
            self.settings_store.path(),
            self.forwarding_service.registry().list_all_saved_forwards(),
            &plugin_settings,
            portable_migration,
            |cx| self.oxide_ai_provider_ids(cx).count(),
            cx,
        );
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
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.select_oxide_import_file(&self.i18n, cx);
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
                let manager = self.connections.session_manager.clone();
                let path = self.settings_store.path().to_path_buf();
                let i18n = self.i18n.clone();
                effects.apply(&manager, &path, &i18n, cx, |core, options, cx| {
                    self.connection_store = core.store;
                    let (result, plugin_failed) = self.import_oxide_sidecars(
                        core.envelope,
                        OxideSidecarOptions::for_dialog(&options),
                        cx,
                    );
                    self.queue_cloud_sync_dirty_refresh(cx);
                    let counts = result.audit_counts(plugin_failed, options.import_app_settings);
                    self.oxide_transfer()
                        .present_oxide_import_result(result, cx);
                    counts
                });
            }
            SessionManagerWorkspaceEvent::RefreshOxideExportPreflight => {
                self.refresh_oxide_export_preflight(cx);
            }
            SessionManagerWorkspaceEvent::ModalBoundaryChanged => cx.notify(),
        }
    }

    fn oxide_ai_provider_ids<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = String> + 'a {
        let key_store = self.ai.state.read(cx).key_store();
        oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
            .into_iter()
            .map(|provider| provider.id)
            .filter(move |id| key_store.has_provider_key(id))
    }

    pub(in crate::workspace) fn refresh_oxide_export_preflight(&mut self, cx: &mut Context<Self>) {
        self.oxide_transfer()
            .refresh_export_preflight(|cx| self.oxide_ai_provider_ids(cx).count(), cx);
    }

    pub(in crate::workspace) fn export_oxide_dialog(&mut self, cx: &mut Context<Self>) {
        self.oxide_transfer().export_dialog(
            self.settings_store.settings(),
            self.settings_store.path(),
            oxideterm_cloud_sync::plugin_settings::load_plugin_settings,
            |cx| self.oxide_ai_provider_ids(cx).count(),
            |cx| {
                let ids = self.oxide_ai_provider_ids(cx).collect::<Vec<_>>();
                self.ai
                    .state
                    .read(cx)
                    .key_store()
                    .get_provider_keys(&ids)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(in crate::workspace) fn import_oxide_sidecars(
        &mut self,
        envelope: ImportResultEnvelope,
        options: OxideSidecarOptions<'_>,
        cx: &mut Context<Self>,
    ) -> (OxideClientStateImportResult, bool) {
        let i18n = self.i18n.clone();
        apply_oxide_sidecars(
            &mut WorkspaceOxideImport {
                workspace: self,
                cx,
            },
            envelope,
            options,
            &i18n,
        )
    }

    pub(in crate::workspace) fn apply_oxide_import_portable_secrets(
        &mut self,
        envelope: &mut ImportResultEnvelope,
        cx: &mut Context<Self>,
    ) {
        let mut host = WorkspaceOxideImport {
            workspace: self,
            cx,
        };
        restore_oxide_portable_secrets(envelope, |id, key| host.store_ai_provider_key(id, key));
    }
}

struct WorkspaceOxideImport<'a, 'cx> {
    workspace: &'a mut WorkspaceApp,
    cx: &'a mut Context<'cx, WorkspaceApp>,
}

impl OxideImportHost for WorkspaceOxideImport<'_, '_> {
    fn import_forwards(
        &mut self,
        records: &[OwnedForwardImportRecord],
        replace_owners: &HashSet<String>,
        merge_owners: &HashSet<String>,
    ) -> Result<usize, String> {
        self.workspace
            .forwarding_service
            .registry()
            .apply_owned_forward_import_records(records, replace_owners, merge_owners)
            .map_err(|error| error.to_string())
    }

    fn import_quick_commands(
        &mut self,
        snapshot: &str,
        strategy: QuickCommandImportStrategy,
    ) -> oxideterm_quick_commands::QuickCommandImportResult {
        self.workspace.terminal.update(self.cx, |terminal, _| {
            terminal
                .quick_commands
                .store
                .apply_snapshot_json(snapshot, strategy)
        })
    }

    fn import_plugin_settings(
        &mut self,
        entries: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
    ) -> Result<usize, String> {
        oxideterm_cloud_sync::plugin_settings::upsert_plugin_settings(
            self.workspace.settings_store.path(),
            entries,
        )
    }

    fn current_settings(&self) -> &oxideterm_settings::PersistedSettings {
        self.workspace.settings_store.settings()
    }

    fn import_settings(&mut self, merged: oxideterm_settings::PersistedSettings) -> Result<(), ()> {
        let previous = self.workspace.settings_store.settings().clone();
        let saved = self
            .workspace
            .settings_store
            .replace_and_save(merged)
            .map_err(|_| ())?;
        self.workspace
            .apply_loaded_settings_to_runtime(&previous, &saved.settings, self.cx);
        self.workspace
            .settings_workspace
            .update(self.cx, |settings, _| {
                settings.acknowledge_external_store_state()
            });
        self.workspace
            .emit_native_plugin_settings_events(&previous, &saved.settings, self.cx);
        self.workspace.sync_tab_titles(self.cx);
        self.cx.notify();
        Ok(())
    }

    fn store_ai_provider_key(
        &mut self,
        id: &str,
        key: zeroize::Zeroizing<String>,
    ) -> Result<(), String> {
        self.workspace
            .ai
            .state
            .read(self.cx)
            .key_store()
            .store_provider_key(id, key)
            .map_err(|error| error.to_string())
    }

    fn show_import_status(&mut self, status: String) {
        self.workspace
            .connections
            .show_catalog_status(status, self.cx);
    }
}
