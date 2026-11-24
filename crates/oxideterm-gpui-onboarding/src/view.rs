// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use gpui::{
    AnyElement, App, Context, CursorStyle, FontWeight, MouseButton, Render, ScrollHandle,
    Subscription, Task, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::motion::{self, MotionDuration, UserSegmentedControlMotionState};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_theme::ThemeTokens;

use crate::{
    ONBOARDING_TOTAL_STEPS, OnboardingFrame, OnboardingRequest, OnboardingStep,
    OnboardingViewBindings,
    steps::{
        render_appearance_step, render_disclaimer_step, render_launch_step, render_overview_step,
    },
    widgets::{
        ONBOARDING_ACCENT_BORDER_ALPHA, ONBOARDING_DISABLED_OPACITY,
        ONBOARDING_HEADER_BORDER_ALPHA, action_button, render_icon,
    },
};

const ONBOARDING_STEPS_MOTION_ID: &str = "onboarding-steps";
const ONBOARDING_CONTENT_MAX_WIDTH: f32 = 880.0;

pub struct OnboardingView {
    bindings: OnboardingViewBindings,
    scroll_handle: ScrollHandle,
    last_step: usize,
    step_motion: UserSegmentedControlMotionState,
    control_motion: UserSegmentedControlMotionState,
    control_motion_tasks: HashMap<&'static str, Task<()>>,
    subscriptions: Vec<Subscription>,
}

impl OnboardingView {
    pub fn new(bindings: OnboardingViewBindings) -> Self {
        Self {
            bindings,
            scroll_handle: ScrollHandle::new(),
            last_step: 0,
            step_motion: UserSegmentedControlMotionState::default(),
            control_motion: UserSegmentedControlMotionState::default(),
            control_motion_tasks: HashMap::new(),
            subscriptions: Vec::new(),
        }
    }

    pub(crate) fn begin_control_motion(
        &mut self,
        control_id: &'static str,
        tokens: &ThemeTokens,
        previous_index: usize,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        if previous_index == target_index {
            return;
        }
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(tokens) else {
            self.control_motion.clear(control_id);
            self.control_motion_tasks.remove(control_id);
            return;
        };
        let generation = self.control_motion.begin_with_previous_index(
            control_id,
            Some(previous_index),
            target_index,
            None,
        );
        let task = cx.spawn(async move |view, cx| {
            cx.background_executor().timer(motion.duration).await;
            let _ = view.update(cx, |this, cx| {
                if this.control_motion.finish(control_id, generation) {
                    cx.notify();
                }
            });
        });
        self.control_motion_tasks.insert(control_id, task);
    }

    pub fn retain_subscription(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_handle = ScrollHandle::new();
    }

    fn sync_step_transition(&mut self, frame: &OnboardingFrame, cx: &mut Context<Self>) {
        if frame.step == self.last_step {
            return;
        }
        let previous = self.last_step;
        let next = frame.step;
        self.last_step = next;
        self.scroll_handle = ScrollHandle::new();
        self.control_motion = UserSegmentedControlMotionState::default();
        self.control_motion_tasks.clear();

        let duration = motion::duration(&frame.tokens, MotionDuration::Control);
        if duration.is_zero() {
            self.step_motion.clear(ONBOARDING_STEPS_MOTION_ID);
            return;
        }
        let generation = self.step_motion.begin_with_previous_index(
            ONBOARDING_STEPS_MOTION_ID,
            Some(previous),
            next,
            None,
        );
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(duration).await;
            let _ = view.update(cx, |this, cx| {
                if this
                    .step_motion
                    .finish(ONBOARDING_STEPS_MOTION_ID, generation)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn render_header(&self, frame: &OnboardingFrame, wide: bool, cx: &mut App) -> AnyElement {
        let theme = frame.tokens.ui;
        let mut rail = div()
            .flex()
            .items_center()
            .gap(px(if wide { 8.0 } else { 4.0 }));

        for index in 0..ONBOARDING_TOTAL_STEPS {
            let step = OnboardingStep::from_index(index);
            let selected = index == frame.step;
            let completed = index < frame.step
                && (index != OnboardingStep::DISCLAIMER_INDEX || frame.disclaimer_accepted);
            let locked = !frame.disclaimer_accepted && index > OnboardingStep::DISCLAIMER_INDEX;
            let dispatch = self.bindings.dispatch.clone();

            rail = rail
                .child(
                    div()
                        .id(("onboarding-step-pill", index))
                        .rounded(px(frame.tokens.radii.md))
                        .when(!locked && !selected, |pill| {
                            pill.hover(move |style| style.bg(rgb(theme.bg_hover)))
                        })
                        .px(px(if wide { 8.0 } else { 6.0 }))
                        .py(px(4.0))
                        .flex()
                        .items_center()
                        .gap(px(7.0))
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
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            if !locked {
                                dispatch(OnboardingRequest::GoToStep(index), window, cx);
                            }
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .size(px(18.0))
                                .rounded_full()
                                .bg(rgb(if selected || completed {
                                    theme.accent
                                } else {
                                    theme.bg_sunken
                                }))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if selected || completed {
                                    theme.accent_text
                                } else {
                                    theme.text_muted
                                }))
                                .child(if completed {
                                    render_icon("lucide/check.svg", 10.5, rgb(theme.accent_text))
                                } else if locked {
                                    render_icon("lucide/lock.svg", 9.5, rgb(theme.text_muted))
                                } else {
                                    div().child(format!("{}", index + 1)).into_any_element()
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                .font_weight(if selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .text_color(rgb(if selected {
                                    theme.text_heading
                                } else {
                                    theme.text_muted
                                }))
                                .child(frame.i18n.t(step.title_key())),
                        ),
                )
                .when(index + 1 < ONBOARDING_TOTAL_STEPS, |row| {
                    row.child(div().w(px(if wide { 20.0 } else { 10.0 })).h(px(1.0)).bg(
                        if completed {
                            rgba((theme.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                        } else {
                            rgba((theme.border << 8) | ONBOARDING_HEADER_BORDER_ALPHA)
                        },
                    ))
                });
        }

        let trailing = if frame.step == 0 {
            div()
                .w(px(156.0))
                .flex_none()
                .child((self.bindings.language_select)(
                    &frame.tokens,
                    &frame.language_label,
                    cx,
                ))
                .into_any_element()
        } else if frame.step + 1 < ONBOARDING_TOTAL_STEPS && frame.disclaimer_accepted {
            let dispatch_skip = self.bindings.dispatch.clone();
            action_button(
                &frame.tokens,
                "onboarding-skip-button",
                frame.i18n.t("onboarding.skip"),
                None,
                ButtonVariant::Ghost,
                false,
                move |window, cx| {
                    dispatch_skip(OnboardingRequest::SkipToLaunch, window, cx);
                },
            )
        } else {
            div().into_any_element()
        };

        div()
            .w_full()
            .pb(px(14.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | ONBOARDING_HEADER_BORDER_ALPHA))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(rail)
            .child(trailing)
            .into_any_element()
    }

    fn render_footer(&self, frame: &OnboardingFrame) -> AnyElement {
        let theme = frame.tokens.ui;
        let can_go_back = frame.step > 0;
        let can_go_next = frame.step + 1 < ONBOARDING_TOTAL_STEPS;
        let blocked_without_disclaimer =
            !frame.disclaimer_accepted && frame.step >= OnboardingStep::DISCLAIMER_INDEX;
        let dispatch_back = self.bindings.dispatch.clone();
        let dispatch_next = self.bindings.dispatch.clone();
        let dispatch_complete = self.bindings.dispatch.clone();

        div()
            .w_full()
            .pt(px(14.0))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | ONBOARDING_HEADER_BORDER_ALPHA))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(if can_go_back {
                action_button(
                    &frame.tokens,
                    "onboarding-back-button",
                    frame.i18n.t("onboarding.back"),
                    Some(("lucide/chevron-left.svg", true)),
                    ButtonVariant::Ghost,
                    false,
                    move |window, cx| {
                        dispatch_back(OnboardingRequest::Back, window, cx);
                    },
                )
            } else {
                div().into_any_element()
            })
            .child(if can_go_next {
                action_button(
                    &frame.tokens,
                    "onboarding-next-button",
                    frame.i18n.t("onboarding.next"),
                    Some(("lucide/chevron-right.svg", false)),
                    ButtonVariant::Default,
                    blocked_without_disclaimer,
                    move |window, cx| {
                        dispatch_next(OnboardingRequest::Next, window, cx);
                    },
                )
            } else {
                action_button(
                    &frame.tokens,
                    "onboarding-complete-button",
                    frame.i18n.t("onboarding.start_exploring"),
                    Some(("lucide/arrow-right.svg", false)),
                    ButtonVariant::Default,
                    blocked_without_disclaimer,
                    move |window, cx| {
                        dispatch_complete(OnboardingRequest::Complete, window, cx);
                    },
                )
            })
            .into_any_element()
    }
}

impl Render for OnboardingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        self.sync_step_transition(&frame, cx);

        let theme = frame.tokens.ui;
        let viewport_width = f32::from(window.viewport_size().width);
        let content_width = (viewport_width - frame.sidebar_occupied_width).max(360.0);
        let wide = content_width >= 820.0;
        let step = OnboardingStep::from_index(frame.step);

        let body = match step {
            OnboardingStep::Overview => render_overview_step(&frame, wide),
            OnboardingStep::Disclaimer => render_disclaimer_step(&frame, &self.bindings),
            OnboardingStep::Appearance => render_appearance_step(
                &frame,
                &self.bindings,
                &self.control_motion,
                cx.entity().downgrade(),
                wide,
                cx,
            ),
            OnboardingStep::Launch => render_launch_step(&frame, &self.bindings, wide),
        };

        let animated_body = if let Some((generation, _)) = self
            .step_motion
            .transition_for(ONBOARDING_STEPS_MOTION_ID, frame.step)
        {
            let previous = self
                .step_motion
                .previous_index_for(ONBOARDING_STEPS_MOTION_ID, frame.step)
                .unwrap_or(frame.step);
            motion::slide_fade_in_x(
                &frame.tokens,
                (ONBOARDING_STEPS_MOTION_ID, generation as usize),
                div().w_full().child(body),
                if frame.step > previous { 14.0 } else { -14.0 },
                MotionDuration::Control,
            )
        } else {
            body
        };

        let dispatch_scroll = self.bindings.dispatch.clone();
        let select_open = frame.open_settings_select_active;
        let view_handle = cx.entity().downgrade();

        div()
            .id("onboarding-workspace-tab")
            .size_full()
            .when(!frame.has_background, |view| view.bg(rgb(theme.bg)))
            .text_color(rgb(theme.text_muted))
            .font_family(frame.ui_font_family.clone())
            .child(
                div()
                    .id(("onboarding-body", frame.step))
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .vertical_scrollbar(&self.scroll_handle)
                    .on_scroll(move |_, window, cx| {
                        if select_open {
                            dispatch_scroll(OnboardingRequest::CloseOpenSelect, window, cx);
                        }
                        let _ = view_handle.update(cx, |_, cx| cx.notify());
                    })
                    .child(
                        div()
                            .w_full()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(px(if wide { 36.0 } else { 20.0 }))
                            .pt(px(20.0))
                            .pb(px(32.0))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(ONBOARDING_CONTENT_MAX_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .gap(px(20.0))
                                    .child(self.render_header(&frame, wide, cx))
                                    .child(animated_body)
                                    .when(frame.save_failed, |panel| {
                                        panel.child(
                                            div()
                                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(theme.error))
                                                .child(frame.i18n.t("onboarding.save_failed")),
                                        )
                                    })
                                    .child(self.render_footer(&frame)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use gpui::{ImageSource, SharedString, div, prelude::*, px};
    use oxideterm_i18n::{I18n, Locale};
    use oxideterm_settings::{AnimationSpeed, FontFamily, PersistedSettings};

    use super::*;
    use crate::{
        OnboardingCliSnapshot, OnboardingImportStatus, OnboardingShortcutItem,
        OnboardingThemeTarget,
    };

    #[test]
    fn onboarding_step_mapping_and_titles_cover_all_four_stages() {
        assert_eq!(OnboardingStep::from_index(0), OnboardingStep::Overview);
        assert_eq!(OnboardingStep::from_index(1), OnboardingStep::Disclaimer);
        assert_eq!(OnboardingStep::from_index(2), OnboardingStep::Appearance);
        assert_eq!(OnboardingStep::from_index(3), OnboardingStep::Launch);
        assert_eq!(OnboardingStep::from_index(99), OnboardingStep::Launch);
        assert_eq!(
            OnboardingStep::Overview.title_key(),
            "onboarding.step_overview"
        );
        assert_eq!(
            OnboardingStep::Disclaimer.title_key(),
            "onboarding.step_disclaimer"
        );
        assert_eq!(
            OnboardingStep::Appearance.title_key(),
            "onboarding.step_appearance"
        );
        assert_eq!(OnboardingStep::Launch.title_key(), "onboarding.step_start");
    }

    #[gpui::test]
    fn onboarding_view_renders_all_four_steps_and_dispatches_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        let requests: Rc<RefCell<Vec<OnboardingRequest>>> = Rc::new(RefCell::new(Vec::new()));
        let step = Rc::new(RefCell::new(0usize));
        let accepted = Rc::new(RefCell::new(false));
        let custom_font = Rc::new(RefCell::new(false));

        let bindings = OnboardingViewBindings {
            frame: Rc::new({
                let step = step.clone();
                let accepted = accepted.clone();
                let custom_font = custom_font.clone();
                move |_cx| {
                    let tokens = oxideterm_theme::default_tokens();
                    let mut settings = PersistedSettings::default();
                    if *custom_font.borrow() {
                        settings.terminal.font_family = FontFamily::Custom;
                    }
                    Some(OnboardingFrame {
                        tokens,
                        preview_tokens: tokens,
                        preview_theme_name: "Default".to_string(),
                        i18n: I18n::new(Locale::ZhCn),
                        settings,
                        sidebar_occupied_width: 0.0,
                        has_background: false,
                        system_dark: false,
                        edit_scheme: Some(false),
                        app_icon_source: ImageSource::from("app-icon.png"),
                        ui_font_family: SharedString::from("sans-serif"),
                        mono_font_family: SharedString::from("monospace"),
                        language_label: "简体中文".to_string(),
                        animation_options: vec![
                            (AnimationSpeed::Normal, "Normal".to_string()),
                            (AnimationSpeed::Off, "Off".to_string()),
                        ],
                        open_settings_select_active: false,
                        step: *step.borrow(),
                        disclaimer_accepted: *accepted.borrow(),
                        save_failed: false,
                        ai_opt_in: true,
                        tool_use_opt_in: false,
                        import_status: OnboardingImportStatus::Idle,
                        imported_count: 0,
                        host_count: Some(3),
                        cli: OnboardingCliSnapshot {
                            has_status: true,
                            installed: true,
                            install_path: Some("/usr/local/bin/oxideterm".to_string()),
                            ..Default::default()
                        },
                        shortcuts: vec![OnboardingShortcutItem {
                            label_key: "onboarding.shortcut_command_palette",
                            key_label: "⌘K".to_string(),
                        }],
                    })
                }
            }),
            dispatch: Rc::new({
                let requests = requests.clone();
                move |request, _window, _cx| {
                    requests.borrow_mut().push(request);
                }
            }),
            language_select: Rc::new(|_, label, _| {
                div()
                    .w(px(140.0))
                    .child(label.to_owned())
                    .into_any_element()
            }),
            theme_select: Rc::new(|target: OnboardingThemeTarget, _, _| {
                div()
                    .w(px(180.0))
                    .child(format!("{target:?}"))
                    .into_any_element()
            }),
            custom_font_input: Rc::new(|_, _| div().h(px(30.0)).into_any_element()),
            radius_slider: Rc::new(|radius, _| {
                div().child(format!("{radius}px")).into_any_element()
            }),
        };

        let (view, cx) = cx.add_window_view(|_, _| OnboardingView::new(bindings));
        cx.simulate_resize(gpui::size(px(1100.0), px(820.0)));
        cx.update(|window, app| {
            window.refresh();
            window.draw(app).clear(app);
        });
        assert!(cx.debug_bounds("onboarding-overview-page").is_some());

        // Step 0 Next is enabled even before accepting the disclaimer.
        let next_bounds = cx.debug_bounds("onboarding-next-button").unwrap();
        cx.simulate_mouse_down(
            next_bounds.center(),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        assert_eq!(requests.borrow().as_slice(), &[OnboardingRequest::Next]);
        requests.borrow_mut().clear();

        *step.borrow_mut() = OnboardingStep::DISCLAIMER_INDEX;
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, app| {
            window.refresh();
            window.draw(app).clear(app);
        });
        assert!(
            cx.debug_bounds("onboarding-disclaimer-accept-row")
                .is_some()
        );

        *accepted.borrow_mut() = true;
        *step.borrow_mut() = 2;
        *custom_font.borrow_mut() = true;
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, app| {
            window.refresh();
            window.draw(app).clear(app);
        });
        assert!(cx.debug_bounds("onboarding-corner-0").is_some());

        *step.borrow_mut() = 3;
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, app| {
            window.refresh();
            window.draw(app).clear(app);
        });
        assert!(cx.debug_bounds("onboarding-launch-new-conn").is_some());
        assert!(cx.debug_bounds("onboarding-import-ssh").is_some());
        assert!(cx.debug_bounds("onboarding-ai-enable").is_some());

        let bounds = cx.debug_bounds("onboarding-launch-new-conn").unwrap();
        cx.simulate_mouse_down(
            bounds.center(),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        assert_eq!(
            requests.borrow().as_slice(),
            &[OnboardingRequest::OpenNewConnection]
        );
    }
}
