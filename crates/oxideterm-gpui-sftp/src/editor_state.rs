// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::{Focusable, Window};
use oxideterm_gpui_editor::EditorContextMenuLabels;

impl SftpWorkspaceEntity {
    pub fn sync_preview_editor_state(
        &mut self,
        editor: &Entity<TextEditorView>,
        cx: &mut Context<Self>,
    ) {
        let content = editor.read(cx).buffer().text_snapshot();
        let content_changed = content.as_ref() != self.preview_editor_observed_content.as_ref();
        self.preview_editor_dirty =
            content.as_ref() != self.preview_editor_initial_content.as_ref();
        if content_changed {
            // Editor notifications include cursor-only movement. Only content
            // changes clear a previous save failure.
            self.preview_editor_observed_content = content;
            self.preview_editor_save_error = None;
            self.preview_editor_network_error = false;
            self.preview_editor_last_atomic_write = None;
            cx.notify();
        }
    }

    pub fn save_preview_editor_content(&mut self, content: Arc<str>, cx: &mut Context<Self>) {
        if self.preview_editor_saving {
            return;
        }
        self.preview_editor_dirty =
            content.as_ref() != self.preview_editor_initial_content.as_ref();
        self.preview_editor_observed_content = content.clone();
        if !self.preview_editor_dirty {
            return;
        }
        let Some(path) = self.preview_path.clone() else {
            return;
        };
        self.preview_editor_saving = true;
        self.preview_editor_save_error = None;
        self.preview_editor_network_error = false;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        cx.emit(SftpWorkspaceEvent::PreviewSaveRequested {
            expected_version: match self.preview_content.as_deref() {
                Some(PreviewContent::Text { version, .. }) => version.clone(),
                _ => None,
            },
            path,
            content,
            encoding: Arc::<str>::from(self.preview_editor_encoding.as_str()),
            line_ending: self.preview_editor_line_ending,
            generation: self.preview_generation,
            delivery: self.worker_tx.clone(),
        });
        cx.notify();
    }

    pub fn retry_preview_editor_save(&mut self, cx: &mut Context<Self>) {
        if self.preview_editor_saving {
            return;
        }
        let Some(editor) = self.preview_editor.clone() else {
            return;
        };
        self.preview_editor_retry_count = self.preview_editor_retry_count.saturating_add(1);
        self.preview_editor_network_error = false;
        self.preview_editor_save_error = None;
        self.preview_editor_retry_task = Some(cx.spawn(async move |entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.preview_editor_retry_task = None;
                sftp.sync_preview_editor_state(&editor, cx);
                let content = editor.read(cx).buffer().text_snapshot();
                sftp.save_preview_editor_content(content, cx);
            });
        }));
        cx.notify();
    }
}

impl SftpWorkspaceEntity {
    pub fn begin_remote_preview(
        &mut self,
        pane: SftpPane,
        file: &SftpFileEntry,
        plugin_preview: Option<SftpDocumentPreview>,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.active_pane = pane;
        self.clear_context_menu_immediately();
        self.stop_preview_media();
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.reset_preview_editor();
        self.preview_pane = Some(pane);
        self.preview_path = Some(file.path.clone());
        self.preview_content = None;
        self.preview_asset_owner = None;
        self.preview_plugin = plugin_preview;
        self.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.preview_markdown_cache = MarkdownPreviewCache::default();
        self.preview_document_scroll = ScrollHandle::new();
        self.font_preview_scroll = ScrollHandle::new();
        self.preview_error = None;
        self.preview_loading = true;
        self.preview_hex_loading_more = false;
        self.preview_markdown_source_mode = false;
        self.preview_font_family = None;
        self.preview_font_error = None;
        self.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
        self.set_dialog(SftpDialog::Preview {
            name: file.name.clone(),
        });
        cx.notify();
        self.preview_generation
    }

    pub fn open_preview_editor(
        &mut self,
        name: &str,
        tokens: oxideterm_theme::ThemeTokens,
        runtime_settings: &SftpPreviewEditorPreferences,
        i18n: &oxideterm_i18n::I18n,
        search_labels: oxideterm_gpui_editor::EditorSearchLabels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((data, language, encoding, preview_path)) = ({
            let sftp = &*self;
            if sftp.preview_pane != Some(SftpPane::Remote) {
                None
            } else {
                match sftp.preview_content.as_deref() {
                    Some(PreviewContent::Text {
                        data,
                        language,
                        encoding,
                        ..
                    }) => Some((
                        data.clone(),
                        language.clone(),
                        encoding.clone(),
                        sftp.preview_path.clone(),
                    )),
                    _ => None,
                }
            }
        }) else {
            return;
        };

        self.stop_preview_media();
        let editor_language = sftp_editor_language(language.as_deref(), name);
        let syntax_language =
            sftp_editor_language_id(language.as_deref(), preview_path.as_deref(), name, &data);
        let syntax_path = preview_path.as_deref().unwrap_or(name).to_string();
        let context_menu_labels = EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        };
        // The SFTP view owns its preview editor, so this backlink must be weak.
        let sftp_entity = cx.weak_entity();
        let (editor_text, line_ending) = normalize_text_line_endings(&data);
        let initial_editor_text: Arc<str> = editor_text.into();
        let existing_editor = self.preview_editor.clone();
        let configure_editor =
            move |editor: &mut TextEditorView, cx: &mut Context<TextEditorView>| {
                editor.set_read_only(false);
                editor.set_search_labels(search_labels);
                editor.set_context_menu_labels(context_menu_labels);
                editor.apply_ide_runtime_settings(
                    &tokens,
                    runtime_settings.editor_font_family.clone(),
                    runtime_settings.editor_font_weight,
                    runtime_settings.editor_font_fallback.clone(),
                    runtime_settings.editor_font_size,
                    runtime_settings.editor_line_height,
                    runtime_settings.word_wrap,
                    runtime_settings.background_active,
                    cx,
                );
                editor.set_language_from_path(syntax_path, syntax_language, cx);
                editor.set_on_save(Box::new(move |text, _window, cx| {
                    let _ = sftp_entity.update(cx, |sftp, cx| {
                        sftp.save_preview_editor_content(text, cx);
                    });
                    Ok(())
                }));
            };
        let editor = if let Some(editor) = existing_editor {
            // Reusing the painted read-only preview preserves the Windows IME
            // input handler across the transition into editable mode.
            editor.update(cx, configure_editor);
            editor
        } else {
            cx.new(|cx| {
                let mut editor = TextEditorView::new(initial_editor_text.clone(), &tokens, cx);
                configure_editor(&mut editor, cx);
                editor
            })
        };
        let observer = cx.observe(&editor, |sftp, editor, cx| {
            sftp.sync_preview_editor_state(&editor, cx);
        });
        self.preview_editor = Some(editor.clone());
        self.preview_editor_observer = Some(observer);
        self.preview_editor_initial_content = initial_editor_text.clone();
        self.preview_editor_observed_content = initial_editor_text;
        self.preview_editor_language = Some(editor_language);
        self.preview_editor_encoding = encoding;
        self.preview_editor_line_ending = line_ending;
        self.preview_editor_dirty = false;
        self.preview_editor_saving = false;
        self.preview_editor_save_error = None;
        self.preview_editor_network_error = false;
        self.preview_editor_retry_count = 0;
        self.preview_editor_last_saved_mtime = None;
        self.preview_editor_last_atomic_write = None;
        self.set_dialog(SftpDialog::Editor {
            name: name.to_string(),
        });
        cx.notify();
        // Commit modal ownership before focusing so root key routing observes
        // the editor and its FocusId in the same interaction.
        let focus_handle = editor.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
    }
}

impl SftpWorkspaceEntity {
    pub(crate) fn can_compare_preview(&self, name: &str) -> bool {
        self.preview_pane == Some(SftpPane::Remote)
            && matches!(
                self.preview_content.as_deref(),
                Some(PreviewContent::Text { .. })
            )
            && self
                .local_files
                .iter()
                .any(|file| file.name == name && file.file_type == SftpFileType::File)
    }

    pub(crate) fn preview_is_markdown(&self) -> bool {
        matches!(self.preview_content.as_deref(), Some(PreviewContent::Text { language, mime_type, .. })
            if sftp_preview_is_markdown(language.as_deref(), mime_type.as_deref()))
    }

    pub(crate) fn toggle_preview_source(&mut self, cx: &mut Context<Self>) {
        self.preview_markdown_source_mode = !self.preview_markdown_source_mode;
        if self.preview_markdown_source_mode {
            self.preview_markdown_cache
                .release(&mut self.preview_markdown_scroll);
        }
        cx.notify();
    }

    pub(crate) fn save_editor(&mut self, cx: &mut Context<Self>) {
        if self.preview_editor_saving {
            return;
        }
        let Some(editor) = self.preview_editor.clone() else {
            return;
        };
        let content = editor.read(cx).buffer().text_snapshot();
        self.sync_preview_editor_state(&editor, cx);
        self.save_preview_editor_content(content, cx);
    }

    // A clean editor can close immediately. Dirty text stays owned by the editor.
    pub fn confirm_or_close_editor(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(SftpDialog::Editor { name } | SftpDialog::EditorCloseConfirm { name }) =
            self.dialog.clone()
        else {
            return false;
        };
        if !self.preview_editor_dirty {
            return true;
        }
        self.set_dialog(SftpDialog::EditorCloseConfirm { name });
        cx.notify();
        false
    }

    pub fn restore_editor(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_dialog(SftpDialog::Editor { name });
        cx.notify();
        if let Some(editor) = &self.preview_editor {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
    }

    pub(crate) fn begin_hex_page(&mut self, cx: &mut Context<Self>) -> Option<(String, u64, u64)> {
        if self.preview_loading || self.preview_hex_loading_more {
            return None;
        }
        let path = self.preview_path.clone()?;
        let PreviewContent::Hex {
            offset,
            has_more: true,
            ..
        } = self.preview_content.as_deref()?
        else {
            return None;
        };
        let offset = offset.saturating_add(16 * 1024);
        self.preview_hex_loading_more = true;
        self.preview_error = None;
        cx.notify();
        Some((path, offset, self.preview_generation))
    }

    pub(crate) fn compare_preview(
        &mut self,
        name: &str,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        if !self.can_compare_preview(name) {
            return;
        }
        let Some((remote_content, local_file, remote_path)) = ({
            let sftp = &*self;
            let remote_content = match sftp.preview_content.as_deref() {
                Some(PreviewContent::Text { data, .. }) => Some(data.clone()),
                _ => None,
            };
            remote_content.and_then(|remote_content| {
                sftp.local_files
                    .iter()
                    .find(|file| file.name == name && file.file_type == SftpFileType::File)
                    .cloned()
                    .map(|local_file| {
                        (
                            remote_content,
                            local_file,
                            sftp.preview_path.clone().unwrap_or_default(),
                        )
                    })
            })
        }) else {
            let error = format!(
                "{}: {}",
                i18n.t("sftp.toast.compare_failed"),
                i18n.t("sftp.toast.compare_no_local")
            );
            self.preview_error = Some(error);
            cx.notify();
            return;
        };

        match self.local_host.read_text(&local_file.path) {
            Ok(local_content) => {
                let view = cx.new(|_| {
                    SftpDiffView::new(
                        local_file.path,
                        &local_content,
                        remote_path,
                        &remote_content,
                    )
                });
                self.set_dialog(SftpDialog::Diff { view });
                cx.notify();
            }
            Err(error) => {
                let error = format!("{}: {}", i18n.t("sftp.toast.compare_failed"), error);
                self.preview_error = Some(error);
                cx.notify();
            }
        }
    }
}
