use super::*;
use gpui::{AnyElement, Focusable, prelude::*};
use oxideterm_gpui_editor::paste::PasteDraft;
use oxideterm_gpui_ui::button::{ButtonOptions, ButtonSize, ButtonVariant, button_with};

pub(super) const LINE_PASTE_INTERVAL: Duration = Duration::from_millis(100);

pub(super) struct PasteEditor {
    draft: PasteDraft,
    prefix: Zeroizing<Vec<u8>>,
}

impl TerminalPane {
    /// Workspace key capture yields while the paste dialog owns the keyboard.
    pub fn paste_editor_focused(&self, window: &Window, cx: &App) -> bool {
        self.paste_editor.as_ref().is_some_and(|draft| {
            draft.draft.submit_focus.is_focused(window)
                || draft
                    .draft
                    .editor
                    .focus_handle(cx)
                    .contains_focused(window, cx)
        })
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
        self.stop_line_paste();
        let draft = PasteDraft::new(text, self.theme.tokens, cx);
        let editor_id = draft.editor.entity_id();
        self.paste_editor = Some(PasteEditor {
            draft,
            prefix: Zeroizing::new(prefix.to_vec()),
        });
        self.refresh_paste_editor(cx);
        self.dismiss_terminal_context_menu(cx);
        // Clipboard routing also runs without a Window. Focus the primary action in
        // the pane's current window once the dispatch stack has unwound.
        let pane = cx.weak_entity();
        cx.defer(move |app| {
            let _ = app.with_window(pane.entity_id(), |window, app| {
                let focus = pane.upgrade().and_then(|pane| {
                    let draft = pane.read(app).paste_editor.as_ref()?;
                    (draft.draft.editor.entity_id() == editor_id)
                        .then(|| draft.draft.submit_focus.clone())
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
        draft.draft.editor.update(cx, |editor, cx| {
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

    fn edited_paste_text(&self, by_line: bool, cx: &App) -> Option<Zeroizing<String>> {
        let draft = &self.paste_editor.as_ref()?.draft;
        Some(if by_line {
            Zeroizing::new(draft.editor.read(cx).buffer().text())
        } else {
            draft.submitted_text(cx)
        })
    }

    fn submit_edited_paste(&mut self, by_line: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self
            .edited_paste_text(by_line, cx)
            .filter(|text| !text.is_empty())
        else {
            return;
        };
        if let Some(draft) = self.paste_editor.take() {
            if !draft.prefix.is_empty() {
                self.send_user_protocol_bytes(&draft.prefix, cx);
            }
            if by_line {
                self.start_line_paste(text, cx);
            } else {
                self.paste_text(&text, cx);
            }
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn start_line_paste(&mut self, text: Zeroizing<String>, cx: &mut Context<Self>) {
        // The pane owns the timer and the sole remaining draft. Dropping the task
        // on cancellation or session replacement also drops its zeroizing buffer.
        let mut broadcast = self
            .line_paste_broadcast_factory
            .as_ref()
            .map(|capture| capture(cx));
        let observer = self.input_broadcaster.clone();
        self.line_paste_submitted = 0;
        self.line_paste_task = Some(cx.spawn(async move |pane, cx| {
            let mut remaining = text.as_str();
            while !remaining.is_empty() {
                // Native text input can leave CR or CRLF in the editor buffer.
                let end = remaining.find(['\r', '\n']).map_or(remaining.len(), |i| {
                    i + if remaining[i..].starts_with("\r\n") {
                        2
                    } else {
                        1
                    }
                });
                let (line, rest) = remaining.split_at(end);
                let more = pane.update(cx, |pane, cx| {
                    let secret = pane.input_answers_privilege_prompt(line.as_bytes());
                    if !pane.paste_text_without_broadcast(line, true, cx) {
                        pane.stop_line_paste();
                        cx.notify();
                        return false;
                    }
                    if !secret {
                        if let Some(send) = broadcast.as_mut() {
                            if !send(line.as_bytes(), cx) {
                                pane.stop_line_paste();
                                cx.notify();
                                return false;
                            }
                        } else if let Some(send) = &observer {
                            send(TerminalBroadcastInputKind::PasteLine, line.as_bytes(), cx);
                        }
                    }
                    pane.line_paste_submitted += 1;
                    if rest.is_empty() {
                        pane.line_paste_task = None;
                        cx.notify();
                        return false;
                    }
                    true
                });
                if !matches!(more, Ok(true)) {
                    return;
                }
                remaining = rest;
                cx.background_executor().timer(LINE_PASTE_INTERVAL).await;
            }
        }));
    }

    pub(super) fn stop_line_paste(&mut self) -> bool {
        if self.line_paste_task.take().is_none() {
            return false;
        }
        if let Some(sink) = &self.preferences.notice_sink {
            let sent = self.line_paste_submitted;
            sink(TerminalNotice {
                title: self
                    .preferences
                    .paste_labels
                    .interrupted
                    .replace("{{sent}}", &sent.to_string())
                    .replace("{{line}}", &(sent + 1).to_string()),
                description: None,
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Warning,
            });
        }
        true
    }

    fn close_paste_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_editor = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn render_paste_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.line_paste_task.is_some() {
            let tokens = &self.theme.tokens;
            return Some(
                button_with(
                    tokens,
                    self.preferences.paste_labels.stop_sending.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Sm,
                        ..Default::default()
                    },
                )
                .id("paste-stop")
                .debug_selector(|| "paste-stop".into())
                .absolute()
                .right(px(tokens.spacing.three))
                .top(px(tokens.spacing.three))
                .on_click(cx.listener(|pane, _, _, cx| {
                    pane.stop_line_paste();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        Some(self.paste_editor.as_ref()?.draft.render::<Self>(
            self.theme.tokens,
            &self.preferences.paste_labels,
            window,
            |pane, accept, w, cx| {
                if accept {
                    pane.submit_edited_paste(false, w, cx)
                } else {
                    pane.close_paste_editor(w, cx)
                }
            },
            Some(|pane, w, cx| pane.submit_edited_paste(true, w, cx)),
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyDownEvent, TestAppContext};

    #[gpui::test]
    fn protected_paste_defaults_to_submit_and_allows_keyboard_editing(cx: &mut TestAppContext) {
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
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
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
        cx.simulate_keystrokes("tab");
        let strip = cx.debug_bounds("paste-strip-fence").unwrap().center();
        cx.simulate_click(strip, gpui::Modifiers::none());
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(false, cx).unwrap().as_str(),
                "echo one\r\n"
            );
        });
        cx.simulate_keystrokes("secondary-z");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(false, cx).unwrap().as_str(),
                "```sh\r\necho one\r\n```"
            );
        });
        cx.simulate_click(strip, gpui::Modifiers::none());
        cx.simulate_keystrokes("enter");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(false, cx).unwrap().as_str(),
                "\r\necho one\r\n"
            );
        });
        cx.simulate_input("你a");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(false, cx).unwrap().as_str(),
                "\r\n你aecho one\r\n"
            );
        });
        cx.simulate_keystrokes("backspace");
        pane.read_with(cx, |pane, cx| {
            assert_eq!(
                pane.edited_paste_text(false, cx).unwrap().as_str(),
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

        cx.update(|window, cx| {
            pane.update(cx, |pane, cx| {
                pane.open_paste_editor(Zeroizing::new(String::new()), &[], cx);
                let editor = &pane.paste_editor.as_ref().unwrap().draft.editor;
                editor.update(cx, |editor, cx| {
                    use gpui::EntityInputHandler;
                    editor.replace_text_in_range(None, "a\rb\r\nc", window, cx);
                });
            });
            window.draw(cx).clear(cx);
        });
        let send = cx.debug_bounds("paste-send-by-line").unwrap().center();
        cx.simulate_click(send, Default::default());
        cx.run_until_parked();
        for _ in 0..2 {
            cx.executor().advance_clock(LINE_PASTE_INTERVAL);
            cx.run_until_parked();
        }
        assert_eq!(
            &*delivered.borrow(),
            &[
                (TerminalBroadcastInputKind::PasteLine, b"a\r".to_vec()),
                (TerminalBroadcastInputKind::PasteLine, b"b\r\n".to_vec()),
                (TerminalBroadcastInputKind::PasteLine, b"c".to_vec()),
            ]
        );
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
                "paste-send-by-line",
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

    #[gpui::test]
    fn ssh_pane_emits_clipboard_image_only_when_clipboard_has_no_text(cx: &mut TestAppContext) {
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
        let emitted = Rc::new(std::cell::RefCell::new(Vec::<TerminalClipboardImage>::new()));
        let emitted_clone = emitted.clone();
        let delivered = Rc::new(std::cell::RefCell::new(Vec::new()));
        let delivered_clone = delivered.clone();
        let _subscription = cx.update(|_, cx| {
            pane.update(cx, |pane, _| {
                pane.test_accepts_input = true;
                pane.set_input_broadcaster(Some(Rc::new(move |kind, bytes, _| {
                    delivered_clone.borrow_mut().push((kind, bytes.to_vec()));
                })));
            });
            cx.subscribe(&pane, move |pane, event, cx| {
                if *event == TerminalPaneEvent::ClipboardImagePasted {
                    if let Some(image) =
                        pane.update(cx, |pane, _| pane.take_pasted_clipboard_image())
                    {
                        emitted_clone.borrow_mut().push(image);
                    }
                }
            })
        });

        let sample_image = gpui::Image {
            format: gpui::ImageFormat::Png,
            bytes: vec![0x89, b'P', b'N', b'G', 1, 2, 3],
            id: 42,
        };

        // Non-SSH panes (including local terminals) must ignore clipboard images.
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_image(&sample_image));
            pane.update(cx, |pane, cx| {
                pane.session_kind = TerminalSessionKind::LocalPty;
                pane.paste_from_clipboard(cx);
            });
        });
        assert!(emitted.borrow().is_empty());
        assert!(delivered.borrow().is_empty());

        // SSH pane emits ClipboardImagePasted for image-only clipboard contents.
        cx.update(|_, cx| {
            pane.update(cx, |pane, cx| {
                pane.session_kind = TerminalSessionKind::SshPty;
                pane.paste_from_clipboard(cx);
            });
        });
        assert_eq!(
            &*emitted.borrow(),
            &[TerminalClipboardImage {
                extension: "png",
                bytes: sample_image.bytes.clone(),
            }]
        );
        assert!(delivered.borrow().is_empty());
        emitted.borrow_mut().clear();

        // When both text and image are present, text paste must still win.
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem {
                entries: vec![
                    gpui::ClipboardEntry::String(gpui::ClipboardString::new("hello".into())),
                    gpui::ClipboardEntry::Image(sample_image.clone()),
                ],
            });
            pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        });
        assert!(emitted.borrow().is_empty());
        assert_eq!(
            &*delivered.borrow(),
            &[(TerminalBroadcastInputKind::Paste, b"hello".to_vec())]
        );
    }
}
