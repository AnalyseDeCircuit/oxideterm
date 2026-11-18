use super::*;

impl WorkspaceApp {
    pub(super) fn settings_known_hosts_section(&self, cx: &Context<Self>) -> AnyElement {
        self.with_settings_controls(cx, |controls, cx| {
            controls.settings_known_hosts_section(
                &self.settings_workspace,
                |path, cx| {
                    if let Ok(url) = url::Url::from_file_path(path) {
                        cx.open_url(url.as_str());
                    }
                },
                cx,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::window_shell::WorkspaceWindowShell;
    use gpui::TestAppContext;
    use oxideterm_ssh::list_known_hosts;

    #[gpui::test]
    fn known_hosts_search_and_confirmed_delete_use_the_selected_endpoint(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_KNOWN_HOSTS_UI_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
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
        };
        let path = list_known_hosts().unwrap().path;
        assert!(path.starts_with(PathBuf::from(fixture_dir)));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let key = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
        let retained = format!("# shared trust\nboard,alias ssh-ed25519 {key}\n");
        let original = format!("{retained}[board]:2222 ssh-ed25519 {key}\n");
        std::fs::write(&path, &original).unwrap();
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
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1400.0), px(1000.0)));
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        workspace.update_in(cx, |this, window, cx| {
            this.settings_workspace.update(cx, |settings, cx| {
                settings.set_active_tab(SettingsTab::Connections, cx);
            });
            this.open_settings(window, cx);
            this.sync_settings_section_list_state(cx);
            let item_ix = settings_section_list_item_index(
                SettingsTab::Connections,
                SettingsSectionId::ConnectionsKnownHosts,
                this.settings_dynamic_section_counts(cx),
            )
            .unwrap();
            this.settings_workspace
                .read(cx)
                .section_list
                .state()
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = workspace.read_with(cx, |this, _| {
            this.text_input_anchors
                .get(WorkspaceImeTarget::Settings(SettingsInput::KnownHostsSearch).anchor_id())
                .unwrap()
                .bounds
        });
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.simulate_input("2222");
        workspace.read_with(cx, |this, cx| {
            assert_eq!(this.settings_workspace.read(cx).known_hosts.query, "2222");
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.sync_settings_section_list_state(cx);
                this.settings_workspace
                    .read(cx)
                    .section_list
                    .state()
                    .scroll_to_end();
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("known-host-board").is_none());
        assert!(cx.debug_bounds("known-host-[board]:2222").is_some());
        let delete = cx.debug_bounds("settings_view.known_hosts.delete").unwrap();
        assert!(
            delete.origin.y >= px(0.0) && delete.bottom() <= px(1000.0),
            "delete bounds: {delete:?}"
        );
        cx.simulate_click(delete.center(), gpui::Modifiers::default());
        workspace.read_with(cx, |this, cx| {
            let entry = this
                .settings_workspace
                .read(cx)
                .known_hosts
                .pending_delete
                .as_ref()
                .expect("clicking the visible delete button must request confirmation");
            assert_eq!((entry.host.as_str(), entry.port), ("board", Some(2222)));
        });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let cancel = cx.debug_bounds("settings_view.known_hosts.cancel").unwrap();
        cx.simulate_click(cancel.center(), gpui::Modifiers::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let delete = cx.debug_bounds("settings_view.known_hosts.delete").unwrap();
        cx.simulate_click(delete.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let confirm = cx
            .debug_bounds("settings_view.known_hosts.confirm")
            .unwrap();
        cx.simulate_click(confirm.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), retained);
        workspace.read_with(cx, |this, cx| {
            let state = &this.settings_workspace.read(cx).known_hosts;
            assert_eq!(state.error, None);
            assert_eq!(
                state
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .map(|entry| (entry.host.as_str(), entry.port))
                    .collect::<Vec<_>>(),
                vec![("alias", Some(22)), ("board", Some(22))]
            );
        });
    }
}
