use super::*;

use gpui::{AnchoredPositionMode, Corner, PathPromptOptions, Window, anchored, deferred, point};
use oxideterm_connections::{
    BatchDuplicateNamePolicy, BatchExistingConnection, BatchIssue, BatchIssueKind, BatchPreview,
    BatchPreviewRow, BatchRowStatus, BatchTemplate, SavedAuth, SecretString, apply_batch,
    parse_batch_data, parse_batch_template, resolve_batch_rows,
};
use oxideterm_gpui_editor::{
    EditorContextMenuLabels, EditorPresentation, EditorSettings, TextEditorView,
};
use oxideterm_gpui_ui::modal::{TAURI_SELECT_LAYER_PRIORITY, popover_backdrop};
use oxideterm_gpui_ui::select::{
    SelectAnchorId, select_anchor_probe, select_overlay_popup_with_max_height,
};

pub(in crate::workspace) const BATCH_ADD_DIALOG_WIDTH: f32 = 880.0;
const BATCH_ADD_TRIGGER_MIN_WIDTH: f32 = 240.0;
const BATCH_ADD_TEXTAREA_LINE_HEIGHT: f32 = 22.0;
const BATCH_ADD_TEXTAREA_VERTICAL_PADDING: f32 = 8.0;
const BATCH_ADD_EDITOR_BOX_HEIGHT: f32 = 148.0;
// A paste can carry thousands of rows; rendering every preview row would stall
// the frame, so the table shows a bounded window and points at the overflow.
const BATCH_ADD_PREVIEW_MAX_ROWS: usize = 120;
const BATCH_ADD_ISSUE_MAX_ROWS: usize = 10;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::workspace) enum BatchAddAuthKind {
    #[default]
    Password,
    Key,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum BatchAddEditorKind {
    Template,
    Data,
}

pub(in crate::workspace) struct BatchAddDialogState {
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) template_editor: Option<Entity<TextEditorView>>,
    pub(super) data_editor: Option<Entity<TextEditorView>>,
    template_editor_subscription: Option<Subscription>,
    data_editor_subscription: Option<Subscription>,
    pub(super) focused_editor: Option<BatchAddEditorKind>,
    pub(super) password: Zeroizing<String>,
    pub(super) key_path: String,
    pub(super) auth_kind: BatchAddAuthKind,
    pub(super) duplicate: BatchDuplicateNamePolicy,
    pub(super) template: Option<BatchTemplate>,
    pub(super) template_issue: Option<BatchIssue>,
    pub(super) preview: BatchPreview,
    pub(super) error: Option<String>,
    pub(super) templates: Vec<BatchTemplatePreset>,
    pub(super) templates_loaded: bool,
    pub(super) selected_template_id: Option<String>,
    pub(in crate::workspace) renaming_id: Option<String>,
    pub(in crate::workspace) rename_draft: String,
    pub(in crate::workspace) new_template_naming: bool,
    pub(in crate::workspace) template_menu_open: bool,
    pub(in crate::workspace) key_menu_open: bool,
    pub(super) data_file_picker_task: Option<Task<()>>,
}

impl Default for BatchAddDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            template_editor: None,
            data_editor: None,
            template_editor_subscription: None,
            data_editor_subscription: None,
            focused_editor: None,
            password: Zeroizing::new(String::new()),
            key_path: String::new(),
            auth_kind: BatchAddAuthKind::default(),
            duplicate: BatchDuplicateNamePolicy::default(),
            template: None,
            template_issue: None,
            preview: BatchPreview::default(),
            error: None,
            templates: Vec::new(),
            templates_loaded: false,
            selected_template_id: None,
            renaming_id: None,
            rename_draft: String::new(),
            new_template_naming: false,
            template_menu_open: false,
            key_menu_open: false,
            data_file_picker_task: None,
        }
    }
}

impl BatchAddDialogState {
    /// The dialog authentication is only a fallback; the template identity file
    /// wins whenever it is present, so this stays `None` for key templates.
    pub(super) fn default_auth(&self) -> Option<SavedAuth> {
        match self.auth_kind {
            BatchAddAuthKind::Password if !self.password.is_empty() => Some(SavedAuth::Password {
                empty_password: false,
                keychain_id: None,
                plaintext_password: Some(SecretString::from(self.password.as_str())),
            }),
            BatchAddAuthKind::Key if !self.key_path.trim().is_empty() => Some(SavedAuth::Key {
                key_path: self.key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            }),
            _ => None,
        }
    }

    /// Whether the fallback authentication row set is relevant for the current
    /// template; a template with an identity path owns authentication itself.
    pub(super) fn needs_fallback_auth(&self) -> bool {
        self.template.as_ref().is_none_or(|template| {
            template.identity_file.is_none() && template.certificate_file.is_none()
        })
    }

    fn editor_text(&self, kind: BatchAddEditorKind, cx: &App) -> String {
        let editor = match kind {
            BatchAddEditorKind::Template => &self.template_editor,
            BatchAddEditorKind::Data => &self.data_editor,
        };
        editor
            .as_ref()
            .map(|editor| editor.read(cx).buffer().text())
            .unwrap_or_default()
    }
}

impl SessionManagerState {
    pub(super) fn begin_batch_add_dialog_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.batch_add.as_mut() else {
            return false;
        };
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_batch_add_dialog_exit(generation, cx);
            return true;
        }
        self.batch_add_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_batch_add_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub(super) fn finish_batch_add_dialog_exit(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        // Dropping the state is the final boundary for the password draft;
        // Zeroizing clears it on drop together with both editors.
        self.batch_add = None;
        self.batch_add_exit_task = None;
        cx.notify();
        true
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_batch_add_dialog(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.focused_input = None;
            session_manager.batch_add = Some(BatchAddDialogState::default());
            cx.notify();
        });
        self.initialize_batch_add_editors(cx);
        // Seeding also preloads the factory template into the editor, which
        // already triggers the first preview computation through the editor
        // subscription.
        self.ensure_batch_templates_loaded(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn begin_batch_add_dialog_exit(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.begin_batch_add_dialog_exit(delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn set_batch_add_auth_kind(
        &mut self,
        kind: BatchAddAuthKind,
        cx: &mut Context<Self>,
    ) {
        let changed = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return false;
            };
            if dialog.auth_kind == kind {
                return false;
            }
            dialog.auth_kind = kind;
            // The key picker trigger only exists in Key mode; leave its overlay
            // trailing a password selection would render without an anchor.
            dialog.key_menu_open = false;
            dialog.error = None;
            cx.notify();
            true
        });
        if changed {
            self.refresh_batch_add_preview(cx);
        }
    }

    pub(in crate::workspace) fn set_batch_add_duplicate_policy(
        &mut self,
        policy: BatchDuplicateNamePolicy,
        cx: &mut Context<Self>,
    ) {
        let changed = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return false;
            };
            if dialog.duplicate == policy {
                return false;
            }
            dialog.duplicate = policy;
            dialog.error = None;
            cx.notify();
            true
        });
        if changed {
            self.refresh_batch_add_preview(cx);
        }
    }

    pub(in crate::workspace) fn focus_batch_add_editor(
        &mut self,
        kind: BatchAddEditorKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return None;
            };
            dialog.focused_editor = Some(kind);
            cx.notify();
            match kind {
                BatchAddEditorKind::Template => dialog.template_editor.clone(),
                BatchAddEditorKind::Data => dialog.data_editor.clone(),
            }
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        if let Some(focus) = editor
            .as_ref()
            .map(|editor| editor.read(cx).focus_handle(cx))
        {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn batch_add_text_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        let Some(dialog) = self.session_manager.read(cx).batch_add.as_ref() else {
            return false;
        };
        [dialog.template_editor.as_ref(), dialog.data_editor.as_ref()]
            .into_iter()
            .flatten()
            .any(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(in crate::workspace) fn refresh_batch_add_preview(&mut self, cx: &mut Context<Self>) {
        let (template_text, data_text, duplicate, default_auth) = {
            let session_manager = self.session_manager.read(cx);
            let Some(dialog) = session_manager.batch_add.as_ref() else {
                return;
            };
            (
                dialog.editor_text(BatchAddEditorKind::Template, cx),
                dialog.editor_text(BatchAddEditorKind::Data, cx),
                dialog.duplicate,
                dialog.default_auth(),
            )
        };
        let (template, template_issue, preview) = if template_text.trim().is_empty()
            && data_text.trim().is_empty()
        {
            // A pristine dialog shows no complaint; parse errors only make
            // sense once the user has entered something.
            (None, None, BatchPreview::default())
        } else {
            let (template, template_issue) = match parse_batch_template(&template_text) {
                Ok(template) => (Some(template), None),
                Err(issue) => (None, Some(issue)),
            };
            let data = parse_batch_data(&data_text);
            let mut preview = BatchPreview::default();
            if let Err(issue) = &data {
                preview.issues.push(issue.clone());
            }
            let preview = match (template.as_ref(), data.ok().as_ref()) {
                (Some(template), Some(data)) => {
                    let existing = self
                        .connection_store
                        .connections()
                        .iter()
                        .map(BatchExistingConnection::from_saved)
                        .collect::<Vec<_>>();
                    resolve_batch_rows(template, data, default_auth.as_ref(), &existing, duplicate)
                }
                _ => preview,
            };
            (template, template_issue, preview)
        };
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return;
            };
            dialog.template = template;
            dialog.template_issue = template_issue;
            dialog.preview = preview;
            if !dialog.needs_fallback_auth() && dialog.key_menu_open {
                // The key picker loses its trigger once the template owns
                // authentication; leaving it open would anchor a stale overlay.
                dialog.key_menu_open = false;
            }
            cx.notify();
        });
    }

    pub(in crate::workspace) fn toggle_batch_add_template_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                dialog.template_menu_open = !dialog.template_menu_open;
                dialog.key_menu_open = false;
                dialog.renaming_id = None;
                dialog.new_template_naming = false;
                dialog.rename_draft.clear();
                cx.notify();
            }
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_batch_add_template_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                if dialog.template_menu_open {
                    dialog.template_menu_open = false;
                    dialog.renaming_id = None;
                    dialog.new_template_naming = false;
                    dialog.rename_draft.clear();
                    cx.notify();
                }
            }
        });
    }

    pub(in crate::workspace) fn apply_batch_add(&mut self, cx: &mut Context<Self>) {
        let Some((rows, invalid)) =
            self.session_manager
                .read(cx)
                .batch_add
                .as_ref()
                .map(|dialog| {
                    (
                        dialog.preview.addable_rows(),
                        dialog.preview.invalid_count(),
                    )
                })
        else {
            return;
        };
        if invalid > 0 {
            // Adding the addable subset would contradict the all-or-nothing
            // transaction the CLI and the guide promise.
            let blocked = self.i18n.t("sessionManager.batchAdd.apply_blocked_issues");
            self.session_manager.update(cx, |session_manager, cx| {
                if let Some(dialog) = session_manager.batch_add.as_mut() {
                    dialog.error = Some(blocked);
                    cx.notify();
                }
            });
            cx.notify();
            return;
        }
        let result = apply_batch(&mut self.connection_store, rows);
        match result {
            Ok(result) => {
                let count = result.added.len();
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.batch_add = None;
                    session_manager.focused_input = None;
                    session_manager.status = Some(self.i18n_with(
                        "sessionManager.batchAdd.toast.added",
                        &[("count", count.to_string())],
                    ));
                    cx.notify();
                });
                if count > 0 {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
                cx.notify();
            }
            Err(error) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    let Some(dialog) = session_manager.batch_add.as_mut() else {
                        return;
                    };
                    dialog.error = Some(error.to_string());
                    cx.notify();
                });
            }
        }
    }

    /// Opens a single-file picker and loads the chosen text file into the
    /// hosts editor, replacing its content. The loaded text stays editable so
    /// the user can trim it before applying.
    pub(in crate::workspace) fn import_batch_data_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                self.i18n
                    .t("sessionManager.batchAdd.choose_hosts_file")
                    .into(),
            ),
        });
        let selected = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths
                .into_iter()
                .next()
                .map(|path| path.display().to_string())
        };
        let task = cx.spawn(async move |this, cx| {
            let Some(path) = selected.await else {
                return;
            };
            let read = std::fs::read_to_string(&path);
            let _ = this.update(cx, |this, cx| {
                // Resolve the localized failure text before the state update;
                // the closure must not capture the workspace receiver.
                let outcome = match read {
                    Ok(text) => Ok(text),
                    Err(error) => Err(format!(
                        "{}: {error}",
                        this.i18n.t("sessionManager.batchAdd.file_read_failed")
                    )),
                };
                let editor = this.session_manager.update(cx, |session_manager, cx| {
                    let Some(dialog) = session_manager.batch_add.as_mut() else {
                        return None;
                    };
                    dialog.data_file_picker_task = None;
                    cx.notify();
                    match &outcome {
                        Ok(text) => {
                            dialog.error = None;
                            Some((dialog.data_editor.clone()?, text.clone()))
                        }
                        Err(message) => {
                            dialog.error = Some(message.clone());
                            None
                        }
                    }
                });
                if let Some((editor, text)) = editor {
                    editor.update(cx, |editor, cx| {
                        editor.replace_text_external(text, cx);
                    });
                }
                cx.notify();
            });
        });
        self.session_manager.update(cx, |session_manager, _| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                dialog.data_file_picker_task = Some(task);
            }
        });
        cx.notify();
    }

    fn initialize_batch_add_editors(&mut self, cx: &mut Context<Self>) {
        let tokens = self.tokens;
        let font_family = settings_mono_font_family(self.settings_store.settings()).to_string();
        // No editor placeholder: a multi-line placeholder renders dimmed and
        // clipped inside the box and reads as if content were already present.
        // The hint line under each box carries the format instead.
        let labels = EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let make_editor = |cx: &mut Context<Self>| {
            let editor = cx.new(|cx| {
                let mut editor = TextEditorView::new(String::new(), &tokens, cx);
                // Inline presentation drops the document gutter and content
                // padding; these boxes are input fields, not documents.
                editor.set_presentation(EditorPresentation::Inline, cx);
                editor.set_settings(
                    EditorSettings {
                        soft_wrap: false,
                        indentation_markers: false,
                        highlight_special_chars: false,
                        highlight_current_line: false,
                        ..EditorSettings::default()
                    },
                    cx,
                );
                editor.set_border_visible(false);
                editor.set_context_menu_labels(labels.clone());
                editor.apply_runtime_settings(
                    &tokens,
                    font_family.clone(),
                    tokens.metrics.ui_text_sm,
                    BATCH_ADD_TEXTAREA_LINE_HEIGHT / tokens.metrics.ui_text_sm,
                    false,
                    false,
                    cx,
                );
                editor
            });
            let mut version = editor.read(cx).buffer().version();
            let subscription = cx.observe(&editor, move |this, editor, cx| {
                let buffer = editor.read(cx).buffer();
                if buffer.version() == version {
                    return;
                }
                version = buffer.version();
                this.refresh_batch_add_preview(cx);
            });
            (editor, subscription)
        };
        let (template_editor, template_editor_subscription) = make_editor(cx);
        let (data_editor, data_editor_subscription) = make_editor(cx);
        self.session_manager.update(cx, |session_manager, _| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                dialog.template_editor = Some(template_editor);
                dialog.template_editor_subscription = Some(template_editor_subscription);
                dialog.data_editor = Some(data_editor);
                dialog.data_editor_subscription = Some(data_editor_subscription);
            }
        });
    }

    pub(in crate::workspace) fn render_batch_add_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some((phase, dialog_snapshot)) = ({
            self.session_manager
                .read(cx)
                .batch_add
                .as_ref()
                .map(|dialog| {
                    (
                        dialog.presence.phase(),
                        BatchAddDialogSnapshot {
                            error: dialog.error.clone(),
                            template_issue: dialog.template_issue.clone(),
                            preview_rows: dialog.preview.rows.clone(),
                            issues: dialog.preview.issues.clone(),
                            addable: dialog.preview.addable_count(),
                            skipped: dialog.preview.skipped_count(),
                            invalid: dialog.preview.invalid_count(),
                            needs_fallback_auth: dialog.needs_fallback_auth(),
                            auth_kind: dialog.auth_kind,
                            duplicate: dialog.duplicate,
                        },
                    )
                })
        }) else {
            return div().into_any_element();
        };

        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.begin_batch_add_dialog_exit(cx);
                cx.stop_propagation();
            }),
        );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(OXIDE_MODAL_SECTION_GAP))
            .child(self.render_batch_add_template_section(cx))
            .child(self.render_batch_add_data_section(cx));
        if let Some(issue) = dialog_snapshot.template_issue.as_ref() {
            body =
                body.child(self.render_oxide_status_line(self.batch_issue_text(issue), true, cx));
        }
        body = body
            .child(self.render_batch_add_summary(&dialog_snapshot, cx))
            .when_some(
                self.render_batch_add_issues(&dialog_snapshot),
                |body, issues| body.child(issues),
            )
            .child(self.render_batch_add_preview_table(&dialog_snapshot))
            .child(self.render_batch_add_duplicate_policy(&dialog_snapshot, cx))
            .when(dialog_snapshot.needs_fallback_auth, |body| {
                body.child(self.render_batch_add_auth_section(&dialog_snapshot, cx))
            })
            .when_some(dialog_snapshot.error.clone(), |body, error| {
                body.child(self.render_oxide_error_banner(error, cx))
            });

        let add_label = self.i18n_with(
            "sessionManager.batchAdd.add_button",
            &[("count", dialog_snapshot.addable.to_string())],
        );
        let footer = self.render_oxide_footer(
            false,
            // Any listed issue blocks the batch, so an enabled button must
            // mean "every row can be written".
            dialog_snapshot.addable == 0 || dialog_snapshot.invalid > 0,
            String::new(),
            add_label,
            None,
            |_this, _event, _window, _cx| {},
            |this, _event, _window, cx| {
                this.apply_batch_add(cx);
            },
            |this, _event, _window, cx| {
                this.begin_batch_add_dialog_exit(cx);
            },
            cx,
        );

        backdrop
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "batch-add-dialog-transition",
                material_surface(&self.tokens, div(), MaterialRole::Dialog)
                    .w(px(BATCH_ADD_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .max_h(relative(OXIDE_MODAL_MAX_HEIGHT_RATIO))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(OXIDE_MODAL_HEADER_PX))
                            .py(px(OXIDE_MODAL_HEADER_PY))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "batch-add-dialog",
                                        "title",
                                        self.i18n.t("sessionManager.batchAdd.title"),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(self.render_oxide_close_button(true, cx)),
                    )
                    .child(
                        div()
                            .id("batch-add-dialog-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle("batch-add-dialog-scroll"),
                            )
                            .p(px(OXIDE_MODAL_BODY_P))
                            .child(body),
                    )
                    .child(
                        div()
                            .px(px(OXIDE_MODAL_BODY_P))
                            .py(px(OXIDE_MODAL_HEADER_PY))
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .child(footer),
                    ),
                phase == oxideterm_gpui_ui::motion::ExitPhase::Visible,
            ))
            .into_any_element()
    }

    /// Small icon action used by the template and hosts section headers; it
    /// follows the compact outline action chrome of the other dialog toolbars.
    /// Disabled actions keep their label visible but lose the pointer affordance.
    fn render_batch_add_icon_action(
        &self,
        id: &'static str,
        icon: LucideIcon,
        label_key: &str,
        disabled: bool,
        cx: &mut Context<Self>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let accent = rgb(if disabled {
            self.tokens.ui.text_muted
        } else {
            self.tokens.ui.accent_secondary
        });
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(accent)
            .opacity(if disabled { 0.55 } else { 1.0 })
            .when(!disabled, |action| {
                action
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
                    .on_mouse_down(MouseButton::Left, cx.listener(listener))
            })
            .child(Self::render_lucide_icon(icon, 14.0, accent))
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    fn render_batch_add_section_header(
        &self,
        label_key: &str,
        actions: Vec<AnyElement>,
    ) -> AnyElement {
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            );
        for action in actions {
            header = header.child(action);
        }
        header.into_any_element()
    }

    /// Template section: the label and the template dropdown share one line so
    /// the editor keeps a stable height; the dropdown owns load, rename, and
    /// delete actions while "save as template" captures the editor content.
    fn render_batch_add_template_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let (templates, selected_id) = {
            let session_manager = self.session_manager.read(cx);
            let Some(dialog) = session_manager.batch_add.as_ref() else {
                return div().into_any_element();
            };
            (
                dialog.templates.clone(),
                dialog.selected_template_id.clone(),
            )
        };
        let selected_name = templates
            .iter()
            .find(|template| Some(&template.id) == selected_id.as_ref())
            .map(|template| template.name.clone())
            .unwrap_or_else(|| self.i18n.t("sessionManager.batchAdd.preset_name"));
        // Overwriting only targets user templates; the factory preset stays
        // immutable, so "save" disables while the preset is selected.
        let selected_is_user_template = selected_id
            .as_deref()
            .is_some_and(|id| !is_factory_batch_template_id(id));
        let workspace = cx.entity();
        let trigger = div()
            .id("batch-add-template-trigger")
            .flex()
            .items_center()
            .gap(px(6.0))
            .w(relative(0.5))
            .min_w(px(BATCH_ADD_TRIGGER_MIN_WIDTH))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(theme.accent)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.toggle_batch_add_template_menu(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
                    .child(selected_name),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronDown,
                12.0,
                rgb(theme.text_muted),
            ));
        let trigger = select_anchor_probe(
            SelectAnchorId::SessionManagerBatchTemplate,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        );

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessionManager.batchAdd.template_label")),
                    )
                    .child(trigger)
                    .child(self.render_batch_add_icon_action(
                        "batch-add-save-template",
                        LucideIcon::Save,
                        "sessionManager.batchAdd.save",
                        // Overwriting only applies to user templates; the factory
                        // presets are re-seeded on every load.
                        !selected_is_user_template,
                        cx,
                        |this, _event, _window, cx| {
                            this.save_batch_template_into_selected(cx);
                        },
                    ))
                    .child(self.render_batch_add_icon_action(
                        "batch-add-save-new-template",
                        LucideIcon::FilePlus,
                        "sessionManager.batchAdd.save_as_new",
                        false,
                        cx,
                        |this, _event, _window, cx| {
                            this.begin_batch_template_naming(cx);
                        },
                    )),
            )
            .child(self.render_batch_add_editor_box(BatchAddEditorKind::Template, cx))
            .child(self.render_batch_add_section_hint("sessionManager.batchAdd.template_hint"))
            .into_any_element()
    }

    /// The template dropdown popup: click a row to load it, pencil renames
    /// inline, trash deletes; the factory preset only loads.
    pub(in crate::workspace) fn render_batch_add_template_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.template_menu_open)
        {
            return None;
        }
        let anchor = self
            .select_anchors
            .get(&SelectAnchorId::SessionManagerBatchTemplate)?;
        let (templates, selected_id, renaming_id, naming_new) = {
            let session_manager = self.session_manager.read(cx);
            let dialog = session_manager.batch_add.as_ref()?;
            (
                dialog.templates.clone(),
                dialog.selected_template_id.clone(),
                dialog.renaming_id.clone(),
                dialog.new_template_naming,
            )
        };
        let width = f32::from(anchor.bounds.size.width).max(BATCH_ADD_TRIGGER_MIN_WIDTH);
        let viewport_height = f32::from(window.viewport_size().height);
        let popup_gap = self.tokens.metrics.settings_select_popup_gap;
        let below = viewport_height - f32::from(anchor.bounds.bottom()) - popup_gap;
        let above = f32::from(anchor.bounds.top()) - popup_gap;
        let opens_above = below < self.tokens.metrics.ui_select_max_height && above > below;
        let max_height = if opens_above { above } else { below }
            .max(self.tokens.metrics.ui_control_height)
            .min(self.tokens.metrics.ui_select_max_height);

        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height);
        if naming_new {
            // The save-as-new prompt leads the popup: committing creates a
            // template from the editor content under the entered name.
            let draft = self
                .session_manager
                .read(cx)
                .batch_add
                .as_ref()
                .map(|dialog| dialog.rename_draft.clone())
                .unwrap_or_default();
            popup = popup.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(4.0))
                    .child(
                        self.render_session_text_input(
                            SessionManagerInput::BatchTemplateName,
                            &draft,
                            self.i18n
                                .t("sessionManager.batchAdd.template_name_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.xs))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.commit_batch_template_name(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Check,
                                14.0,
                                rgb(self.tokens.ui.accent),
                            )),
                    ),
            );
        }
        for template in &templates {
            let renaming = renaming_id.as_deref() == Some(template.id.as_str());
            if renaming {
                let draft = self
                    .session_manager
                    .read(cx)
                    .batch_add
                    .as_ref()
                    .map(|dialog| dialog.rename_draft.clone())
                    .unwrap_or_default();
                popup = popup.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(4.0))
                        .child(self.render_session_text_input(
                            SessionManagerInput::BatchTemplateName,
                            &draft,
                            template.name.clone(),
                            cx,
                        ))
                        .child(
                            div()
                                .flex_none()
                                .size(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(self.tokens.radii.xs))
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.commit_batch_template_name(cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Check,
                                    14.0,
                                    rgb(self.tokens.ui.accent),
                                )),
                        ),
                );
                continue;
            }
            let selected = selected_id.as_deref() == Some(template.id.as_str());
            let is_preset = is_factory_batch_template_id(&template.id);
            let load_id = template.id.clone();
            let rename_id = template.id.clone();
            let delete_id = template.id.clone();

            // Custom row: the name owns the free space and the row actions sit
            // at the trailing edge, so rename and delete never crowd the label
            // or the selected marker.
            let mut row = div()
                .id(format!("batch-template-option-{}", template.id))
                .flex()
                .items_center()
                .gap(px(8.0))
                .w_full()
                .h(px(self.tokens.metrics.ui_control_height))
                .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                .rounded(px(self.tokens.radii.xs))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_batch_add_template(&load_id, cx);
                        this.close_batch_add_template_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .children(selected.then(|| {
                    Self::render_lucide_icon(LucideIcon::Check, 14.0, rgb(self.tokens.ui.accent))
                }))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(if selected {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text
                        }))
                        .child(template.name.clone()),
                );
            if is_preset {
                // The preset name is self-explanatory; no trailing icon.
                row = row.child(div().flex_none().w(px(14.0)));
            } else {
                row = row
                    .child(
                        div()
                            .flex_none()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.xs))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.begin_batch_template_rename(&rename_id, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Pencil,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.xs))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.delete_batch_template(&delete_id, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Trash2,
                                14.0,
                                rgb(self.tokens.ui.error),
                            )),
                    );
            }
            popup = popup.child(row);
        }

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };
        let popup = popup.into_any_element();
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_batch_add_template_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_batch_add_template_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(anchor_corner)
                            .position(position)
                            .offset(point(px(0.0), px(offset_y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    /// Hosts section: header with file import, the data editor, and the format
    /// hint. Importing replaces the editor content, which stays editable.
    fn render_batch_add_data_section(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_batch_add_section_header(
                "sessionManager.batchAdd.data_label",
                vec![self.render_batch_add_icon_action(
                    "batch-add-import-hosts",
                    LucideIcon::Upload,
                    "sessionManager.batchAdd.import_from_file",
                    false,
                    cx,
                    |this, _event, _window, cx| {
                        this.import_batch_data_file(cx);
                    },
                )],
            ))
            .child(self.render_batch_add_editor_box(BatchAddEditorKind::Data, cx))
            .child(self.render_batch_add_section_hint("sessionManager.batchAdd.data_hint"))
            .into_any_element()
    }

    fn render_batch_add_section_hint(&self, hint_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t(hint_key))
            .into_any_element()
    }

    fn render_batch_add_editor_box(
        &self,
        kind: BatchAddEditorKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .and_then(|dialog| dialog.focused_editor)
            == Some(kind);
        let editor =
            self.session_manager
                .read(cx)
                .batch_add
                .as_ref()
                .and_then(|dialog| match kind {
                    BatchAddEditorKind::Template => dialog.template_editor.clone(),
                    BatchAddEditorKind::Data => dialog.data_editor.clone(),
                });
        div()
            .id(match kind {
                BatchAddEditorKind::Template => "batch-add-template-editor",
                BatchAddEditorKind::Data => "batch-add-data-editor",
            })
            .w_full()
            .min_w_0()
            .h(px(BATCH_ADD_EDITOR_BOX_HEIGHT))
            .flex_none()
            .p(px(BATCH_ADD_TEXTAREA_VERTICAL_PADDING))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(if focused {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _event, window, cx| {
                // Focus without consuming the event: the editor underneath must
                // still see the mouse down for caret placement and selection.
                this.focus_batch_add_editor(kind, window, cx);
            }))
            .children(editor)
            .into_any_element()
    }

    fn render_batch_add_summary(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = self.i18n_with(
            "sessionManager.batchAdd.summary",
            &[
                ("added", snapshot.addable.to_string()),
                ("skipped", snapshot.skipped.to_string()),
                ("invalid", snapshot.invalid.to_string()),
            ],
        );
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(if snapshot.invalid > 0 {
                self.tokens.ui.error
            } else {
                self.tokens.ui.text_muted
            }))
            .child(self.render_selectable_display_text(
                "batch-add-summary",
                &text,
                text.clone(),
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn render_batch_add_issues(&self, snapshot: &BatchAddDialogSnapshot) -> Option<AnyElement> {
        if snapshot.issues.is_empty() {
            return None;
        }
        let mut list = div().flex().flex_col().gap(px(2.0));
        for issue in snapshot.issues.iter().take(BATCH_ADD_ISSUE_MAX_ROWS) {
            list = list.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(self.batch_issue_text(issue)),
            );
        }
        let hidden = snapshot
            .issues
            .len()
            .saturating_sub(BATCH_ADD_ISSUE_MAX_ROWS);
        if hidden > 0 {
            list = list.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n_with(
                        "sessionManager.batchAdd.more_issues",
                        &[("count", hidden.to_string())],
                    )),
            );
        }
        Some(list.into_any_element())
    }

    fn render_batch_add_preview_table(&self, snapshot: &BatchAddDialogSnapshot) -> AnyElement {
        let theme = self.tokens.ui;
        let header = div()
            .flex()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.text_muted))
            .child(self.table_cell("sessionManager.batchAdd.column.name", 0.0, 120.0))
            .child(self.table_cell("sessionManager.batchAdd.column.host", 140.0, 140.0))
            .child(self.table_cell("sessionManager.batchAdd.column.port", 50.0, 50.0))
            .child(self.table_cell("sessionManager.batchAdd.column.user", 100.0, 100.0))
            .child(self.table_cell("sessionManager.batchAdd.column.group", 90.0, 90.0))
            .child(self.table_cell("sessionManager.batchAdd.column.tags", 110.0, 110.0))
            .child(self.table_cell("sessionManager.batchAdd.column.auth", 60.0, 60.0))
            .child(self.table_cell("sessionManager.batchAdd.column.proxy", 50.0, 50.0))
            .child(self.table_cell("sessionManager.batchAdd.column.status", 150.0, 150.0));
        let mut table = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .p(px(8.0))
            .child(header);
        for row in snapshot
            .preview_rows
            .iter()
            .take(BATCH_ADD_PREVIEW_MAX_ROWS)
        {
            let proxy_label = if row.has_proxy_command {
                self.i18n.t("sessionManager.batchAdd.proxy_yes")
            } else {
                String::new()
            };
            let dimmed = matches!(
                row.status,
                BatchRowStatus::Unchanged
                    | BatchRowStatus::SkippedDuplicateTarget
                    | BatchRowStatus::SkippedExistingName
            );
            table = table.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
                    .opacity(if dimmed { 0.55 } else { 1.0 })
                    .child(self.table_text(&row.name, 120.0))
                    .child(self.table_text(&row.host, 140.0))
                    .child(self.table_text(&row.port.to_string(), 50.0))
                    .child(self.table_text(&row.username, 100.0))
                    .child(self.table_text(row.group.as_deref().unwrap_or_default(), 90.0))
                    .child(self.table_text(&row.tags.join(", "), 110.0))
                    .child(self.table_text(
                        &self.i18n.t(if row.auth_type == AuthType::Password {
                            "sessionManager.batchAdd.auth_password"
                        } else {
                            "sessionManager.batchAdd.auth_key"
                        }),
                        60.0,
                    ))
                    .child(self.table_text(&proxy_label, 50.0))
                    .child(self.table_wrapped_text(
                        &self.i18n.t(&format!(
                            "sessionManager.batchAdd.status.{}",
                            row.status.label_key()
                        )),
                        150.0,
                    )),
            );
        }
        if snapshot.preview_rows.len() > BATCH_ADD_PREVIEW_MAX_ROWS {
            table = table.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n_with(
                        "sessionManager.batchAdd.preview_overflow",
                        &[("count", BATCH_ADD_PREVIEW_MAX_ROWS.to_string())],
                    )),
            );
        }
        // The columns have fixed widths, so a narrow window scrolls the table
        // instead of clipping the trailing status column.
        div()
            .id("batch-add-preview-scroll")
            .w_full()
            .min_w(px(0.0))
            .overflow_x_scroll()
            .child(table)
            .into_any_element()
    }

    fn table_cell(&self, key: &str, min_width: f32, width: f32) -> AnyElement {
        div()
            .when(min_width > 0.0, |cell| cell.min_w(px(min_width)))
            .w(px(width))
            .flex_none()
            .child(self.i18n.t(key))
            .into_any_element()
    }

    fn table_text(&self, text: &str, width: f32) -> AnyElement {
        div()
            .w(px(width))
            .flex_none()
            .truncate()
            .child(text.to_owned())
            .into_any_element()
    }

    /// Status labels are localized sentences, so they wrap instead of being
    /// clipped at the column edge.
    fn table_wrapped_text(&self, text: &str, width: f32) -> AnyElement {
        div()
            .w(px(width))
            .flex_none()
            .child(text.to_owned())
            .into_any_element()
    }

    fn render_batch_add_duplicate_policy(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = [
            (
                BatchDuplicateNamePolicy::Skip,
                "sessionManager.batchAdd.duplicate_skip",
            ),
            (
                BatchDuplicateNamePolicy::Rename,
                "sessionManager.batchAdd.duplicate_rename",
            ),
        ];
        let mut row = div().flex().items_center().gap(px(12.0)).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text))
                .child(self.i18n.t("sessionManager.batchAdd.duplicate_label")),
        );
        for (policy, key) in options {
            let selected = snapshot.duplicate == policy;
            row = row.child(
                div()
                    .id(match policy {
                        BatchDuplicateNamePolicy::Skip => "batch-add-duplicate-skip",
                        BatchDuplicateNamePolicy::Rename => "batch-add-duplicate-rename",
                    })
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(if selected {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
                    .child(self.i18n.t(key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_batch_add_duplicate_policy(policy, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    fn render_batch_add_auth_section(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kinds = [
            (
                BatchAddAuthKind::Password,
                "sessionManager.batchAdd.auth_password",
            ),
            (BatchAddAuthKind::Key, "sessionManager.batchAdd.auth_key"),
        ];
        let mut row = div().flex().items_center().gap(px(12.0)).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text))
                .child(self.i18n.t("sessionManager.batchAdd.auth_label")),
        );
        for (kind, key) in kinds {
            let selected = snapshot.auth_kind == kind;
            row = row.child(
                div()
                    .id(match kind {
                        BatchAddAuthKind::Password => "batch-add-auth-password",
                        BatchAddAuthKind::Key => "batch-add-auth-key",
                    })
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(if selected {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
                    .child(self.i18n.t(key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_batch_add_auth_kind(kind, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let mut column = div().flex().flex_col().gap(px(8.0)).child(row);
        match snapshot.auth_kind {
            BatchAddAuthKind::Password => {
                column = column.child(self.render_session_password_input(
                    SessionManagerInput::BatchPassword,
                    self.i18n.t("sessionManager.batchAdd.password_placeholder"),
                    cx,
                ));
            }
            BatchAddAuthKind::Key => {
                // The dropdown only suggests discovered keys; the path input
                // below stays editable for keys outside ~/.ssh.
                column = column.child(self.render_batch_add_key_trigger(cx)).child(
                    self.render_session_text_input(
                        SessionManagerInput::BatchKeyPath,
                        &self
                            .session_manager
                            .read(cx)
                            .batch_add
                            .as_ref()
                            .map(|dialog| dialog.key_path.clone())
                            .unwrap_or_default(),
                        self.i18n.t("sessionManager.batchAdd.key_path_placeholder"),
                        cx,
                    ),
                );
            }
        }
        column.into_any_element()
    }

    fn render_batch_add_key_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let key_path = {
            let session_manager = self.session_manager.read(cx);
            let Some(dialog) = session_manager.batch_add.as_ref() else {
                return div().into_any_element();
            };
            dialog.key_path.clone()
        };
        let selected_name = std::path::Path::new(&key_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                self.i18n
                    .t("sessionManager.batchAdd.key_select_placeholder")
            });
        let workspace = cx.entity();
        let trigger = div()
            .id("batch-add-key-trigger")
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w(px(0.0))
            .flex_1()
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(theme.accent)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.toggle_batch_add_key_menu(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(if key_path.trim().is_empty() {
                        theme.text_muted
                    } else {
                        theme.text
                    }))
                    .child(selected_name),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronDown,
                12.0,
                rgb(theme.text_muted),
            ));
        select_anchor_probe(
            SelectAnchorId::SessionManagerBatchKeyAuth,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    /// The key dropdown: discovered private keys from the SSH directory; a row
    /// click fills the key path input below the dropdown.
    pub(in crate::workspace) fn render_batch_add_key_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.key_menu_open)
        {
            return None;
        }
        let anchor = self
            .select_anchors
            .get(&SelectAnchorId::SessionManagerBatchKeyAuth)?;
        let key_path = self
            .session_manager
            .read(cx)
            .batch_add
            .as_ref()
            .map(|dialog| dialog.key_path.clone())
            .unwrap_or_default();
        let width = f32::from(anchor.bounds.size.width).max(280.0);
        let viewport_height = f32::from(window.viewport_size().height);
        let popup_gap = self.tokens.metrics.settings_select_popup_gap;
        let below = viewport_height - f32::from(anchor.bounds.bottom()) - popup_gap;
        let above = f32::from(anchor.bounds.top()) - popup_gap;
        let opens_above = below < self.tokens.metrics.ui_select_max_height && above > below;
        let max_height = if opens_above { above } else { below }
            .max(self.tokens.metrics.ui_control_height)
            .min(self.tokens.metrics.ui_select_max_height);

        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height);
        let keys = oxideterm_connections::list_available_ssh_keys();
        if keys.is_empty() {
            popup = popup.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.xs))
                    .py(px(self.tokens.metrics.ui_menu_item_padding_y))
                    .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("sessionManager.batchAdd.key_none_found")),
            );
        }
        for (key_index, key) in keys.into_iter().enumerate() {
            let selected = key_path == key.path;
            let key_path_owned = key.path.clone();
            let mut row = div()
                .id(format!("batch-key-option-{key_index}"))
                .flex()
                .items_center()
                .gap(px(8.0))
                .w_full()
                .h(px(self.tokens.metrics.ui_control_height))
                .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                .rounded(px(self.tokens.radii.xs))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_batch_add_key(&key_path_owned, cx);
                        this.close_batch_add_key_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .children(selected.then(|| {
                    Self::render_lucide_icon(LucideIcon::Check, 14.0, rgb(self.tokens.ui.accent))
                }))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(if selected {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text
                        }))
                        .child(key.name.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .truncate()
                        .max_w(px(160.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(key.key_type.clone()),
                );
            if key.has_passphrase {
                row = row.child(Self::render_lucide_icon(
                    LucideIcon::Lock,
                    13.0,
                    rgb(self.tokens.ui.text_muted),
                ));
            }
            popup = popup.child(row);
        }

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };
        let popup = popup.into_any_element();
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_batch_add_key_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_batch_add_key_menu(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(anchor_corner)
                            .position(position)
                            .offset(point(px(0.0), px(offset_y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn select_batch_add_key(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.batch_add.as_mut() else {
                return;
            };
            dialog.key_path = path.to_owned();
            dialog.error = None;
            cx.notify();
        });
        self.refresh_batch_add_preview(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_batch_add_key_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                dialog.key_menu_open = !dialog.key_menu_open;
                dialog.template_menu_open = false;
                cx.notify();
            }
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_batch_add_key_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if let Some(dialog) = session_manager.batch_add.as_mut() {
                if dialog.key_menu_open {
                    dialog.key_menu_open = false;
                    cx.notify();
                }
            }
        });
    }

    pub(super) fn batch_issue_text(&self, issue: &BatchIssue) -> String {
        let kind = &issue.kind;
        let key = format!("sessionManager.batchAdd.errors.{}", kind.message_key());
        let line = issue.line.to_string();
        let text = match kind {
            BatchIssueKind::DirectiveUnsupported { keyword } => {
                self.i18n_with(&key, &[("keyword", keyword.clone())])
            }
            BatchIssueKind::PlaceholderUnknown { placeholder } => {
                self.i18n_with(&key, &[("placeholder", placeholder.clone())])
            }
            BatchIssueKind::QueryKeyUnknown { key } => {
                self.i18n_with(&key, &[("key", key.clone())])
            }
            BatchIssueKind::QueryValueEmpty { key } => {
                self.i18n_with(&key, &[("key", key.clone())])
            }
            BatchIssueKind::RowLimitExceeded { limit } => {
                self.i18n_with(&key, &[("limit", limit.to_string())])
            }
            _ => self.i18n.t(&key),
        };
        self.i18n_with("sessionManager.batchAdd.issue_line", &[("line", line)]) + &text
    }
}

/// Render-time projection of the dialog so borrows do not cross listener setup.
struct BatchAddDialogSnapshot {
    error: Option<String>,
    template_issue: Option<BatchIssue>,
    preview_rows: Vec<BatchPreviewRow>,
    issues: Vec<BatchIssue>,
    addable: usize,
    skipped: usize,
    invalid: usize,
    needs_fallback_auth: bool,
    auth_kind: BatchAddAuthKind,
    duplicate: BatchDuplicateNamePolicy,
}
