use super::*;
use oxideterm_connections::BatchExistingConnection;
use oxideterm_gpui_connections::session_manager::BatchAddEditorKind;

impl WorkspaceApp {
    pub(in crate::workspace) fn apply_batch_window_action(
        &mut self,
        action: BatchWindowAction,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match action {
            BatchWindowAction::Apply => self.apply_batch_add(cx),
            BatchWindowAction::PickDataFile => self.import_batch_data_file(cx),
            BatchWindowAction::FocusEditor(kind) => {
                if let Some(window) = window {
                    self.focus_batch_add_editor(kind, window, cx);
                }
            }
            BatchWindowAction::ChooseTemplate(id) => self.select_batch_add_template(&id, cx),
            BatchWindowAction::NameTemplate => self.begin_batch_template_naming(cx),
            BatchWindowAction::RenameTemplate(id) => self.begin_batch_template_rename(&id, cx),
            BatchWindowAction::CommitTemplateName => self.commit_batch_template_name(cx),
            BatchWindowAction::DeleteTemplate(id) => self.delete_batch_template(&id, cx),
            BatchWindowAction::SaveTemplate => self.save_batch_template_into_selected(cx),
        }
    }

    pub(in crate::workspace) fn open_batch_add_dialog(&mut self, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        let existing = std::rc::Rc::new(move |cx: &App| {
            owner.upgrade().map_or_else(Vec::new, |owner| {
                owner
                    .read(cx)
                    .connection_store
                    .connections()
                    .iter()
                    .map(BatchExistingConnection::from_saved)
                    .collect()
            })
        });
        let font = settings_mono_font_family(self.settings_store.settings()).to_string();
        self.connections.session_manager.update(cx, |state, cx| {
            state.open_batch_add(self.tokens, font, &self.i18n, existing, cx)
        });
        self.ensure_batch_templates_loaded(cx);
    }

    pub(in crate::workspace) fn begin_batch_add_dialog_exit(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.connections.session_manager.update(cx, |state, cx| {
            state.begin_batch_add_dialog_exit(delay, cx);
        });
    }

    pub(in crate::workspace) fn focus_batch_add_editor(
        &mut self,
        kind: BatchAddEditorKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self
            .connections
            .session_manager
            .update(cx, |state, cx| state.focus_batch_editor(kind, cx));
        self.clear_ime_selection();
        self.ime_marked_text = None;
        if let Some(editor) = editor {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
    }

    pub(in crate::workspace) fn batch_add_text_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        let Some(dialog) = self.connections.session_manager.read(cx).batch_add.as_ref() else {
            return false;
        };
        [dialog.template_editor.as_ref(), dialog.data_editor.as_ref()]
            .into_iter()
            .flatten()
            .any(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(in crate::workspace) fn apply_batch_add(&mut self, cx: &mut Context<Self>) {
        if self.connections.session_manager.update(cx, |state, cx| {
            state.apply_batch_to_store(&mut self.connection_store, &self.i18n, cx)
        }) {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(in crate::workspace) fn import_batch_data_file(&mut self, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, |state, cx| state.pick_batch_data_file(&self.i18n, cx));
    }

    pub(in crate::workspace) fn refresh_batch_add_preview(&mut self, cx: &mut Context<Self>) {
        // The preview reads the live connection store after this IME update releases WorkspaceApp.
        let manager = self.connections.session_manager.clone();
        cx.defer(move |cx| manager.update(cx, SessionManagerState::refresh_batch_preview));
    }
}
