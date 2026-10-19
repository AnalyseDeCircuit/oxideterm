use super::*;
use oxideterm_gpui_editor::{EditorPresentation, TextEditorView};

pub(super) struct TextAreaChanged(pub Zeroizing<String>);

/// The shared editor owns selection, composition, undo and scroll state.
pub(super) struct PluginTextArea {
    editor: Entity<TextEditorView>,
    source: Zeroizing<String>,
    version: u64,
    tokens: ThemeTokens,
    locale: oxideterm_i18n::Locale,
    _observer: Subscription,
}

impl gpui::EventEmitter<TextAreaChanged> for PluginTextArea {}

impl PluginTextArea {
    pub(super) fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.editor.read(cx).focus_handle(cx).is_focused(window)
    }
    pub(super) fn new(
        control: &plugin_host::NativePluginDeclarativeUiControl,
        tokens: ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = Zeroizing::new(native_plugin_control_value_label(control));
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new_verbatim(source.to_string(), &tokens, cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.set_context_menu_labels(Self::labels(i18n));
            editor.set_read_only(control.disabled);
            editor.set_placeholder(control.placeholder.clone(), cx);
            editor
        });
        let version = editor.read(cx).buffer().version();
        let observer = cx.observe(&editor, |this, editor, cx| {
            let version = editor.read(cx).buffer().version();
            if version != this.version {
                this.version = version;
                cx.emit(TextAreaChanged(Zeroizing::new(
                    editor.read(cx).buffer().text(),
                )));
            }
        });
        Self {
            editor,
            source,
            version,
            tokens,
            locale: i18n.locale(),
            _observer: observer,
        }
    }

    fn labels(i18n: &I18n) -> oxideterm_gpui_editor::EditorContextMenuLabels {
        oxideterm_gpui_editor::EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        }
    }

    pub(super) fn update_definition(
        &mut self,
        control: &plugin_host::NativePluginDeclarativeUiControl,
        tokens: ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        let source = Zeroizing::new(native_plugin_control_value_label(control));
        let source_changed = source != self.source;
        let theme_changed = tokens != self.tokens;
        let locale_changed = i18n.locale() != self.locale;
        self.editor.update(cx, |editor, cx| {
            if source_changed {
                editor.replace_text_external(source.to_string(), cx);
            }
            editor.set_read_only(control.disabled);
            editor.set_placeholder(control.placeholder.clone(), cx);
            if locale_changed {
                editor.set_context_menu_labels(Self::labels(i18n));
            }
            if theme_changed {
                editor.apply_runtime_settings(
                    &tokens,
                    tokens.metrics.markdown_code_font_family.to_string(),
                    tokens.metrics.markdown_body_font_size
                        * tokens.metrics.markdown_code_font_scale,
                    1.5,
                    false,
                    false,
                    cx,
                );
            }
        });
        // Host-driven replacement must not echo back as a user edit.
        if source_changed {
            self.version = self.editor.read(cx).buffer().version();
        }
        self.source = source;
        self.tokens = tokens;
        self.locale = i18n.locale();
    }
}

impl Render for PluginTextArea {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_full().h(px(180.0)).child(self.editor.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn textarea_keeps_multiline_draft_on_refresh_and_emits_edits(cx: &mut gpui::TestAppContext) {
        let mut control: plugin_host::NativePluginDeclarativeUiControl = serde_json::from_value(
            serde_json::json!({"kind":"textarea", "id":"body", "value":"first\nsecond"}),
        )
        .unwrap();
        let tokens = oxideterm_theme::default_tokens();
        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        let area = cx.new(|cx| PluginTextArea::new(&control, tokens, &i18n, cx));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&area, move |_, event: &TextAreaChanged, _| {
                captured.borrow_mut().push(event.0.to_string())
            })
        });
        area.update(cx, |area, cx| {
            area.editor.update(cx, |editor, cx| {
                editor.move_cursor_to_document_end(cx);
                editor.insert_text("\n第三行", cx);
            })
        });
        cx.run_until_parked();
        assert_eq!(*events.borrow(), vec!["first\nsecond\n第三行"]);
        area.update(cx, |area, cx| {
            area.update_definition(&control, tokens, &i18n, cx);
            assert_eq!(
                area.editor.read(cx).buffer().text(),
                "first\nsecond\n第三行"
            );
            area.editor
                .update(cx, |editor, cx| editor.insert_text("!", cx));
        });
        cx.run_until_parked();
        assert_eq!(events.borrow().last().unwrap(), "first\nsecond\n第三行!");
        control.value = Some(serde_json::json!("replacement\nbody"));
        area.update(cx, |area, cx| {
            area.update_definition(&control, tokens, &i18n, cx)
        });
        cx.run_until_parked();
        assert_eq!(
            area.read_with(cx, |area, cx| area.editor.read(cx).buffer().text()),
            "replacement\nbody"
        );
        assert_eq!(events.borrow().len(), 2);
    }
}
