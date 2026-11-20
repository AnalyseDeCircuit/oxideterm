mod tests {
    use super::super::super::overlay::coalesce_connection_trace_running_events;
    use super::super::super::*;
    use std::{cell::Cell, rc::Rc};

    struct ZenExitWindow {
        shell: Entity<window_shell::WorkspaceWindowShell>,
        pointer_presses: Rc<Cell<usize>>,
    }

    impl Render for ZenExitWindow {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let presses = self.pointer_presses.clone();
            div()
                .size_full()
                .relative()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(MouseButton::Left, move |_, _, _| {
                            presses.set(presses.get() + 1)
                        }),
                )
                .child(self.shell.clone())
        }
    }

    fn zen_exit_fixture(
        cx: &mut gpui::TestAppContext,
    ) -> Option<(
        Entity<WorkspaceApp>,
        Rc<Cell<usize>>,
        &mut gpui::VisualTestContext,
    )> {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_ZEN_EXIT_TEST_DIR";
        let Some(directory) = std::env::var_os(fixture_key) else {
            // The workspace resolves its data root once per process.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            fs::hard_link(&executable, &child).unwrap();
            fs::write(directory.path().join("portable"), []).unwrap();
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
            return None;
        };
        assert!(default_settings_path().starts_with(PathBuf::from(directory)));
        let mut settings = SettingsStore::load_default().unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().onboarding_disclaimer_accepted = true;
        settings.save().unwrap();
        crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(&default_settings_path())
            .unwrap();
        cx.executor().allow_parking();
        let pointer_presses = Rc::new(Cell::new(0));
        let (root, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.tokens.motion.enabled = false;
                workspace.toggle_zen_mode(cx);
            });
            ZenExitWindow {
                shell: cx.new(|cx| window_shell::WorkspaceWindowShell::new(workspace, window, cx)),
                pointer_presses: pointer_presses.clone(),
            }
        });
        let workspace = root.read_with(cx, |root, cx| root.shell.read(cx).session_entity());
        cx.simulate_resize(gpui::size(px(900.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        Some((workspace, pointer_presses, cx))
    }

    #[gpui::test]
    fn zen_exit_stays_visible_across_the_gap_and_only_the_button_blocks_clicks(
        cx: &mut gpui::TestAppContext,
    ) {
        let Some((workspace, presses, cx)) = zen_exit_fixture(cx) else {
            return;
        };
        let modifiers = gpui::Modifiers::default();
        let strip = cx.debug_bounds("zen-exit-hot-zone").unwrap();
        cx.simulate_mouse_move(strip.center(), None, modifiers);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let button = cx.debug_bounds("zen-exit-affordance-pill").unwrap();

        let gap = gpui::point(button.center().x, button.top() - px(2.0));
        cx.simulate_mouse_move(gap, None, modifiers);
        cx.executor()
            .advance_clock(zen_exit::HIDE_DELAY + Duration::from_millis(1));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("zen-exit-affordance-pill").is_some());
        cx.simulate_click(gap, modifiers);
        assert_eq!(
            presses.get(),
            1,
            "the transparent strip must pass clicks through"
        );

        let content = gpui::point(button.center().x, button.bottom() + px(20.0));
        cx.simulate_mouse_move(content, None, modifiers);
        cx.executor()
            .advance_clock(zen_exit::HIDE_DELAY + Duration::from_millis(1));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("zen-exit-affordance-pill").is_none());

        cx.simulate_mouse_move(strip.center(), None, modifiers);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let button = cx.debug_bounds("zen-exit-affordance-pill").unwrap();
        cx.simulate_mouse_move(button.center(), None, modifiers);
        cx.simulate_click(button.center(), modifiers);
        assert!(!workspace.read_with(cx, |w, _| w.settings_store.settings().sidebar_ui.zen_mode));
        assert_eq!(
            presses.get(),
            1,
            "the visible exit button must own the press"
        );
    }

    #[gpui::test]
    fn zen_exit_is_hidden_when_reenabled_by_a_settings_reload(cx: &mut gpui::TestAppContext) {
        let Some((workspace, _, cx)) = zen_exit_fixture(cx) else {
            return;
        };
        let strip = cx.debug_bounds("zen-exit-hot-zone").unwrap();
        cx.simulate_mouse_move(strip.center(), None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let button = cx.debug_bounds("zen-exit-affordance-pill").unwrap();
        cx.simulate_mouse_move(button.center(), None, gpui::Modifiers::default());

        for (zen_mode, keep_tabbar, expect_tabbar) in [
            (false, false, true),
            (true, false, false),
            (true, true, true),
            (true, false, false),
        ] {
            workspace.update(cx, |workspace, cx| {
                let path = workspace.settings_store.path().to_path_buf();
                let mut store = SettingsStore::load_from_path(path).unwrap();
                store.settings_mut().sidebar_ui.zen_mode = zen_mode;
                store.settings_mut().appearance.zen_mode_keep_tabbar = keep_tabbar;
                store.save().unwrap();
                workspace.reload_after_external_sync(cx).unwrap();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("workspace-tab-scroll-viewport").is_some(),
                expect_tabbar,
            );
            assert_eq!(
                cx.debug_bounds("workspace-titlebar-drag-left").is_some(),
                !zen_mode
            );
            assert_eq!(
                cx.debug_bounds("zen-exit-hot-zone").is_some(),
                zen_mode && !keep_tabbar
            );
            assert!(cx.debug_bounds("zen-exit-affordance-pill").is_none());
            let exit = cx.debug_bounds("zen-exit-tabbar-button");
            assert_eq!(exit.is_some(), zen_mode && keep_tabbar);
            if let Some(exit) = exit {
                let tabs = cx.debug_bounds("workspace-tab-scroll-viewport").unwrap();
                assert!(tabs.right() <= exit.left());
                cx.simulate_click(exit.center(), gpui::Modifiers::default());
                assert!(
                    !workspace.read_with(cx, |w, _| w
                        .settings_store
                        .settings()
                        .sidebar_ui
                        .zen_mode)
                );
            }
        }
    }

    fn connection_trace_event(
        status: ConnectionTraceStatus,
        stage: ConnectionTraceStage,
        progress: f32,
    ) -> ConnectionTraceEvent {
        ConnectionTraceEvent {
            attempt_id: "attempt-1".to_string(),
            node_id: NodeId::new("node-1"),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Connect,
        }
    }

    #[test]
    fn connection_trace_coalesces_running_progress_but_preserves_terminal_transitions() {
        let events = vec![
            connection_trace_event(
                ConnectionTraceStatus::Running,
                ConnectionTraceStage::Queued,
                5.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Running,
                ConnectionTraceStage::Authentication,
                62.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Ready,
                ConnectionTraceStage::Ready,
                100.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Failed,
                ConnectionTraceStage::Authentication,
                100.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Cancelled,
                ConnectionTraceStage::Authentication,
                100.0,
            ),
        ];

        let coalesced = coalesce_connection_trace_running_events(events);

        assert_eq!(coalesced.len(), 4);
        assert_eq!(coalesced[0].stage, ConnectionTraceStage::Authentication);
        assert_eq!(coalesced[1].status, ConnectionTraceStatus::Ready);
        assert_eq!(coalesced[2].status, ConnectionTraceStatus::Failed);
        assert_eq!(coalesced[3].status, ConnectionTraceStatus::Cancelled);
    }

    #[test]
    fn failed_session_tree_replace_preserves_previous_snapshot() {
        let tempdir =
            std::env::temp_dir().join(format!("oxideterm-session-tree-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tempdir).unwrap();
        let path = tempdir.join("session_tree.json");
        let previous = NodeTreePersistenceSnapshot {
            version: 1,
            exported_at_ms: 1,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        write_session_tree_snapshot(&path, &previous).unwrap();
        let previous_bytes = fs::read(&path).unwrap();
        let replacement = NodeTreePersistenceSnapshot {
            version: 1,
            exported_at_ms: 2,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        inject_session_tree_replace_failure();

        assert!(write_session_tree_snapshot(&path, &replacement).is_err());
        assert_eq!(fs::read(path).unwrap(), previous_bytes);
        let _ = fs::remove_dir_all(tempdir);
    }
}
