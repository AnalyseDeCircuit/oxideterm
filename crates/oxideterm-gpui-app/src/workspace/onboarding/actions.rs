use super::*;
use oxideterm_connections::{
    list_ssh_config_hosts, resolve_ssh_config_alias, saved_connection_from_ssh_host,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::window_shell::WorkspaceWindowShell;
    use gpui::TestAppContext;

    #[gpui::test]
    fn onboarding_completion_waits_for_durable_settings_and_survives_restart(
        cx: &mut TestAppContext,
    ) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_ONBOARDING_SAVE_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; keep the real workspace in a portable child.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
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
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut store = SettingsStore::load_from_path(&settings_path).unwrap();
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();
        let unreadable = b"unreadable settings fixture";
        std::fs::write(&settings_path, unreadable).unwrap();
        let recovered = SettingsStore::load_from_path(&settings_path).unwrap();
        let previous_settings = recovered.settings().clone();
        workspace.update(cx, |this, cx| {
            this.settings_store = recovered;
            this.onboarding.disclaimer_accepted = true;
            this.onboarding.step = ONBOARDING_TOTAL_STEPS - 1;
            this.onboarding.ai_opt_in = true;
            this.onboarding.tool_use_opt_in = true;
            this.complete_onboarding(cx);
            assert!(
                this.onboarding.open,
                "a failed save must keep onboarding open"
            );
            assert!(this.onboarding.save_failed);
            assert_eq!(this.onboarding.step, ONBOARDING_TOTAL_STEPS - 1);
            assert_eq!(this.settings_store.settings(), &previous_settings);
        });
        assert_eq!(std::fs::read(&settings_path).unwrap(), unreadable);
        // Recover the isolated fixture and retry the same user choices.
        std::fs::remove_file(&settings_path).unwrap();
        let restored = SettingsStore::load_from_path(&settings_path).unwrap();
        workspace.update(cx, |this, cx| {
            this.settings_store = restored;
            this.complete_onboarding(cx);
            assert!(!this.onboarding.open);
            assert!(!this.onboarding.save_failed);
        });
        let restarted = SettingsStore::load_from_path(&settings_path).unwrap();
        assert!(restarted.settings().onboarding_completed);
        assert!(restarted.settings().onboarding_disclaimer_accepted);
        assert!(restarted.settings().ai.enabled);
        assert!(restarted.settings().ai.enabled_confirmed);
        assert!(restarted.settings().ai.tool_use.enabled);
        assert!(!OnboardingState::from_settings(restarted.settings()).open);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_onboarding_from_palette(&mut self, cx: &mut Context<Self>) {
        let disclaimer_accepted = self.onboarding.disclaimer_accepted
            || self
                .settings_store
                .settings()
                .onboarding_disclaimer_accepted
            || self.settings_store.settings().onboarding_completed;
        self.edit_settings(
            move |settings| {
                settings.onboarding_completed = false;
                // Persist the implied acceptance when migrating a completed legacy flow.
                settings.onboarding_disclaimer_accepted = disclaimer_accepted;
            },
            cx,
        );
        self.onboarding
            .reset_for_open(self.settings_store.settings());
        cx.notify();
    }

    pub(in crate::workspace) fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        let previous_settings = self.settings_store.settings().clone();
        let mut next_settings = previous_settings.clone();
        next_settings.onboarding_completed = true;
        next_settings.onboarding_disclaimer_accepted = true;
        if self.onboarding.ai_opt_in {
            next_settings.ai.enabled = true;
            next_settings.ai.enabled_confirmed = true;
            if self.onboarding.tool_use_opt_in {
                next_settings.ai.tool_use.enabled = true;
            }
        }
        // Publish completion and runtime opt-ins only after the durable file swap succeeds.
        match self.settings_store.replace_and_save(next_settings) {
            Ok(saved) => {
                self.apply_loaded_settings_to_runtime(&previous_settings, &saved.settings, cx);
                self.settings_workspace.update(cx, |settings, _cx| {
                    settings.acknowledge_external_store_state()
                });
                self.emit_native_plugin_settings_events(&previous_settings, &saved.settings, cx);
                self.sync_tab_titles(cx);
                self.onboarding.save_failed = false;
                self.onboarding.open = false;
            }
            Err(_) => {
                // Settings errors may contain file content; expose only a localized save failure.
                self.onboarding.save_failed = true;
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_onboarding_disclaimer_acceptance(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let accepted = !self.onboarding.disclaimer_accepted;
        self.onboarding.disclaimer_accepted = accepted;
        self.edit_settings(
            move |settings| settings.onboarding_disclaimer_accepted = accepted,
            cx,
        );
    }

    pub(in crate::workspace) fn onboarding_go_to_step(
        &mut self,
        step: usize,
        cx: &mut Context<Self>,
    ) {
        if step >= ONBOARDING_TOTAL_STEPS || (!self.onboarding.disclaimer_accepted && step > 1) {
            return;
        }
        self.onboarding.step = step;
        self.onboarding.scroll_handle = ScrollHandle::new();
        if OnboardingStep::from_index(step) == OnboardingStep::CliCompanion
            && self
                .settings_workspace
                .read(cx)
                .cli_companion_needs_refresh()
        {
            self.refresh_cli_companion_status(cx);
        }
        if OnboardingStep::from_index(step) == OnboardingStep::QuickStart
            && self.onboarding.host_count.is_none()
        {
            self.refresh_onboarding_ssh_host_count(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn onboarding_next(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.step == 1 && !self.onboarding.disclaimer_accepted {
            return;
        }
        if self.onboarding.step + 1 < ONBOARDING_TOTAL_STEPS {
            self.onboarding_go_to_step(self.onboarding.step + 1, cx);
        } else {
            self.complete_onboarding(cx);
        }
    }

    pub(in crate::workspace) fn onboarding_back(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.step > 0 {
            self.onboarding_go_to_step(self.onboarding.step - 1, cx);
        }
    }

    pub(in crate::workspace) fn onboarding_skip_to_quick_start(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.disclaimer_accepted {
            self.onboarding_go_to_step(ONBOARDING_TOTAL_STEPS - 1, cx);
        }
    }

    pub(in crate::workspace) fn close_onboarding_if_allowed(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.disclaimer_accepted {
            self.complete_onboarding(cx);
        }
    }

    pub(in crate::workspace) fn handle_onboarding_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.onboarding.open {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_onboarding_if_allowed(cx),
            "enter" => self.onboarding_next(cx),
            "arrowleft" => self.onboarding_back(cx),
            "arrowright" => self.onboarding_next(cx),
            _ => return false,
        }
        true
    }

    pub(in crate::workspace) fn refresh_onboarding_ssh_host_count(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let count = runtime
                .spawn_blocking(|| {
                    list_ssh_config_hosts(&HashSet::new())
                        .map(|hosts| hosts.into_iter().filter(|host| host.alias != "*").count())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()))
                .unwrap_or(0);
            let _ = weak.update(cx, |this, cx| {
                this.onboarding.host_count = Some(count);
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn import_onboarding_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.import_state != OnboardingImportState::Idle {
            return;
        }
        self.onboarding.import_state = OnboardingImportState::Loading;
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        let aliases = list_ssh_config_hosts(&existing_names)
            .map(|hosts| {
                hosts
                    .into_iter()
                    .filter(|host| host.alias != "*")
                    .map(|host| host.alias)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut imported = 0usize;
        for alias in aliases {
            if self
                .connection_store
                .connections()
                .iter()
                .any(|connection| connection.name == alias)
            {
                continue;
            }
            let Ok(Some(host)) = resolve_ssh_config_alias(&alias) else {
                continue;
            };
            let Ok(connection) = saved_connection_from_ssh_host(host) else {
                continue;
            };
            if self
                .connection_store
                .import_ssh_connection(connection)
                .is_ok()
            {
                imported += 1;
            }
        }
        let _ = self.connection_store.save();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        self.onboarding.imported_count = imported;
        self.onboarding.import_state = OnboardingImportState::Done;
        self.onboarding.host_count = Some(imported);
        cx.notify();
    }

    pub(in crate::workspace) fn onboarding_open_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        let _ = self.create_local_terminal_tab(window, cx);
    }

    pub(in crate::workspace) fn onboarding_open_new_connection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        self.open_new_connection_form(window, cx);
    }

    pub(in crate::workspace) fn onboarding_open_connection_importers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        self.open_connection_importers_settings(window, cx);
    }

    pub(in crate::workspace) fn onboarding_open_cli_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.complete_onboarding(cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::General, cx)
        });
        self.open_settings(window, cx);
    }
}
