use gpui::{AnyElement, Div, ParentElement, Styled, div, prelude::*, px, rgb};
use oxideterm_theme::ThemeTokens;

use crate::{SurfaceKind, SurfaceOptions, SurfacePadding, semantic_surface};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityListRowDensity {
    Compact,
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityListRowOptions {
    pub active: bool,
    pub disabled: bool,
    pub density: EntityListRowDensity,
    pub has_background_image: bool,
}

impl EntityListRowOptions {
    pub const fn new() -> Self {
        Self {
            active: false,
            disabled: false,
            density: EntityListRowDensity::Normal,
            has_background_image: false,
        }
    }

    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub const fn compact(mut self) -> Self {
        self.density = EntityListRowDensity::Compact;
        self
    }

    pub const fn has_background_image(mut self, has_background_image: bool) -> Self {
        self.has_background_image = has_background_image;
        self
    }
}

impl Default for EntityListRowOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn entity_list_row(
    tokens: &ThemeTokens,
    options: EntityListRowOptions,
    leading: Option<AnyElement>,
    title: AnyElement,
    subtitle: Option<AnyElement>,
    badges: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
) -> Div {
    let min_height = match options.density {
        EntityListRowDensity::Compact => 32.0,
        EntityListRowDensity::Normal => 42.0,
    };
    let padding = match options.density {
        EntityListRowDensity::Compact => SurfacePadding::Compact,
        EntityListRowDensity::Normal => SurfacePadding::Normal,
    };
    // The row owns only the flexible slot. Callers own text style and
    // truncation so GPUI measures a single text container against the real
    // row width instead of a nested truncate wrapper.
    let mut content = div()
        .min_w_0()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .child(title);
    if let Some(subtitle) = subtitle {
        content = content.child(subtitle);
    }

    // Rows keep activation, metadata, and actions in distinct slots so pointer
    // handlers in app code can attach without fighting the row's text layout.
    let row = semantic_surface(
        tokens,
        SurfaceOptions::new(SurfaceKind::EntityRow)
            .padding(padding)
            .active(options.active)
            .has_background_image(options.has_background_image),
    )
    .w_full()
    .min_w_0()
    .min_h(px(min_height))
    .flex()
    .items_center()
    .gap(px(tokens.spacing.two))
    .opacity(if options.disabled { 0.55 } else { 1.0 })
    .when(!options.disabled, |row| {
        row.hover(move |style| style.bg(rgb(tokens.ui.bg_hover)))
    });

    let mut row = row.when_some(leading, |row, leading| row.child(leading));
    row = row.child(content);
    if !badges.is_empty() {
        row = row.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .children(badges),
        );
    }
    if !trailing.is_empty() {
        row = row.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .children(trailing),
        );
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_list_row_defaults_are_dense_but_not_active() {
        let options = EntityListRowOptions::default();

        assert!(!options.active);
        assert!(!options.disabled);
        assert_eq!(options.density, EntityListRowDensity::Normal);
    }

    #[test]
    fn entity_list_row_options_are_chainable() {
        let options = EntityListRowOptions::new()
            .active(true)
            .disabled(true)
            .compact()
            .has_background_image(true);

        assert!(options.active);
        assert!(options.disabled);
        assert_eq!(options.density, EntityListRowDensity::Compact);
        assert!(options.has_background_image);
    }
}
