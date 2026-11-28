use super::*;
use gpui::Focusable;
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use oxideterm_gpui_ui::text_input::TextInputAnchor;

pub(super) struct BrowserFields {
    pub(super) views: std::collections::HashMap<FileManagerInput, Entity<LineField>>,
    pub(super) path_anchor: Rc<std::cell::Cell<Option<TextInputAnchor>>>,
    subscriptions: Vec<Subscription>,
    focused: Option<FileManagerInput>,
}

impl BrowserFields {
    pub(super) fn new(dialog: bool, tokens: ThemeTokens, cx: &mut App) -> Self {
        let inputs = if dialog {
            &[FileManagerInput::DialogValue][..]
        } else {
            &[FileManagerInput::Path, FileManagerInput::Filter][..]
        };
        Self {
            views: inputs
                .iter()
                .map(|&i| {
                    (
                        i,
                        cx.new(|cx| LineField::new(tokens, String::new(), !dialog, cx)),
                    )
                })
                .collect(),
            subscriptions: Vec::new(),
            focused: None,
            path_anchor: Default::default(),
        }
    }
}

impl FileManagerView {
    pub fn field_focused(&self, w: &Window, cx: &App) -> bool {
        self.fields
            .views
            .values()
            .any(|f| f.focus_handle(cx).is_focused(w))
    }
    pub(super) fn sync_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.fields.subscriptions.is_empty() {
            for (&input, field) in &self.fields.views {
                self.fields.subscriptions.push(cx.subscribe_in(
                    field,
                    w,
                    move |this, field, event, _, cx| {
                        if this.file_manager.read(cx).focused_input != Some(input) {
                            return;
                        }
                        match event {
                            FieldEvent::Changed => {
                                let value = field.read(cx).text().to_owned();
                                this.file_manager.update(cx, |s, cx| {
                                    let end = s.input_value(input).encode_utf16().count();
                                    s.replace_input(input, Some(0..end), &value, cx);
                                });
                                if input == FileManagerInput::Path {
                                    this.refresh_file_manager_path_completion(cx);
                                }
                            }
                            FieldEvent::Submit | FieldEvent::Cancel => {
                                let key = if matches!(event, FieldEvent::Submit) {
                                    "enter"
                                } else {
                                    "escape"
                                };
                                let event = KeyDownEvent {
                                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                                    is_held: false,
                                    prefer_character_input: false,
                                };
                                if !this.handle_file_manager_dialog_input_footer_key(&event, cx) {
                                    this.handle_file_manager_key(&event, None, None, cx);
                                }
                            }
                        }
                    },
                ));
            }
        }
        let input = self
            .file_manager
            .read(cx)
            .focused_input
            .filter(|i| self.fields.views.contains_key(i));
        if input != self.fields.focused {
            self.fields.focused = input;
            if let Some(input) = input {
                w.focus(&self.fields.views[&input].focus_handle(cx), cx);
            } else if self.field_focused(w, cx) {
                self.pointer(FileManagerPointer::Focus, w, cx);
            }
        }
    }
}
