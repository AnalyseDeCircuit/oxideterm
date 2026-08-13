use gpui::{Div, Styled, div, prelude::*, px};
use oxideterm_theme::ThemeTokens;

use crate::{SurfaceKind, SurfaceOptions, SurfacePadding, semantic_surface};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommandPanelOptions {
    pub width: Option<f32>,
    pub max_width_ratio: Option<f32>,
    pub max_height: Option<f32>,
    pub padding: SurfacePadding,
    pub terminal_owned: bool,
    pub has_background_image: bool,
}

impl CommandPanelOptions {
    pub const fn new() -> Self {
        Self {
            width: None,
            max_width_ratio: None,
            max_height: None,
            padding: SurfacePadding::Normal,
            terminal_owned: false,
            has_background_image: false,
        }
    }

    pub const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub const fn max_width_ratio(mut self, ratio: f32) -> Self {
        self.max_width_ratio = Some(ratio);
        self
    }

    pub const fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = Some(max_height);
        self
    }

    pub const fn padding(mut self, padding: SurfacePadding) -> Self {
        self.padding = padding;
        self
    }

    pub const fn terminal_owned(mut self) -> Self {
        self.terminal_owned = true;
        self
    }

    pub const fn has_background_image(mut self, has_background_image: bool) -> Self {
        self.has_background_image = has_background_image;
        self
    }
}

impl Default for CommandPanelOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn command_panel(tokens: &ThemeTokens, options: CommandPanelOptions) -> Div {
    let kind = if options.terminal_owned {
        SurfaceKind::TerminalOverlay
    } else {
        SurfaceKind::ElevatedPopover
    };
    // Command panels own search/results chrome, so padding stays at the shell
    // edge and feature code can place dividers without nested card frames.
    semantic_surface(
        tokens,
        SurfaceOptions::new(kind)
            .padding(options.padding)
            .has_background_image(options.has_background_image),
    )
    .overflow_hidden()
    .flex()
    .flex_col()
    .gap(px(tokens.spacing.three))
    .when_some(options.width, |panel, width| panel.w(px(width)))
    .when_some(options.max_width_ratio, |panel, ratio| {
        panel.max_w(gpui::relative(ratio))
    })
    .when_some(options.max_height, |panel, max_height| {
        panel.max_h(px(max_height))
    })
}

pub fn command_panel_body(tokens: &ThemeTokens) -> Div {
    // The body helper makes command-panel scroll ownership visually explicit:
    // callers can add overflow/ListState here without wrapping more cards.
    div()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
}
