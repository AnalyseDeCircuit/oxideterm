use super::*;
use gpui::{AnyElement, Focusable};
use oxideterm_gpui_editor::paste::PasteDraft;

pub(super) struct PasteEditor {
    draft: PasteDraft,
    prefix: Zeroizing<Vec<u8>>,
}

impl TerminalPane {
    pub fn paste_editor_focused(&self, window: &Window, cx: &App) -> bool {
        self.paste_editor.as_ref().is_some_and(|paste| {
            paste
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
        let draft = PasteDraft::new(text, self.theme.tokens, cx);
        let editor_id = draft.editor.entity_id();
        self.paste_editor = Some(PasteEditor {
            draft,
            prefix: Zeroizing::new(prefix.to_vec()),
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
                    (draft.draft.editor.entity_id() == editor_id)
                        .then(|| draft.draft.editor.focus_handle(app))
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

    fn edited_paste_text(&self, cx: &App) -> Option<Zeroizing<String>> {
        self.paste_editor
            .as_ref()
            .map(|p| p.draft.submitted_text(cx))
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

    pub(super) fn render_paste_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        Some(self.paste_editor.as_ref()?.draft.render(
            self.theme.tokens,
            &self.preferences.paste_labels,
            window,
            |pane, accept, w, cx| {
                if accept {
                    pane.submit_edited_paste(w, cx)
                } else {
                    pane.close_paste_editor(w, cx)
                }
            },
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyDownEvent, TestAppContext};

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
