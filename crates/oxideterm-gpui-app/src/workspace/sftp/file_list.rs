use super::*;

#[cfg(test)]
mod drag_tests {
    use super::*;
    use crate::workspace::window_shell::WorkspaceWindowShell;
    use gpui::TestAppContext;

    fn file(path: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: "drag.txt".into(),
            path: path.into(),
            file_type: SftpFileType::File,
            size_known: true,
            size: 12,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    #[gpui::test]
    fn sftp_pointer_and_keyboard_actions_use_visible_selection(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_SFTP_DRAG_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; isolate the real workspace in a portable child.
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
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().sftp.conflict_action = oxideterm_settings::ConflictAction::Ask;
        settings.save().unwrap();
        // Real workspace workers and drag timers wake the UI from I/O threads.
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.open_sftp_tab_surface(NodeId::new("drag-fixture"), None, cx);
            });
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();
        let view = workspace.read_with(cx, |workspace, _| workspace.sftp_view().clone());
        for (source, target, direction, path, release_outside, font_size, row_height) in [
            (
                SftpPane::Local,
                SftpPane::Remote,
                Some(SftpTransferDirection::Upload),
                "/local/drag.txt",
                false,
                14,
                25.0,
            ),
            (
                SftpPane::Remote,
                SftpPane::Local,
                Some(SftpTransferDirection::Download),
                "/remote/drag.txt",
                false,
                21,
                37.5,
            ),
            (
                SftpPane::Local,
                SftpPane::Local,
                None,
                "/local/drag.txt",
                false,
                24,
                42.5,
            ),
            (
                SftpPane::Local,
                SftpPane::Remote,
                None,
                "/local/drag.txt",
                true,
                14,
                25.0,
            ),
        ] {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .edit_settings(|settings| settings.appearance.ui_font_size = font_size, cx);
            });
            view.update(cx, |sftp, cx| {
                sftp.local_files = vec![file("/local/drag.txt")];
                sftp.remote_files = vec![file("/remote/drag.txt")];
                sftp.local_selected.clear();
                sftp.remote_selected.clear();
                sftp.local_path = "/local".into();
                sftp.remote_path = "/remote".into();
                sftp.remote_loading = false;
                sftp.remote_load_pending = false;
                sftp.remote_load_inflight = false;
                sftp.init_error = None;
                sftp.dialog = None;
                sftp.conflict_state = None;
                cx.notify();
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            let (start, end) = view.read_with(cx, |sftp, _| {
                let bounds = |pane| match pane {
                    SftpPane::Local => sftp.local_file_scroll.0.borrow().base_handle.bounds(),
                    SftpPane::Remote => sftp.remote_file_scroll.0.borrow().base_handle.bounds(),
                };
                let start = bounds(source);
                let measured = match source {
                    SftpPane::Local => {
                        sftp.local_file_scroll
                            .0
                            .borrow()
                            .last_item_size
                            .unwrap()
                            .contents
                            .height
                    }
                    SftpPane::Remote => {
                        sftp.remote_file_scroll
                            .0
                            .borrow()
                            .last_item_size
                            .unwrap()
                            .contents
                            .height
                    }
                };
                assert_eq!(measured, px(row_height), "UI font size {font_size}");
                assert!(start.size.width > px(0.0) && start.size.height > px(row_height));
                (
                    Point::new(start.center().x, start.top() + px(row_height / 2.0)),
                    bounds(target).center(),
                )
            });
            cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
            view.read_with(cx, |sftp, _| {
                let drag = sftp
                    .drag_state
                    .as_ref()
                    .expect("mouse-down must capture the source row");
                assert_eq!(drag.names, ["drag.txt"]);
                assert_eq!(drag.source_pane, source);
                assert!(drag.active);
                assert_eq!(sftp.drag_over_pane, Some(target));
            });
            let release = if release_outside {
                Point::new(px(-10.0), px(-10.0))
            } else {
                end
            };
            cx.simulate_mouse_up(release, MouseButton::Left, gpui::Modifiers::default());
            view.read_with(cx, |sftp, _| {
                if let Some(direction) = direction {
                    let conflicts = sftp
                        .conflict_state
                        .as_ref()
                        .expect("drop must reach the existing transfer flow");
                    let pending = &conflicts.pending_transfers;
                    assert_eq!(pending.len(), 1);
                    assert_eq!(pending[0].name, "drag.txt");
                    assert_eq!(pending[0].source.path, path);
                    assert_eq!(pending[0].direction, direction);
                } else {
                    assert!(
                        sftp.conflict_state.is_none(),
                        "cancelled drops must not transfer"
                    );
                    assert!(sftp.transfers.is_empty());
                }
                assert!(sftp.drag_state.is_none());
                assert!(sftp.drag_over_pane.is_none());
                assert!(sftp.drag_autoscroll_position.is_none());
            });
        }

        // Reuse the isolated workspace to exercise real keyboard dispatch on
        // nested tree entries, with a different stale flat selection present.
        workspace.update(cx, |workspace, cx| {
            workspace.edit_settings(|settings| settings.appearance.ui_font_size = 21, cx);
            workspace.set_main_window_active_tab(None, cx);
            workspace.open_sftp_sidebar_surface(NodeId::new("tree-fixture"), None, cx);
        });
        cx.run_until_parked();
        workspace.update(cx, |workspace, cx| {
            workspace
                .settings_store
                .settings_mut()
                .sftp
                .sidebar_view_mode = oxideterm_settings::SftpSidebarViewMode::Tree;
            workspace.sftp_view.update(cx, |sftp, _| {
                sftp.current_surface_id = Some(SftpSurfaceId::Sidebar);
                sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("tree-fixture")));
                sftp.active_pane = SftpPane::Local;
                sftp.remote_path = "/remote".into();
                sftp.remote_loading = false;
                sftp.remote_load_pending = false;
                sftp.remote_load_inflight = false;
                sftp.init_error = None;
                sftp.local_files = vec![file("/local/drag.txt")];
                let mut directory = file("/remote/dir");
                directory.name = "dir".into();
                directory.file_type = SftpFileType::Directory;
                sftp.remote_files = vec![directory, file("/remote/drag.txt")];
                sftp.tree_children
                    .insert("/remote/dir".into(), vec![file("/remote/dir/drag.txt")]);
                sftp.tree_expanded.insert("/remote/dir".into());
                sftp.remote_selected.insert("drag.txt".into());
            });
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let child_position = workspace.read_with(cx, |workspace, cx| {
            let bounds = workspace
                .sftp_view
                .read(cx)
                .remote_file_scroll
                .0
                .borrow()
                .base_handle
                .bounds();
            assert!(bounds.size.width > px(0.0) && bounds.size.height > px(75.0));
            Point::new(bounds.center().x, bounds.top() + px(56.25))
        });
        cx.simulate_mouse_down(
            child_position,
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            child_position,
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                assert_eq!(sftp.active_pane, SftpPane::Remote);
                assert_eq!(sftp.tree_selected.as_deref(), Some("/remote/dir/drag.txt"));
                assert!(sftp.remote_selected.is_empty());
                // Keyboard routing must ignore flat selection even if another
                // consumer left one behind after the pointer interaction.
                sftp.remote_selected.insert("drag.txt".into());
            });
        });
        let dispatch = |key: &str, cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                workspace.update(cx, |workspace, cx| {
                    let _scope = workspace.enter_sftp_surface(SftpSurfaceId::Sidebar);
                    workspace.handle_sftp_key(
                        &gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse(key).unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    )
                })
            })
        };
        assert!(dispatch("f2", cx));
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                assert!(matches!(&sftp.dialog,
                    Some(SftpDialog::Rename { old_path: Some(path), .. })
                    if path == "/remote/dir/drag.txt"));
                sftp.dialog = None;
                sftp.focused_input = None;
            });
        });
        assert!(dispatch("delete", cx));
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                assert!(matches!(&sftp.dialog,
                    Some(SftpDialog::Delete { files, paths, .. })
                    if files == &["drag.txt"] && paths == &["/remote/dir/drag.txt"]));
                sftp.dialog = None;
            });
        });
        assert!(dispatch("down", cx));
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace.sftp_view.read(cx).tree_selected.as_deref(),
                Some("/remote/drag.txt")
            );
        });
        assert!(dispatch("up", cx));
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace.sftp_view.read(cx).tree_selected.as_deref(),
                Some("/remote/dir/drag.txt")
            );
        });
        assert!(dispatch("left", cx));
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                let pending = &sftp.conflict_state.as_ref().unwrap().pending_transfers;
                assert_eq!(
                    pending
                        .iter()
                        .map(|transfer| (transfer.source.path.as_str(), transfer.direction))
                        .collect::<Vec<_>>(),
                    [("/remote/dir/drag.txt", SftpTransferDirection::Download)]
                );
                sftp.conflict_state = None;
                sftp.dialog = None;
                sftp.select_tree_file("/remote/dir".into());
            });
        });
        assert!(dispatch("enter", cx));
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                assert!(!sftp.tree_expanded.contains("/remote/dir"));
                // Selecting a filtered flat row replaces the tree focus.
                *sftp.input_value_mut(SftpInput::RemoteFilter) = "drag".into();
                sftp.select_file(
                    SftpPane::Remote,
                    "drag.txt".into(),
                    gpui::Modifiers::default(),
                );
            });
        });
        assert!(dispatch("f2", cx));
        workspace.update(cx, |workspace, cx| {
            workspace.sftp_view.update(cx, |sftp, _| {
                assert!(matches!(&sftp.dialog, Some(SftpDialog::Rename { old_name, old_path: None, .. }) if old_name == "drag.txt"));
                sftp.dialog = None;
                sftp.focused_input = None;
                sftp.input_value_mut(SftpInput::RemoteFilter).clear();
            });
        });
        assert!(
            !dispatch("delete", cx),
            "clearing the filter must not retain a hidden flat selection"
        );
    }
}
