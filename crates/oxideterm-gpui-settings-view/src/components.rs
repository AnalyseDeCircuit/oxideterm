use gpui::{AnyElement, App, Div, Rgba, SharedString, div, prelude::*, px, rgb, rgba};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_theme::ThemeTokens;

mod terminal_display;
pub use terminal_display::TerminalPreviewFont;
mod controls;

pub use controls::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsCalloutTone {
    Info,
    Warning,
    Error,
}

/// Shared page styling; does not own input focus or application state.
pub struct SettingsComponents<'a> {
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub settings: &'a PersistedSettings,
    pub has_background: bool,
}

impl SettingsComponents<'_> {
    fn settings_card_frame(&self, children: Vec<AnyElement>) -> AnyElement {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg.max(10.0)))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .children(children);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub fn settings_card_header_text(
        &self,
        title: String,
        description: Option<String>,
        icon: Option<AnyElement>,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        let description = description.filter(|text| !text.is_empty());
        let has_description = description.is_some();
        let heading = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .when_some(icon, |row, icon| row.child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(title),
            );

        let left = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(heading)
            .when_some(description, |col, description| {
                col.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(description),
                )
            });

        let header = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_between()
            .gap(px(12.0));
        let header = if has_description {
            header.items_start()
        } else {
            header.items_center()
        };

        header
            .child(left)
            .when_some(actions, |row, actions| row.child(actions))
            .into_any_element()
    }

    pub fn settings_card(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.settings_card_with_header(title_key, description_key, None, None, rows)
    }

    pub fn settings_card_with_header(
        &self,
        title_key: &str,
        description_key: &str,
        icon: Option<AnyElement>,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        let description = (!description_key.is_empty()).then(|| self.i18n.t(description_key));
        let has_header =
            !title_key.is_empty() || description.is_some() || icon.is_some() || actions.is_some();
        if !has_header {
            return self.settings_card_frame(rows);
        }
        let header =
            self.settings_card_header_text(self.i18n.t(title_key), description, icon, actions);
        if rows.is_empty() {
            return div()
                .w_full()
                .min_w(px(0.0))
                .px(px(2.0))
                .child(header)
                .into_any_element();
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(div().w_full().min_w(px(0.0)).px(px(2.0)).child(header))
            .child(self.settings_card_frame(rows))
            .into_any_element()
    }

    pub fn plain_settings_card(&self, rows: Vec<AnyElement>) -> AnyElement {
        self.settings_card_frame(rows)
    }

    pub fn separated_settings_rows(&self, rows: Vec<AnyElement>) -> Vec<AnyElement> {
        // Group separated rows in a zero-gap column so hairlines sit cleanly
        // between items without inheriting the outer card stack gap twice.
        let total = rows.len();
        if total == 0 {
            return Vec::new();
        }
        let mut group = div().w_full().min_w(px(0.0)).flex().flex_col();
        for (index, row) in rows.into_iter().enumerate() {
            if index > 0 {
                group = group.child(self.card_separator());
            }
            let mut slot = div().w_full().min_w(px(0.0));
            if index > 0 {
                slot = slot.pt(px(10.0));
            }
            if index + 1 < total {
                slot = slot.pb(px(10.0));
            }
            group = group.child(slot.child(row));
        }
        vec![group.into_any_element()]
    }

    pub fn card_subsection_header(
        &self,
        title: impl Into<SharedString>,
        hint: Option<String>,
    ) -> AnyElement {
        let hint = hint.filter(|text| !text.is_empty());
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_secondary))
                    .child(title.into()),
            )
            .when_some(hint, |col, hint| {
                col.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(hint),
                )
            })
            .into_any_element()
    }

    pub fn settings_callout(
        &self,
        tone: SettingsCalloutTone,
        icon: Option<&'static str>,
        body: AnyElement,
    ) -> AnyElement {
        let ui = self.tokens.ui;
        let (border_color, background, icon_color, text_color) = match tone {
            SettingsCalloutTone::Info => (
                rgba((ui.info << 8) | 0x4d),
                rgba((ui.info << 8) | 0x14),
                rgb(ui.info),
                rgb(ui.text_muted),
            ),
            SettingsCalloutTone::Warning => (
                rgba((ui.warning << 8) | 0x4d),
                rgba((ui.warning << 8) | 0x14),
                rgb(ui.warning),
                rgb(ui.text_muted),
            ),
            SettingsCalloutTone::Error => (
                rgba((ui.error << 8) | 0x4d),
                rgba((ui.error << 8) | 0x14),
                rgb(ui.error),
                rgb(ui.error),
            ),
        };
        let icon_offset_y = ((self.tokens.metrics.ui_text_xs * 1.45 - 14.0) * 0.5).max(1.0);
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border_color)
            .bg(background)
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(10.0))
            .when_some(icon, |row, icon| {
                row.child(
                    div()
                        .flex_none()
                        .mt(px(icon_offset_y))
                        .child(gpui::svg().path(icon).size(px(14.0)).text_color(icon_color)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(gpui::relative(1.45))
                    .text_color(text_color)
                    .child(body),
            )
            .into_any_element()
    }

    pub fn settings_mono_box(&self, body: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_family(settings_mono_font_family(self.settings))
            .text_color(rgb(self.tokens.ui.text))
            .line_height(px((self.tokens.metrics.ui_text_xs + 4.0).max(16.0)))
            .child(body)
            .into_any_element()
    }

    pub fn card_separator(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(rgba((self.tokens.ui.border << 8) | 0x66))
            .into_any_element()
    }

    pub fn settings_panel_background(&self, color: u32) -> Rgba {
        if self.has_background {
            rgba((color << 8) | oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens))
        } else {
            rgb(color)
        }
    }

    pub fn settings_card_surface(&self, card: Div, _color: u32) -> Div {
        let chrome = oxideterm_gpui_ui::surface_chrome(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(self.has_background),
        );
        // Settings cards sit flat in the Z=0 ground plane with a crisp border
        // and translucent glass fallback when custom background media is active.
        let card = card
            .rounded(px(chrome.radius.max(10.0)))
            .border_color(chrome.border)
            .bg(chrome.background);
        card.child(oxideterm_gpui_ui::surface::panel_edge_highlight(
            &self.tokens,
            if self.has_background {
                oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens) as f32 / 255.0
            } else {
                1.0
            },
            chrome.radius.max(10.0),
        ))
    }

    pub fn text_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x1a))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }
}
