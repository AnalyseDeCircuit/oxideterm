use super::*;
use oxideterm_gpui_settings_view::connections_page::ConnectionRequest;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_settings_managed_key_dialog(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .managed_key_dialog_open()
            .then(|| {
                self.settings_views
                    .managed_key_dialog_view
                    .clone()
                    .into_any_element()
            })
    }

    pub(in crate::workspace) fn render_settings_ssh_config_import_dialog(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .ssh_config_import_dialog_open()
            .then(|| {
                self.settings_views
                    .ssh_import_dialog_view
                    .clone()
                    .into_any_element()
            })
    }

    fn prepare_discovery_edit(
        &mut self,
        source: &mut oxideterm_discovery::DiscoverySource,
        cx: &mut Context<Self>,
    ) -> bool {
        if source.id.is_empty() {
            source.id = uuid::Uuid::new_v4().to_string();
            return true;
        }
        // A second window may have deleted the source while this editor was open.
        if self
            .discovery
            .source(&source.id)
            .is_some_and(|saved| !saved.removal_pending)
        {
            return true;
        }
        self.report_discovery_failure(oxideterm_discovery::DiscoveryFailure::StaleTarget, cx);
        false
    }

    pub(super) fn handle_connection_page_request(
        &mut self,
        request: ConnectionRequest,
        cx: &mut Context<Self>,
    ) {
        match request {
            ConnectionRequest::SaveDiscovery {
                mut source,
                secrets,
            } => {
                if self.prepare_discovery_edit(&mut source, cx) {
                    self.save_discovery_source(source, secrets, cx);
                }
            }
            ConnectionRequest::RunDiscoverySourceAction {
                source_id,
                action_id,
            } => self.run_discovery_source_action(source_id, action_id, cx),
            ConnectionRequest::DeleteDiscovery(id) => self.delete_discovery_source(&id, cx),
            ConnectionRequest::SetDiscoveryEnabled(id, enabled) => {
                let mut saved = self.settings_store.clone();
                if saved.set_discovery_source_enabled(&id, enabled).is_ok() {
                    self.reconcile_discovery_sources(cx);
                    cx.notify();
                } else {
                    self.push_command_palette_toast(
                        self.i18n.t("settings_view.discovery.title"),
                        Some(self.i18n.t("settings_view.discovery.toggle_failed")),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                }
            }
            ConnectionRequest::RefreshDiscovery(id) => self.discovery.request_directory(&id, true),
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
            ConnectionRequest::CloseKey => self.close_managed_key_dialog(cx),
            ConnectionRequest::PickKey => self
                .settings_workspace
                .update(cx, |s, cx| s.pick_managed_key_file(&self.i18n, cx)),
            ConnectionRequest::PickImport(directory) => {
                self.settings_workspace.update(cx, |s, cx| {
                    s.pick_connection_import(&self.i18n, directory, cx)
                })
            }
            ConnectionRequest::ClearFooterFocus => self.clear_standard_confirm_focus(),
            ConnectionRequest::Local(action) => {
                let changed = self.settings_workspace.update(cx, |settings, cx| {
                    settings.apply_connection_action(
                        action,
                        &mut self.connection_store,
                        &self.i18n,
                        cx,
                    )
                });
                if changed {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
            }
        }
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
}

#[cfg(test)]
mod settings_connection_entity_tests {
    use gpui::{AppContext, TestAppContext, point};

    use super::*;

    #[gpui::test]
    fn discovery_settings_list_toggles_source_and_opens_plugin_manager(cx: &mut TestAppContext) {
        let Some(_) = crate::workspace::session_manager::tests::isolated_workspace_fixture(
            cx,
            "OXIDETERM_DISCOVERY_SETTINGS_LIST_TEST_DIR",
        ) else {
            return;
        };
        let mut store = SettingsStore::load_default().unwrap();
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().onboarding_disclaimer_accepted = true;
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.save().unwrap();
        store
            .replace_discovery_sources(vec![oxideterm_discovery::DiscoverySource {
                id: "unavailable".into(),
                name: "Build assets".into(),
                provider_id: "assets".into(),
                plugin_id: Some("missing.plugin".into()),
                ..Default::default()
            }])
            .unwrap();
        crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(&default_settings_path())
            .unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            crate::workspace::window_shell::WorkspaceWindowShell::new(workspace, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1400.0), px(1000.0)));
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        workspace.update_in(cx, |this, window, cx| {
            this.settings_workspace.update(cx, |settings, cx| {
                settings.set_active_tab(SettingsTab::Discovery, cx);
            });
            this.open_settings(window, cx);
            this.sync_settings_section_list_state(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let toggle = cx.debug_bounds("discovery-enabled-unavailable").unwrap();
        cx.simulate_click(toggle.center(), gpui::Modifiers::default());
        let saved = SettingsStore::load_default().unwrap();
        assert!(!saved.settings().discovery_sources[0].enabled);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let plugins = cx.debug_bounds("discovery-open-plugins").unwrap();
        cx.simulate_click(plugins.center(), gpui::Modifiers::default());
        workspace.read_with(cx, |this, cx| {
            assert_eq!(
                this.active_content_tab(cx).unwrap().kind,
                TabKind::PluginManager
            );
        });
    }

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
            this.handle_connection_page_request(
                ConnectionRequest::Local(
                    oxideterm_gpui_settings_view::connections_page::ConnectionAction::OpenKeyPaste,
                ),
                cx,
            );
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
