use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, div, prelude::FluentBuilder, px, relative, rgb, rgba,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::ButtonVariant,
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
    text_input::{TextInputView, text_input, text_input_anchor_probe},
};

use crate::workspace::{ime::WorkspaceImeTarget, settings::settings_dialog_transition};

use super::{
    PORTABLE_SETTINGS_DIALOG_WIDTH, PortableSettingsAction, PortableSettingsDialog, WorkspaceApp,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_portable_password_change_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.portable_settings_dialog != Some(PortableSettingsDialog::ChangePassword) {
            return None;
        }
        let pending =
            self.portable_settings_action_pending == Some(PortableSettingsAction::ChangePassword);
        let can_submit = !pending && !self.portable_current_password.is_empty();

        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_portable_password_change_dialog(cx);
                cx.stop_propagation();
            }),
        );
        let form = dialog_content(&self.tokens)
            .w(px(PORTABLE_SETTINGS_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.general.portable_change_password_title"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.general.portable_change_password_description"),
                    )),
            )
            .child(
                div()
                    .px(px(24.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.portable_password_field(
                        "settings_view.general.portable_current_password",
                        SettingsInput::PortableCurrentPassword,
                        self.portable_current_password.clone(),
                        cx,
                    ))
                    .child(self.portable_password_field(
                        "settings_view.general.portable_new_password",
                        SettingsInput::PortableNewPassword,
                        self.portable_new_password.clone(),
                        cx,
                    ))
                    .child(self.portable_password_field(
                        "settings_view.general.portable_confirm_password",
                        SettingsInput::PortableConfirmPassword,
                        self.portable_confirm_password.clone(),
                        cx,
                    ))
                    .when_some(
                        self.portable_settings_action_error.clone(),
                        |body, error| {
                            body.child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x4d))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.error))
                                    .child(error),
                            )
                        },
                    ),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        pending,
                        |this, _event, _window, cx| {
                            this.close_portable_password_change_dialog(cx);
                        },
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        if pending {
                            self.i18n
                                .t("settings_view.general.portable_change_password_pending")
                        } else {
                            self.i18n
                                .t("settings_view.general.portable_submit_change_password")
                        },
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_submit,
                        |this, _event, _window, cx| {
                            this.submit_portable_password_change(cx);
                        },
                        cx,
                    )),
            );
        Some(settings_dialog_transition(
            &self.tokens,
            "portable-password-dialog-form",
            backdrop,
            form,
            self.portable_settings_dialog_presence,
        ))
    }

    pub(in crate::workspace) fn portable_password_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.portable_password_input(input, value, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn portable_password_input(
        &self,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder: String::new(),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: true,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }
}
