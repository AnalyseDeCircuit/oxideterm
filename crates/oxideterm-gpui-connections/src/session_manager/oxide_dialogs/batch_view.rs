use super::*;
use gpui::{AnchoredPositionMode, Corner, anchored, deferred, point};
use oxideterm_connections::{AuthType, BatchIssue, BatchPreviewRow, BatchRowStatus, BatchTemplate};
use oxideterm_gpui_ui::{
    badge::{StatusPillOptions, StatusTone, status_pill},
    modal::{TAURI_SELECT_LAYER_PRIORITY, popover_backdrop},
    select::{
        OverlayAnchor, SelectAnchorId, select_anchor_probe, select_overlay_popup_with_max_height,
    },
    tabs::{segmented_tab, segmented_tabs},
};

const BATCH_ADD_DIALOG_WIDTH: f32 = 960.0;
const BATCH_ADD_DIALOG_HEIGHT: f32 = 640.0;
const BATCH_ADD_TRIGGER_MIN_WIDTH: f32 = 160.0;
const BATCH_ADD_TEXTAREA_VERTICAL_PADDING: f32 = 8.0;
const BATCH_ADD_TEMPLATE_EDITOR_HEIGHT: f32 = 124.0;
const BATCH_ADD_DATA_EDITOR_MIN_HEIGHT: f32 = 176.0;
const BATCH_ADD_PREVIEW_MAX_ROWS: usize = 120;
const BATCH_ADD_ISSUE_MAX_ROWS: usize = 10;

struct TemplateLabel {
    id: String,
    name: String,
}

struct BatchPreviewRowLabel {
    name: String,
    host: String,
    port: u16,
    username: String,
    group: Option<String>,
    tags: Vec<String>,
    has_proxy_command: bool,
    auth_type: AuthType,
    status: BatchRowStatus,
}

impl From<&BatchPreviewRow> for BatchPreviewRowLabel {
    fn from(row: &BatchPreviewRow) -> Self {
        Self {
            name: row.name.clone(),
            host: row.host.clone(),
            port: row.port,
            username: row.username.clone(),
            group: row.group.clone(),
            tags: row.tags.clone(),
            has_proxy_command: row.has_proxy_command,
            auth_type: row.auth_type,
            status: row.status,
        }
    }
}

impl OxideDialogs {
    fn batch_action(&self, action: BatchWindowAction, window: Option<&mut Window>, cx: &mut App) {
        (self.dispatch)(Request::Batch(action), window, cx);
    }

    fn begin_batch_add_dialog_exit(&self, cx: &mut App) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |manager, cx| {
            manager.begin_batch_add_dialog_exit(delay, cx);
        });
    }

    fn update_select_anchor(&self, anchor: OverlayAnchor, cx: &mut App) {
        if self.batch_anchors.borrow_mut().insert(anchor.id, anchor) != Some(anchor) {
            let open = self
                .session_manager
                .read(cx)
                .batch_add
                .as_ref()
                .is_some_and(|dialog| dialog.template_menu_open || dialog.key_menu_open);
            if open {
                self.session_manager.update(cx, |_, cx| cx.notify());
            }
        }
    }

    fn i18n_with(&self, key: &str, replacements: &[(&str, String)]) -> String {
        super::super::batch_i18n(&self.i18n, key, replacements)
    }

    fn batch_issue_text(&self, issue: &BatchIssue) -> String {
        super::super::batch_issue_text(&self.i18n, issue)
    }

    pub(super) fn render_batch_add_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
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
                            template_summary: batch_template_summary(dialog.template.as_ref()),
                            template_expanded: dialog.template_expanded
                                || dialog.template_issue.is_some(),
                            syntax_help_open: dialog.syntax_help_open,
                            preview_rows: dialog
                                .preview
                                .rows
                                .iter()
                                .take(BATCH_ADD_PREVIEW_MAX_ROWS)
                                .map(BatchPreviewRowLabel::from)
                                .collect(),
                            preview_count: dialog.preview.rows.len(),
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

        let left_column = div()
            .w(relative(0.46))
            .min_w(px(320.0))
            .min_h(px(0.0))
            .border_r_1()
            .border_color(rgb(theme.border))
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_batch_add_template_section(&dialog_snapshot, cx))
            .child(self.render_batch_add_data_section(&dialog_snapshot, cx))
            .when(dialog_snapshot.needs_fallback_auth, |col| {
                col.child(self.render_batch_add_auth_section(&dialog_snapshot, cx))
            });

        let right_column = div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(self.render_batch_add_preview_panel(&dialog_snapshot, cx))
            .when_some(dialog_snapshot.error.clone(), |col, error| {
                col.child(
                    div()
                        .px(px(16.0))
                        .pb(px(12.0))
                        .child(self.render_oxide_error_banner(error, cx)),
                )
            });

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .child(left_column)
            .child(right_column);

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
                this.batch_action(BatchWindowAction::Apply, None, cx);
            },
            |this, _event, _window, cx| {
                this.begin_batch_add_dialog_exit(cx);
            },
            cx,
        );

        let close_button = self.workspace_icon_action_button(
            LucideIcon::X,
            16.0,
            rgb(theme.text_muted),
            IconButtonOptions {
                size: 24.0,
                radius: ButtonRadius::Sm,
                disabled: false,
                loading: false,
                has_background: false,
                background: None,
                border: None,
                hover_background: Some(rgba(0x00000000)),
                hover_opacity: Some(1.0),
                focus_visible: false,
                idle_opacity: 0.7,
                disabled_opacity: 0.35,
            },
            |this, _event, _window, cx| {
                this.begin_batch_add_dialog_exit(cx);
                cx.notify();
                cx.stop_propagation();
            },
            cx,
        );

        backdrop
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "batch-add-dialog-transition",
                material_surface(&self.tokens, div(), MaterialRole::Dialog)
                    .w(px(BATCH_ADD_DIALOG_WIDTH))
                    .h(px(BATCH_ADD_DIALOG_HEIGHT))
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
                            .child(close_button),
                    )
                    .child(body)
                    .child(
                        div()
                            .px(px(OXIDE_MODAL_BODY_P))
                            .pt(px(8.0))
                            .pb(px(OXIDE_MODAL_HEADER_PY))
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .child(footer),
                    ),
                phase == oxideterm_gpui_ui::motion::ExitPhase::Visible,
            ))
            .into_any_element()
    }

    fn render_batch_add_toolbar_button(
        &self,
        id: &'static str,
        icon: LucideIcon,
        label_key: &str,
        variant: ButtonVariant,
        disabled: bool,
        cx: &mut Context<Self>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let icon_color = rgb(if disabled {
            self.tokens.ui.text_muted
        } else {
            self.tokens.ui.text
        });
        let button = self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            Some(Self::render_lucide_icon(icon, 13.0, icon_color).into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled,
                    ..ButtonOptions::default()
                },
                height: Some(26.0),
                padding_x: Some(8.0),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, event, window, cx| {
                listener(this, event, window, cx);
                cx.stop_propagation();
            }),
        );
        button.id(id).into_any_element()
    }

    /// Template card: collapsed by default with a single-line parsed summary so
    /// the hosts input owns the primary space; clicking the header expands the
    /// SSH config editor and template management buttons.
    fn render_batch_add_template_section(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (templates, selected_id) = {
            let session_manager = self.session_manager.read(cx);
            let Some(dialog) = session_manager.batch_add.as_ref() else {
                return div().into_any_element();
            };
            (
                dialog
                    .templates
                    .iter()
                    .map(|template| TemplateLabel {
                        id: template.id.clone(),
                        name: template.name.clone(),
                    })
                    .collect::<Vec<_>>(),
                dialog.selected_template_id.clone(),
            )
        };
        let selected_name = templates
            .iter()
            .find(|template| Some(&template.id) == selected_id.as_ref())
            .map(|template| template.name.clone())
            .unwrap_or_else(|| self.i18n.t("sessionManager.batchAdd.preset_name"));
        let selected_is_user_template = selected_id
            .as_deref()
            .is_some_and(|id| !is_factory_batch_template_id(id));
        let workspace = cx.entity();
        let trigger = div()
            .id("batch-add-template-trigger")
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .flex_1()
            .min_w(px(BATCH_ADD_TRIGGER_MIN_WIDTH))
            .max_w(px(210.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
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
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
                    .child(selected_name),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronDown,
                13.0,
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

        let toggle_icon = if snapshot.template_expanded {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };

        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pb(px(14.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("batch-add-template-toggle")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|style| style.text_color(rgb(theme.accent)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.session_manager.update(cx, |manager, cx| {
                                        manager.toggle_batch_template_expanded(cx);
                                    });
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                toggle_icon,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("sessionManager.batchAdd.template_label")),
                            ),
                    )
                    .child(trigger),
            );

        if snapshot.template_expanded {
            card = card
                .child(self.render_batch_add_editor_box(
                    BatchAddEditorKind::Template,
                    Some(BATCH_ADD_TEMPLATE_EDITOR_HEIGHT),
                    cx,
                ))
                .when_some(snapshot.template_issue.as_ref(), |card, issue| {
                    card.child(self.render_oxide_status_line(
                        self.batch_issue_text(issue),
                        true,
                        cx,
                    ))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("sessionManager.batchAdd.template_hint")),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(self.render_batch_add_toolbar_button(
                                    "batch-add-save-template",
                                    LucideIcon::Save,
                                    "sessionManager.batchAdd.save",
                                    ButtonVariant::Outline,
                                    !selected_is_user_template,
                                    cx,
                                    |this, _event, _window, cx| {
                                        this.batch_action(
                                            BatchWindowAction::SaveTemplate,
                                            None,
                                            cx,
                                        );
                                    },
                                ))
                                .child(self.render_batch_add_toolbar_button(
                                    "batch-add-save-new-template",
                                    LucideIcon::FilePlus,
                                    "sessionManager.batchAdd.save_as_new",
                                    ButtonVariant::Outline,
                                    false,
                                    cx,
                                    |this, _event, _window, cx| {
                                        this.batch_action(
                                            BatchWindowAction::NameTemplate,
                                            None,
                                            cx,
                                        );
                                    },
                                )),
                        ),
                );
        } else {
            let summary = snapshot.template_summary.clone().unwrap_or_else(|| {
                self.i18n
                    .t("sessionManager.batchAdd.template_summary_empty")
            });
            card = card.child(
                div()
                    .pl(px(20.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(summary),
            );
        }

        card.into_any_element()
    }

    /// The template dropdown popup: click a row to load it, pencil renames
    /// inline, trash deletes; the factory preset only loads.
    pub(super) fn render_batch_add_template_overlay(
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
            .batch_anchors
            .borrow()
            .get(&SelectAnchorId::SessionManagerBatchTemplate)
            .copied()?;
        let (templates, selected_id, renaming_id, naming_new) = {
            let session_manager = self.session_manager.read(cx);
            let dialog = session_manager.batch_add.as_ref()?;
            (
                dialog
                    .templates
                    .iter()
                    .map(|template| TemplateLabel {
                        id: template.id.clone(),
                        name: template.name.clone(),
                    })
                    .collect::<Vec<_>>(),
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
                                    this.batch_action(
                                        BatchWindowAction::CommitTemplateName,
                                        None,
                                        cx,
                                    );
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
                                        this.batch_action(
                                            BatchWindowAction::CommitTemplateName,
                                            None,
                                            cx,
                                        );
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
                        this.batch_action(
                            BatchWindowAction::ChooseTemplate(load_id.clone()),
                            None,
                            cx,
                        );
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
                                    this.batch_action(
                                        BatchWindowAction::RenameTemplate(rename_id.clone()),
                                        None,
                                        cx,
                                    );
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
                                    this.batch_action(
                                        BatchWindowAction::DeleteTemplate(delete_id.clone()),
                                        None,
                                        cx,
                                    );
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
        let popup_offset_y = if opens_above { 4.0 } else { -4.0 };
        let popup_origin = gpui::point(0.5, if opens_above { 1.0 } else { 0.0 });
        let popup = oxideterm_gpui_ui::motion::popover_zoom_enter(
            &self.tokens,
            "batch-add-template-menu",
            div().child(popup),
            popup_offset_y,
            popup_origin,
        );
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

    /// Hosts section: primary editor surface with file import and a toggleable
    /// syntax card so the form stays calm until reference is needed.
    fn render_batch_add_data_section(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut section = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("sessionManager.batchAdd.data_label")),
                            )
                            .child(self.render_batch_add_toolbar_button(
                                "batch-add-syntax-help",
                                LucideIcon::HelpCircle,
                                "sessionManager.batchAdd.syntax_help",
                                if snapshot.syntax_help_open {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                },
                                false,
                                cx,
                                |this, _event, _window, cx| {
                                    this.session_manager.update(cx, |manager, cx| {
                                        manager.toggle_batch_syntax_help(cx);
                                    });
                                },
                            )),
                    )
                    .child(self.render_batch_add_toolbar_button(
                        "batch-add-import-hosts",
                        LucideIcon::Upload,
                        "sessionManager.batchAdd.import_from_file",
                        ButtonVariant::Outline,
                        false,
                        cx,
                        |this, _event, _window, cx| {
                            this.batch_action(BatchWindowAction::PickDataFile, None, cx);
                        },
                    )),
            );

        if snapshot.syntax_help_open {
            section = section.child(
                div()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((theme.bg << 8) | 0x99))
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(16.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sessionManager.batchAdd.data_hint")),
            );
        }

        section
            .child(self.render_batch_add_editor_box(BatchAddEditorKind::Data, None, cx))
            .into_any_element()
    }

    fn render_batch_add_editor_box(
        &self,
        kind: BatchAddEditorKind,
        fixed_height: Option<f32>,
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
        let box_div = div()
            .id(match kind {
                BatchAddEditorKind::Template => "batch-add-template-editor",
                BatchAddEditorKind::Data => "batch-add-data-editor",
            })
            .w_full()
            .min_w_0();
        let box_div = match fixed_height {
            Some(height) => box_div.h(px(height)).flex_none(),
            None => box_div.flex_1().min_h(px(BATCH_ADD_DATA_EDITOR_MIN_HEIGHT)),
        };
        box_div
            .p(px(BATCH_ADD_TEXTAREA_VERTICAL_PADDING))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(if focused {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(rgb(self.tokens.ui.bg))
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _event, window, cx| {
                // Focus without consuming the event: the editor underneath must
                // still see the mouse down for caret placement and selection.
                this.batch_action(BatchWindowAction::FocusEditor(kind), Some(window), cx);
            }))
            .children(editor)
            .into_any_element()
    }

    fn render_batch_add_preview_panel(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut panel = div().flex_1().min_h(px(0.0)).flex().flex_col().child(
            div()
                .px(px(20.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(rgba((theme.border << 8) | 0x80))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .flex_wrap()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(self.i18n.t("sessionManager.batchAdd.preview_title")),
                        )
                        .child(self.render_batch_add_summary(snapshot)),
                )
                .child(self.render_batch_add_duplicate_policy(snapshot, cx)),
        );

        if let Some(issues) = self.render_batch_add_issues(snapshot) {
            panel = panel.child(div().px(px(20.0)).pt(px(12.0)).child(issues));
        }

        panel
            .child(self.render_batch_add_preview_table(snapshot))
            .into_any_element()
    }

    fn render_batch_add_summary(&self, snapshot: &BatchAddDialogSnapshot) -> AnyElement {
        if snapshot.preview_count == 0 && snapshot.invalid == 0 {
            return div().into_any_element();
        }
        let added_label = self.i18n_with(
            "sessionManager.batchAdd.summary_added",
            &[("count", snapshot.addable.to_string())],
        );
        let mut row = div().flex().items_center().gap(px(6.0)).child(status_pill(
            &self.tokens,
            added_label,
            StatusPillOptions::new(if snapshot.addable > 0 {
                StatusTone::Success
            } else {
                StatusTone::Neutral
            })
            .compact(),
        ));
        if snapshot.skipped > 0 {
            row = row.child(status_pill(
                &self.tokens,
                self.i18n_with(
                    "sessionManager.batchAdd.summary_skipped",
                    &[("count", snapshot.skipped.to_string())],
                ),
                StatusPillOptions::new(StatusTone::Warning).compact(),
            ));
        }
        if snapshot.invalid > 0 {
            row = row.child(status_pill(
                &self.tokens,
                self.i18n_with(
                    "sessionManager.batchAdd.summary_invalid",
                    &[("count", snapshot.invalid.to_string())],
                ),
                StatusPillOptions::new(StatusTone::Error).compact(),
            ));
        }
        row.into_any_element()
    }

    fn render_batch_add_issues(&self, snapshot: &BatchAddDialogSnapshot) -> Option<AnyElement> {
        if snapshot.issues.is_empty() {
            return None;
        }
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((OXIDE_RED_500 << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((OXIDE_RED_500 << 8) | OXIDE_TONE_BG_ALPHA))
            .px(px(10.0))
            .py(px(8.0));
        for issue in snapshot.issues.iter().take(BATCH_ADD_ISSUE_MAX_ROWS) {
            list = list.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(OXIDE_RED_500))
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
        if snapshot.preview_rows.is_empty() {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .p(px(24.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .child(Self::render_lucide_icon(
                    LucideIcon::Server,
                    24.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t("sessionManager.batchAdd.preview_empty_title")),
                )
                .child(
                    div()
                        .max_w(px(300.0))
                        .text_center()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("sessionManager.batchAdd.preview_empty_hint")),
                )
                .into_any_element();
        }

        let header = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(20.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgba((theme.bg_panel << 8) | 0x66))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.text_muted))
            .child(self.table_cell("sessionManager.batchAdd.column.name", 96.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.0))
                    .truncate()
                    .child(self.i18n.t("sessionManager.batchAdd.column.target")),
            )
            .child(self.table_cell("sessionManager.batchAdd.column.user", 68.0))
            .child(self.table_cell("sessionManager.batchAdd.column.auth", 56.0))
            .child(self.table_cell("sessionManager.batchAdd.column.status", 88.0));

        let mut rows = div()
            .id("batch-add-preview-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();

        for row in snapshot
            .preview_rows
            .iter()
            .take(BATCH_ADD_PREVIEW_MAX_ROWS)
        {
            let dimmed = matches!(
                row.status,
                BatchRowStatus::Unchanged
                    | BatchRowStatus::SkippedDuplicateTarget
                    | BatchRowStatus::SkippedExistingName
            );
            let mut meta_parts = Vec::new();
            if let Some(group) = row.group.as_deref().filter(|g| !g.is_empty()) {
                meta_parts.push(group.to_string());
            }
            if !row.tags.is_empty() {
                meta_parts.push(row.tags.join(", "));
            }
            let meta_line = (!meta_parts.is_empty()).then(|| meta_parts.join(" · "));

            let auth_text = if row.has_proxy_command {
                format!(
                    "{} · {}",
                    self.i18n.t(if row.auth_type == AuthType::Password {
                        "sessionManager.batchAdd.auth_password"
                    } else {
                        "sessionManager.batchAdd.auth_key"
                    }),
                    self.i18n.t("sessionManager.batchAdd.proxy_yes")
                )
            } else {
                self.i18n.t(if row.auth_type == AuthType::Password {
                    "sessionManager.batchAdd.auth_password"
                } else {
                    "sessionManager.batchAdd.auth_key"
                })
            };

            let status_tone = match row.status {
                BatchRowStatus::Add => StatusTone::Success,
                BatchRowStatus::Renamed => StatusTone::Accent,
                BatchRowStatus::Update => StatusTone::Info,
                BatchRowStatus::Unchanged
                | BatchRowStatus::SkippedDuplicateTarget
                | BatchRowStatus::SkippedExistingName => StatusTone::Neutral,
            };
            let status_label = self.i18n.t(&format!(
                "sessionManager.batchAdd.status.{}",
                row.status.label_key()
            ));

            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(20.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x44))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
                    .opacity(if dimmed { 0.55 } else { 1.0 })
                    .child(
                        div()
                            .w(px(96.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(div().truncate().child(row.name.clone()))
                            .when_some(meta_line, |cell, meta| {
                                cell.child(
                                    div()
                                        .truncate()
                                        .text_color(rgb(theme.text_muted))
                                        .child(meta),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(120.0))
                            .truncate()
                            .child(format!("{}:{}", row.host, row.port)),
                    )
                    .child(self.table_text(&row.username, 68.0))
                    .child(self.table_text(&auth_text, 56.0))
                    .child(div().w(px(88.0)).flex_none().flex().justify_start().child(
                        status_pill(
                            &self.tokens,
                            status_label,
                            StatusPillOptions::new(status_tone).compact(),
                        ),
                    )),
            );
        }

        if snapshot.preview_count > BATCH_ADD_PREVIEW_MAX_ROWS {
            rows = rows.child(
                div()
                    .px(px(20.0))
                    .py(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n_with(
                        "sessionManager.batchAdd.preview_overflow",
                        &[("count", BATCH_ADD_PREVIEW_MAX_ROWS.to_string())],
                    )),
            );
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(header)
            .child(rows)
            .into_any_element()
    }

    fn table_cell(&self, key: &str, width: f32) -> AnyElement {
        div()
            .w(px(width))
            .flex_none()
            .truncate()
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

    fn render_batch_add_duplicate_policy(
        &self,
        snapshot: &BatchAddDialogSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = [
            (
                BatchDuplicateNamePolicy::Skip,
                "sessionManager.batchAdd.duplicate_skip",
                "batch-add-duplicate-skip",
            ),
            (
                BatchDuplicateNamePolicy::Rename,
                "sessionManager.batchAdd.duplicate_rename",
                "batch-add-duplicate-rename",
            ),
        ];
        let mut group = segmented_tabs(&self.tokens)
            .border_1()
            .border_color(rgb(self.tokens.ui.border));
        for (policy, key, id) in options {
            let selected = snapshot.duplicate == policy;
            group = group.child(
                segmented_tab(&self.tokens, self.i18n.t(key), selected)
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.session_manager.update(cx, |manager, cx| {
                                manager.set_batch_duplicate_policy(policy, cx)
                            });
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("sessionManager.batchAdd.duplicate_label")),
            )
            .child(group)
            .into_any_element()
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
                "batch-add-auth-password",
            ),
            (
                BatchAddAuthKind::Key,
                "sessionManager.batchAdd.auth_key",
                "batch-add-auth-key",
            ),
        ];
        let mut group = segmented_tabs(&self.tokens)
            .border_1()
            .border_color(rgb(self.tokens.ui.border));
        for (kind, key, id) in kinds {
            let selected = snapshot.auth_kind == kind;
            group = group.child(
                segmented_tab(&self.tokens, self.i18n.t(key), selected)
                    .id(id)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.session_manager
                                .update(cx, |manager, cx| manager.set_batch_auth_kind(kind, cx));
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("sessionManager.batchAdd.auth_label")),
            )
            .child(group);
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pt(px(14.0))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            .child(row);
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
    pub(super) fn render_batch_add_key_overlay(
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
            .batch_anchors
            .borrow()
            .get(&SelectAnchorId::SessionManagerBatchKeyAuth)
            .copied()?;
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
                        this.session_manager.update(cx, |manager, cx| {
                            manager.select_batch_key(&key_path_owned, cx)
                        });
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
        let popup_offset_y = if opens_above { 4.0 } else { -4.0 };
        let popup_origin = gpui::point(0.5, if opens_above { 1.0 } else { 0.0 });
        let popup = oxideterm_gpui_ui::motion::popover_zoom_enter(
            &self.tokens,
            "batch-add-key-menu",
            div().child(popup),
            popup_offset_y,
            popup_origin,
        );
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

    fn toggle_batch_add_template_menu(&self, cx: &mut App) {
        self.session_manager
            .update(cx, |manager, cx| manager.set_batch_template_menu(None, cx));
    }

    fn close_batch_add_template_menu(&self, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            manager.set_batch_template_menu(Some(false), cx)
        });
    }

    fn toggle_batch_add_key_menu(&self, cx: &mut App) {
        self.session_manager
            .update(cx, |manager, cx| manager.set_batch_key_menu(None, cx));
    }

    fn close_batch_add_key_menu(&self, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            manager.set_batch_key_menu(Some(false), cx)
        });
    }
}

/// Render-time projection of the dialog so borrows do not cross listener setup.
struct BatchAddDialogSnapshot {
    error: Option<String>,
    template_issue: Option<BatchIssue>,
    template_summary: Option<String>,
    template_expanded: bool,
    syntax_help_open: bool,
    preview_rows: Vec<BatchPreviewRowLabel>,
    preview_count: usize,
    issues: Vec<BatchIssue>,
    addable: usize,
    skipped: usize,
    invalid: usize,
    needs_fallback_auth: bool,
    auth_kind: BatchAddAuthKind,
    duplicate: BatchDuplicateNamePolicy,
}

fn batch_template_summary(template: Option<&BatchTemplate>) -> Option<String> {
    let template = template?;
    let mut parts = Vec::new();
    if let Some(user) = template.username.as_deref().filter(|v| !v.is_empty()) {
        parts.push(format!("User {user}"));
    }
    if let Some(port) = template.port {
        parts.push(format!("Port {port}"));
    }
    if let Some(identity) = template.identity_file.as_deref().filter(|v| !v.is_empty()) {
        parts.push(format!("IdentityFile {identity}"));
    }
    if let Some(cert) = template
        .certificate_file
        .as_deref()
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("CertificateFile {cert}"));
    }
    if template.proxy_command.is_some() {
        parts.push("ProxyCommand".to_string());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::ConnectionsEnvironment;

    #[gpui::test]
    fn batch_dialog_rename_click_updates_the_conflicting_row(cx: &mut gpui::TestAppContext) {
        let manager = cx.new(|_| SessionManagerState::default());
        let tokens = oxideterm_theme::default_tokens();
        let i18n = I18n::new(oxideterm_i18n::Locale::En);
        manager.update(cx, |state, cx| {
            state.open_batch_add(tokens, "monospace".into(), &i18n, Rc::new(|_| vec![]), cx);
            let dialog = state.batch_add.as_ref().unwrap();
            dialog
                .template_editor
                .as_ref()
                .unwrap()
                .update(cx, |e, cx| {
                    e.replace_text_external("Host *\n  User demo\n  IdentityFile ~/.ssh/demo", cx)
                });
            dialog.data_editor.as_ref().unwrap().update(cx, |e, cx| {
                e.replace_text_external("one.test?name=lab\ntwo.test?name=lab", cx)
            });
        });
        let (_, cx) = cx.add_window_view(|_, cx| {
            OxideDialogs::new(
                ManagerOverlay::Batch,
                manager.clone(),
                Rc::new(move |_| {
                    Some(ConnectionsEnvironment {
                        tokens,
                        i18n: i18n.clone(),
                    })
                }),
                SessionManagerViewBindings {
                    catalog: Rc::new(|_, _| None),
                    oxide_environment: Rc::new(|_| Some(Default::default())),
                    ime: Rc::new(|_, _| None),
                    text: Rc::new(|_, _, _, text, _, _, _| div().child(text).into_any_element()),
                },
                Rc::new(|_, _, _| {}),
                cx,
                Default::default(),
            )
        });
        cx.simulate_resize(gpui::size(px(1100.), px(1050.)));
        for (button, name, status) in [
            (
                "batch-add-duplicate-rename",
                "lab-2",
                BatchRowStatus::Renamed,
            ),
            (
                "batch-add-duplicate-skip",
                "lab",
                BatchRowStatus::SkippedExistingName,
            ),
        ] {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let bounds = cx.debug_bounds(button).expect("duplicate policy button");
            cx.simulate_click(bounds.center(), gpui::Modifiers::default());
            manager.read_with(cx, |state, _| {
                let row = &state.batch_add.as_ref().unwrap().preview.rows[1];
                assert_eq!(
                    (row.host.as_str(), row.name.as_str(), row.status),
                    ("two.test", name, status)
                );
            });
        }
    }
}
