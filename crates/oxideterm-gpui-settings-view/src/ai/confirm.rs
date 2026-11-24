use std::time::Duration;

use gpui::{
    AnyElement, Context, Div, EventEmitter, MouseButton, Render, Task, Window, div, prelude::*, px,
    relative, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, SplitFooterButtonEdge,
        SplitFooterButtonOptions, ToolbarButtonOptions, split_footer_button, toolbar_button,
    },
    icons::LucideIcon,
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
    motion::{ExitPhase, ExitPresence, MotionDuration, duration},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::dialog::settings_dialog_inert_overlay;

const AI_CONFIRM_DIALOG_WIDTH: f32 = 448.0;
const AI_KEY_REMOVE_DIALOG_WIDTH: f32 = 384.0;
const AI_CONFIRM_BULLET_SIZE: f32 = 4.0;
const AI_CONFIRM_ICON_WRAP: f32 = 48.0;
const AI_CONFIRM_ICON: f32 = 24.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiSettingsConfirmIntent {
    Enable,
    RemoveProviderKey { provider_id: String },
    RemoveProvider { provider_id: String },
}

struct Draft {
    intent: AiSettingsConfirmIntent,
    provider_name: Option<String>,
}

pub struct AiSettingsConfirm {
    draft: Option<Draft>,
    tokens: ThemeTokens,
    i18n: I18n,
    presence: ExitPresence,
    exit_task: Option<Task<()>>,
    focus: Option<ConfirmDialogAction>,
}

impl EventEmitter<AiSettingsConfirmIntent> for AiSettingsConfirm {}

impl AiSettingsConfirm {
    pub fn new(tokens: ThemeTokens, i18n: I18n) -> Self {
        Self {
            draft: None,
            tokens,
            i18n,
            presence: ExitPresence::visible(),
            exit_task: None,
            focus: None,
        }
    }

    pub fn open(
        &mut self,
        intent: AiSettingsConfirmIntent,
        provider_name: Option<String>,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) {
        self.exit_task = None;
        self.presence.reopen();
        self.focus = None;
        self.tokens = tokens;
        self.i18n = i18n;
        self.draft = Some(Draft {
            intent,
            provider_name,
        });
        cx.notify();
    }

    pub fn state(&self) -> Option<(&AiSettingsConfirmIntent, ExitPhase)> {
        self.draft
            .as_ref()
            .map(|draft| (&draft.intent, self.presence.phase()))
    }

    pub fn set_focus(&mut self, focus: Option<ConfirmDialogAction>, cx: &mut Context<Self>) {
        self.focus = focus;
        cx.notify();
    }

    pub fn begin_exit(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        self.exit(
            confirmed,
            duration(&self.tokens, MotionDuration::Overlay),
            cx,
        );
    }

    fn exit(&mut self, confirmed: bool, delay: Duration, cx: &mut Context<Self>) {
        if self.draft.is_none() {
            return;
        }
        let Some(generation) = self.presence.begin_exit() else {
            return;
        };
        self.focus = None;
        if delay.is_zero() {
            self.complete(generation, confirmed, cx);
        } else {
            self.exit_task = Some(cx.spawn(async move |entity, cx| {
                cx.background_executor().timer(delay).await;
                let _ = entity.update(cx, |entity, cx| {
                    entity.exit_task = None;
                    entity.complete(generation, confirmed, cx);
                });
            }));
            cx.notify();
        }
    }

    fn complete(&mut self, generation: u64, confirmed: bool, cx: &mut Context<Self>) {
        if self.presence.finish_exit(generation) {
            self.presence.reopen();
            if let Some(draft) = self.draft.take()
                && confirmed
            {
                cx.emit(draft.intent);
            }
            cx.notify();
        }
    }

    fn standard_footer_action_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        cx: &Context<Self>,
    ) -> Div {
        toolbar_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    disabled: false,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                },
                focus_visible: self.focus == Some(action),
                ..Default::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.begin_exit(action == ConfirmDialogAction::Confirm, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn split_confirm_footer_action_button(
        &self,
        label: String,
        action: ConfirmDialogAction,
        destructive: bool,
        draw_right_separator: bool,
        cx: &Context<Self>,
    ) -> Div {
        let ui = self.tokens.ui;
        split_footer_button(
            &self.tokens,
            label,
            SplitFooterButtonOptions {
                text_color: rgb(if destructive { ui.error } else { ui.text_muted }),
                hover_text_color: rgb(if destructive { ui.error } else { ui.text }),
                hover_background: if destructive {
                    rgba((ui.error << 8) | 0x1a)
                } else {
                    rgba((ui.bg_hover << 8) | 0x80)
                },
                font_weight: if destructive {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::MEDIUM
                },
                focus_visible: self.focus == Some(action),
                right_separator: draw_right_separator,
                separator_color: Some(rgba((ui.border << 8) | 0x66)),
                disabled: false,
                loading: false,
                height: None,
                padding_y: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                edge: SplitFooterButtonEdge::None,
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.begin_exit(action == ConfirmDialogAction::Confirm, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_ai_enable_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_visible = self.presence.phase() == ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri SettingsView AI confirm is a Radix Dialog bound to
                    // setShowAiConfirm, so outside click is the Cancel path.
                    this.begin_exit(false, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-enable-confirm-form",
                dialog_content(&self.tokens)
                    .w(px(AI_CONFIRM_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        dialog_header(&self.tokens)
                            .child(dialog_title(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.title"),
                            ))
                            .child(dialog_description(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.description"),
                            )),
                    )
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai_confirm.intro")),
                            )
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                                    .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
                                    .p(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        self.ai_confirm_bullet(
                                            "settings_view.ai_confirm.point_local",
                                        ),
                                    )
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_no_server",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_context",
                                    )),
                            ),
                    )
                    .child(
                        dialog_footer(&self.tokens)
                            .child(self.standard_footer_action_button(
                                self.i18n.t("settings_view.ai_confirm.cancel"),
                                ButtonVariant::Ghost,
                                ConfirmDialogAction::Cancel,
                                cx,
                            ))
                            .child(self.standard_footer_action_button(
                                self.i18n.t("settings_view.ai_confirm.enable"),
                                ButtonVariant::Default,
                                ConfirmDialogAction::Confirm,
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }

    fn ai_confirm_bullet(&self, label_key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .mt(px(6.0))
                    .size(px(AI_CONFIRM_BULLET_SIZE))
                    .rounded(px(AI_CONFIRM_BULLET_SIZE / 2.0))
                    .bg(rgb(self.tokens.ui.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .into_any_element()
    }

    fn render_ai_provider_key_remove_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_visible = self.presence.phase() == ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri provider-key removal uses the shared confirm
                    // dialog; outside close cancels the pending removal.
                    this.begin_exit(false, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-provider-key-remove-form",
                dialog_content(&self.tokens)
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .rounded(px(self.tokens.radii.lg))
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .size(px(AI_CONFIRM_ICON_WRAP))
                                    .rounded(px(AI_CONFIRM_ICON_WRAP / 2.0))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x33))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::AlertTriangle,
                                        AI_CONFIRM_ICON,
                                        rgb(self.tokens.ui.error),
                                    )),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai.remove_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("common.actions.cancel"),
                                ConfirmDialogAction::Cancel,
                                false,
                                true,
                                cx,
                            ))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("settings_view.ai.remove"),
                                ConfirmDialogAction::Confirm,
                                true,
                                false,
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }

    fn render_ai_provider_remove_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = {
            let provider_name = self
                .draft
                .as_ref()
                .and_then(|draft| draft.provider_name.as_deref())
                .unwrap_or_default();
            self.i18n
                .t("settings_view.ai.remove_provider_confirm")
                .replace("{{name}}", provider_name)
        };
        let is_visible = self.presence.phase() == ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri remove-provider confirm is cancellable via
                    // Dialog onOpenChange(false).
                    this.begin_exit(false, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-provider-remove-form",
                dialog_content(&self.tokens)
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .rounded(px(self.tokens.radii.lg))
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .size(px(AI_CONFIRM_ICON_WRAP))
                                    .rounded(px(AI_CONFIRM_ICON_WRAP / 2.0))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x33))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::AlertTriangle,
                                        AI_CONFIRM_ICON,
                                        rgb(self.tokens.ui.error),
                                    )),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("common.actions.cancel"),
                                ConfirmDialogAction::Cancel,
                                false,
                                true,
                                cx,
                            ))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("settings_view.ai.remove"),
                                ConfirmDialogAction::Confirm,
                                true,
                                false,
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }
}

impl Render for AiSettingsConfirm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.draft.as_ref().map(|draft| &draft.intent) {
            Some(AiSettingsConfirmIntent::Enable) => self.render_ai_enable_confirm_dialog(cx),
            Some(AiSettingsConfirmIntent::RemoveProviderKey { .. }) => {
                self.render_ai_provider_key_remove_confirm_dialog(cx)
            }
            Some(AiSettingsConfirmIntent::RemoveProvider { .. }) => {
                self.render_ai_provider_remove_confirm_dialog(cx)
            }
            None => div().into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use std::{cell::RefCell, rc::Rc};

    #[gpui::test]
    fn replacement_and_cancel_do_not_emit_stale_confirmations(cx: &mut TestAppContext) {
        let tokens = oxideterm_theme::default_tokens();
        let i18n = I18n::default();
        let dialog = cx.new(|_| AiSettingsConfirm::new(tokens, i18n.clone()));
        let intents = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|cx| {
            let intents = intents.clone();
            cx.subscribe(&dialog, move |_, intent: &AiSettingsConfirmIntent, _| {
                intents.borrow_mut().push(intent.clone());
            })
        });
        let remove = AiSettingsConfirmIntent::RemoveProvider {
            provider_id: "provider-test".into(),
        };
        dialog.update(cx, |dialog, cx| {
            dialog.open(
                remove.clone(),
                Some("Provider Test".into()),
                tokens,
                i18n.clone(),
                cx,
            );
            dialog.exit(true, Duration::from_millis(10), cx);
            dialog.open(
                AiSettingsConfirmIntent::Enable,
                None,
                tokens,
                i18n.clone(),
                cx,
            );
            dialog.exit(true, Duration::ZERO, cx);
            dialog.open(remove, Some("Provider Test".into()), tokens, i18n, cx);
            dialog.exit(false, Duration::ZERO, cx);
        });
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        assert_eq!(*intents.borrow(), [AiSettingsConfirmIntent::Enable]);
    }
}
