//! A browser IME commits text independently of keydown. Let the shared editor own the draft
//! selection and composition; publish only committed buffer edits to the chat state.
use super::*;
use gpui::EntityInputHandler;
use oxideterm_gpui_editor::{EditorContextMenuLabels, EditorPresentation, TextEditorView};

pub(super) struct Composer {
    kind: AiEditorKind,
    state: Entity<AiWorkspaceEntity>,
    editor: Entity<TextEditorView>,
    version: u64,
    conversation: Option<String>,
    text: Zeroizing<String>,
    width: f32,
    env: Rc<RefCell<(ThemeTokens, I18n, SharedString)>>,
    _buffer: Subscription,
    state_observer: Option<Subscription>,
    send: WeakEntity<Assistant>,
}

impl Composer {
    pub(super) fn new(
        kind: AiEditorKind,
        state: Entity<AiWorkspaceEntity>,
        env: Rc<RefCell<(ThemeTokens, I18n, SharedString)>>,
        send: WeakEntity<Assistant>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = state.read(cx);
        let (id, draft) = Self::source(kind, snapshot);
        let conversation = id.clone();
        let text = Zeroizing::new(draft.clone());
        let (editor, subscription) = Self::new_editor(&text, &env, cx);
        let version = editor.read(cx).buffer().version();
        Self {
            kind,
            state,
            editor,
            version,
            conversation,
            text,
            width: 320.,
            env,
            _buffer: subscription,
            state_observer: None,
            send,
        }
    }

    fn configure(
        editor: &mut TextEditorView,
        env: &(ThemeTokens, I18n, SharedString),
        cx: &mut Context<TextEditorView>,
    ) {
        let (tokens, i, font) = env;
        editor.set_presentation(EditorPresentation::Inline, cx);
        editor.set_placeholder(Some(i.t("ai.input.placeholder")), cx);
        editor.set_context_menu_labels(EditorContextMenuLabels {
            copy: i.t("menu.copy"),
            cut: i.t("fileManager.cut"),
            paste: i.t("menu.paste"),
            select_all: i.t("fileManager.selectAll"),
        });
        editor.apply_runtime_settings(tokens, font.to_string(), 13., 20. / 13., true, false, cx);
    }

    fn new_editor(
        text: &str,
        env: &RefCell<(ThemeTokens, I18n, SharedString)>,
        cx: &mut Context<Self>,
    ) -> (Entity<TextEditorView>, Subscription) {
        let editor = cx.new(|cx| {
            let mut e = TextEditorView::new_verbatim(text.to_owned(), &env.borrow().0, cx);
            Self::configure(&mut e, &env.borrow(), cx);
            e.move_cursor_to_document_end(cx);
            e
        });
        let subscription = cx.observe(&editor, |this, _, cx| this.commit(cx));
        (editor, subscription)
    }

    fn source(kind: AiEditorKind, s: &AiWorkspaceEntity) -> (&Option<String>, &String) {
        match kind {
            AiEditorKind::Composer => (
                &s.conversation_state().active_conversation_id,
                &s.chat_ui().draft,
            ),
            AiEditorKind::MessageEdit => (
                &s.chat_ui().editing_message_id,
                &s.chat_ui().editing_message_draft,
            ),
        }
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        let buffer = self.editor.read(cx).buffer();
        if self.version == buffer.version() {
            return;
        }
        self.version = buffer.version();
        self.text = Zeroizing::new(buffer.text());
        self.state.update(cx, |s, cx| {
            match self.kind {
                AiEditorKind::Composer => s.set_chat_draft(self.text.to_string()),
                AiEditorKind::MessageEdit => {
                    if s.chat_ui().editing_message_id != self.conversation {
                        return;
                    }
                    let end = s.chat_ui().editing_message_draft.encode_utf16().count();
                    s.replace_message_edit(Some(0..end), &self.text);
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn sync(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let ai = self.state.read(cx);
        let (id, next) = Self::source(self.kind, ai);
        let switch = &self.conversation != id;
        if !switch && self.text.as_str() == next {
            return;
        }
        self.conversation = id.clone();
        self.text = Zeroizing::new(next.clone());
        if switch || self.text.is_empty() {
            // A submitted draft or another conversation needs a fresh undo history.
            let focused = self.focus_handle(cx).is_focused(w);
            (self.editor, self._buffer) = Self::new_editor(&self.text, &self.env, cx);
            if (focused && self.kind == AiEditorKind::Composer)
                || (self.kind == AiEditorKind::MessageEdit && self.conversation.is_some())
            {
                w.focus(&self.focus_handle(cx), cx);
            }
        } else {
            self.editor.update(cx, |e, cx| {
                e.replace_text_external(self.text.to_string(), cx);
                e.move_cursor_to_document_end(cx);
            });
        }
        self.version = self.editor.read(cx).buffer().version();
        cx.notify();
    }

    pub(super) fn appearance(&self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |e, cx| Self::configure(e, &self.env.borrow(), cx));
    }

    pub(super) fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.width != width {
            self.width = width;
            cx.notify();
        }
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for Composer {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.state_observer.is_none() {
            self.state_observer =
                Some(cx.observe_in(&self.state, w, |this, _, w, cx| this.sync(w, cx)));
            self.sync(w, cx);
        }
        let rows = ai_input_visual_lines(&self.text, ai_input_soft_wrap_columns(self.width))
            .len()
            .clamp(1, 12);
        div()
            .w_full()
            .min_w_0()
            .h(px(rows as f32 * 20. + 2.))
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left {
                    this.state.update(cx, |s, cx| {
                        match this.kind {
                            AiEditorKind::Composer => s.focus_chat_input(),
                            AiEditorKind::MessageEdit => s.focus_message_edit(),
                        }
                        s.set_model_selector_search_focused(false);
                        cx.notify();
                    });
                }
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key != "enter"
                    || event.keystroke.modifiers.control
                    || event.keystroke.modifiers.platform
                    || event.keystroke.modifiers.alt
                {
                    return;
                }
                if this
                    .editor
                    .update(cx, |e, cx| e.marked_text_range(w, cx).is_some())
                {
                    return;
                }
                if event.keystroke.modifiers.shift {
                    this.editor.update(cx, |e, cx| e.insert_text("\n", cx));
                } else {
                    this.commit(cx);
                    let _ = this.send.update(cx, |a, cx| match this.kind {
                        AiEditorKind::Composer => a.send(cx),
                        AiEditorKind::MessageEdit => a.start_turn(Turn::Edit, cx),
                    });
                }
                cx.stop_propagation();
                w.prevent_default();
            }))
            .child(self.editor.clone())
    }
}
