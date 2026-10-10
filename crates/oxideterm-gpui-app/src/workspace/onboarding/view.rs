use super::*;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::modal::modal_backdrop;
use oxideterm_gpui_ui::motion::{self, MotionDuration};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{SegmentedControlOptions, segmented_control, segmented_control_item};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let margin = if f32::from(viewport.width) < 700.0 {
            12.0
        } else {
            32.0
        };
        let width = (f32::from(viewport.width) - margin * 2.0).max(0.0);
        let height = (f32::from(viewport.height) - margin * 2.0).max(0.0);
        let wide = width >= 1100.0;
        let step = OnboardingStep::from_index(self.onboarding.step);
        let content = match step {
            OnboardingStep::Welcome => self.onboarding_columns(
                wide,
                self.render_onboarding_welcome(cx),
                self.render_onboarding_disclaimer(cx),
            ),
            OnboardingStep::Appearance => self.render_onboarding_appearance(wide, cx),
            OnboardingStep::Workflow => div()
                .w_full()
                .max_w(px(1440.0))
                .mx_auto()
                .child(self.render_onboarding_workflow(wide, cx))
                .child(self.render_onboarding_features(wide, cx))
                .into_any_element(),
            OnboardingStep::Tools => self.onboarding_columns(
                wide,
                div()
                    .flex()
                    .flex_col()
                    .child(self.render_onboarding_ai_intro(cx))
                    .child(self.render_onboarding_ai_setup(window, cx))
                    .into_any_element(),
                self.render_onboarding_cli_companion(window, cx),
            ),
            OnboardingStep::QuickStart => div()
                .w_full()
                .max_w(px(1100.0))
                .mx_auto()
                .child(self.render_onboarding_quick_start(window, cx))
                .into_any_element(),
        };

        let content = if let Some((generation, _)) =
            self.segmented_control_user_transition(ONBOARDING_STEPS_MOTION_ID, self.onboarding.step)
        {
            let previous = self
                .segmented_control_user_previous_index(
                    ONBOARDING_STEPS_MOTION_ID,
                    self.onboarding.step,
                )
                .unwrap_or(self.onboarding.step);
            motion::slide_fade_in_x(
                &self.tokens,
                (ONBOARDING_STEPS_MOTION_ID, generation as usize),
                div().w_full().child(content),
                if self.onboarding.step > previous {
                    12.0
                } else {
                    -12.0
                },
                MotionDuration::Control,
            )
        } else {
            content
        };

        modal_backdrop(rgba((theme.bg << 8) | 0x99))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(motion::form_enter(
                &self.tokens,
                "onboarding-dialog",
                oxideterm_gpui_ui::modal_container(&self.tokens)
                    .w(px(width))
                    .h(px(height))
                    // Onboarding copy must remain readable above the user's workspace.
                    .bg(rgb(theme.bg_panel))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.onboarding_progress(cx))
                    .child(
                        div()
                            .id(("onboarding-body", self.onboarding.step))
                            .flex_1()
                            .min_h(px(0.0))
                            .px(px(if wide { 24.0 } else { 0.0 }))
                            .py(px(16.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.onboarding.scroll_handle)
                            .vertical_scrollbar(&self.onboarding.scroll_handle)
                            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                                if this.open_settings_select.is_some() {
                                    this.close_settings_select();
                                    this.clear_settings_select_anchors();
                                    cx.notify();
                                }
                            }))
                            .child(content),
                    )
                    .when(self.onboarding.save_failed, |panel| {
                        panel.child(
                            div()
                                .px(px(24.0))
                                .py(px(8.0))
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(theme.error))
                                .child(self.i18n.t("onboarding.save_failed")),
                        )
                    })
                    .child(self.onboarding_footer(
                        self.onboarding.step > 0,
                        self.onboarding.step + 1 < ONBOARDING_TOTAL_STEPS,
                        !self.onboarding.disclaimer_accepted,
                        cx,
                    )),
            ))
            .into_any_element()
    }

    pub(super) fn onboarding_columns(
        &self,
        wide: bool,
        first: AnyElement,
        second: AnyElement,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w(px(1440.0))
            .mx_auto()
            .grid()
            .grid_cols(if wide { 2 } else { 1 })
            .child(div().min_w(px(0.0)).child(first))
            .child(div().min_w(px(0.0)).child(second))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_progress(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut items = Vec::new();
        for index in 0..ONBOARDING_TOTAL_STEPS {
            let step = OnboardingStep::from_index(index);
            let selected = index == self.onboarding.step;
            let locked = !self.onboarding.disclaimer_accepted && index > 0;
            items.push(
                segmented_control_item(&self.tokens, self.i18n.t(step.title_key()), selected)
                    .whitespace_normal()
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
                        cx.listener(move |this, _, _, cx| {
                            if !locked {
                                this.onboarding_go_to_step(index, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element(),
            );
        }
        let previous = self
            .segmented_control_user_previous_index(ONBOARDING_STEPS_MOTION_ID, self.onboarding.step)
            .unwrap_or(self.onboarding.step);
        let steps = segmented_control(
            &self.tokens,
            ONBOARDING_STEPS_MOTION_ID,
            SegmentedControlOptions::new(self.onboarding.step, previous, ONBOARDING_TOTAL_STEPS)
                .underline(900.0)
                .user_transition_active(self.segmented_control_user_transition_active(
                    ONBOARDING_STEPS_MOTION_ID,
                    self.onboarding.step,
                )),
            items,
        );
        div()
            .flex_none()
            .px(px(24.0))
            .py(px(16.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.0))
            .child(
                div().flex().items_center().justify_center().child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child(format!(
                            "{} / {}",
                            self.onboarding.step + 1,
                            ONBOARDING_TOTAL_STEPS
                        )),
                ),
            )
            .child(steps)
            .into_any_element()
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
            .flex_none()
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
                self.tokens.radii.md,
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
        .when(variant == ButtonVariant::Default, |button| {
            button
                .bg(rgb(theme.accent))
                .text_color(rgb(theme.accent_text))
        })
        .when(!disabled, |button| {
            button.hover(move |button| {
                button.bg(rgb(if variant == ButtonVariant::Default {
                    theme.accent_hover
                } else {
                    theme.bg_hover
                }))
            })
        })
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
            button = button.child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(if variant == ButtonVariant::Default {
                    theme.accent_text
                } else {
                    theme.text
                }),
            ));
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
