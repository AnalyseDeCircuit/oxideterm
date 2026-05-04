use gpui::{Div, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SeparatorOrientation {
    Horizontal,
    Vertical,
}

pub(crate) fn separator(tokens: &ThemeTokens, orientation: SeparatorOrientation) -> Div {
    let base = div().flex_none().bg(rgb(tokens.ui.border));
    match orientation {
        SeparatorOrientation::Horizontal => base.h(px(1.0)).w_full(),
        SeparatorOrientation::Vertical => base.h_full().w(px(1.0)),
    }
}
