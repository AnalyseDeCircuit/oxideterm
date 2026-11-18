use super::*;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::motion::{self, MotionDuration};
use oxideterm_gpui_ui::scroll::ScrollableElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn lock_workspace_chrome_when_onboarding_locked(
        &self,
        element: AnyElement,
        locked: bool,
    ) -> AnyElement {
        if !locked {
            return element;
        }
        div()
            .relative()
            .flex_none()
            .child(div().opacity(0.42).child(element))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .cursor(CursorStyle::OperationNotAllowed)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_scroll(|_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_tab(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport_width = f32::from(window.viewport_size().width);
        let sidebar_occupied = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let content_width = (viewport_width - sidebar_occupied).max(360.0);
        let wide = content_width >= 820.0;
        let horizontal_padding = if wide { 40.0 } else { 20.0 };
        let step = OnboardingStep::from_index(self.onboarding.step);
        let has_background = self.window_background_preferences().is_some();

        let body = match step {
            OnboardingStep::Welcome => self.render_onboarding_welcome_stage(wide, cx),
            OnboardingStep::Appearance => {
                self.render_onboarding_appearance(wide, content_width, cx)
            }
            OnboardingStep::Launch => self.render_onboarding_launch_stage(wide, window, cx),
        };

        let animated_body = if let Some((generation, _)) =
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
                div().w_full().child(body),
                if self.onboarding.step > previous {
                    14.0
                } else {
                    -14.0
                },
                MotionDuration::Control,
            )
        } else {
            body
        };

        div()
            .id("onboarding-workspace-tab")
            .size_full()
            .when(!has_background, |view| view.bg(rgb(theme.bg)))
            .flex()
            .flex_col()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .max_w(px(1080.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .px(px(horizontal_padding))
                    .pt(px(if wide { 24.0 } else { 16.0 }))
                    .pb(px(16.0))
                    .flex()
                    .flex_col()
                    .child(self.onboarding_progress(wide, cx))
                    .child(
                        div()
                            .id(("onboarding-body", self.onboarding.step))
                            .flex_1()
                            .min_h(px(0.0))
                            .py(px(if wide { 24.0 } else { 16.0 }))
                            .overflow_y_scroll()
                            .track_scroll(&self.onboarding.scroll_handle)
                            .vertical_scrollbar(&self.onboarding.scroll_handle)
                            .on_scroll(cx.listener(|this, _, _, cx| {
                                if this.open_settings_select.is_some() {
                                    this.close_settings_select();
                                    this.clear_settings_select_anchors();
                                    cx.notify();
                                }
                            }))
                            .child(div().w_full().flex().flex_col().child(animated_body)),
                    )
                    .when(self.onboarding.save_failed, |panel| {
                        panel.child(
                            div()
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
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_progress(
        &self,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = self.settings_store.settings();
        let mut rail = div().flex().items_center().gap(px(if wide { 8.0 } else { 4.0 }));
        for index in 0..ONBOARDING_TOTAL_STEPS {
            let step = OnboardingStep::from_index(index);
            let selected = index == self.onboarding.step;
            let completed = index < self.onboarding.step && self.onboarding.disclaimer_accepted;
            let locked = !self.onboarding.disclaimer_accepted && index > 0;
            let pill_bg = if selected {
                rgba((theme.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgba(0x00000000)
            };
            let badge_bg = if selected || completed {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_sunken)
            };
            let badge_fg = if selected || completed {
                rgb(theme.accent_text)
            } else {
                rgb(theme.text_muted)
            };
            rail = rail
                .child(
                    div()
                        .id(("onboarding-step-pill", index))
                        .rounded(px(self.tokens.radii.md))
                        .bg(pill_bg)
                        .when(!locked && !selected, |pill| {
                            pill.hover(move |style| style.bg(rgb(theme.bg_hover)))
                        })
                        .px(px(if wide { 10.0 } else { 7.0 }))
                        .py(px(5.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
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
                        .child(
                            div()
                                .size(px(20.0))
                                .rounded_full()
                                .bg(badge_bg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(self.tokens.metrics.ui_text_2xs))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(badge_fg)
                                .child(if completed {
                                    Self::render_lucide_icon(LucideIcon::Check, 11.0, badge_fg)
                                        .into_any_element()
                                } else if locked {
                                    Self::render_lucide_icon(LucideIcon::Lock, 10.0, badge_fg)
                                        .into_any_element()
                                } else {
                                    div().child(format!("{}", index + 1)).into_any_element()
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .font_weight(if selected {
                                    gpui::FontWeight::SEMIBOLD
                                } else {
                                    gpui::FontWeight::MEDIUM
                                })
                                .text_color(rgb(if selected {
                                    theme.text_heading
                                } else {
                                    theme.text_muted
                                }))
                                .child(self.i18n.t(step.title_key())),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if !locked {
                                    this.onboarding_go_to_step(index, cx);
                                }
                                cx.stop_propagation();
                            }),
                        ),
                )
                .when(index + 1 < ONBOARDING_TOTAL_STEPS, |row| {
                    row.child(
                        div()
                            .w(px(if wide { 24.0 } else { 10.0 }))
                            .h(px(1.5))
                            .rounded_full()
                            .bg(if completed {
                                rgba((theme.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                            } else {
                                rgba((theme.border << 8) | ONBOARDING_HAIRLINE_ALPHA)
                            }),
                    )
                });
        }

        let trailing = if self.onboarding.step == 0 {
            div()
                .flex_none()
                .child(self.select_setting_row(
                    "",
                    "",
                    SettingsSelect::Language,
                    self.language_label(settings.general.language),
                    156.0,
                    cx,
                ))
                .into_any_element()
        } else if self.onboarding.step + 1 < ONBOARDING_TOTAL_STEPS
            && self.onboarding.disclaimer_accepted
        {
            self.onboarding_button(
                self.i18n.t("onboarding.skip"),
                None,
                ButtonVariant::Ghost,
                false,
                |this, _window, cx| this.onboarding_skip_to_quick_start(cx),
                cx,
            )
        } else {
            div().into_any_element()
        };

        div()
            .flex_none()
            .pb(px(14.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(rail)
            .child(trailing)
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
            .pt(px(14.0))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
            .child(if can_go_back {
                self.onboarding_button(
                    self.i18n.t("onboarding.back"),
                    Some((LucideIcon::ChevronLeft, true)),
                    ButtonVariant::Ghost,
                    false,
                    |this, _window, cx| this.onboarding_back(cx),
                    cx,
                )
            } else {
                div().into_any_element()
            })
            .child(if can_go_next {
                self.onboarding_button(
                    self.i18n.t("onboarding.next"),
                    Some((LucideIcon::ChevronRight, false)),
                    ButtonVariant::Default,
                    next_disabled,
                    |this, _window, cx| this.onboarding_next(cx),
                    cx,
                )
            } else {
                self.onboarding_button(
                    self.i18n.t("onboarding.start_exploring"),
                    Some((LucideIcon::ArrowRight, false)),
                    ButtonVariant::Default,
                    !self.onboarding.disclaimer_accepted,
                    |this, _window, cx| this.complete_onboarding(cx),
                    cx,
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_button(
        &self,
        label: String,
        icon: Option<(LucideIcon, bool)>,
        variant: ButtonVariant,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let icon_color = rgb(if variant == ButtonVariant::Default {
            theme.accent_text
        } else {
            theme.text
        });
        let button = button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Default,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .when_some(icon, |button, (icon_kind, leading)| {
            button
                .when(leading, |button| button.flex_row_reverse())
                .child(Self::render_lucide_icon(icon_kind, 14.0, icon_color))
        })
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
