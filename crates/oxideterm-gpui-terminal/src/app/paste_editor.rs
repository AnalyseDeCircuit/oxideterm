use super::*;
use crate::reading_tools::{
    ExtractionShell, extraction_command, remove_line_numbers, remove_prefix,
};
use gpui::{AnyElement, Entity, Focusable, SharedString, div, prelude::*, rgb, rgba};
use oxideterm_gpui_editor::{EditorPresentation, EditorSettings, TextEditorView};
use oxideterm_gpui_ui::button::{
    ActionChipOptions, ButtonOptions, ButtonSize, ButtonVariant, action_chip, button_with,
};
use oxideterm_gpui_ui::modal::{
    dialog_content, dialog_footer, dialog_header, dialog_overlay, modal_body,
    overlay_content_boundary,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::segmented_control::{
    SegmentedControlOptions, segmented_control, segmented_control_item,
};

pub(super) struct PasteEditor {
    pub editor: Entity<TextEditorView>,
    prefix: Entity<TextEditorView>,
    prefix_open: bool,
    original: Zeroizing<String>,
    ending_crlf: Option<bool>,
    ending_revisions: std::collections::HashMap<u64, Option<bool>>,
    archive: bool,
    shell: Option<ExtractionShell>,
    error: bool,
    _observations: Vec<Subscription>,
}

#[derive(Clone, Copy)]
pub(super) enum PasteConversion {
    Numbers,
    Prefix,
    TabsToSpaces,
    SpacesToTabs,
    Lf,
    Crlf,
}

impl TerminalPane {
    pub fn open_paste_editor(
        &mut self,
        from_clipboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from_clipboard {
            let Some(text) = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .filter(|text| !text.is_empty())
            else {
                return;
            };
            self.pending_paste = Some(Zeroizing::new(text));
            self.pending_paste_prefix = None;
        }
        let Some(text) = self.pending_paste.as_deref().map(|value| value.as_str()) else {
            return;
        };
        self.create_paste_editor(text.to_string(), false, window, cx);
    }

    pub fn open_archive_editor(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_paste_editor(path, true, window, cx);
    }

    pub(super) fn refresh_reading_editors(&mut self, cx: &mut Context<Self>) {
        let tokens = self.theme.tokens;
        if let Some(query) = &self.outline.query {
            query.update(cx, |editor, cx| {
                editor.apply_runtime_settings(
                    &tokens,
                    tokens.metrics.font_family.to_string(),
                    tokens.metrics.ui_text_sm,
                    1.5,
                    false,
                    true,
                    cx,
                );
                editor.set_placeholder(Some(self.preferences.reading_labels.search.clone()), cx);
            });
        }
        if let Some(draft) = &self.paste_editor {
            draft.prefix.update(cx, |editor, cx| {
                editor.apply_runtime_settings(
                    &tokens,
                    tokens.metrics.font_family.to_string(),
                    tokens.metrics.ui_text_sm,
                    1.5,
                    false,
                    true,
                    cx,
                )
            });
            draft.editor.update(cx, |editor, cx| {
                editor.apply_runtime_settings(
                    &tokens,
                    self.preferences.font_family.clone(),
                    self.preferences.font_size,
                    self.preferences.line_height,
                    true,
                    true,
                    cx,
                )
            });
        }
    }

    fn create_paste_editor(
        &mut self,
        text: String,
        archive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tokens = self.theme.tokens;
        let make_editor = |text: &str, inline: bool, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut editor = TextEditorView::new(text, &tokens, cx);
                editor.set_border_visible(false);
                editor.set_settings(
                    EditorSettings {
                        soft_wrap: true,
                        soft_wrap_column: None,
                        indentation_markers: false,
                        ..Default::default()
                    },
                    cx,
                );
                if inline {
                    editor.set_presentation(EditorPresentation::Inline, cx);
                }
                editor
            })
        };
        let editor = make_editor(if archive { "" } else { &text }, false, cx);
        let prefix = make_editor(if archive { &text } else { "" }, true, cx);
        let observations = vec![
            cx.observe(&editor, |pane, editor, cx| {
                if let Some(draft) = &mut pane.paste_editor {
                    let revision = editor.read(cx).buffer().content_revision();
                    draft.ending_crlf = *draft
                        .ending_revisions
                        .entry(revision)
                        .or_insert(draft.ending_crlf);
                }
                cx.notify();
            }),
            cx.observe(&prefix, |_, _, cx| cx.notify()),
        ];
        let shell = match self.preferences.semantic_shell {
            oxideterm_terminal_semantic::SemanticShellDialect::Bash
            | oxideterm_terminal_semantic::SemanticShellDialect::Zsh => {
                Some(ExtractionShell::Posix)
            }
            oxideterm_terminal_semantic::SemanticShellDialect::Fish => Some(ExtractionShell::Fish),
            oxideterm_terminal_semantic::SemanticShellDialect::PowerShell => {
                Some(ExtractionShell::PowerShell)
            }
            _ => None,
        };
        window.focus(&editor.focus_handle(cx), cx);
        self.paste_editor = Some(PasteEditor {
            editor,
            prefix,
            prefix_open: false,
            original: Zeroizing::new(text),
            ending_crlf: None,
            ending_revisions: [(0, None)].into(),
            archive,
            shell,
            error: false,
            _observations: observations,
        });
        self.refresh_reading_editors(cx);
        if archive && shell.is_some() {
            self.generate_archive_command(cx);
        }
        self.dismiss_terminal_context_menu(cx);
        cx.notify();
    }

    fn generate_archive_command(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.paste_editor.as_mut() else {
            return;
        };
        let Some(shell) = draft.shell else {
            return;
        };
        let path = Zeroizing::new(draft.prefix.read(cx).buffer().text());
        match extraction_command(&path, shell) {
            Some(command) => {
                draft
                    .editor
                    .update(cx, |editor, cx| editor.replace_text_external(command, cx));
                draft.error = false;
            }
            None => draft.error = true,
        }
        cx.notify();
    }

    fn convert_paste(&mut self, conversion: PasteConversion, cx: &mut Context<Self>) {
        let Some(draft) = self.paste_editor.as_mut() else {
            return;
        };
        let text = Zeroizing::new(draft.editor.read(cx).buffer().text());
        if matches!(conversion, PasteConversion::Lf | PasteConversion::Crlf) {
            let current = draft.editor.read(cx).buffer().content_revision();
            draft.ending_revisions.insert(current, draft.ending_crlf);
            let revision = draft
                .editor
                .update(cx, |editor, cx| editor.push_undo_checkpoint(cx));
            draft.ending_crlf = Some(matches!(conversion, PasteConversion::Crlf));
            draft.ending_revisions.insert(revision, draft.ending_crlf);
            draft.error = false;
            cx.notify();
            return;
        }
        let converted = match conversion {
            PasteConversion::Numbers => remove_line_numbers(&text),
            PasteConversion::Prefix => remove_prefix(&text, &draft.prefix.read(cx).buffer().text()),
            PasteConversion::TabsToSpaces => Some(text.replace('\t', "    ")),
            PasteConversion::SpacesToTabs => Some(text.replace("    ", "\t")),
            PasteConversion::Lf | PasteConversion::Crlf => unreachable!(),
        };
        draft.error = converted.is_none();
        if let Some(text) = converted {
            draft
                .editor
                .update(cx, |editor, cx| editor.replace_text_external(text, cx));
        }
        cx.notify();
    }

    fn edited_paste_text(&self, cx: &App) -> Option<Zeroizing<String>> {
        let draft = self.paste_editor.as_ref()?;
        let edited = Zeroizing::new(draft.editor.read(cx).buffer().text());
        let normalized = Zeroizing::new(draft.original.replace("\r\n", "\n").replace('\r', "\n"));
        if !draft.archive
            && draft
                .ending_revisions
                .get(&draft.editor.read(cx).buffer().content_revision())
                .copied()
                .unwrap_or(draft.ending_crlf)
                .is_none()
            && *edited == *normalized
        {
            return Some(draft.original.clone());
        }
        let ending = draft
            .ending_revisions
            .get(&draft.editor.read(cx).buffer().content_revision())
            .copied()
            .unwrap_or(draft.ending_crlf);
        let crlf = ending.unwrap_or_else(|| draft.original.contains("\r\n"));
        Some(if crlf {
            Zeroizing::new(edited.replace('\n', "\r\n"))
        } else {
            edited
        })
    }

    fn finish_paste_editor(&mut self, sender: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .paste_editor
            .as_ref()
            .is_some_and(|draft| draft.archive && draft.shell.is_none())
        {
            return;
        }
        let Some(text) = self.edited_paste_text(cx).filter(|text| !text.is_empty()) else {
            return;
        };
        let archive = self
            .paste_editor
            .as_ref()
            .is_some_and(|draft| draft.archive);
        self.paste_editor = None;
        if sender || archive {
            // A sender draft is not terminal input, so discard any pending protocol prelude.
            self.pending_paste = None;
            self.pending_paste_prefix = None;
            self.pending_sender_text = Some(text);
            self.request_context_action(TerminalContextAction::OpenSenderDraft, false, cx);
        } else {
            self.pending_paste = Some(text);
            self.confirm_pending_paste(cx);
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn close_paste_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_editor = None;
        self.cancel_pending_paste(cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub fn take_sender_draft(&mut self) -> Option<Zeroizing<String>> {
        self.pending_sender_text.take()
    }

    pub(super) fn render_paste_editor(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(draft) = self.paste_editor.as_ref() else {
            return div().into_any_element();
        };
        let labels = self.preferences.reading_labels.clone();
        let tokens = self.theme.tokens;
        let has_background = self.reading_background_active();
        let mut controls = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(tokens.spacing.two));
        if draft.archive {
            controls = controls.child(if draft.shell.is_none() {
                labels.choose_shell.clone()
            } else {
                labels.shell.clone()
            });
            for (shell, label) in [
                (ExtractionShell::Posix, "Bash / Zsh"),
                (ExtractionShell::Fish, "Fish"),
                (ExtractionShell::PowerShell, "PowerShell"),
            ] {
                controls = controls.child(
                    action_chip(
                        &tokens,
                        label.into(),
                        None,
                        ActionChipOptions::new()
                            .active(draft.shell == Some(shell))
                            .height(tokens.metrics.ui_button_sm_height)
                            .font_size(tokens.metrics.ui_text_sm),
                    )
                    .id(SharedString::from(format!("archive-shell-{shell:?}")))
                    .on_click(cx.listener(move |pane, _, _, cx| {
                        if let Some(draft) = pane.paste_editor.as_mut() {
                            draft.shell = Some(shell);
                        }
                        pane.generate_archive_command(cx);
                    })),
                );
            }
            controls = controls.child(
                button_with(
                    &tokens,
                    labels.generate.clone(),
                    ButtonOptions {
                        size: ButtonSize::Sm,
                        ..Default::default()
                    },
                )
                .id("archive-generate")
                .on_click(cx.listener(|pane, _, _, cx| pane.generate_archive_command(cx))),
            );
        } else {
            for (conversion, label) in [
                (PasteConversion::Numbers, labels.remove_numbers.clone()),
                (PasteConversion::TabsToSpaces, labels.tabs_to_spaces.clone()),
                (PasteConversion::SpacesToTabs, labels.spaces_to_tabs.clone()),
            ] {
                controls = controls.child(
                    action_chip(
                        &tokens,
                        label.clone(),
                        None,
                        ActionChipOptions::new()
                            .height(tokens.metrics.ui_button_sm_height)
                            .font_size(tokens.metrics.ui_text_sm),
                    )
                    .id(SharedString::from(label))
                    .on_click(
                        cx.listener(move |pane, _, _, cx| pane.convert_paste(conversion, cx)),
                    ),
                );
            }
            controls = controls.child(
                action_chip(
                    &tokens,
                    labels.remove_prefix.clone(),
                    None,
                    ActionChipOptions::new()
                        .active(draft.prefix_open)
                        .height(tokens.metrics.ui_button_sm_height)
                        .font_size(tokens.metrics.ui_text_sm),
                )
                .id("paste-prefix-toggle")
                .on_click(cx.listener(|pane, _, window, cx| {
                    if let Some(draft) = pane.paste_editor.as_mut() {
                        draft.prefix_open = !draft.prefix_open;
                        window.focus(
                            &if draft.prefix_open {
                                draft.prefix.focus_handle(cx)
                            } else {
                                draft.editor.focus_handle(cx)
                            },
                            cx,
                        );
                    }
                    cx.notify();
                })),
            );
            let crlf = draft
                .ending_crlf
                .unwrap_or_else(|| draft.original.contains("\r\n"));
            let selected = usize::from(crlf);
            controls = controls.child(segmented_control(
                &tokens,
                "paste-line-ending",
                SegmentedControlOptions::new(selected, selected, 2)
                    .compact(120.0)
                    .has_background_image(has_background),
                [
                    (PasteConversion::Lf, labels.lf.clone(), !crlf),
                    (PasteConversion::Crlf, labels.crlf.clone(), crlf),
                ]
                .into_iter()
                .map(|(conversion, label, selected)| {
                    segmented_control_item(&tokens, label.clone(), selected)
                        .id(SharedString::from(label))
                        .on_click(
                            cx.listener(move |pane, _, _, cx| pane.convert_paste(conversion, cx)),
                        )
                        .into_any_element()
                })
                .collect(),
            ));
        }
        let header = oxideterm_gpui_ui::section::section_header(
            &tokens,
            if draft.archive {
                labels.archive.clone()
            } else {
                labels.paste_edit.clone()
            },
            oxideterm_gpui_ui::section::SectionHeaderOptions::new(),
            None,
            Some(self.reading_close_button(
                "paste-editor-close",
                |pane, _, window, cx| pane.close_paste_editor(window, cx),
                cx,
            )),
        );
        let mut content = modal_body(&tokens)
            .min_h_0()
            .overflow_y_scrollbar()
            .child(controls);
        if draft.archive || draft.prefix_open {
            let prefix_input = oxideterm_gpui_ui::text_input::text_input_frame(
                &tokens,
                draft.prefix.focus_handle(cx).is_focused(window),
            )
            .flex_1()
            .min_w_0()
            .capture_key_down(|event, window, cx| {
                if event.keystroke.key == "enter" {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .w_full()
                    .h(px(tokens.metrics.ui_text_sm * 1.5))
                    .child(draft.prefix.clone()),
            );
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(tokens.spacing.two))
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(if draft.archive {
                                labels.path.clone()
                            } else {
                                labels.prefix.clone()
                            }),
                    )
                    .child(prefix_input)
                    .when(!draft.archive, |row| {
                        row.child(
                            button_with(
                                &tokens,
                                labels.remove_prefix.clone(),
                                ButtonOptions {
                                    size: ButtonSize::Sm,
                                    ..Default::default()
                                },
                            )
                            .id("paste-prefix-apply")
                            .on_click(cx.listener(|pane, _, _, cx| {
                                pane.convert_paste(PasteConversion::Prefix, cx)
                            })),
                        )
                    }),
            );
        }
        content = content.child(
            div()
                .h(px(
                    300.0_f32.min(f32::from(window.viewport_size().height) * 0.42)
                ))
                .min_h(px(80.0))
                .flex_none()
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .overflow_hidden()
                .child(draft.editor.clone()),
        );
        if draft.error {
            content = content.child(
                div()
                    .text_color(rgb(tokens.ui.error))
                    .child(labels.invalid_transform.clone()),
            );
        }
        let button_options = ButtonOptions {
            size: ButtonSize::Default,
            ..Default::default()
        };
        let history = div()
            .flex()
            .items_center()
            .gap(px(tokens.spacing.one))
            .child(
                button_with(
                    &tokens,
                    labels.undo.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        ..button_options
                    },
                )
                .id("paste-undo")
                .debug_selector(|| "paste-editor-undo".into())
                .on_click(cx.listener(|pane, _, _, cx| {
                    if let Some(draft) = &pane.paste_editor {
                        draft
                            .editor
                            .update(cx, |editor, cx| editor.undo_external(cx));
                    }
                })),
            )
            .child(
                button_with(
                    &tokens,
                    labels.redo.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        ..button_options
                    },
                )
                .id("paste-redo")
                .debug_selector(|| "paste-editor-redo".into())
                .on_click(cx.listener(|pane, _, _, cx| {
                    if let Some(draft) = &pane.paste_editor {
                        draft
                            .editor
                            .update(cx, |editor, cx| editor.redo_external(cx));
                    }
                })),
            );
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .flex_wrap()
            .gap(px(tokens.spacing.two))
            .ml_auto()
            .child(
                button_with(&tokens, labels.cancel.clone(), button_options)
                    .id("paste-cancel")
                    .debug_selector(|| "paste-editor-cancel".into())
                    .on_click(
                        cx.listener(|pane, _, window, cx| pane.close_paste_editor(window, cx)),
                    ),
            )
            .child(
                button_with(
                    &tokens,
                    labels.send_to_sender.clone(),
                    ButtonOptions {
                        variant: if draft.archive {
                            ButtonVariant::Default
                        } else {
                            ButtonVariant::Secondary
                        },
                        disabled: draft.archive && draft.shell.is_none(),
                        ..button_options
                    },
                )
                .id("paste-sender")
                .debug_selector(|| "paste-editor-sender".into())
                .on_click(
                    cx.listener(|pane, _, window, cx| pane.finish_paste_editor(true, window, cx)),
                ),
            )
            .when(!draft.archive, |row| {
                row.child(
                    button_with(
                        &tokens,
                        labels.paste.clone(),
                        ButtonOptions {
                            variant: ButtonVariant::Default,
                            ..button_options
                        },
                    )
                    .id("paste-submit")
                    .debug_selector(|| "paste-editor-submit".into())
                    .on_click(cx.listener(|pane, _, window, cx| {
                        pane.finish_paste_editor(false, window, cx)
                    })),
                )
            });
        let footer = dialog_footer(&tokens)
            .h_auto()
            .min_h(px(tokens.metrics.modal_footer_height))
            .flex_none()
            .py(px(tokens.spacing.two))
            .flex_wrap()
            .justify_between()
            .bg(rgba(0x00000000))
            .child(history)
            .child(actions);
        let body = dialog_content(&tokens)
            .debug_selector(|| "paste-editor-dialog".into())
            .bg(oxideterm_gpui_ui::color_for_background(
                tokens.ui.bg_elevated,
                has_background,
                (tokens.metrics.panel_vibrancy_alpha.clamp(0.0, 1.0) * 255.0).round() as u32,
            ))
            .w(px(
                720.0_f32.min(f32::from(window.viewport_size().width) - 32.0)
            ))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 32.0).max(0.0)
            ))
            .flex()
            .flex_col()
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .font_family(tokens.metrics.font_family)
            .child(dialog_header(&tokens).bg(rgba(0x00000000)).child(header))
            .child(content)
            .child(footer);
        let overlay = dialog_overlay(
            &tokens,
            overlay_content_boundary(body).on_key_down(cx.listener(
                |pane, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        pane.close_paste_editor(window, cx);
                    }
                    cx.stop_propagation();
                },
            )),
        )
        .into_any_element();
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopLeft)
                .position_mode(gpui::AnchoredPositionMode::Window)
                .position(gpui::point(px(0.0), px(0.0)))
                .child(
                    div()
                        .relative()
                        .w(window.viewport_size().width)
                        .h(window.viewport_size().height)
                        .child(overlay),
                ),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[gpui::test]
    fn paste_editor_footer_actions_fit_narrow_windows(cx: &mut TestAppContext) {
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
        for width in [720.0, 360.0] {
            cx.simulate_resize(gpui::size(px(width), px(600.0)));
            cx.update(|window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.theme
                        .tokens
                        .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                    pane.theme.tokens.metrics.ui_text_sm = 18.0;
                    pane.pending_paste = Some(Zeroizing::new("echo one\necho two".into()));
                    pane.open_paste_editor(false, window, cx);
                })
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let dialog = cx
                .debug_bounds("paste-editor-dialog")
                .expect("visible editor dialog");
            assert!(dialog.left() >= px(0.0) && dialog.right() <= px(width));
            let mut buttons: Vec<gpui::Bounds<gpui::Pixels>> = Vec::new();
            for selector in [
                "paste-editor-undo",
                "paste-editor-redo",
                "paste-editor-cancel",
                "paste-editor-sender",
                "paste-editor-submit",
            ] {
                let button = cx.debug_bounds(selector).expect("visible editor action");
                assert!(
                    dialog.contains(&button.origin) && dialog.contains(&button.bottom_right()),
                    "{selector} exceeds dialog at {width}px: {button:?} outside {dialog:?}"
                );
                for previous in &buttons {
                    assert!(
                        button.left() >= previous.right()
                            || button.right() <= previous.left()
                            || button.top() >= previous.bottom()
                            || button.bottom() <= previous.top(),
                        "editor actions overlap at {width}px"
                    );
                }
                buttons.push(button);
            }
        }
    }

    #[gpui::test]
    fn paste_editor_undo_preserves_line_endings_and_submit_preserves_protocol_prefix(
        cx: &mut TestAppContext,
    ) {
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
        let delivered = Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorder = delivered.clone();
        cx.update(|window, cx| {
            pane.update(cx, |pane, cx| {
                pane.test_accepts_input = true;
                pane.set_input_broadcaster(Some(Rc::new(move |kind, bytes, _| {
                    recorder.borrow_mut().push((kind, bytes.to_vec()))
                })));
                pane.pending_paste = Some(Zeroizing::new("1. echo one\r\n2. echo two\r\n".into()));
                pane.pending_paste_prefix = Some(Zeroizing::new(b"\x1b[2~".to_vec()));
                pane.open_paste_editor(false, window, cx);
                pane.convert_paste(PasteConversion::Numbers, cx);
                assert_eq!(
                    pane.edited_paste_text(cx).unwrap().as_str(),
                    "echo one\r\necho two\r\n"
                );
                pane.convert_paste(PasteConversion::Lf, cx);
                assert_eq!(
                    pane.edited_paste_text(cx).unwrap().as_str(),
                    "echo one\necho two\n"
                );
                let editor = pane.paste_editor.as_ref().unwrap().editor.clone();
                editor.update(cx, |editor, cx| editor.undo_external(cx));
                assert_eq!(
                    pane.edited_paste_text(cx).unwrap().as_str(),
                    "echo one\r\necho two\r\n"
                );
                editor.update(cx, |editor, cx| editor.undo_external(cx));
                assert_eq!(
                    pane.edited_paste_text(cx).unwrap().as_str(),
                    "1. echo one\r\n2. echo two\r\n"
                );
                editor.update(cx, |editor, cx| editor.redo_external(cx));
                pane.finish_paste_editor(false, window, cx);
            })
        });
        assert_eq!(
            &*delivered.borrow(),
            &[
                (TerminalBroadcastInputKind::Protocol, b"\x1b[2~".to_vec()),
                (
                    TerminalBroadcastInputKind::Paste,
                    b"echo one\r\necho two\r\n".to_vec()
                )
            ]
        );
        delivered.borrow_mut().clear();
        cx.update(|window, cx| {
            pane.update(cx, |pane, cx| {
                pane.pending_paste = Some(Zeroizing::new("do not send".into()));
                pane.open_paste_editor(false, window, cx);
            })
        });
        cx.simulate_keystrokes("enter");
        pane.read_with(cx, |pane, cx| {
            assert!(
                pane.paste_editor
                    .as_ref()
                    .unwrap()
                    .editor
                    .read(cx)
                    .buffer()
                    .text()
                    .contains('\n')
            )
        });
        assert_eq!(&*delivered.borrow(), &[]);
        cx.update(|window, cx| pane.update(cx, |pane, cx| pane.close_paste_editor(window, cx)));
        assert_eq!(&*delivered.borrow(), &[]);
    }
}
