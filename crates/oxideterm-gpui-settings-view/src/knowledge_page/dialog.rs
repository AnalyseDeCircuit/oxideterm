use std::rc::Rc;

use gpui::{AnyElement, App, Div, MouseButton, div, prelude::*, px, relative, rgb};
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
    motion::ExitPhase,
    text_input::TextInputContentAlign,
};
use oxideterm_settings_model::{KnowledgeDeleteConfirm, KnowledgeDeleteTarget};

use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsCalloutTone, SettingsControls},
    dialog::settings_dialog_transition,
};

const KNOWLEDGE_DIALOG_WIDTH: f32 = 520.0;
const KNOWLEDGE_DIALOG_FIELD_WIDTH: f32 = 420.0;

#[derive(Clone, Copy)]
pub enum KnowledgeDialogAction {
    Backdrop,
    Cancel,
    Confirm,
}

pub enum KnowledgeDialogContent<'a> {
    Collection {
        name: &'a str,
        scope_label: String,
        phase: ExitPhase,
    },
    Document {
        title: &'a str,
        format_label: String,
        error: Option<&'a str>,
        phase: ExitPhase,
    },
    Delete(&'a KnowledgeDeleteConfirm),
}

pub struct KnowledgeDialog<'a> {
    pub controls: SettingsControls<'a>,
    pub focus: Option<ConfirmDialogAction>,
    pub dispatch: Rc<dyn Fn(KnowledgeDialogAction, &mut App)>,
}

impl KnowledgeDialog<'_> {
    pub fn render(&self, content: KnowledgeDialogContent<'_>, cx: &App) -> AnyElement {
        self.render_with_fields(content, None, cx)
    }

    pub(crate) fn render_with_fields(
        &self,
        content: KnowledgeDialogContent<'_>,
        fields: Option<(AnyElement, AnyElement)>,
        cx: &App,
    ) -> AnyElement {
        let controls = &self.controls;
        let tokens = controls.tokens;
        let i18n = controls.i18n;
        let mut dialog = dialog_content(tokens)
            .w(px(KNOWLEDGE_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let (title_key, description, confirm_key, confirm_variant, disabled, form) = match content {
            KnowledgeDialogContent::Collection {
                name,
                scope_label,
                phase,
            } => (
                "settings_view.knowledge.create_collection",
                i18n.t("settings_view.knowledge.create_description"),
                "settings_view.knowledge.create_collection",
                ButtonVariant::Default,
                name.trim().is_empty(),
                Some((
                    "knowledge-create-dialog-form",
                    phase,
                    self.form(name, scope_label, false, None, fields, cx),
                )),
            ),
            KnowledgeDialogContent::Document {
                title,
                format_label,
                error,
                phase,
            } => (
                "settings_view.knowledge.new_document",
                i18n.t("settings_view.knowledge.new_document_description"),
                "settings_view.knowledge.new_document",
                ButtonVariant::Default,
                title.trim().is_empty(),
                Some((
                    "knowledge-document-dialog-form",
                    phase,
                    self.form(title, format_label, true, error, fields, cx),
                )),
            ),
            KnowledgeDialogContent::Delete(confirm) => {
                let key = match confirm.target {
                    KnowledgeDeleteTarget::Collection => {
                        "settings_view.knowledge.delete_collection_confirm"
                    }
                    KnowledgeDeleteTarget::Document => {
                        "settings_view.knowledge.delete_document_confirm"
                    }
                };
                (
                    "settings_view.knowledge.delete_confirm_title",
                    i18n.t(key).replace("{{name}}", &confirm.name),
                    "common.delete",
                    ButtonVariant::Destructive,
                    false,
                    None,
                )
            }
        };
        dialog = dialog.child(
            dialog_header(tokens)
                .child(dialog_title(tokens, i18n.t(title_key)))
                .child(dialog_description(tokens, description)),
        );
        let mut motion = None;
        if let Some((id, phase, form)) = form {
            dialog = dialog.child(form);
            motion = Some((id, phase));
        }
        dialog = dialog.child(
            dialog_footer(tokens)
                .child(self.footer(
                    "common.actions.cancel",
                    ButtonVariant::Outline,
                    false,
                    ConfirmDialogAction::Cancel,
                ))
                .child(self.footer(
                    confirm_key,
                    confirm_variant,
                    disabled,
                    ConfirmDialogAction::Confirm,
                )),
        );
        let dispatch = self.dispatch.clone();
        let backdrop =
            dismissible_dialog_backdrop().on_mouse_down(MouseButton::Left, move |_, _, cx| {
                dispatch(KnowledgeDialogAction::Backdrop, cx);
                cx.stop_propagation();
            });
        if let Some((id, phase)) = motion {
            settings_dialog_transition(tokens, id, backdrop, dialog, phase)
        } else {
            backdrop.child(dialog).into_any_element()
        }
    }

    fn form(
        &self,
        value: &str,
        choice: String,
        document: bool,
        error: Option<&str>,
        fields: Option<(AnyElement, AnyElement)>,
        cx: &App,
    ) -> Div {
        let controls = &self.controls;
        let (input, label, placeholder, select, select_label) = if document {
            (
                SettingsInput::KnowledgeDocumentTitle,
                "settings_view.knowledge.new_document_title",
                "settings_view.knowledge.new_document_title_placeholder",
                SettingsSelect::KnowledgeDocumentFormat,
                "settings_view.knowledge.format",
            )
        } else {
            (
                SettingsInput::KnowledgeCollectionName,
                "settings_view.knowledge.collection_name",
                "settings_view.knowledge.collection_name_placeholder",
                SettingsSelect::KnowledgeCollectionScope,
                "settings_view.knowledge.scope",
            )
        };
        let (field, select) = fields.unwrap_or_else(|| {
            (
                controls.field(
                    input,
                    value,
                    controls.i18n.t(placeholder),
                    Some(KNOWLEDGE_DIALOG_FIELD_WIDTH),
                    TextInputContentAlign::Start,
                    false,
                    cx,
                ),
                controls.settings_select_control(
                    select,
                    choice,
                    false,
                    Some(KNOWLEDGE_DIALOG_FIELD_WIDTH),
                    cx,
                ),
            )
        });
        let labeled = |key, field| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(controls.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(controls.tokens.ui.text))
                        .child(controls.i18n.t(key)),
                )
                .child(field)
        };
        div()
            .px(px(24.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(labeled(label, field))
            .child(labeled(select_label, select))
            .when_some(error, |form, error| {
                form.child(controls.settings_callout(
                    SettingsCalloutTone::Error,
                    Some("lucide/alert-circle.svg"),
                    div().child(error.to_owned()).into_any_element(),
                ))
            })
    }

    fn footer(
        &self,
        key: &str,
        variant: ButtonVariant,
        disabled: bool,
        action: ConfirmDialogAction,
    ) -> Div {
        let dispatch = self.dispatch.clone();
        toolbar_button(
            self.controls.tokens,
            self.controls.i18n.t(key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                focus_visible: self.focus == Some(action),
                ..Default::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            if !disabled {
                dispatch(
                    if action == ConfirmDialogAction::Confirm {
                        KnowledgeDialogAction::Confirm
                    } else {
                        KnowledgeDialogAction::Cancel
                    },
                    cx,
                );
            }
            cx.stop_propagation();
        })
    }
}
