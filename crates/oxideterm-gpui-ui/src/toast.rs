use gpui::{
    AnimationExt, AnyElement, Div, FontWeight, ParentElement, Styled, div, prelude::*, px, rgb,
    rgba, svg,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastVariant {
    Default,
    Success,
    Error,
    Warning,
}

pub struct ToastView {
    pub id: String,
    pub phase: crate::motion::ExitPhase,
    pub title: String,
    pub description: Option<String>,
    pub status_text: Option<String>,
    pub progress: Option<f32>,
    pub variant: ToastVariant,
    pub actions: Option<AnyElement>,
    pub close: Option<AnyElement>,
}

fn toast_content_width(tokens: &ThemeTokens) -> f32 {
    // Tauri toast root is 420px wide with p-4 and pr-8. Keep the text column
    // inside that same usable width so GPUI's line wrapper can wrap long text.
    (tokens.metrics.ui_toast_width - tokens.metrics.ui_toast_padding * 3.0).max(0.0)
}

fn toast_text(tokens: &ThemeTokens, text: String) -> Div {
    div()
        .w_full()
        .max_w(px(toast_content_width(tokens)))
        .min_w_0()
        .whitespace_normal()
        .text_size(px(tokens.metrics.ui_text_sm))
        .child(text)
}

pub fn toast(tokens: &ThemeTokens, view: ToastView) -> AnyElement {
    let animation_id = gpui::ElementId::Name(format!("toast-motion-{}", view.id).into());
    let (border, bg, text) = match view.variant {
        ToastVariant::Default => (tokens.ui.border, tokens.ui.bg_elevated, tokens.ui.text),
        ToastVariant::Success => (tokens.ui.success, tokens.ui.success, tokens.ui.success),
        ToastVariant::Error => (tokens.ui.error, tokens.ui.error, tokens.ui.error),
        ToastVariant::Warning => (tokens.ui.warning, tokens.ui.warning, tokens.ui.warning),
    };
    let toast = div()
        .relative()
        .flex()
        .w_full()
        .max_w(px(tokens.metrics.ui_toast_width))
        .items_center()
        .justify_between()
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if view.variant == ToastVariant::Default {
            rgb(border)
        } else {
            rgba((border << 8) | 0x80)
        })
        .bg(if view.variant == ToastVariant::Default {
            rgba((bg << 8) | 0xf2)
        } else {
            rgba((bg << 8) | 0x1a)
        })
        .p(px(tokens.metrics.ui_toast_padding))
        .pr(px(tokens.metrics.ui_toast_padding * 2.0))
        .text_color(rgb(text))
        .shadow_lg()
        .child(
            div()
                .grid()
                .w_full()
                .max_w(px(toast_content_width(tokens)))
                .min_w_0()
                .flex_1()
                .gap(px(tokens.spacing.one))
                .child(toast_text(tokens, view.title).font_weight(FontWeight::SEMIBOLD))
                .when_some(view.description, |content, description| {
                    content.child(toast_text(tokens, description).opacity(0.9))
                })
                .when_some(view.status_text, |content, status_text| {
                    content.child(toast_text(tokens, status_text).opacity(0.9))
                })
                .when_some(view.progress, |content, progress| {
                    content.child(
                        div()
                            .mt(px(tokens.spacing.two))
                            .h(px(tokens.spacing.one))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .bg(rgba((tokens.ui.border << 8) | 0x80))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(rgb(tokens.ui.accent))
                                    .with_spring(
                                        "toast-progress",
                                        crate::motion::spatial_spring(
                                            tokens,
                                            crate::motion::MotionDuration::Control,
                                            (progress / 100.0).clamp(0.0, 1.0),
                                        ),
                                        |bar, value| bar.w(gpui::relative(value.clamp(0.0, 1.0))),
                                    ),
                            ),
                    )
                })
                .when_some(view.actions, |content, actions| {
                    content.child(
                        div()
                            .mt(px(tokens.spacing.two))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(tokens.spacing.two))
                            .child(actions),
                    )
                }),
        )
        .when_some(view.close, |toast, close| toast.child(close));
    crate::motion::slide_fade_x(
        tokens,
        animation_id,
        toast,
        8.0,
        crate::motion::MotionDuration::Control,
        view.phase == crate::motion::ExitPhase::Visible,
    )
}

pub fn toast_action(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .h(px(tokens.metrics.ui_button_sm_height))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba(0x00000000))
        .px(px(tokens.metrics.ui_button_sm_padding_x))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(label.into())
}

pub fn toast_close(tokens: &ThemeTokens) -> Div {
    let button_size = tokens.metrics.ui_toast_close_size + tokens.spacing.two;

    div()
        .absolute()
        .right(px(tokens.spacing.two))
        .top(px(tokens.spacing.two))
        .flex()
        .size(px(button_size))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(tokens.radii.xs))
        .text_color(rgb(tokens.ui.text_muted))
        .opacity(0.75)
        .hover(|button| button.opacity(1.0).text_color(rgb(tokens.ui.text)))
        // Tauri's Radix ToastClose is an absolute top-right affordance with a
        // 16px X icon and 4px padding. GPUI uses the same visual footprint here.
        .child(
            svg()
                .path("lucide/x.svg")
                .size(px(tokens.metrics.ui_toast_close_size))
                // GPUI SVG elements do not inherit the parent text color.
                .text_color(rgb(tokens.ui.text_muted)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, Window};
    use oxideterm_theme::UiMotionProfile;
    use std::time::Duration;

    struct ProgressToast {
        tokens: ThemeTokens,
        progress: f32,
    }

    impl Render for ProgressToast {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            toast(
                &self.tokens,
                ToastView {
                    id: "transfer".into(),
                    phase: crate::motion::ExitPhase::Visible,
                    title: String::new(),
                    description: None,
                    status_text: None,
                    progress: Some(self.progress),
                    variant: ToastVariant::Default,
                    actions: None,
                    close: None,
                },
            )
        }
    }

    #[gpui::test]
    fn progress_respects_reduced_and_disabled_profiles(cx: &mut TestAppContext) {
        for profile in [
            UiMotionProfile::Off,
            UiMotionProfile::Reduced,
            UiMotionProfile::Normal,
            UiMotionProfile::Fast,
        ] {
            let (view, cx) = cx.add_window_view(|_, _| {
                let mut tokens = oxideterm_theme::default_tokens();
                tokens.apply_motion(profile);
                ProgressToast {
                    tokens,
                    progress: 10.0,
                }
            });
            cx.simulate_resize(gpui::size(px(420.0), px(200.0)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.executor().advance_clock(Duration::from_secs(1));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let color: gpui::Hsla = view.read_with(cx, |view, _| rgb(view.tokens.ui.accent).into());
            let width = |cx: &mut gpui::VisualTestContext| {
                cx.update(|window, _| {
                    window
                        .painted_quads()
                        .iter()
                        .find(|quad| quad.background.as_solid() == Some(color))
                        .expect("painted progress indicator")
                        .bounds
                        .size
                        .width
                        .0
                })
            };
            let before = width(cx);
            view.update(cx, |view, cx| {
                view.progress = 90.0;
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let immediate = width(cx);
            if matches!(profile, UiMotionProfile::Off | UiMotionProfile::Reduced) {
                assert!((immediate - before * 9.0).abs() < 5.0, "{profile:?}");
            } else {
                assert_eq!(immediate, before, "{profile:?}");
                cx.executor().advance_clock(Duration::from_secs(1));
                cx.update(|window, cx| window.draw(cx).clear(cx));
                assert!((width(cx) - before * 9.0).abs() < 5.0, "{profile:?}");
            }
        }
    }
}
