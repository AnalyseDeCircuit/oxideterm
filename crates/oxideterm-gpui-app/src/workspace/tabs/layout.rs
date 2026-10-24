use super::*;
use gpui::{TextRun, font};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TabStripItemKind {
    Live,
    Returning,
    Exiting(usize),
}

pub(super) struct TabStripItem {
    pub tab_id: TabId,
    pub width: f32,
    pub kind: TabStripItemKind,
}

pub(super) struct TabStripLayout {
    pub items: Vec<TabStripItem>,
    leading: f32,
}

impl TabStripLayout {
    pub fn return_index(&self) -> Option<usize> {
        let mut index = 0;
        for item in &self.items {
            match item.kind {
                TabStripItemKind::Returning => return Some(index),
                TabStripItemKind::Live => index += 1,
                TabStripItemKind::Exiting(_) => {}
            }
        }
        None
    }

    pub fn bounds(&self, tab_id: TabId) -> Option<std::ops::Range<f32>> {
        let mut left = self.leading;
        for item in &self.items {
            if item.tab_id == tab_id {
                return Some(left..left + item.width);
            }
            left += item.width;
        }
        None
    }

    pub fn content_width(&self) -> f32 {
        self.leading + self.items.iter().map(|item| item.width).sum::<f32>()
    }

    pub fn live_widths(&self) -> Vec<f32> {
        self.items
            .iter()
            .filter(|item| item.kind == TabStripItemKind::Live)
            .map(|item| item.width)
            .collect()
    }

    pub fn insertion_index(&self, pointer_x: f32) -> usize {
        let mut left = self.leading;
        let mut index = 0;
        for item in &self.items {
            if item.kind == TabStripItemKind::Live {
                if pointer_x < left + item.width / 2.0 {
                    return index;
                }
                index += 1;
            }
            left += item.width;
        }
        index
    }
}

impl WorkspaceApp {
    pub(super) fn tab_strip_layout(&self, window: &Window, cx: &App) -> TabStripLayout {
        let mut layout = TabStripLayout {
            items: self
                .main_tabs(cx)
                .map(|(_, tab)| TabStripItem {
                    tab_id: tab.id,
                    width: self.tab_natural_width(tab, window, cx),
                    kind: TabStripItemKind::Live,
                })
                .collect(),
            leading: self.tokens.metrics.tabbar_leading_offset,
        };
        let returning = self.detached_tab_return_drag.filter(|drag| {
            drag.active
                && self.main_window_tabbar_drop_bounds.is_some_and(|bounds| {
                    bounds.contains(&gpui::point(
                        px(drag.current_screen_x),
                        px(drag.current_screen_y),
                    ))
                })
        });
        if let Some(tab) = returning.and_then(|drag| self.tab_by_id(drag.tab_id, cx)) {
            layout.items.push(TabStripItem {
                tab_id: tab.id,
                width: self.tab_natural_width(tab, window, cx),
                kind: TabStripItemKind::Returning,
            });
        }
        let mut exits = self
            .main_window_tabs
            .exiting_tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let width = if self.tokens.motion.spatial_enabled {
                    tab.width * tab.exit_visibility(&self.tokens)
                } else {
                    tab.width
                };
                (index, tab, width)
            })
            .collect::<Vec<_>>();
        exits.sort_by_key(|(_, tab, _)| tab.visual_index);
        for (index, exiting, width) in exits {
            layout.items.insert(
                exiting.visual_index.min(layout.items.len()),
                TabStripItem {
                    tab_id: exiting.tab_id,
                    width,
                    kind: TabStripItemKind::Exiting(index),
                },
            );
        }
        if let Some(drag) = returning
            && let Some(index) = layout
                .items
                .iter()
                .position(|item| item.kind == TabStripItemKind::Returning)
        {
            let item = layout.items.remove(index);
            let bounds = self.main_window_tabbar_drop_bounds.unwrap();
            let x = drag.current_screen_x
                - f32::from(bounds.origin.x)
                - f32::from(self.main_window_tabs.scroll_handle.offset().x);
            let target = layout.insertion_index(x);
            let slot = layout
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == TabStripItemKind::Live)
                .nth(target)
                .map_or(layout.items.len(), |(index, _)| index);
            layout.items.insert(slot, item);
        }
        layout
    }

    fn tab_natural_width(&self, tab: &Tab, window: &Window, cx: &App) -> f32 {
        let metrics = self.tokens.metrics;
        let title = self.tab_display_title(tab);
        let controls = self
            .reconnect_node_id_for_tab(tab, cx)
            .and_then(|node| {
                self.workspace_runtime
                    .read(cx)
                    .reconnect_active_progress(&node)
            })
            .map_or(metrics.tab_close_button_size, |job| {
                self.tab_reconnect_controls_width(&job, window)
            });
        (self.tab_text_width(&title, metrics.tab_font_size, window)
            + metrics.tab_padding_x * 2.0
            + metrics.tab_icon_size
            + metrics.tab_gap * 2.0
            + controls
            + 1.0)
            .ceil()
    }

    pub(super) fn tab_text_width(&self, text: &str, size: f32, window: &Window) -> f32 {
        let text = SharedString::from(text.to_string());
        let run = TextRun {
            len: text.len(),
            font: font(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            )),
            color: rgb(self.tokens.ui.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        };
        // Use the same shaper and font as the label, including fallback glyphs.
        f32::from(
            window
                .text_system()
                .shape_line(text, px(size), &[run], None)
                .width,
        )
    }
}

impl ExitingTabVisual {
    pub(super) fn exit_visibility(&self, tokens: &oxideterm_theme::ThemeTokens) -> f32 {
        let duration = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if duration.is_zero() {
            return 0.0;
        }
        1.0 - oxideterm_gpui_ui::motion::ease_in_out_cubic(
            self.started_at.elapsed().as_secs_f32() / duration.as_secs_f32(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn tab_strip_render_drag_close_and_scroll_use_full_title_widths(cx: &mut gpui::TestAppContext) {
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
            return;
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
                workspace.tokens.motion.enabled = true;
                workspace.tokens.motion.spatial_enabled = true;
                workspace.close_tab_by_id(TabId(103), window, cx);
                let duration = oxideterm_gpui_ui::motion::duration(
                    &workspace.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                );
                workspace.main_window_tabs.exiting_tabs[0].started_at =
                    Instant::now() - duration / 2;
            });
            window.draw(cx).clear(cx);
        });
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
                let duration = oxideterm_gpui_ui::motion::duration(
                    &workspace.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                );
                workspace.main_window_tabs.exiting_tabs[0].started_at = Instant::now() - duration;
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
                    >= workspace.tab_text_width(
                        &text,
                        workspace.tokens.metrics.tab_font_size,
                        window
                    )
            );
        });
    }
}
