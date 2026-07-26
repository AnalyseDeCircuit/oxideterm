use gpui::{Div, ParentElement, Styled, div, px, relative, rgb, rgba};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct SliderView {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub disabled: bool,
}

impl SliderView {
    pub fn percent(self) -> f32 {
        if (self.max - self.min).abs() <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
}

pub fn slider_pointer_percent(position: f32, width: f32, thumb_size: f32) -> f32 {
    // The thumb center travels between the two ends of the inset track.
    // Matching pointer math to that travel range keeps dragging under the thumb.
    let bounded_width = width.max(1.0);
    let bounded_thumb = thumb_size.clamp(0.0, bounded_width);
    let travel_width = (bounded_width - bounded_thumb).max(1.0);
    ((position - bounded_thumb / 2.0) / travel_width).clamp(0.0, 1.0)
}

pub fn slider(tokens: &ThemeTokens, view: SliderView) -> Div {
    let pct = view.percent();
    let thumb = tokens.metrics.ui_slider_thumb_size;
    div()
        .relative()
        .w_full()
        .h(px(thumb))
        .flex()
        .items_center()
        .px(px(thumb / 2.0))
        .opacity(if view.disabled { 0.5 } else { 1.0 })
        .child(
            div()
                .relative()
                .h(px(tokens.metrics.ui_slider_track_height))
                .w_full()
                .rounded_full()
                .bg(rgba((tokens.ui.border << 8) | 0x99))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .rounded_full()
                        .bg(rgb(tokens.ui.accent))
                        .w(relative(pct)),
                )
                .child(
                    div()
                        .absolute()
                        .size(px(thumb))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(tokens.ui.border))
                        .bg(rgb(tokens.ui.bg_elevated))
                        .left(relative(pct))
                        .top(relative(0.5))
                        .ml(px(-thumb / 2.0))
                        .mt(px(-thumb / 2.0)),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::slider_pointer_percent;

    #[test]
    fn pointer_percent_tracks_thumb_centers() {
        let width = 128.0;
        let thumb = 16.0;

        assert_eq!(slider_pointer_percent(8.0, width, thumb), 0.0);
        assert_eq!(slider_pointer_percent(64.0, width, thumb), 0.5);
        assert_eq!(slider_pointer_percent(120.0, width, thumb), 1.0);
    }

    #[test]
    fn pointer_percent_clamps_outside_slider() {
        assert_eq!(slider_pointer_percent(-20.0, 128.0, 16.0), 0.0);
        assert_eq!(slider_pointer_percent(200.0, 128.0, 16.0), 1.0);
    }
}
