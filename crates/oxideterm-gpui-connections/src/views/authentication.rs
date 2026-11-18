use gpui::{
    AnyElement, App, Context, Entity, MouseButton, MouseDownEvent, Render, SharedString, Window,
    div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    MaterialRole, TextInputView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_action_button,
    },
    form_field, material_surface,
    modal::{modal_container, rounded_shell_child_radius},
};
use oxideterm_i18n::I18n;
use oxideterm_ssh::HostKeyStatus;
use oxideterm_theme::ThemeTokens;

use super::{
    ConnectionsInput, ConnectionsViewBindings, ConnectionsWindowRequest, connection_text_input,
};
use crate::{
    entity::{AuthenticationChanged, ConnectionFlowEntity, HostKeyDialogSnapshot},
    prompts::{KeyboardInteractiveChallenge, NativeSshPromptSender},
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostKeyButtonAction {
    Cancel,
    TrustOnce,
    TrustSave,
    ReplaceSaved,
}

pub(super) struct AuthenticationView {
    flow: Entity<ConnectionFlowEntity>,
    bindings: ConnectionsViewBindings,
    _authentication_changed: gpui::Subscription,
}

impl AuthenticationView {
    pub(super) fn new(
        flow: Entity<ConnectionFlowEntity>,
        bindings: ConnectionsViewBindings,
        cx: &mut Context<Self>,
    ) -> Self {
        let authentication_changed =
            cx.subscribe(&flow, |_, _, _: &AuthenticationChanged, cx| cx.notify());
        Self {
            flow,
            bindings,
            _authentication_changed: authentication_changed,
        }
    }
}

impl Render for AuthenticationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(environment) = (self.bindings.environment)(cx) else {
            return div();
        };
        let frame = AuthenticationFrame {
            tokens: environment.tokens,
            i18n: environment.i18n,
            bindings: &self.bindings,
        };
        let host_key = self
            .flow
            .read(cx)
            .host_key_dialog_snapshot()
            .map(|challenge| frame.render_host_key_dialog(challenge, cx));
        let authentication = self
            .flow
            .read(cx)
            .keyboard_interactive_challenge()
            .map(|challenge| frame.render_keyboard_interactive_dialog(challenge, &self.flow, cx));
        div()
            .absolute()
            .inset_0()
            .children(host_key)
            .children(authentication)
    }
}

struct AuthenticationFrame<'a> {
    tokens: ThemeTokens,
    i18n: I18n,
    bindings: &'a ConnectionsViewBindings,
}

impl AuthenticationFrame<'_> {
    fn on_action<F>(
        &self,
        request: F,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static + use<F>
    where
        F: Fn() -> ConnectionsWindowRequest + 'static,
    {
        let dispatch = self.bindings.dispatch.clone();
        move |_, window, cx| {
            dispatch(request(), Some(window), cx);
            cx.stop_propagation();
        }
    }

    fn render_host_key_dialog(&self, challenge: HostKeyDialogSnapshot, cx: &mut App) -> AnyElement {
        let dialog_visible = challenge.visible;
        let theme = self.tokens.ui;
        let (title, message, key_type, fingerprint, changed) = match &challenge.status {
            HostKeyStatus::Unknown {
                fingerprint,
                key_type,
            } => (
                self.i18n.t("ssh.host_key.title_unknown"),
                self.i18n.t("ssh.host_key.unknown_message"),
                key_type.clone(),
                fingerprint.clone(),
                false,
            ),
            HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => (
                self.i18n.t("ssh.host_key.title_changed"),
                format!(
                    "{}\n{}: {}\n{}: {}",
                    self.i18n.t("ssh.host_key.changed_warning"),
                    self.i18n.t("ssh.host_key.expected_fingerprint"),
                    expected_fingerprint,
                    self.i18n.t("ssh.host_key.actual_fingerprint"),
                    actual_fingerprint
                ),
                key_type.clone(),
                actual_fingerprint.clone(),
                true,
            ),
            HostKeyStatus::Error { message } => (
                self.i18n.t("ssh.host_key.title_error"),
                message.clone(),
                String::new(),
                String::new(),
                false,
            ),
            HostKeyStatus::Verified => (
                self.i18n.t("ssh.host_key.title_unknown"),
                String::new(),
                String::new(),
                String::new(),
                false,
            ),
        };

        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                self.on_action(|| ConnectionsWindowRequest::CancelHostKey),
            )
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                &self.tokens,
                "host-key-dialog-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "host-key-dialog-transition",
                material_surface(&self.tokens, div(), MaterialRole::Dialog)
                    .w(px(480.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme.text))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{}:{}", challenge.host, challenge.port)),
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .border_1()
                                    .border_color(if changed {
                                        rgba((theme.error << 8) | 0x80)
                                    } else {
                                        rgba((theme.warning << 8) | 0x80)
                                    })
                                    .bg(if changed {
                                        rgba((theme.error << 8) | 0x18)
                                    } else {
                                        rgba((theme.warning << 8) | 0x14)
                                    })
                                    .p(px(12.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(rgb(if changed {
                                        theme.error
                                    } else {
                                        theme.warning
                                    }))
                                    .child(message),
                            )
                            .when(!key_type.is_empty(), |body| {
                                body.child(self.render_host_key_value(
                                    self.i18n.t("ssh.host_key.key_type_label"),
                                    key_type,
                                    cx,
                                ))
                                .child(
                                    self.render_host_key_value(
                                        self.i18n.t("ssh.host_key.fingerprint_label"),
                                        fingerprint,
                                        cx,
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .py(px(12.0))
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_host_key_button(
                                self.i18n.t("ssh.host_key.actions.cancel"),
                                false,
                                HostKeyButtonAction::Cancel,
                            ))
                            .when(changed, |footer| {
                                footer.child(self.render_host_key_button(
                                    self.i18n.t("ssh.host_key.actions.replace_saved"),
                                    true,
                                    HostKeyButtonAction::ReplaceSaved,
                                ))
                            })
                            .when(!changed, |footer| {
                                footer
                                    .child(self.render_host_key_button(
                                        self.i18n.t("ssh.host_key.actions.trust_once"),
                                        false,
                                        HostKeyButtonAction::TrustOnce,
                                    ))
                                    .child(self.render_host_key_button(
                                        self.i18n.t("ssh.host_key.actions.trust_save"),
                                        true,
                                        HostKeyButtonAction::TrustSave,
                                    ))
                            }),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    fn render_host_key_value(&self, label: String, value: String, cx: &mut App) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child((self.bindings.selectable_text)(
                        &label,
                        &value,
                        true,
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg_hover))
                    .p(px(8.0))
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(rgb(self.tokens.ui.text))
                    .font_family(SharedString::from("SF Mono"))
                    .child((self.bindings.selectable_text)(
                        &label,
                        &value,
                        false,
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_key_button(
        &self,
        label: String,
        primary: bool,
        action: HostKeyButtonAction,
    ) -> AnyElement {
        let variant = match action {
            HostKeyButtonAction::Cancel => ButtonVariant::Outline,
            HostKeyButtonAction::TrustOnce => ButtonVariant::Secondary,
            HostKeyButtonAction::TrustSave if primary => ButtonVariant::Default,
            HostKeyButtonAction::ReplaceSaved if primary => ButtonVariant::Destructive,
            _ => ButtonVariant::Secondary,
        };
        // Host-key prompts are protected dialogs; only the button chrome moves
        // to the shared shadcn-style primitive. The explicit challenge actions
        // and non-dismissible backdrop semantics stay local.
        toolbar_action_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(self.tokens.metrics.form_button_height),
                padding_x: Some(self.tokens.metrics.form_button_padding_x),
                font_size: Some(self.tokens.metrics.form_text_font_size),
                ..ToolbarButtonOptions::default()
            },
            self.on_action(move || match action {
                HostKeyButtonAction::Cancel => ConnectionsWindowRequest::CancelHostKey,
                HostKeyButtonAction::TrustOnce => {
                    ConnectionsWindowRequest::AcceptHostKey { persist: false }
                }
                HostKeyButtonAction::TrustSave | HostKeyButtonAction::ReplaceSaved => {
                    ConnectionsWindowRequest::AcceptHostKey { persist: true }
                }
            }),
        )
        .into_any_element()
    }
    fn render_keyboard_interactive_dialog(
        &self,
        challenge: &KeyboardInteractiveChallenge,
        flow: &Entity<ConnectionFlowEntity>,
        cx: &App,
    ) -> AnyElement {
        let dialog_visible =
            challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let theme = self.tokens.ui;
        let title = if challenge.request.name.trim().is_empty() {
            self.i18n.t("ssh.kbi.title")
        } else {
            challenge.request.name.clone()
        };
        let timed_out = challenge.timed_out();
        let seconds_left = challenge.seconds_left();
        let can_submit = !timed_out && challenge.all_responses_filled();
        let external_wait = challenge.external_wait;

        let mut prompt_list = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap));
        for (index, prompt) in challenge.request.prompts.iter().enumerate() {
            let target = ConnectionsInput::KeyboardInteractive(index);
            let Some(frame) = (self.bindings.ime)(target, cx) else {
                continue;
            };
            let focused = challenge.focused_prompt == index;
            let value = challenge
                .responses
                .get(index)
                .map(String::as_str)
                .unwrap_or_default();
            prompt_list = prompt_list.child(form_field(
                &self.tokens,
                if prompt.prompt == "ssh.form.password" {
                    self.i18n.t("ssh.form.password")
                } else {
                    prompt.prompt.clone()
                },
                connection_text_input(
                    &self.tokens,
                    &self.bindings.dispatch,
                    target,
                    frame,
                    TextInputView {
                        value,
                        placeholder: String::new(),
                        focused,
                        caret_visible: false,
                        secret: !prompt.echo,
                        selected_all: false,
                        selected_range: None,
                        marked_text: None,
                    },
                ),
            ));
        }

        if challenge
            .response_tx
            .as_ref()
            .is_some_and(NativeSshPromptSender::can_remember)
        {
            prompt_list = prompt_list.child(
                oxideterm_gpui_ui::checkbox_with(
                    &self.tokens,
                    "ssh-prompt-save-password",
                    self.i18n.t("ssh.kbi.save_password_after_login"),
                    challenge.remember_password,
                    oxideterm_gpui_ui::CheckboxOptions {
                        disabled: timed_out,
                        ..Default::default()
                    },
                )
                .id("ssh-prompt-save-password")
                .on_mouse_down(MouseButton::Left, {
                    let flow = flow.downgrade();
                    move |_, _, cx| {
                        let _ =
                            flow.update(cx, |flow, cx| flow.toggle_prompt_password_remember(cx));
                        cx.stop_propagation();
                    }
                }),
            );
        }

        // Authentication fields redraw while waiting and typing. Avoid sampling
        // the entire window for blur on each of those frames.
        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                self.on_action(|| ConnectionsWindowRequest::CancelAuthentication),
            )
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                &self.tokens,
                "keyboard-interactive-dialog-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "keyboard-interactive-dialog-transition",
                modal_container(&self.tokens)
                    .bg(rgb(theme.bg_elevated))
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            // Browser DialogContent clips the painted header
                            // into the shell radius; keep native KBI prompts
                            // from exposing square top-corner pixels.
                            .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .when(
                                !challenge.request.instructions.trim().is_empty(),
                                |header| {
                                    header.child(
                                        div()
                                            .mt_1()
                                            .text_size(px(self
                                                .tokens
                                                .metrics
                                                .modal_description_font_size))
                                            .text_color(rgb(theme.text_muted))
                                            .child(challenge.request.instructions.clone()),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_body_gap))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x80)
                                    } else {
                                        rgb(theme.border)
                                    })
                                    .bg(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x18)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x80)
                                    })
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(if seconds_left <= 15 {
                                        rgb(theme.error)
                                    } else {
                                        rgb(theme.text_muted)
                                    })
                                    .child(if timed_out {
                                        self.i18n.t("modals.kbi.timeout")
                                    } else {
                                        self.i18n
                                            .t("modals.kbi.time_remaining")
                                            .replace("{{seconds}}", &seconds_left.to_string())
                                    }),
                            )
                            .child(prompt_list),
                    )
                    .child(
                        div()
                            .h(px(self.tokens.metrics.modal_footer_height))
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            // Footer chrome is flush with the dialog bottom,
                            // so it owns the inner bottom corners too.
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            .child(self.render_keyboard_interactive_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                false,
                            ))
                            .when(!external_wait, |footer| {
                                footer.child(self.render_keyboard_interactive_button(
                                    self.i18n.t("ssh.kbi.continue"),
                                    true,
                                    !can_submit,
                                ))
                            }),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    fn render_keyboard_interactive_button(
        &self,
        label: String,
        submit: bool,
        disabled: bool,
    ) -> AnyElement {
        let variant = if submit {
            ButtonVariant::Default
        } else {
            ButtonVariant::Ghost
        };
        // Keyboard-interactive prompts are authentication-protected dialogs, so
        // keep submit/cancel ownership here while sharing the Tauri Button
        // variant chrome.
        toolbar_action_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                height: Some(self.tokens.metrics.form_button_height),
                padding_x: Some(self.tokens.metrics.form_button_padding_x),
                font_size: Some(self.tokens.metrics.form_text_font_size),
                ..ToolbarButtonOptions::default()
            },
            self.on_action(move || {
                if submit {
                    ConnectionsWindowRequest::SubmitAuthentication
                } else {
                    ConnectionsWindowRequest::CancelAuthentication
                }
            }),
        )
        .into_any_element()
    }
}
