use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Subscription,
    Window, div, prelude::*, px, relative, rgb, rgba,
};
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
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{SettingsInput, dialog::settings_dialog_transition, entity::SettingsWorkspaceEntity};

const PORTABLE_SETTINGS_DIALOG_WIDTH: f32 = 460.0;

pub struct PortablePasswordFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub focused_action: Option<ConfirmDialogAction>,
}

pub struct PortablePasswordDialog {
    settings: Entity<SettingsWorkspaceEntity>,
    frame: Rc<dyn Fn(&App) -> Option<PortablePasswordFrame>>,
    // The native IME adapter borrows the secret for the input renderer only.
    field: Rc<dyn Fn(SettingsInput, &App) -> AnyElement>,
    dispatch: Rc<dyn Fn(bool, &mut App)>,
    _settings_changed: Subscription,
}

impl PortablePasswordDialog {
    pub fn new(
        settings: Entity<SettingsWorkspaceEntity>,
        frame: Rc<dyn Fn(&App) -> Option<PortablePasswordFrame>>,
        field: Rc<dyn Fn(SettingsInput, &App) -> AnyElement>,
        dispatch: Rc<dyn Fn(bool, &mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings_changed = cx.observe(&settings, |_, settings, cx| {
            if settings.read(cx).portable_password_dialog_open() {
                cx.notify();
            }
        });
        Self {
            settings,
            frame,
            field,
            dispatch,
            _settings_changed: settings_changed,
        }
    }

    fn request(&self, submit: bool) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(submit, cx);
            cx.stop_propagation();
        }
    }

    fn footer_action(
        &self,
        frame: &PortablePasswordFrame,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
    ) -> Div {
        let request = self.request(action == ConfirmDialogAction::Confirm);
        toolbar_button(
            &frame.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                focus_visible: frame.focused_action == Some(action),
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if disabled {
                cx.stop_propagation();
            } else {
                request(event, window, cx);
            }
        })
    }

    fn render_dialog(&self, frame: &PortablePasswordFrame, cx: &App) -> AnyElement {
        let dialog = self.settings.read(cx).portable_password_dialog_snapshot();
        if !dialog.open {
            return div().into_any_element();
        }
        let pending = dialog.pending;
        let can_submit = !pending && dialog.current_password_present;

        let backdrop =
            dismissible_dialog_backdrop().on_mouse_down(MouseButton::Left, self.request(false));
        let form = dialog_content(&frame.tokens)
            .w(px(PORTABLE_SETTINGS_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&frame.tokens)
                    .child(dialog_title(
                        &frame.tokens,
                        frame
                            .i18n
                            .t("settings_view.general.portable_change_password_title"),
                    ))
                    .child(dialog_description(
                        &frame.tokens,
                        frame
                            .i18n
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
                    .child(self.portable_entity_password_field(
                        frame,
                        "settings_view.general.portable_current_password",
                        SettingsInput::PortableCurrentPassword,
                        cx,
                    ))
                    .child(self.portable_entity_password_field(
                        frame,
                        "settings_view.general.portable_new_password",
                        SettingsInput::PortableNewPassword,
                        cx,
                    ))
                    .child(self.portable_entity_password_field(
                        frame,
                        "settings_view.general.portable_confirm_password",
                        SettingsInput::PortableConfirmPassword,
                        cx,
                    ))
                    .when_some(dialog.error, |body, error| {
                        body.child(
                            div()
                                .rounded(px(frame.tokens.radii.md))
                                .border_1()
                                .border_color(rgba((frame.tokens.ui.error << 8) | 0x4d))
                                .bg(rgba((frame.tokens.ui.error << 8) | 0x1a))
                                .px(px(10.0))
                                .py(px(8.0))
                                .text_size(px(frame.tokens.metrics.ui_text_sm))
                                .text_color(rgb(frame.tokens.ui.error))
                                .child(error),
                        )
                    }),
            )
            .child(
                dialog_footer(&frame.tokens)
                    .child(self.footer_action(
                        frame,
                        frame.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        pending,
                    ))
                    .child(self.footer_action(
                        frame,
                        if pending {
                            frame
                                .i18n
                                .t("settings_view.general.portable_change_password_pending")
                        } else {
                            frame
                                .i18n
                                .t("settings_view.general.portable_submit_change_password")
                        },
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_submit,
                    )),
            );
        settings_dialog_transition(
            &frame.tokens,
            "portable-password-dialog-form",
            backdrop,
            form,
            dialog.presence.phase(),
        )
    }

    fn portable_entity_password_field(
        &self,
        frame: &PortablePasswordFrame,
        label_key: &str,
        input: SettingsInput,
        cx: &App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(frame.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(frame.tokens.ui.text))
                    .child(frame.i18n.t(label_key)),
            )
            .child((self.field)(input, cx))
            .into_any_element()
    }
}

impl Render for PortablePasswordDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.frame)(cx)
            .map(|frame| self.render_dialog(&frame, cx))
            .unwrap_or_else(|| div().into_any_element())
    }
}
