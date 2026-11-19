use super::super::selectable_text::selectable_document_group_id;
use super::*;
use oxideterm_gpui_sftp::{
    SftpDialogFrame, SftpDialogRequest, SftpDialogView, SftpPreviewEditorPreferences,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_sftp_dialog(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = self.sftp_view().clone();
        let surface = self.sftp_surface_id();
        let window_owner = cx.entity().downgrade();
        let text = self.selectable_text_render_state(cx);
        let localization = self.localized_markdown_options();
        let editor = self.ide_runtime_settings();
        let frame = SftpDialogFrame {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            markdown_options: localization,
            markdown_actions: self.markdown_mermaid_actions(cx),
            editor_preferences: SftpPreviewEditorPreferences {
                editor_font_family: editor.editor_font_family,
                editor_font_weight: editor.editor_font_weight,
                editor_font_fallback: editor.editor_font_fallback,
                editor_font_size: editor.editor_font_size,
                editor_line_height: editor.editor_line_height,
                word_wrap: editor.word_wrap,
                background_active: editor.background_active,
            },
            search_labels: self.editor_search_labels(),
            mono_font: settings_mono_font_family(self.settings_store.settings()),
            ui_font: settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ),
            ime: self.sftp_input_ime(surface, SftpInput::DialogValue, cx),
            has_background,
            selectable: Rc::new(move |id, value, color| {
                text.render_display_text_with_id_in_group(
                    SelectableTextRole::PlainDocument,
                    selectable_document_group_id(),
                    id,
                    0,
                    value,
                    color,
                )
            }),
            dispatch: Rc::new(move |request, window, cx| {
                let _ = window_owner.update(cx, |app, cx| {
                    if !app.has_sftp_surface(surface) {
                        return;
                    }
                    let _scope = app.enter_sftp_surface(surface);
                    app.handle_sftp_dialog_request(surface, request, window, cx);
                });
            }),
        };
        SftpDialogView::mount(&source, frame, cx)
    }

    fn handle_sftp_dialog_request(
        &mut self,
        surface: SftpSurfaceId,
        request: SftpDialogRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            SftpDialogRequest::Close => self.close_sftp_dialog(cx),
            SftpDialogRequest::Accept => self.accept_sftp_dialog(cx),
            SftpDialogRequest::Download(name) => self.download_sftp_preview(&name, cx),
            SftpDialogRequest::OpenExternal(path) => self.open_sftp_preview_external(&path, cx),
            SftpDialogRequest::LoadHex {
                path,
                offset,
                generation,
            } => self.spawn_remote_sftp_preview_hex(path, offset, generation, cx),
            SftpDialogRequest::Resolve(action) => self.resolve_sftp_transfer_conflict(action, cx),
            SftpDialogRequest::CancelConflicts => self.cancel_sftp_transfer_conflicts(cx),
            SftpDialogRequest::InputPressed(event) => {
                self.ime_marked_text = None;
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(
                    WorkspaceImeTarget::Sftp(surface, SftpInput::DialogValue),
                    &event,
                    window,
                    cx,
                );
            }
            SftpDialogRequest::InputDragged(event) => {
                self.update_ime_selection_drag_from_mouse_move(&event, window, cx);
            }
        }
    }
}
