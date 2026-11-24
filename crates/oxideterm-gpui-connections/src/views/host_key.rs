use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, KeyDownEvent, MouseButton, Render,
    SharedString, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{MaterialRole, button::ButtonVariant, material_surface};

use super::{ConnectionsEnvironment, ConnectionsSelectableText, dialog::DialogRenderer};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostKeyPresentation {
    Unknown {
        key_type: String,
        fingerprint: String,
    },
    Changed {
        key_type: String,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
    Error {
        message: String,
    },
    Verified,
}

pub struct HostKeyFrame {
    pub host: String,
    pub port: u16,
    pub status: HostKeyPresentation,
    /// False while the existing dialog animates out.
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostKeyDecision {
    Cancel,
    AcceptOnce,
    AcceptAndPersist,
}

/// Mount one entity per challenge. The recipient removes the dialog after its decision.
pub struct PortableHostKeyDialog {
    environment: ConnectionsEnvironment,
    frame: HostKeyFrame,
    selectable_text: ConnectionsSelectableText,
    focus: FocusHandle,
    decide: Option<Box<dyn FnOnce(HostKeyDecision, &mut App)>>,
}

impl PortableHostKeyDialog {
    pub fn new(
        environment: ConnectionsEnvironment,
        frame: HostKeyFrame,
        selectable_text: ConnectionsSelectableText,
        decide: impl FnOnce(HostKeyDecision, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            environment,
            frame,
            selectable_text,
            focus,
            decide: Some(Box::new(decide)),
        }
    }

    fn finish(&mut self, decision: HostKeyDecision, cx: &mut Context<Self>) {
        if !self.frame.visible {
            return;
        }
        if let Some(decide) = self.decide.take() {
            // Only an unknown or changed key has a fingerprint the caller can trust.
            let decision = match self.frame.status {
                HostKeyPresentation::Error { .. } | HostKeyPresentation::Verified => {
                    HostKeyDecision::Cancel
                }
                _ => decision,
            };
            self.frame.visible = false;
            cx.notify();
            cx.defer(move |cx| decide(decision, cx));
        }
    }
}

impl Focusable for PortableHostKeyDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PortableHostKeyDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        let renderer = DialogRenderer {
            tokens: &self.environment.tokens,
            i18n: &self.environment.i18n,
            dispatch: Rc::new(move |decision, _, cx| {
                let _ = owner.update(cx, |view, cx| view.finish(decision, cx));
            }),
        };
        div()
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    view.finish(HostKeyDecision::Cancel, cx);
                    cx.stop_propagation();
                }
            }))
            .child(renderer.render_host_key_dialog(&self.frame, &self.selectable_text, cx))
    }
}

impl DialogRenderer<'_, HostKeyDecision> {
    pub(super) fn render_host_key_dialog(
        &self,
        challenge: &HostKeyFrame,
        selectable_text: &ConnectionsSelectableText,
        cx: &mut App,
    ) -> AnyElement {
        let dialog_visible = challenge.visible;
        let theme = self.tokens.ui;
        let (title, message, key_type, fingerprint, changed) = match &challenge.status {
            HostKeyPresentation::Unknown {
                fingerprint,
                key_type,
            } => (
                self.i18n.t("ssh.host_key.title_unknown"),
                self.i18n.t("ssh.host_key.unknown_message"),
                key_type.clone(),
                fingerprint.clone(),
                false,
            ),
            HostKeyPresentation::Changed {
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
            HostKeyPresentation::Error { message } => (
                self.i18n.t("ssh.host_key.title_error"),
                message.clone(),
                String::new(),
                String::new(),
                false,
            ),
            HostKeyPresentation::Verified => (
                self.i18n.t("ssh.host_key.title_unknown"),
                String::new(),
                String::new(),
                String::new(),
                false,
            ),
        };

        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(MouseButton::Left, self.on_action(HostKeyDecision::Cancel))
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                self.tokens,
                "host-key-dialog-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                self.tokens,
                "host-key-dialog-transition",
                material_surface(self.tokens, div(), MaterialRole::Dialog)
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
                                    selectable_text,
                                    self.i18n.t("ssh.host_key.key_type_label"),
                                    key_type,
                                    cx,
                                ))
                                .child(
                                    self.render_host_key_value(
                                        selectable_text,
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
                            .child(self.button(
                                self.i18n.t("ssh.host_key.actions.cancel"),
                                ButtonVariant::Outline,
                                false,
                                HostKeyDecision::Cancel,
                            ))
                            .when(changed, |footer| {
                                footer.child(self.button(
                                    self.i18n.t("ssh.host_key.actions.replace_saved"),
                                    ButtonVariant::Destructive,
                                    false,
                                    HostKeyDecision::AcceptAndPersist,
                                ))
                            })
                            .when(!changed, |footer| {
                                footer
                                    .child(self.button(
                                        self.i18n.t("ssh.host_key.actions.trust_once"),
                                        ButtonVariant::Secondary,
                                        false,
                                        HostKeyDecision::AcceptOnce,
                                    ))
                                    .child(self.button(
                                        self.i18n.t("ssh.host_key.actions.trust_save"),
                                        ButtonVariant::Default,
                                        false,
                                        HostKeyDecision::AcceptAndPersist,
                                    ))
                            }),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    fn render_host_key_value(
        &self,
        selectable_text: &ConnectionsSelectableText,
        label: String,
        value: String,
        cx: &mut App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child((selectable_text)(
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
                    .child((selectable_text)(
                        &label,
                        &value,
                        false,
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }
}
