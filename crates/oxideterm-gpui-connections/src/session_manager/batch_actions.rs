// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{path::PathBuf, rc::Rc};

use gpui::{App, AppContext, Context, Entity, PathPromptOptions};
use oxideterm_connections::{
    BatchDuplicateNamePolicy, BatchExistingConnection, BatchPreview, ConnectionStore, apply_batch,
    parse_batch_data, parse_batch_template, resolve_batch_rows,
};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels, EditorPresentation, EditorSettings, TextEditorView,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use zeroize::Zeroizing;

use super::*;

const EDITOR_LINE_HEIGHT: f32 = 22.0;

impl SessionManagerState {
    pub fn open_batch_add(
        &mut self,
        tokens: ThemeTokens,
        font_family: String,
        i18n: &I18n,
        existing: Rc<dyn Fn(&App) -> Vec<BatchExistingConnection>>,
        cx: &mut Context<Self>,
    ) {
        self.focused_input = None;
        let mut dialog = BatchAddDialogState::default();
        dialog.existing_connections = Some(existing);
        let labels = EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        };
        let make_editor = |placeholder: Option<String>, cx: &mut Context<Self>| {
            let editor = cx.new(|cx| {
                let mut editor = TextEditorView::new(String::new(), &tokens, cx);
                editor.set_presentation(EditorPresentation::Inline, cx);
                editor.set_settings(
                    EditorSettings {
                        soft_wrap: false,
                        indentation_markers: false,
                        highlight_special_chars: false,
                        highlight_current_line: false,
                        placeholder,
                        ..Default::default()
                    },
                    cx,
                );
                editor.set_border_visible(false);
                editor.set_context_menu_labels(labels.clone());
                editor.apply_runtime_settings(
                    &tokens,
                    font_family.clone(),
                    tokens.metrics.ui_text_sm,
                    EDITOR_LINE_HEIGHT / tokens.metrics.ui_text_sm,
                    false,
                    false,
                    cx,
                );
                editor
            });
            let mut version = editor.read(cx).buffer().version();
            let subscription = cx.observe(&editor, move |manager, editor, cx| {
                let current = editor.read(cx).buffer().version();
                if version != current {
                    version = current;
                    manager.refresh_batch_preview(cx);
                }
            });
            (editor, subscription)
        };
        let (template, template_subscription) = make_editor(None, cx);
        let (data, data_subscription) =
            make_editor(Some(i18n.t("sessionManager.batchAdd.data_placeholder")), cx);
        dialog.template_editor = Some(template);
        dialog.template_editor_subscription = Some(template_subscription);
        dialog.data_editor = Some(data);
        dialog.data_editor_subscription = Some(data_subscription);
        self.batch_add = Some(dialog);
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    pub fn set_batch_auth_kind(&mut self, kind: BatchAddAuthKind, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        if dialog.auth_kind != kind {
            dialog.auth_kind = kind;
            dialog.key_menu_open = false;
            dialog.error = None;
            self.refresh_batch_preview(cx);
        }
    }

    pub fn set_batch_duplicate_policy(
        &mut self,
        policy: BatchDuplicateNamePolicy,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        if dialog.duplicate != policy {
            dialog.duplicate = policy;
            dialog.error = None;
            self.refresh_batch_preview(cx);
        }
    }

    pub fn select_batch_key(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        dialog.key_path = path.to_owned();
        dialog.error = None;
        self.refresh_batch_preview(cx);
    }

    pub fn refresh_batch_preview(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        let template_text = Zeroizing::new(dialog.editor_text(BatchAddEditorKind::Template, cx));
        let data_text = Zeroizing::new(dialog.editor_text(BatchAddEditorKind::Data, cx));
        let (template, template_issue, preview) =
            if template_text.trim().is_empty() && data_text.trim().is_empty() {
                (None, None, BatchPreview::default())
            } else {
                let (template, issue) = match parse_batch_template(&template_text) {
                    Ok(template) => (Some(template), None),
                    Err(issue) => (None, Some(issue)),
                };
                let preview = match parse_batch_data(&data_text) {
                    Ok(data) => template
                        .as_ref()
                        .map_or_else(BatchPreview::default, |template| {
                            let existing = dialog
                                .existing_connections
                                .as_ref()
                                .map_or_else(Vec::new, |read| read(cx));
                            resolve_batch_rows(
                                template,
                                &data,
                                dialog.default_auth().as_ref(),
                                &existing,
                                dialog.duplicate,
                            )
                        }),
                    Err(error) => {
                        let mut preview = BatchPreview::default();
                        preview.issues.push(error);
                        preview
                    }
                };
                (template, issue, preview)
            };
        dialog.template = template;
        dialog.template_issue = template_issue;
        dialog.preview = preview;
        if !dialog.needs_fallback_auth() {
            dialog.key_menu_open = false;
        }
        cx.notify();
    }

    pub fn apply_batch_to_store(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.batch_add.as_mut() else {
            return false;
        };
        if dialog.preview.invalid_count() > 0 {
            dialog.error = Some(i18n.t("sessionManager.batchAdd.apply_blocked_issues"));
            cx.notify();
            return false;
        }
        match apply_batch(store, dialog.preview.addable_rows()) {
            Ok(result) => {
                let count = result.added.len();
                self.batch_add = None;
                self.focused_input = None;
                self.status = Some(batch_i18n(
                    i18n,
                    "sessionManager.batchAdd.toast.added",
                    &[("count", count.to_string())],
                ));
                cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
                cx.notify();
                count > 0
            }
            Err(error) => {
                dialog.error = Some(error.to_string());
                cx.notify();
                false
            }
        }
    }

    pub fn pick_batch_data_file(&mut self, i18n: &I18n, cx: &mut Context<Self>) {
        let Some(dialog) = self.batch_add.as_mut() else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(i18n.t("sessionManager.batchAdd.choose_hosts_file").into()),
        });
        let error_label = i18n.t("sessionManager.batchAdd.file_read_failed");
        let executor = cx.background_executor().clone();
        dialog.data_file_picker_task = Some(cx.spawn(async move |owner, cx| {
            let path: Option<PathBuf> = selection
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let result = match path {
                Some(path) => Some(
                    executor
                        .spawn(async move { std::fs::read_to_string(path) })
                        .await,
                ),
                None => None,
            };
            let _ = owner.update(cx, |manager, cx| {
                let Some(dialog) = manager.batch_add.as_mut() else {
                    return;
                };
                dialog.data_file_picker_task = None;
                match result {
                    Some(Ok(text)) => {
                        dialog.error = None;
                        if let Some(editor) = &dialog.data_editor {
                            editor.update(cx, |editor, cx| editor.replace_text_external(text, cx));
                        }
                    }
                    Some(Err(error)) => dialog.error = Some(format!("{error_label}: {error}")),
                    None => {}
                }
                cx.notify();
            });
        }));
    }

    pub fn focus_batch_editor(
        &mut self,
        kind: BatchAddEditorKind,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TextEditorView>> {
        let dialog = self.batch_add.as_mut()?;
        dialog.focused_editor = Some(kind);
        cx.notify();
        match kind {
            BatchAddEditorKind::Template => dialog.template_editor.clone(),
            BatchAddEditorKind::Data => dialog.data_editor.clone(),
        }
    }
}
