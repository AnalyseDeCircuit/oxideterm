// Hallmark · pre-emit critique: P5 H4 E5 S5 R5 V4
use gpui::{
    Bounds, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Path, PathBuilder,
    Pixels, Styled, canvas, div, point, prelude::*, px, relative, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use super::tokens::*;

pub fn ai_status_indicator(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
    active: bool,
) -> Div {
    // Tool availability is fixed semantic data. Let the model selector absorb
    // row compression instead of clipping the tool count behind a width cap.
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if active {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .opacity(if active { 1.0 } else { 0.7 })
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(bg_alpha(tokens, tokens.ui.accent, AI_CHIP_BG_ALPHA))
                .text_color(rgb(tokens.ui.text))
        })
        .child(div().flex_none().child(icon))
        .child(div().whitespace_nowrap().child(label.into()))
}

pub fn ai_safety_indicator(
    tokens: &ThemeTokens,
    mode: AiSafetyMode,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    let bypass = mode == AiSafetyMode::Bypass;
    div()
        .flex()
        .flex_none()
        .max_w(px(AI_SAFETY_INDICATOR_MAX_WIDTH))
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if bypass {
            rgb(tokens.ui.warning)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .when(bypass, |indicator| {
            indicator
                .border_1()
                .border_color(tone_border(tokens, AiTone::Amber, AI_CHIP_BORDER_ALPHA))
                .bg(tone_bg(tokens, AiTone::Amber, AI_CHIP_BG_ALPHA))
        })
        .when(!bypass, |indicator| {
            indicator.hover(|style| {
                style
                    .bg(bg_alpha(tokens, tokens.ui.accent, AI_CHIP_BG_ALPHA))
                    .text_color(rgb(tokens.ui.text))
            })
        })
        .child(div().flex_none().child(icon))
        .child(div().min_w_0().truncate().child(label.into()))
}

pub fn ai_context_usage_indicator(
    tokens: &ThemeTokens,
    usage: AiContextUsage,
    label: impl Into<String>,
    interactive: bool,
) -> Div {
    let tone = if usage.danger {
        AiTone::Red
    } else if usage.warning {
        AiTone::Amber
    } else {
        AiTone::Accent
    };
    let progress = ai_context_usage_fraction(usage.percentage);
    // Keep the track quiet and reserve semantic color for the used portion.
    // This reads as capacity at compact scale instead of a loading spinner.
    let track_color = bg_alpha(tokens, tokens.ui.text_muted, AI_CONTEXT_RING_TRACK_ALPHA);
    let progress_color = rgb(tone_color(tokens, tone));
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(tokens.spacing.two))
        .when(interactive, |indicator| {
            indicator
                .cursor_pointer()
                .hover(|style| style.text_color(rgb(tone_color(tokens, tone))).opacity(1.0))
        })
        .text_color(if usage.danger || usage.warning {
            rgb(tone_color(tokens, tone))
        } else {
            rgb(tokens.ui.text_muted)
        })
        .child(
            div().flex_none().size(px(AI_CONTEXT_RING_SIZE)).child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        if let Some(track) = ai_context_ring_path(bounds, 1.0) {
                            window.paint_path(track, track_color);
                        }
                        if let Some(progress_path) = ai_context_ring_path(bounds, progress) {
                            window.paint_path(progress_path, progress_color);
                        }
                    },
                )
                .size_full(),
            ),
        )
        .child(
            div()
                .text_size(px(AI_TEXT_10))
                .font_weight(FontWeight::MEDIUM)
                .opacity(if usage.danger || usage.warning {
                    0.9
                } else {
                    0.7
                })
                .child(label.into()),
        )
}

fn ai_context_usage_fraction(percentage: f32) -> f32 {
    if percentage.is_finite() {
        (percentage / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn ai_context_ring_path(bounds: Bounds<Pixels>, fraction: f32) -> Option<Path<Pixels>> {
    if fraction <= 0.0 {
        return None;
    }
    let diameter = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
    let radius = (diameter - AI_CONTEXT_RING_STROKE_WIDTH).max(0.0) / 2.0;
    if radius <= 0.0 {
        return None;
    }

    let center_x = bounds.origin.x + px(f32::from(bounds.size.width) / 2.0);
    let center_y = bounds.origin.y + px(f32::from(bounds.size.height) / 2.0);
    let radius_point = point(px(radius), px(radius));
    let start = point(center_x, center_y - px(radius));
    let mut builder = PathBuilder::stroke(px(AI_CONTEXT_RING_STROKE_WIDTH));
    builder.move_to(start);

    if fraction >= 1.0 {
        // SVG arcs need two halves because a single arc cannot end where it starts.
        let bottom = point(center_x, center_y + px(radius));
        builder.arc_to(radius_point, px(0.0), false, true, bottom);
        builder.arc_to(radius_point, px(0.0), false, true, start);
    } else {
        let end_angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction;
        let end = point(
            center_x + px(end_angle.cos() * radius),
            center_y + px(end_angle.sin() * radius),
        );
        builder.arc_to(radius_point, px(0.0), fraction > 0.5, true, end);
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::ai_context_usage_fraction;

    #[test]
    fn context_usage_fraction_clamps_invalid_and_over_limit_values() {
        assert_eq!(ai_context_usage_fraction(f32::NAN), 0.0);
        assert_eq!(ai_context_usage_fraction(-10.0), 0.0);
        assert_eq!(ai_context_usage_fraction(25.0), 0.25);
        assert_eq!(ai_context_usage_fraction(140.0), 1.0);
    }
}

pub fn ai_context_popover(tokens: &ThemeTokens) -> Div {
    let popover = div()
        .w(px(AI_CONTEXT_POPOVER_WIDTH))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, AI_HEADER_BORDER_ALPHA))
        .bg(rgb(tokens.ui.bg_panel))
        // Keep wheel input local to the popover, matching browser popover
        // scroll chaining rules even when the compact panel has no scrollbar.
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
    crate::surface::theme_overlay_surface_shadow(popover, tokens)
}

pub fn ai_context_popover_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    usage: AiContextUsage,
    value_label: impl Into<String>,
) -> Div {
    let tone = if usage.danger {
        AiTone::Red
    } else if usage.warning {
        AiTone::Amber
    } else {
        AiTone::Accent
    };
    div()
        .px(px(tokens.spacing.three))
        .pt(px(tokens.spacing.three))
        .pb(px(tokens.spacing.two))
        .child(
            div()
                .mb(px(tokens.spacing.one / 2.0))
                .text_size(px(AI_TEXT_11))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text))
                .child(title.into()),
        )
        .child(
            div()
                .mb(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .flex()
                .items_baseline()
                .justify_between()
                .child(
                    div()
                        .text_size(px(AI_TEXT_12))
                        .text_color(rgb(tokens.ui.text))
                        .child(value_label.into()),
                )
                .child(
                    div()
                        .text_size(px(AI_TEXT_11))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(tone_color(tokens, tone)))
                        .child(format!("{}%", usage.percentage.round() as i64)),
                ),
        )
        .child(
            div()
                .h(px(AI_CONTEXT_MINI_BAR_HEIGHT))
                .w_full()
                .overflow_hidden()
                .rounded_full()
                .bg(bg_alpha(tokens, tokens.ui.border, AI_CONTEXT_BAR_BG_ALPHA))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(rgb(tone_color(tokens, tone)))
                        .w(relative((usage.percentage / 100.0).clamp(0.0, 1.0))),
                ),
        )
}

pub fn ai_model_selector_trigger(
    tokens: &ThemeTokens,
    provider_label: impl Into<String>,
    model_label: impl Into<String>,
    icon: impl IntoElement,
    chevron: impl IntoElement,
    ready: bool,
) -> Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(tokens.spacing.two))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(bg_alpha(tokens, tokens.ui.bg_card, 0x99))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .cursor_pointer()
        .child(icon)
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(tokens.spacing.one / 2.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(provider_label.into()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_9))
                        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
                        .child(model_label.into()),
                ),
        )
        .child(div().size(px(6.0)).rounded_full().bg(if ready {
            rgb(tokens.ui.success)
        } else {
            rgb(tokens.ui.text_muted)
        }))
        .child(chevron)
}

pub fn ai_model_selector_panel(tokens: &ThemeTokens, up: bool) -> Div {
    let panel = div()
        .absolute()
        .left_0()
        .right_0()
        .when(up, |panel| panel.bottom_full().mb(px(tokens.spacing.one)))
        .when(!up, |panel| panel.top_full().mt(px(tokens.spacing.one)))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(rgb(tokens.ui.bg_panel));
    crate::surface::theme_overlay_surface_shadow(panel, tokens)
}

pub fn ai_model_selector_row(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    detail: impl Into<String>,
    selected: bool,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .bg(if selected {
            bg_alpha(tokens, tokens.ui.accent, 0x26)
        } else {
            rgba(0x00000000)
        })
        .text_color(if selected {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.text)
        })
        .cursor_pointer()
        .child(icon)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_12))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.into()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
                        .child(detail.into()),
                ),
        )
}
