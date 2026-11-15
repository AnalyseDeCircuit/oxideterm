use super::*;
use gpui::{AnyElement, Entity, Focusable, KeyDownEvent, div, prelude::*, rgb};
use oxideterm_gpui_editor::{EditorSettings, TextEditorView};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, ButtonVariant, button_with},
    modal::{
        dialog_content, dialog_footer, dialog_header, dialog_overlay, dialog_title,
        overlay_content_boundary,
    },
    separator::{SeparatorOrientation, separator},
};

pub(super) struct PasteEditor {
    original: Zeroizing<String>,
    prefix: Zeroizing<Vec<u8>>,
    editor: Entity<TextEditorView>,
    _observation: Subscription,
}

impl TerminalPane {
    pub fn paste_editor_focused(&self, window: &Window, cx: &App) -> bool {
        self.paste_editor
            .as_ref()
            .is_some_and(|draft| draft.editor.focus_handle(cx).contains_focused(window, cx))
    }

    pub(super) fn edit_clipboard_paste(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .filter(|text| !text.is_empty())
        else {
            return;
        };
        self.open_paste_editor(Zeroizing::new(text), &[], cx);
    }

    pub(super) fn open_paste_editor(
        &mut self,
        text: Zeroizing<String>,
        prefix: &[u8],
        cx: &mut Context<Self>,
    ) {
        if self.paste_editor.is_some() {
            return;
        }
        let tokens = self.theme.tokens;
        // This pane owns the transient editor and its undo history. Neither is persisted;
        // submission or cancellation drops the entity and all draft UI state.
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(text.as_str(), &tokens, cx);
            editor.set_border_visible(false);
            editor.set_settings(
                EditorSettings {
                    soft_wrap: true,
                    indentation_markers: false,
                    highlight_current_line: false,
                    ..Default::default()
                },
                cx,
            );
            editor
        });
        let editor_id = editor.entity_id();
        let observation = cx.observe(&editor, |_, _, cx| cx.notify());
        self.paste_editor = Some(PasteEditor {
            original: text,
            prefix: Zeroizing::new(prefix.to_vec()),
            editor,
            _observation: observation,
        });
        self.refresh_paste_editor(cx);
        self.dismiss_terminal_context_menu(cx);
        // Clipboard routing also runs without a Window. Focus the draft in
        // the pane's current window once the dispatch stack has unwound.
        let pane = cx.weak_entity();
        cx.defer(move |app| {
            let _ = app.with_window(pane.entity_id(), |window, app| {
                let focus = pane.upgrade().and_then(|pane| {
                    let draft = pane.read(app).paste_editor.as_ref()?;
                    (draft.editor.entity_id() == editor_id).then(|| draft.editor.focus_handle(app))
                });
                if let Some(focus) = focus {
                    window.focus(&focus, app);
                }
            });
        });
        cx.notify();
    }

    pub(super) fn refresh_paste_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = &self.paste_editor else {
            return;
        };
        draft.editor.update(cx, |editor, cx| {
            editor.set_context_menu_labels(self.preferences.paste_labels.editor_menu.clone());
            editor.apply_ide_runtime_settings(
                &self.theme.tokens,
                self.preferences.font_family.clone(),
                self.preferences.font_weight,
                self.preferences.cjk_font_family.clone(),
                self.preferences.font_size,
                self.preferences.line_height,
                true,
                false,
                cx,
            );
        });
    }

    fn edited_paste_text(&self, cx: &App) -> Option<Zeroizing<String>> {
        let draft = self.paste_editor.as_ref()?;
        let original = &draft.original;
        let edited = Zeroizing::new(draft.editor.read(cx).buffer().text());
        let normalized = Zeroizing::new(original.replace("\r\n", "\n").replace('\r', "\n"));
        // Opening the editor or undoing all edits must preserve the original bytes,
        // including mixed endings. Edited documents retain the source's CRLF convention.
        if *edited == *normalized {
            Some(original.clone())
        } else if original.contains("\r\n") {
            Some(Zeroizing::new(edited.replace('\n', "\r\n")))
        } else {
            Some(edited)
        }
    }

    fn submit_edited_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.edited_paste_text(cx).filter(|text| !text.is_empty()) else {
            return;
        };
        if let Some(draft) = self.paste_editor.take() {
            if !draft.prefix.is_empty() {
                self.send_user_protocol_bytes(&draft.prefix, cx);
            }
            self.paste_text(&text, cx);
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn close_paste_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_editor = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn strip_paste_fence(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = &self.paste_editor else {
            return;
        };
        let text = Zeroizing::new(draft.editor.read(cx).buffer().text());
        if let Some(body) = fenced_body(&text) {
            draft
                .editor
                .update(cx, |editor, cx| editor.replace_text_external(body, cx));
        }
    }

    pub(super) fn render_paste_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let draft = self.paste_editor.as_ref()?;
        let tokens = self.theme.tokens;
        let labels = &self.preferences.paste_labels;
        let width = (f32::from(window.viewport_size().width) - 32.0)
            .max(0.0)
            .min(760.0);
        let height = (f32::from(window.viewport_size().height) - 32.0).clamp(0.0, 520.0);
        let options = ButtonOptions {
            size: ButtonSize::Sm,
            ..Default::default()
        };
        let text = Zeroizing::new(draft.editor.read(cx).buffer().text());
        let empty = text.is_empty();
        let tool_options = ButtonOptions {
            variant: ButtonVariant::Ghost,
            ..options
        };
        let content = dialog_content(&tokens)
            .debug_selector(|| "paste-dialog".into())
            .w(px(width))
            .h(px(height))
            .bg(rgb(tokens.ui.bg_panel))
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&tokens))
            .flex()
            .flex_col()
            .font_family(tokens.metrics.font_family)
            .text_color(rgb(tokens.ui.text))
            .child(dialog_header(&tokens).child(dialog_title(&tokens, labels.edit_title.clone())))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .flex_none()
                    .px(px(tokens.spacing.two))
                    .py(px(tokens.spacing.one))
                    .border_b_1()
                    .border_color(rgb(tokens.ui.border))
                    .child(
                        button_with(&tokens, labels.undo.clone(), tool_options)
                            .id("paste-undo")
                            .debug_selector(|| "paste-undo".into())
                            .on_click(cx.listener(|pane, _, _, cx| {
                                if let Some(draft) = &pane.paste_editor {
                                    draft
                                        .editor
                                        .update(cx, |editor, cx| editor.undo_external(cx));
                                }
                            })),
                    )
                    .child(
                        button_with(&tokens, labels.redo.clone(), tool_options)
                            .id("paste-redo")
                            .on_click(cx.listener(|pane, _, _, cx| {
                                if let Some(draft) = &pane.paste_editor {
                                    draft
                                        .editor
                                        .update(cx, |editor, cx| editor.redo_external(cx));
                                }
                            })),
                    )
                    .child(
                        separator(&tokens, SeparatorOrientation::Vertical)
                            .h(px(tokens.metrics.ui_text_sm))
                            .mx(px(tokens.spacing.one)),
                    )
                    .child(
                        button_with(
                            &tokens,
                            labels.strip_fence.clone(),
                            ButtonOptions {
                                disabled: fenced_body(&text).is_none(),
                                ..tool_options
                            },
                        )
                        .id("paste-strip-fence")
                        .debug_selector(|| "paste-strip-fence".into())
                        .on_click(cx.listener(|pane, _, _, cx| pane.strip_paste_fence(cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .debug_selector(|| "paste-editor".into())
                    .child(draft.editor.clone()),
            )
            .child(
                dialog_footer(&tokens)
                    .h_auto()
                    .flex_none()
                    .min_h(px(tokens.metrics.modal_footer_height))
                    .py(px(tokens.spacing.two))
                    .flex_wrap()
                    .child(
                        button_with(
                            &tokens,
                            labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                ..options
                            },
                        )
                        .id("paste-cancel")
                        .debug_selector(|| "paste-cancel".into())
                        .on_click(
                            cx.listener(|pane, _, window, cx| pane.close_paste_editor(window, cx)),
                        ),
                    )
                    .child(
                        button_with(
                            &tokens,
                            labels.paste.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                disabled: empty,
                                ..options
                            },
                        )
                        .id("paste-submit")
                        .debug_selector(|| "paste-submit".into())
                        .on_click(cx.listener(|pane, _, window, cx| {
                            pane.submit_edited_paste(window, cx);
                        })),
                    ),
            );
        let content =
            overlay_content_boundary(content)
                .on_key_up(|_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(move |pane, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        pane.close_paste_editor(window, cx);
                        window.prevent_default();
                    } else if pane.paste_editor_focused(window, cx)
                        && !event.keystroke.modifiers.control
                        && !event.keystroke.modifiers.platform
                        && event.keystroke.key_char.as_deref().is_some_and(|text| {
                            !text.is_empty() && !text.chars().any(char::is_control)
                        })
                    {
                        // X11 and Wayland commit characters only after an unhandled keydown.
                        // The focused editor owns text input; the terminal skips keys while editing.
                        return;
                    }
                    cx.stop_propagation();
                }));
        Some(
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
                            .child(dialog_overlay(&tokens, content)),
                    ),
            )
            .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
            .into_any_element(),
        )
    }
}

// Only unwrap one complete outer fenced block. Prose, multiple blocks and code
// whitespace stay untouched; this operation is never part of ordinary paste.
fn fenced_body(text: &str) -> Option<&str> {
    let text = text.trim();
    let (opening, rest) = text.split_once('\n')?;
    let marker = *opening.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let count = opening.bytes().take_while(|byte| *byte == marker).count();
    if count < 3 || (marker == b'`' && opening[count..].contains('`')) {
        return None;
    }
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let closing = line.trim();
        if closing.len() >= count && closing.bytes().all(|byte| byte == marker) {
            return rest[offset + line.len()..]
                .trim()
                .is_empty()
                .then_some(&rest[..offset]);
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn paste_fence_removal_preserves_body_and_requires_one_complete_block() {
        for (input, expected) in [
            (
                "```sh\n  echo one\n\n\techo two  \n```",
                Some("  echo one\n\n\techo two  \n"),
            ),
            ("\n~~~bash\r\necho one\r\n~~~~\r\n", Some("echo one\r\n")),
            (
                "````markdown\n```sh\necho one\n```\n````",
                Some("```sh\necho one\n```\n"),
            ),
            ("```\n```", Some("")),
            ("echo `date`\n", None),
            ("intro\n```sh\necho one\n```", None),
            ("```sh\necho one\n```\noutro", None),
            ("```sh\necho one\n```\n```sh\necho two\n```", None),
            ("```sh\necho one", None),
            ("```sh\necho one\n~~~", None),
        ] {
            assert_eq!(fenced_body(input), expected);
        }
    }

    #[gpui::test]
    fn protected_paste_opens_editor_and_only_button_delivers(cx: &mut TestAppContext) {
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
        cx.simulate_resize(gpui::size(px(900.0), px(650.0)));
        let delivered = Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorder = delivered.clone();
        cx.update(|window, cx| {
            window.activate_window();
            pane.update(cx, |pane, cx| {
                pane.theme
                    .tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                pane.test_accepts_input = true;
                pane.settings.paste_protection = true;
                pane.set_input_broadcaster(Some(Rc::new(move |kind, bytes, _| {
                    recorder.borrow_mut().push((kind, bytes.to_vec()));
                })));
                window.focus(&pane.focus_handle, cx);
            });
            window.draw(cx).clear(cx);
        });

        let original = "echo one\r\necho two\recho three\n";
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(original.into()));
            pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("paste-editor").is_some());
        assert!(delivered.borrow().is_empty());
        let submit = cx.debug_bounds("paste-submit").unwrap().center();
        cx.simulate_click(submit, gpui::Modifiers::none());
        assert_eq!(
            &*delivered.borrow(),
            &[(
                TerminalBroadcastInputKind::Paste,
                original.as_bytes().to_vec()
            )]
        );
        delivered.borrow_mut().clear();

        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("```sh\r\necho one\r\n```".into()));
            pane.update(cx, |pane, cx| {
                pane.paste_from_clipboard_after(b"\x1b[2~", cx)
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let strip = cx.debug_bounds("paste-strip-fence").unwrap().center();
        cx.simulate_click(strip, gpui::Modifiers::none());
        pane.read_with(cx, |pane, cx| {
            assert_eq!(pane.edited_paste_text(cx).unwrap().as_str(), "echo one\r\n");
        });
        cx.simulate_keystrokes("secondary-z");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(cx).unwrap().as_str(),
                "```sh\r\necho one\r\n```"
            );
        });
        cx.simulate_click(strip, gpui::Modifiers::none());
        cx.simulate_keystrokes("enter");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(cx).unwrap().as_str(),
                "\r\necho one\r\n"
            );
        });
        cx.simulate_input("你a");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(cx).unwrap().as_str(),
                "\r\n你aecho one\r\n"
            );
        });
        cx.simulate_keystrokes("backspace");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(cx).unwrap().as_str(),
                "\r\n你echo one\r\n"
            );
        });
        pane.update(cx, |pane, cx| {
            pane.handle_focus_change(false, cx);
            assert!(!pane.handle_unfocused_key(
                &KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("x").unwrap().with_simulated_ime(),
                    is_held: false,
                    prefer_character_input: false,
                },
                cx,
            ));
        });
        assert!(delivered.borrow().is_empty(), "editing must not send input");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let submit = cx.debug_bounds("paste-submit").unwrap().center();
        cx.simulate_click(submit, gpui::Modifiers::none());
        assert_eq!(
            &*delivered.borrow(),
            &[
                (TerminalBroadcastInputKind::Protocol, b"\x1b[2~".to_vec()),
                (
                    TerminalBroadcastInputKind::Paste,
                    "\r\n你echo one\r\n".as_bytes().to_vec()
                ),
            ]
        );
        delivered.borrow_mut().clear();
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("echo cancel\necho again".into()));
            pane.update(cx, |pane, cx| {
                pane.paste_from_clipboard_after(b"\x1b[2~", cx)
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("escape");
        pane.read_with(cx, |pane, _| {
            assert!(pane.paste_editor.is_none());
        });
        assert!(delivered.borrow().is_empty(), "cancel must not send input");
    }

    #[gpui::test]
    fn paste_dialog_keeps_editor_and_actions_inside_narrow_windows(cx: &mut TestAppContext) {
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
        for width in [900.0, 360.0] {
            cx.simulate_resize(gpui::size(px(width), px(480.0)));
            cx.update(|window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.paste_editor = None;
                    pane.theme
                        .tokens
                        .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                    pane.preferences.paste_labels.edit_title = "粘贴前编辑".into();
                    pane.preferences.paste_labels.strip_fence =
                        "Codeblock-Markierungen entfernen".into();
                    pane.open_paste_editor(Zeroizing::new("```sh\necho one\n```".into()), &[], cx);
                });
                window.draw(cx).clear(cx);
            });
            let dialog = cx.debug_bounds("paste-dialog").unwrap();
            assert!(dialog.left() >= px(0.0) && dialog.right() <= px(width));
            assert!(dialog.top() >= px(0.0) && dialog.bottom() <= px(480.0));
            for name in [
                "paste-editor",
                "paste-strip-fence",
                "paste-cancel",
                "paste-submit",
            ] {
                let bounds = cx.debug_bounds(name).unwrap();
                assert!(
                    dialog.contains(&bounds.origin) && dialog.contains(&bounds.bottom_right()),
                    "{name} outside dialog"
                );
                assert!(bounds.size.height > px(0.0), "{name} must remain visible");
            }
        }
    }
}
