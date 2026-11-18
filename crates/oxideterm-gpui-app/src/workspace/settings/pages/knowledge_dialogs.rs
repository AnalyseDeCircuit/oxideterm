use super::*;
use oxideterm_gpui_settings_view::knowledge_page::dialog::{
    KnowledgeDialog, KnowledgeDialogAction, KnowledgeDialogContent,
};

impl WorkspaceApp {
    fn render_knowledge_dialog(
        &self,
        content: KnowledgeDialogContent<'_>,
        dispatch: impl Fn(&mut Self, KnowledgeDialogAction, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        self.with_settings_controls(cx, |controls, cx| {
            KnowledgeDialog {
                controls,
                focus: self.standard_confirm_focus(),
                dispatch: Rc::new(move |action, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        app.clear_standard_confirm_focus();
                        dispatch(app, action, cx);
                    });
                }),
            }.render(content, cx)
        })
    }

    pub(in crate::workspace) fn render_knowledge_create_collection_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ai = self.ai_entity.read(cx);
        ai.knowledge_create_dialog_open().then(|| self.render_knowledge_dialog(
            KnowledgeDialogContent::Collection {
                name: ai.knowledge_new_collection_name(),
                scope_label: self.knowledge_collection_scope_label(cx),
                phase: ai.knowledge_create_presence().phase(),
            },
            |app, action, cx| {
                if matches!(action, KnowledgeDialogAction::Confirm) {
                    app.knowledge_create_collection(cx);
                }
                app.close_knowledge_create_dialog(cx);
            },
            cx,
        ))
    }

    pub(in crate::workspace) fn render_knowledge_new_document_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ai = self.ai_entity.read(cx);
        ai.knowledge_document_dialog_open().then(|| self.render_knowledge_dialog(
            KnowledgeDialogContent::Document {
                title: ai.knowledge_new_document_title(),
                format_label: self.knowledge_document_format_label(cx),
                error: ai.knowledge_new_document_error(),
                phase: ai.knowledge_document_presence().phase(),
            },
            |app, action, cx| {
                if !matches!(action, KnowledgeDialogAction::Confirm) || app.knowledge_create_blank_document(cx) {
                    app.close_knowledge_document_dialog(cx);
                }
            },
            cx,
        ))
    }

    pub(in crate::workspace) fn open_knowledge_create_dialog(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |entity, cx| {
            entity.open_knowledge_create_dialog();
            cx.notify();
        });
        cx.notify();
    }

    fn knowledge_collection_scope_label(&self, cx: &App) -> String {
        let selected_connection_id = self
            .ai_entity
            .read(cx)
            .knowledge_new_collection_connection_id();
        selected_connection_id
            .and_then(|connection_id| self.connection_store.get(connection_id))
            .map(|connection| connection.name.clone())
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.scope_global"))
    }

    pub(in crate::workspace) fn close_knowledge_create_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_knowledge_dialog_input_state(cx);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.ai_entity.update(cx, |entity, cx| {
            entity.close_knowledge_create_dialog(delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn open_knowledge_document_dialog(
        &mut self,
        collection_id: String,
        open_in_workspace: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let owner_window_id = window.window_handle().window_id();
        self.ai_entity.update(cx, |entity, cx| {
            entity.open_knowledge_document_dialog(
                collection_id,
                owner_window_id,
                open_in_workspace,
            );
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_knowledge_document_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_knowledge_dialog_input_state(cx);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.ai_entity.update(cx, |entity, cx| {
            entity.close_knowledge_document_dialog(delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn render_knowledge_delete_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let confirm = self.ai_entity.read(cx).knowledge_delete_confirm()?;
        Some(self.render_knowledge_dialog(
            KnowledgeDialogContent::Delete(confirm),
            |app, action, cx| {
                if matches!(action, KnowledgeDialogAction::Confirm) {
                    app.knowledge_confirm_delete(cx);
                } else {
                    app.ai_entity.update(cx, |ai, cx| {
                        ai.clear_knowledge_delete_confirm();
                        cx.notify();
                    });
                    cx.notify();
                }
            },
            cx,
        ))
    }
}
