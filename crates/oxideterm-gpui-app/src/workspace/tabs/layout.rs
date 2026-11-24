use super::*;
use oxideterm_gpui_shell::tabs::geometry::{TabStripLayout, TabStripState};

impl WorkspaceApp {
    pub(super) fn tab_strip_layout(&self, window: &Window, cx: &App) -> TabStripLayout {
        self.measure_tab_strip(&self.main_window_tabs.read(cx).state, window, cx)
    }
    pub(super) fn measure_tab_strip(
        &self,
        state: &TabStripState,
        window: &Window,
        cx: &App,
    ) -> TabStripLayout {
        let width = |tab: &Tab| {
            let metrics = self.tokens.metrics;
            let font =
                settings_ui_font_family(&self.settings_store.settings().appearance.ui_font_family);
            let controls = self
                .reconnect_node_id_for_tab(tab, cx)
                .and_then(|node| {
                    self.workspace_runtime
                        .read(cx)
                        .reconnect_active_progress(&node)
                })
                .map_or(metrics.tab_close_button_size, |job| {
                    self.tab_reconnect_controls_width(state, &job, window)
                });
            state.natural_width(
                &self.tab_display_title(tab),
                controls,
                font,
                &self.tokens,
                window,
            )
        };
        let returning = self
            .detached_tab_return_drag
            .filter(|drag| drag.active)
            .and_then(|drag| {
                let bounds = self.main_window_tabbar_drop_bounds?;
                if !bounds.contains(&gpui::point(
                    px(drag.current_screen_x),
                    px(drag.current_screen_y),
                )) {
                    return None;
                }
                let tab = self.tab_by_id(drag.tab_id, cx)?;
                Some((
                    tab.id,
                    width(tab),
                    drag.current_screen_x
                        - f32::from(bounds.left() + state.scroll_handle.offset().x),
                ))
            });
        state.layout(
            self.main_tabs(cx).map(|(_, tab)| (tab.id, width(tab))),
            returning,
            &self.tokens,
            cx.background_executor().now(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn tab_strip_fixture(
        cx: &mut gpui::TestAppContext,
    ) -> Option<(Entity<WorkspaceApp>, &mut gpui::VisualTestContext)> {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_TAB_LAYOUT_TEST_DIR";
        let Some(directory) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; do not touch user settings.
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
            return None;
        };
        assert!(default_settings_path().starts_with(PathBuf::from(directory)));
        let mut settings = SettingsStore::load_from_path(default_settings_path()).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.tokens.motion.enabled = false;
                workspace.tab_host.update(cx, |host, _| {
                    host.insert_and_select_main_tab(Tab {
                        id: TabId(101),
                        kind: TabKind::SessionManager,
                        title: "设置".into(),
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    });
                });
            });
            window_shell::WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        Some((workspace, cx))
    }

    #[gpui::test]
    fn holding_a_tab_at_either_edge_scrolls_to_its_drop_slot(cx: &mut gpui::TestAppContext) {
        let Some((workspace, cx)) = tab_strip_fixture(cx) else {
            return;
        };
        cx.simulate_resize(gpui::size(px(900.0), px(700.0)));
        workspace.update(cx, |workspace, cx| {
            workspace.tab_host.update(cx, |host, _| {
                for id in 102..=115 {
                    host.insert_and_select_main_tab(Tab {
                        id: TabId(id),
                        kind: TabKind::SessionManager,
                        title: format!("Terminal session {id}"),
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    });
                }
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let viewport = cx.debug_bounds("workspace-tab-scroll-viewport").unwrap();
        let modifiers = gpui::Modifiers::default();
        let offset = |cx: &gpui::VisualTestContext| {
            workspace.read_with(cx, |w, cx| {
                w.main_window_tabs.read(cx).state.scroll_handle.offset().x
            })
        };
        let hold = |cx: &mut gpui::VisualTestContext, frames| {
            // No fresh pointer events. Service only frames requested by the window.
            for _ in 0..frames {
                cx.executor().advance_clock(Duration::from_millis(16));
                cx.update(|window, cx| {
                    if window.simulate_next_frame(cx) != 0 {
                        window.draw(cx).clear(cx);
                    }
                });
            }
        };
        for right in [true, false] {
            let tab = cx.debug_bounds("workspace-tab-101").unwrap();
            let edge = if right {
                viewport.right() - px(2.0)
            } else {
                viewport.left() + px(2.0)
            };
            let pointer = gpui::point(edge, tab.center().y);
            cx.simulate_mouse_down(tab.center(), MouseButton::Left, modifiers);
            cx.simulate_mouse_move(pointer, Some(MouseButton::Left), modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let initial = offset(cx);
            hold(cx, 4);
            let scrolled = offset(cx);
            assert!(
                if right {
                    scrolled < initial
                } else {
                    scrolled > initial
                },
                "holding the pointer must scroll beyond the initial move"
            );
            if right {
                cx.simulate_mouse_move(viewport.center(), Some(MouseButton::Left), modifiers);
                let paused = offset(cx);
                hold(cx, 4);
                assert_eq!(offset(cx), paused, "leaving the edge must stop scrolling");
                cx.simulate_mouse_move(pointer, Some(MouseButton::Left), modifiers);
                cx.update(|window, cx| window.draw(cx).clear(cx));
            }
            hold(cx, 150);
            // Check the painted insertion line, not just the model's target slot.
            cx.update(|window, cx| {
                let accent = rgb(workspace.read(cx).tokens.ui.accent).into();
                let scale = window.scale_factor();
                assert!(
                    window.painted_quads().iter().any(|quad| {
                        quad.background.as_solid() == Some(accent)
                            && (quad.bounds.size.width.0 / scale - 2.0).abs() < 0.1
                            && (quad.bounds.center().x.0 / scale - f32::from(edge)).abs() <= 2.0
                            && (quad.bounds.center().y.0 / scale - f32::from(pointer.y)).abs()
                                <= 2.0
                    }),
                    "the insertion line must be visible at the held edge"
                );
            });
            cx.simulate_mouse_up(pointer, MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            workspace.read_with(cx, |w, cx| {
                let actual: Vec<_> = w.main_tabs(cx).map(|(_, tab)| tab.id).collect();
                let mut expected: Vec<_> = (102..=115).map(TabId).collect();
                expected.insert(if right { expected.len() } else { 0 }, TabId(101));
                assert_eq!(actual, expected);
            });
        }
    }

    #[gpui::test]
    fn reduced_motion_releases_tab_width_at_the_end_of_the_fade(cx: &mut gpui::TestAppContext) {
        let Some((workspace, cx)) = tab_strip_fixture(cx) else {
            return;
        };
        cx.simulate_resize(gpui::size(px(900.0), px(700.0)));
        workspace.update(cx, |workspace, cx| {
            workspace
                .tokens
                .apply_motion(oxideterm_theme::UiMotionProfile::Reduced);
            workspace.tab_host.update(cx, |host, _| {
                host.insert_and_select_main_tab(Tab {
                    id: TabId(102),
                    kind: TabKind::SessionManager,
                    title: "Adjacent tab".into(),
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                });
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let closing = cx.debug_bounds("workspace-tab-101").unwrap();
        let adjacent = cx.debug_bounds("workspace-tab-102").unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.close_tab_by_id(TabId(101), window, cx);
            });
        });
        while cx.executor().tick() {}
        // The Reduced control tier fades out in 78 ms without shrinking.
        cx.executor().advance_clock(Duration::from_millis(77));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let exiting = cx.debug_bounds("workspace-tab-exit-101").unwrap();
        assert_eq!(exiting.size.width, closing.size.width);
        assert_eq!(
            cx.debug_bounds("workspace-tab-102").unwrap().left(),
            adjacent.left()
        );

        cx.executor().advance_clock(Duration::from_millis(1));
        while cx.executor().tick() {}
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("workspace-tab-exit-101").is_none());
        assert_eq!(
            cx.debug_bounds("workspace-tab-102").unwrap().left(),
            closing.left()
        );
    }

    #[gpui::test]
    fn tab_strip_render_drag_close_and_scroll_use_full_title_widths(cx: &mut gpui::TestAppContext) {
        let Some((workspace, cx)) = tab_strip_fixture(cx) else {
            return;
        };
        cx.simulate_resize(gpui::size(px(1200.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let viewport = cx.debug_bounds("workspace-tab-scroll-viewport").unwrap();
        let single = cx.debug_bounds("workspace-tab-101").unwrap();
        assert!(
            single.size.width < px(160.0),
            "a short tab must not stretch across the strip"
        );
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.tab_host.update(cx, |host, _| {
                    for (id, title) in [(102, "家里的主机"), (103, "IDE · Work")] {
                        host.insert_and_select_main_tab(Tab {
                            id: TabId(id),
                            kind: TabKind::SessionManager,
                            title: title.into(),
                            title_source: TabTitleSource::Static,
                            root_pane: None,
                            active_pane_id: None,
                        });
                    }
                });
            });
            window.draw(cx).clear(cx);
        });
        let first = cx.debug_bounds("workspace-tab-101").unwrap();
        let second = cx.debug_bounds("workspace-tab-102").unwrap();
        let last = cx.debug_bounds("workspace-tab-103").unwrap();
        assert!((f32::from(first.left() - viewport.left())).abs() < 1.0);
        assert_eq!(first.size.width, single.size.width);
        assert!(last.right() < viewport.right());
        cx.simulate_mouse_down(
            first.center(),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(
            gpui::point(last.right() - px(2.0), last.center().y),
            Some(MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            gpui::point(last.right() - px(2.0), last.center().y),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace
                    .main_tabs(cx)
                    .map(|(_, tab)| tab.id)
                    .collect::<Vec<_>>(),
                vec![TabId(102), TabId(103), TabId(101)]
            );
        });
        cx.simulate_resize(gpui::size(px(900.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
        let viewport = cx.debug_bounds("workspace-tab-scroll-viewport").unwrap();
        let last = cx.debug_bounds("workspace-tab-101").unwrap();
        assert_eq!(last.size.width, single.size.width);
        let closing = cx.debug_bounds("workspace-tab-103").unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                let index = workspace.tab_index_by_id(TabId(103), cx).unwrap();
                let exit = workspace.tab_exit_visual(index, window, cx).unwrap();
                assert!((exit.width - f32::from(closing.size.width)).abs() < 1.0);
                workspace
                    .tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Normal);
                workspace.close_tab_by_id(TabId(103), window, cx);
            });
        });
        cx.executor().advance_clock(Duration::from_millis(65));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let exit = cx.debug_bounds("workspace-tab-exit-103").unwrap();
        let before = cx.debug_bounds("workspace-tab-102").unwrap();
        let after = cx.debug_bounds("workspace-tab-101").unwrap();
        assert!(exit.size.width > px(0.0) && exit.size.width < closing.size.width);
        assert!((f32::from(before.right() - exit.left())).abs() < 1.0);
        assert!((f32::from(after.left() - exit.right())).abs() < 1.0);
        assert_eq!(before.size.width, second.size.width);
        assert_eq!(after.size.width, single.size.width);
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.tokens.motion.enabled = false;
                workspace.tab_host.update(cx, |host, _| {
                    host.insert_and_select_main_tab(Tab {
                        id: TabId(104),
                        kind: TabKind::SessionManager,
                        title: "完整的长标题 · Wide WWW iii / ".repeat(12),
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    });
                });
            });
            window.draw(cx).clear(cx);
        });
        let wide = cx.debug_bounds("workspace-tab-104").unwrap();
        assert!(wide.size.width > viewport.size.width);
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.reveal_active_tab(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        let wide = cx.debug_bounds("workspace-tab-104").unwrap();
        assert!((f32::from(wide.left() - viewport.left())).abs() < 1.0);
        let title = cx.debug_bounds("workspace-tab-title-104").unwrap();
        cx.update(|window, cx| {
            let workspace = workspace.read(cx);
            let text = workspace.tab_display_title(workspace.tab_by_id(TabId(104), cx).unwrap());
            assert!(
                f32::from(title.size.width)
                    >= workspace.main_window_tabs.read(cx).state.text_width(
                        &text,
                        workspace.tokens.metrics.tab_font_size,
                        settings_ui_font_family(
                            &workspace
                                .settings_store
                                .settings()
                                .appearance
                                .ui_font_family
                        ),
                        window
                    )
            );
        });
    }
}
