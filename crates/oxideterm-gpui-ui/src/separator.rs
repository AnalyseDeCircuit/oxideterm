use gpui::{Div, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

pub const SIDEBAR_RESIZE_DIVIDER_WIDTH: f32 = 1.0;
pub const SIDEBAR_RESIZE_HOTZONE_WIDTH: f32 = 9.0;

pub fn sidebar_resize_hotzone_chrome(
    element_id: &'static str,
    line_color: gpui::Rgba,
    divider_at_right: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(element_id)
        .group(element_id)
        .absolute()
        .w(px(SIDEBAR_RESIZE_HOTZONE_WIDTH))
        .cursor_col_resize()
        // Keep the full drag target inside the sidebar so adjacent content
        // remains selectable even when it has no padding.
        .occlude()
        .bg(rgba(0x00000000))
        .child(
            div()
                .absolute()
                .when(divider_at_right, |divider| divider.right_0())
                .when(!divider_at_right, |divider| divider.left_0())
                .top_0()
                .bottom_0()
                .w(px(SIDEBAR_RESIZE_DIVIDER_WIDTH))
                // Keep the resize cursor when the pointer lands on the painted line itself.
                .cursor_col_resize()
                .bg(line_color)
                .group_hover(element_id, move |style| style.bg(line_color.alpha(1.0))),
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeparatorOrientation {
    Horizontal,
    Vertical,
}

pub fn separator(tokens: &ThemeTokens, orientation: SeparatorOrientation) -> Div {
    let base = div().flex_none().bg(rgb(tokens.ui.border));
    match orientation {
        SeparatorOrientation::Horizontal => base.h(px(1.0)).w_full(),
        SeparatorOrientation::Vertical => base.h_full().w(px(1.0)),
    }
}
