use crate::{EditorSettings, TextEditorView};
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, ButtonVariant, button_with},
    modal::{
        dialog_content, dialog_footer, dialog_header, dialog_overlay, dialog_title,
        overlay_content_boundary,
    },
    separator::{SeparatorOrientation, separator},
};
use oxideterm_theme::ThemeTokens;
use zeroize::Zeroizing;

#[derive(Clone, Debug)]
pub struct TerminalPasteLabels {
    pub edit_title: String,
    pub strip_fence: String,
    pub undo: String,
    pub redo: String,
    pub editor_menu: crate::EditorContextMenuLabels,
    pub cancel: String,
    pub paste: String,
}

impl Default for TerminalPasteLabels {
    fn default() -> Self {
        Self {
            edit_title: "Edit before pasting".into(),
            strip_fence: "Remove code block markers".into(),
            undo: "Undo".into(),
            redo: "Redo".into(),
            editor_menu: crate::EditorContextMenuLabels::default(),
            cancel: "Cancel".to_string(),
            paste: "Paste".to_string(),
        }
    }
}

pub struct PasteDraft {
    original: Zeroizing<String>,
    pub editor: Entity<TextEditorView>,
    _observation: Subscription,
}

impl PasteDraft {
    pub fn new<T: 'static>(
        text: Zeroizing<String>,
        tokens: ThemeTokens,
        cx: &mut Context<T>,
    ) -> Self {
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
        let subscription = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            original: text,
            editor,
            _observation: subscription,
        }
    }

    pub fn submitted_text(&self, cx: &App) -> Zeroizing<String> {
        let original = &self.original;
        let edited = Zeroizing::new(self.editor.read(cx).buffer().text());
        let normalized = Zeroizing::new(original.replace("\r\n", "\n").replace('\r', "\n"));
        // Opening the editor or undoing all edits must preserve the original bytes,
        // including mixed endings. Edited documents retain the source's CRLF convention.
        if *edited == *normalized {
            original.clone()
        } else if original.contains("\r\n") {
            Zeroizing::new(edited.replace('\n', "\r\n"))
        } else {
            edited
        }
    }

    pub fn render<T: 'static>(
        &self,
        tokens: ThemeTokens,
        labels: &TerminalPasteLabels,
        window: &Window,
        finish: fn(&mut T, bool, &mut Window, &mut Context<T>),
        cx: &Context<T>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width) - 32.0)
            .max(0.0)
            .min(760.0);
        let height = (f32::from(window.viewport_size().height) - 32.0).clamp(0.0, 520.0);
        let options = ButtonOptions {
            size: ButtonSize::Sm,
            ..Default::default()
        };
        let text = Zeroizing::new(self.editor.read(cx).buffer().text());
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
                            .on_click({
                                let editor = self.editor.clone();
                                move |_, _, cx| editor.update(cx, |e, cx| e.undo_external(cx))
                            }),
                    )
                    .child(
                        button_with(&tokens, labels.redo.clone(), tool_options)
                            .id("paste-redo")
                            .on_click({
                                let editor = self.editor.clone();
                                move |_, _, cx| editor.update(cx, |e, cx| e.redo_external(cx))
                            }),
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
                        .on_click({
                            let editor = self.editor.clone();
                            move |_, _, cx| {
                                let text = Zeroizing::new(editor.read(cx).buffer().text());
                                if let Some(body) = fenced_body(&text) {
                                    editor.update(cx, |e, cx| e.replace_text_external(body, cx));
                                }
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .debug_selector(|| "paste-editor".into())
                    .child(self.editor.clone()),
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
                        .on_click(cx.listener(move |owner, _, w, cx| finish(owner, false, w, cx))),
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
                        .on_click(cx.listener(
                            move |owner, _, window, cx| {
                                finish(owner, true, window, cx);
                            },
                        )),
                    ),
            );
        let focus = self.editor.focus_handle(cx);
        let content =
            overlay_content_boundary(content)
                .on_key_up(|_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(move |owner, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        finish(owner, false, window, cx);
                        window.prevent_default();
                    } else if focus.contains_focused(window, cx)
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
        .into_any_element()
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
    use super::fenced_body;
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
}
