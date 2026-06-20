use super::*;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let step = OnboardingStep::from_index(self.onboarding.step);
        let can_go_back = self.onboarding.step > 0;
        let can_go_next = self.onboarding.step + 1 < ONBOARDING_TOTAL_STEPS;
        let next_disabled =
            step == OnboardingStep::Disclaimer && !self.onboarding.disclaimer_accepted;

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba((theme.bg << 8) | 0x99))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri prevents outside dismiss until the disclaimer is accepted.
                    this.close_onboarding_if_allowed(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .w(px(ONBOARDING_WIDTH))
                    .max_h(px(ONBOARDING_MAX_HEIGHT))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | 0xf2))
                    .shadow_lg()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .child(self.onboarding_progress(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scrollbar()
                            .child(match step {
                                OnboardingStep::Welcome => self.render_onboarding_welcome(cx),
                                OnboardingStep::Disclaimer => self.render_onboarding_disclaimer(cx),
                                OnboardingStep::Appearance => self.render_onboarding_appearance(cx),
                                OnboardingStep::Workflow => self.render_onboarding_workflow(cx),
                                OnboardingStep::Features => {
                                    self.render_onboarding_features(window, cx)
                                }
                                OnboardingStep::AiIntro => self.render_onboarding_ai_intro(cx),
                                OnboardingStep::AiSetup => {
                                    self.render_onboarding_ai_setup(window, cx)
                                }
                                OnboardingStep::CliCompanion => {
                                    self.render_onboarding_cli_companion(window, cx)
                                }
                                OnboardingStep::QuickStart => {
                                    self.render_onboarding_quick_start(window, cx)
                                }
                            }),
                    )
                    .child(self.onboarding_footer(can_go_back, can_go_next, next_disabled, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_progress(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .pt(px(20.0))
            .pb(px(4.0));
        for index in 0..ONBOARDING_TOTAL_STEPS {
            let step = OnboardingStep::from_index(index);
            let selected = index == self.onboarding.step;
            let completed = index < self.onboarding.step;
            let locked = !self.onboarding.disclaimer_accepted && index > 1;
            let bg = if selected {
                rgb(theme.accent)
            } else if completed {
                rgba((theme.accent << 8) | 0x33)
            } else {
                rgb(theme.bg_card)
            };
            let color = if selected {
                rgb(theme.accent_text)
            } else if completed {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            };
            row = row.child(
                div()
                    .size(px(ONBOARDING_PROGRESS_ICON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(if selected || completed {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(bg)
                    .opacity(if locked {
                        ONBOARDING_DISABLED_OPACITY
                    } else {
                        1.0
                    })
                    .cursor(if locked {
                        CursorStyle::OperationNotAllowed
                    } else {
                        CursorStyle::PointingHand
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.onboarding_go_to_step(index, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_lucide_icon(step.icon(), 14.0, color)),
            );
        }
        row.into_any_element()
    }

    pub(in crate::workspace) fn onboarding_footer(
        &self,
        can_go_back: bool,
        can_go_next: bool,
        next_disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(32.0))
            .py(px(16.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_card << 8) | ONBOARDING_CARD_ALPHA))
            // The onboarding footer is the rounded shell's bottom painted
            // child; keep its background clipped to the browser panel curve.
            .rounded_b(px(oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                self.tokens.radii.lg,
            )))
            .child(if can_go_back {
                self.onboarding_button(
                    self.i18n.t("onboarding.back"),
                    Some(LucideIcon::ChevronLeft),
                    ButtonVariant::Ghost,
                    false,
                    |this, _window, cx| this.onboarding_back(cx),
                    cx,
                )
            } else {
                div().into_any_element()
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .when(can_go_next && self.onboarding.disclaimer_accepted, |row| {
                        row.child(self.onboarding_button(
                            self.i18n.t("onboarding.skip"),
                            None,
                            ButtonVariant::Ghost,
                            false,
                            |this, _window, cx| this.onboarding_skip_to_quick_start(cx),
                            cx,
                        ))
                    })
                    .child(if can_go_next {
                        self.onboarding_button(
                            self.i18n.t("onboarding.next"),
                            Some(LucideIcon::ChevronRight),
                            ButtonVariant::Default,
                            next_disabled,
                            |this, _window, cx| this.onboarding_next(cx),
                            cx,
                        )
                    } else {
                        self.onboarding_button(
                            self.i18n.t("onboarding.start_exploring"),
                            Some(LucideIcon::ArrowRight),
                            ButtonVariant::Default,
                            !self.onboarding.disclaimer_accepted,
                            |this, _window, cx| this.complete_onboarding(cx),
                            cx,
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_button(
        &self,
        label: String,
        icon: Option<LucideIcon>,
        variant: ButtonVariant,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut button = button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .gap(px(6.0))
        .opacity(if disabled {
            ONBOARDING_DISABLED_OPACITY
        } else {
            1.0
        })
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        });
        if let Some(icon) = icon {
            button = button.child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text)));
        }
        button
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if !disabled {
                        action(this, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
