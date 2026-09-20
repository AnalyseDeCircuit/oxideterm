use super::*;
use gpui::{AnyElement, Entity, Focusable, MouseButton, SharedString, div, prelude::*, rgb};
use oxideterm_gpui_editor::{EditorPresentation, EditorSettings, TextEditorView};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonVariant, IconButtonOptions, button_with, icon_button},
    color_for_background,
    entity_row::{EntityListRowOptions, entity_list_row},
    modal::overlay_content_boundary,
    section::{SectionHeaderOptions, section_header},
    segmented_control::{SegmentedControlOptions, segmented_control, segmented_control_item},
    state::empty_state,
    text_input::text_input_frame,
    tooltip::tooltip_view,
};

pub(super) struct CommandOutline {
    pub open: bool,
    pub width: f32,
    pub resize: Option<(f32, f32)>,
    pub(super) query: Option<Entity<TextEditorView>>,
    _query_observation: Option<Subscription>,
    failures_only: bool,
    preview_id: Option<String>,
    preview_revision: u64,
    preview: Zeroizing<String>,
    truncated: bool,
    unavailable: bool,
}

impl Default for CommandOutline {
    fn default() -> Self {
        Self {
            open: false,
            width: 280.0,
            resize: None,
            query: None,
            _query_observation: None,
            failures_only: false,
            preview_id: None,
            preview_revision: u64::MAX,
            preview: Zeroizing::new(String::new()),
            truncated: false,
            unavailable: false,
        }
    }
}

fn matches_outline(mark: &TerminalCommandMark, query: &str, failures_only: bool) -> bool {
    (!failures_only || mark.exit_code.is_some_and(|exit| exit != 0))
        && mark
            .command
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
}

impl TerminalPane {
    pub fn toggle_command_outline(&mut self, cx: &mut Context<Self>) {
        self.outline.open = !self.outline.open;
        if self.outline.open && self.outline.query.is_none() {
            if let Some(bounds) = self.bounds {
                self.outline.width = self.outline.width.min(f32::from(bounds.size.width) * 0.45);
            }
            let tokens = self.theme.tokens;
            let placeholder = self.preferences.reading_labels.search.clone();
            let query = cx.new(|cx| {
                let mut editor = TextEditorView::new("", &tokens, cx);
                editor.set_presentation(EditorPresentation::Inline, cx);
                editor.apply_runtime_settings(
                    &tokens,
                    tokens.metrics.font_family.to_string(),
                    tokens.metrics.ui_text_sm,
                    1.5,
                    false,
                    true,
                    cx,
                );
                editor.set_settings(
                    EditorSettings {
                        placeholder: Some(placeholder),
                        indentation_markers: false,
                        ..Default::default()
                    },
                    cx,
                );
                editor
            });
            self.outline._query_observation = Some(cx.observe(&query, |_, _, cx| cx.notify()));
            self.outline.query = Some(query);
        }
        cx.notify();
    }

    pub fn reveal_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(mark) = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == id && !mark.stale && !mark.command_line_clipped)
        else {
            return false;
        };
        let line = mark.start_line;
        let end_line = self.selectable_command_mark_end_line(mark);
        self.selected_command_mark_id = Some(id.to_string());
        self.scroll_to_absolute_line(line, cx);
        if !self.settings.command_marks_enabled {
            let scrollback = self.snapshot.scrollback_lines as i32;
            self.set_selection(Some(TerminalSelection {
                anchor: TerminalGridPoint {
                    line: line as i32 - scrollback,
                    col: 0,
                },
                head: TerminalGridPoint {
                    line: end_line as i32 - scrollback,
                    col: self.snapshot.cols.saturating_sub(1),
                },
                mode: TerminalSelectionMode::Lines,
            }));
        }
        true
    }

    fn update_outline_preview(&mut self) {
        let Some(id) = self.outline.preview_id.as_deref() else {
            return;
        };
        if self.outline.preview_revision == self.terminal_content_revision {
            return;
        }
        self.outline.preview_revision = self.terminal_content_revision;
        self.outline.preview.clear();
        let Some(mark) = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == id && !mark.stale && !mark.command_line_clipped)
        else {
            self.outline.unavailable = true;
            self.outline.truncated = false;
            return;
        };
        self.outline.unavailable = false;
        // A sentinel line and UTF-8 code point distinguish a full preview from a truncated one.
        let raw = Zeroizing::new(self.terminal.lock().command_output_text_with_limits(
            mark,
            101,
            16 * 1024 + 4,
        ));
        let mut end = raw.len().min(16 * 1024);
        if let Some((offset, _)) = raw.match_indices('\n').nth(99) {
            end = end.min(offset);
        }
        while !raw.is_char_boundary(end) {
            end -= 1;
        }
        self.outline.truncated = end < raw.len();
        self.outline.preview.push_str(&raw[..end]);
    }

    pub(super) fn outline_button(
        &self,
        id: impl Into<SharedString>,
        label: String,
        action: impl Fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        button_with(
            &self.theme.tokens,
            label,
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                ..Default::default()
            },
        )
        .id(id.into())
        .on_click(cx.listener(action))
        .into_any_element()
    }

    pub(super) fn reading_background_active(&self) -> bool {
        self.preferences.transparent_background
            || (self.preferences.render_policy.allow_background_images
                && self.preferences.background.is_some())
    }

    pub(super) fn reading_icon(&self, name: &'static str, size: f32) -> AnyElement {
        gpui::svg()
            .path(format!("lucide/{name}.svg"))
            .size(px(size))
            .text_color(rgb(self.theme.tokens.ui.text_muted))
            .into_any_element()
    }

    pub(super) fn reading_close_button(
        &self,
        id: &'static str,
        action: impl Fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tokens = self.theme.tokens;
        let label = self.preferences.reading_labels.close.clone();
        icon_button(
            &tokens,
            self.reading_icon("x", tokens.metrics.sidebar_action_icon_size),
            IconButtonOptions::compact(tokens.metrics.ui_button_sm_height),
        )
        .id(id)
        .tooltip(move |_, cx| tooltip_view(tokens, label.clone(), None, cx))
        .on_click(cx.listener(action))
        .into_any_element()
    }

    pub(super) fn render_command_outline(
        &mut self,
        top: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.update_outline_preview();
        let labels = self.preferences.reading_labels.clone();
        let tokens = self.theme.tokens;
        let has_background = self.reading_background_active();
        let query = Zeroizing::new(
            self.outline
                .query
                .as_ref()
                .map(|query| query.read(cx).buffer().text().to_lowercase())
                .unwrap_or_default(),
        );
        let ids = self
            .command_marks
            .iter()
            .filter(|mark| matches_outline(mark, query.trim(), self.outline.failures_only))
            .map(|mark| mark.command_id.clone())
            .collect::<Vec<_>>();
        let owner = cx.entity();
        let count = ids.len();
        let list = gpui::uniform_list("command-outline-list", count, move |range, _, cx| {
            owner.update(cx, |pane, cx| {
                range
                    .map(|index| pane.render_outline_row(&ids[index], cx))
                    .collect::<Vec<_>>()
            })
        })
        .flex_1()
        .min_h_0();
        let close = self.reading_close_button(
            "outline-close",
            |pane, _, window, cx| {
                pane.outline.open = false;
                window.focus(&pane.focus_handle, cx);
                cx.notify();
            },
            cx,
        );
        let header = section_header(
            &tokens,
            labels.outline.clone(),
            SectionHeaderOptions::new()
                .compact()
                .count(count.to_string()),
            Some(self.reading_icon("list-checks", tokens.metrics.sidebar_action_icon_size)),
            Some(close),
        );
        let opacity =
            (tokens.metrics.sidebar_vibrancy_alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        let mut panel = div()
            .id("command-outline")
            .absolute()
            .top(px(top))
            .bottom_0()
            .right_0()
            .w(px(self.outline.width))
            .bg(color_for_background(
                tokens.ui.bg_panel,
                has_background,
                opacity,
            ))
            .border_l_1()
            .border_color(color_for_background(tokens.ui.border, has_background, 0x80))
            .flex()
            .flex_col()
            .gap(px(tokens.spacing.two))
            .p(px(tokens.spacing.three))
            .overflow_hidden()
            .font_family(tokens.metrics.font_family)
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .on_key_down(|_, _, cx| cx.stop_propagation())
            .child(header);
        if let Some(query) = self.outline.query.clone() {
            let focused = query.focus_handle(cx).is_focused(window);
            panel = panel.child(
                text_input_frame(&tokens, focused)
                    .capture_key_down(|event, window, cx| {
                        if event.keystroke.key == "enter" {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    })
                    .gap(px(tokens.spacing.two))
                    .child(self.reading_icon("search", tokens.metrics.sidebar_action_icon_size))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(tokens.metrics.ui_text_sm * 1.5))
                            .child(query),
                    ),
            );
        }
        let selected = usize::from(self.outline.failures_only);
        panel = panel.child(segmented_control(
            &tokens,
            "outline-filter",
            SegmentedControlOptions::new(selected, selected, 2)
                .compact(self.outline.width)
                .has_background_image(has_background),
            [(false, labels.all.clone()), (true, labels.failed.clone())]
                .into_iter()
                .map(|(failed, label)| {
                    segmented_control_item(&tokens, label, failed == self.outline.failures_only)
                        .id(if failed {
                            "outline-failed"
                        } else {
                            "outline-all"
                        })
                        .on_click(cx.listener(move |pane, _, _, cx| {
                            pane.outline.failures_only = failed;
                            cx.notify();
                        }))
                        .into_any_element()
                })
                .collect(),
        ));
        if count == 0 {
            panel = panel.child(
                empty_state(
                    &tokens,
                    self.reading_icon("search", tokens.metrics.ui_menu_icon_size),
                    if query.is_empty() && !self.outline.failures_only {
                        labels.no_commands.clone()
                    } else {
                        labels.no_results.clone()
                    },
                    None,
                    None,
                )
                .flex_1()
                .min_h_0(),
            );
        } else {
            panel = panel.child(list);
        }
        if self.outline.preview_id.is_some() {
            use oxideterm_gpui_ui::scroll::ScrollableElement;
            panel = panel.child(
                div()
                    .border_t_1()
                    .border_color(rgb(tokens.ui.border))
                    .pt(px(tokens.spacing.two))
                    .flex()
                    .flex_col()
                    .gap(px(tokens.spacing.two))
                    .child(section_header(
                        &tokens,
                        labels.preview.clone(),
                        SectionHeaderOptions::new().compact(),
                        None,
                        None,
                    ))
                    .child(
                        div()
                            .max_h(px(180.0))
                            .overflow_y_scrollbar()
                            .font_family(self.preferences.font_family.clone())
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .child(if self.outline.unavailable {
                                labels.unavailable.clone()
                            } else {
                                self.outline.preview.to_string()
                            }),
                    )
                    .when(self.outline.truncated, |preview| {
                        preview.child(
                            div()
                                .text_color(rgb(tokens.ui.text_muted))
                                .child(labels.truncated.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .child(self.outline_button(
                                "outline-copy-command",
                                labels.copy_command.clone(),
                                |pane, _, _, cx| {
                                    let id = pane.outline.preview_id.clone();
                                    pane.copy_command_mark_command_to_clipboard(id.as_deref(), cx);
                                },
                                cx,
                            ))
                            .child(self.outline_button(
                                "outline-copy-output",
                                labels.copy_output.clone(),
                                |pane, _, _, cx| {
                                    if let Some(mark) = pane.command_marks.iter().find(|mark| {
                                        Some(&mark.command_id) == pane.outline.preview_id.as_ref()
                                            && !mark.stale
                                            && !mark.command_line_clipped
                                    }) {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            pane.terminal.lock().command_output_text_with_limits(
                                                mark,
                                                usize::MAX,
                                                usize::MAX,
                                            ),
                                        ));
                                    }
                                },
                                cx,
                            )),
                    ),
            );
        }
        panel = panel.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(5.0))
                .cursor_col_resize()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|pane, event: &gpui::MouseDownEvent, _, cx| {
                        pane.outline.resize =
                            Some((f32::from(event.position.x), pane.outline.width));
                        cx.stop_propagation();
                    }),
                ),
        );
        overlay_content_boundary(panel.on_mouse_move(cx.listener(
            |pane, event: &gpui::MouseMoveEvent, _, cx| {
                if pane.outline.resize.is_some() {
                    pane.handle_mouse_move(event, cx);
                }
            },
        )))
        .into_any_element()
    }

    fn render_outline_row(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(mark) = self.command_marks.iter().find(|mark| mark.command_id == id) else {
            return div().into_any_element();
        };
        let labels = &self.preferences.reading_labels;
        let status = if mark.stale || mark.command_line_clipped {
            labels.unavailable.clone()
        } else if !mark.is_closed {
            labels.running.clone()
        } else {
            mark.exit_code
                .map(|code| format!("{} {code}", labels.exit_code))
                .unwrap_or_else(|| labels.unknown.clone())
        };
        let duration = mark
            .duration_ms
            .map(|ms| format!(" · {ms} ms"))
            .unwrap_or_default();
        let clicked = id.to_string();
        let hovered = clicked.clone();
        let theme = self.theme.tokens.ui;
        entity_list_row(
            &self.theme.tokens,
            EntityListRowOptions::new()
                .compact()
                .active(self.outline.preview_id.as_deref() == Some(id))
                .has_background_image(self.reading_background_active()),
            None,
            div()
                .truncate()
                .font_family(self.preferences.font_family.clone())
                .child(mark.command.clone().unwrap_or_default())
                .into_any_element(),
            Some(
                div()
                    .truncate()
                    .text_size(px(self.theme.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{status}{duration}"))
                    .into_any_element(),
            ),
            Vec::new(),
            Vec::new(),
        )
        .id(SharedString::from(format!("outline-{id}")))
        .h(px(56.0))
        .cursor_pointer()
        .on_click(cx.listener(move |pane, _, _, cx| {
            pane.reveal_command(&clicked, cx);
            pane.outline.preview_id = Some(clicked.clone());
            pane.outline.preview_revision = u64::MAX;
            cx.notify();
        }))
        .on_hover(cx.listener(move |pane, hovered_now: &bool, _, cx| {
            if *hovered_now && pane.outline.preview_id.as_deref() != Some(&hovered) {
                pane.outline.preview_id = Some(hovered.clone());
                pane.outline.preview_revision = u64::MAX;
                cx.notify();
            }
        }))
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[gpui::test]
    fn open_outline_records_typed_commands_without_enabling_visual_marks(cx: &mut TestAppContext) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                24,
                TerminalUiPreferences {
                    command_marks_enabled: false,
                    command_marks_user_input_observed: false,
                    ..Default::default()
                },
                window,
                cx,
            )
            .unwrap()
        });
        pane.update(cx, |pane, cx| {
            pane.test_accepts_input = true;
            pane.toggle_command_outline(cx);
            pane.terminal
                .lock()
                .feed_recording_output(b"\x1b]7;file://localhost/tmp\x07$ ");
            let events = pane.terminal.lock().take_events();
            for event in events {
                pane.handle_terminal_event(event, cx);
            }
            pane.commit_text("echo outline", cx);
            pane.send_user_protocol_bytes(b"\r", cx);
            assert_eq!(
                pane.command_marks
                    .iter()
                    .filter_map(|mark| mark.command.as_deref())
                    .collect::<Vec<_>>(),
                ["echo outline"]
            );
            assert!(!pane.settings.command_marks_enabled);
            assert!(!pane.preferences.command_marks_enabled);
            assert_eq!(pane.command_mark_gutter_width(), 0.0);
        });
    }

    #[gpui::test]
    fn outline_preview_bounds_utf8_and_joins_soft_wrapped_output(cx: &mut TestAppContext) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                1000,
                24,
                TerminalUiPreferences::default(),
                window,
                cx,
            )
            .unwrap()
        });
        pane.update(cx, |pane, cx| {
            pane.settings.command_marks_enabled = false;
            let id = pane
                .begin_command_mark("cmd", TerminalCommandMarkDetectionSource::CommandBar, cx)
                .unwrap();
            let output = "界".repeat(7000);
            pane.terminal
                .lock()
                .feed_recording_output(format!("cmd\r\n{output}").as_bytes());
            let mark = pane.command_marks.last_mut().unwrap();
            mark.end_line = Some(14);
            mark.is_closed = true;
            assert_eq!(pane.terminal.lock().command_output_text(mark), output);
            pane.outline.preview_id = Some(id);
            pane.update_outline_preview();
            assert_eq!(pane.outline.preview.as_str(), "界".repeat(5461));
            assert!(pane.outline.truncated);
        });
    }

    #[gpui::test]
    fn outline_filters_exact_records_and_never_reveals_discarded_output(cx: &mut TestAppContext) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                24,
                TerminalUiPreferences::default(),
                window,
                cx,
            )
            .unwrap()
        });
        pane.update(cx, |pane, cx| {
            pane.settings.command_marks_enabled = false;
            let first = pane
                .begin_command_mark(
                    "docker build",
                    TerminalCommandMarkDetectionSource::CommandBar,
                    cx,
                )
                .unwrap();
            pane.terminal
                .lock()
                .feed_recording_output(b"docker build\r\nfirst line\r\nsecond line");
            let mark = pane.command_marks.last_mut().unwrap();
            mark.end_line = Some(2);
            mark.is_closed = true;
            mark.exit_code = Some(1);
            assert_eq!(
                pane.terminal
                    .lock()
                    .command_output_text_with_limits(mark, usize::MAX, usize::MAX),
                "first line\nsecond line"
            );
            assert_eq!(
                pane.terminal
                    .lock()
                    .command_output_text_with_limits(mark, 1, 16 * 1024),
                "first line"
            );
            let second = pane
                .begin_command_mark(
                    "docker ps",
                    TerminalCommandMarkDetectionSource::CommandBar,
                    cx,
                )
                .unwrap();
            pane.command_marks.last_mut().unwrap().is_closed = true;
            let third = pane
                .begin_command_mark(
                    "echo done",
                    TerminalCommandMarkDetectionSource::CommandBar,
                    cx,
                )
                .unwrap();
            pane.command_marks.last_mut().unwrap().exit_code = Some(0);
            for (query, failed, expected) in [
                ("docker", false, vec![first.clone(), second]),
                ("docker", true, vec![first.clone()]),
                ("echo", true, vec![]),
                ("echo", false, vec![third]),
            ] {
                assert_eq!(
                    pane.command_marks
                        .iter()
                        .filter(|mark| matches_outline(mark, query, failed))
                        .map(|mark| mark.command_id.clone())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            pane.outline.preview_id = Some(first.clone());
            pane.update_outline_preview();
            assert_eq!(pane.outline.preview.as_str(), "first line\nsecond line");
            assert!(pane.reveal_command(&first, cx));
            assert_eq!(pane.command_mark_gutter_width(), 0.0);
            assert_eq!(
                pane.selected_text_snapshot().as_deref(),
                Some("docker build\nfirst line\nsecond line\n")
            );
            assert_eq!(
                pane.selected_command_mark_id.as_deref(),
                Some(first.as_str())
            );
            pane.command_marks[0].stale = true;
            pane.selected_command_mark_id = None;
            assert!(!pane.reveal_command(&first, cx));
            assert_eq!(pane.selected_command_mark_id, None);
            pane.outline.preview_revision = u64::MAX;
            pane.update_outline_preview();
            assert!(pane.outline.unavailable);
            assert_eq!(pane.outline.preview.as_str(), "");
        });
    }
}
