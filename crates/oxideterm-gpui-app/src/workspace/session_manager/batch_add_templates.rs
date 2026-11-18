use super::*;

impl WorkspaceApp {
    fn batch_templates(&self) -> BatchTemplates<'_> {
        BatchTemplates {
            manager: &self.connections.session_manager,
            path: self.settings_store.path(),
            i18n: &self.i18n,
        }
    }

    pub(in crate::workspace) fn ensure_batch_templates_loaded(&mut self, cx: &mut Context<Self>) {
        self.batch_templates().ensure_batch_templates_loaded(cx);
    }

    pub(in crate::workspace) fn select_batch_add_template(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.batch_templates().select_batch_add_template(id, cx);
    }

    pub(in crate::workspace) fn save_batch_template_into_selected(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.batch_templates().save_batch_template_into_selected(cx);
    }

    pub(in crate::workspace) fn begin_batch_template_naming(&mut self, cx: &mut Context<Self>) {
        self.batch_templates().begin_batch_template_naming(cx);
        if self.connections.session_manager.read(cx).focused_input()
            == Some(SessionManagerInput::BatchTemplateName)
        {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace) fn commit_batch_template_name(&mut self, cx: &mut Context<Self>) {
        self.batch_templates().commit_batch_template_name(cx);
    }

    pub(in crate::workspace) fn cancel_batch_template_name(&mut self, cx: &mut Context<Self>) {
        self.batch_templates().cancel_batch_template_name(cx);
    }

    pub(in crate::workspace) fn begin_batch_template_rename(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.batch_templates().begin_batch_template_rename(id, cx);
        if self.connections.session_manager.read(cx).focused_input()
            == Some(SessionManagerInput::BatchTemplateName)
        {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace) fn delete_batch_template(&mut self, id: &str, cx: &mut Context<Self>) {
        self.batch_templates().delete_batch_template(id, cx);
    }
}
