use super::*;
use oxideterm_gpui_editor::{
    EditorContextMenuLabels,
    paste::{PasteDraft, TerminalPasteLabels},
};
use zeroize::Zeroizing;

pub(crate) struct PasteReview {
    pub(crate) session: TerminalSessionId,
    draft: PasteDraft,
    labels: TerminalPasteLabels,
}

impl Workspace {
    pub(crate) fn insert_chat_command(
        &mut self,
        command: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.active_session() else {
            return;
        };
        let Some(session) = self.sessions.get(&id) else {
            return;
        };
        if !session.writable(self.ready) {
            self.notice("connections.state.disconnected", cx);
            return;
        }
        let text = Zeroizing::new(command);
        if text.chars().any(char::is_control) {
            self.review_paste(id, text, w, cx);
        } else {
            w.focus(&session.view.focus_handle(cx), cx);
            session.view.update(cx, |v, cx| v.paste_text(&text, cx));
        }
    }
    pub(crate) fn review_paste(
        &mut self,
        session: TerminalSessionId,
        text: Zeroizing<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.paste_review.is_some() || self.active_session() != Some(session) {
            return;
        }
        let draft = PasteDraft::new(text, self.tokens, cx);
        let labels = TerminalPasteLabels {
            edit_title: self.i18n.t("terminal.paste.edit_title"),
            strip_fence: self.i18n.t("terminal.paste.strip_fence"),
            undo: self.i18n.t("terminal.paste.undo"),
            redo: self.i18n.t("terminal.paste.redo"),
            editor_menu: EditorContextMenuLabels {
                copy: self.i18n.t("menu.copy"),
                cut: self.i18n.t("fileManager.cut"),
                paste: self.i18n.t("menu.paste"),
                select_all: self.i18n.t("fileManager.selectAll"),
            },
            cancel: self.i18n.t("terminal.paste.cancel"),
            paste: self.i18n.t("terminal.paste.paste"),
        };
        let terminal = &self.settings.read(cx).settings(cx).terminal;
        let family = terminal
            .font_family
            .terminal_family_name(&terminal.custom_font_family);
        let fallback =
            (!terminal.cjk_font_family.is_empty()).then(|| terminal.cjk_font_family.clone());
        let (weight, size, height) = (
            terminal.font_weight as f32,
            terminal.font_size as f32,
            terminal.line_height as f32,
        );
        draft.editor.update(cx, |e, cx| {
            e.set_context_menu_labels(labels.editor_menu.clone());
            e.apply_ide_runtime_settings(
                &self.tokens,
                family,
                weight,
                fallback,
                size,
                height,
                true,
                false,
                cx,
            );
        });
        w.focus(&draft.editor.focus_handle(cx), cx);
        self.paste_review = Some(PasteReview {
            session,
            draft,
            labels,
        });
        cx.notify();
    }

    pub(super) fn render_paste_review(&self, w: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        let review = self.paste_review.as_ref()?;
        Some(review.draft.render(
            self.tokens,
            &review.labels,
            w,
            Self::finish_paste_review,
            cx,
        ))
    }

    fn finish_paste_review(&mut self, submit: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.paste_review.take() else {
            return;
        };
        if let Some(s) = self.sessions.get(&review.session) {
            w.focus(&s.view.focus_handle(cx), cx);
            if submit && s.writable(self.ready) {
                let text = review.draft.submitted_text(cx);
                s.view.update(cx, |v, cx| v.paste_text(&text, cx));
            } else if submit {
                self.notice("connections.state.disconnected", cx);
            }
        }
        cx.notify();
    }
}
