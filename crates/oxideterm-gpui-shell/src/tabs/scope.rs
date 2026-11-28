use gpui::{prelude::*, *};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy)]
pub enum ScopeButton {
    Pick,
    Clear,
}

impl ScopeButton {
    pub fn id(self) -> &'static str {
        match self {
            Self::Pick => "titlebar-tab-scope",
            Self::Clear => "titlebar-show-all-tabs",
        }
    }
}

pub struct ScopeTrigger {
    pub label: String,
    pub filtered: bool,
    pub visible_count: usize,
    pub total_count: usize,
    pub available_width: f32,
    pub text_color: u32,
    pub hover_bg: u32,
}

impl ScopeTrigger {
    pub fn render(
        self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mut bind: impl FnMut(Stateful<Div>, ScopeButton, String) -> Stateful<Div>,
    ) -> Option<AnyElement> {
        let Self {
            label,
            filtered,
            visible_count,
            total_count,
            available_width,
            text_color,
            hover_bg,
        } = self;
        let icon = |i: LucideIcon, n, c| svg().path(i.path()).size(px(n)).text_color(c);
        // Hide the scope trigger when the workspace is unfiltered and empty so
        // the titlebar remains a continuous drag bar over the Welcome view.
        if !filtered && total_count == 0 {
            return None;
        }

        let theme = tokens.ui;
        let visible = visible_count.to_string();
        let total = total_count.to_string();
        let pill_height = (tokens.metrics.titlebar_height - 8.0).max(20.0);
        let tooltip = i18n
            .t(if filtered {
                "sidebar.tab_scope.filtered_tooltip"
            } else {
                "sidebar.tab_scope.all_tooltip"
            })
            .replace("{{visible}}", &visible)
            .replace("{{total}}", &total)
            .replace("{{scope}}", &label);

        if !filtered {
            let max_width = available_width.max(0.0).min(280.0);
            let hover_text = theme.text;
            let count_color = rgba((text_color << 8) | 0xc8);
            let trigger = div()
                .group("titlebar-tab-scope")
                .id("titlebar-tab-scope")
                .h(px(pill_height))
                .max_w(px(max_width))
                .min_w(px(0.0))
                .flex_none()
                .mx(px(tokens.spacing.two))
                .px(px(8.0))
                .rounded(px(tokens.radii.md))
                .bg(rgba(0x00000000))
                .overflow_hidden()
                .occlude()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(5.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(text_color))
                .hover(move |trigger| trigger.bg(rgb(hover_bg)).text_color(rgb(hover_text)))
                .child(div().flex_initial().min_w(px(0.0)).truncate().child(label))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(tokens.metrics.ui_text_2xs))
                        .text_color(count_color)
                        .child(total),
                )
                .child(
                    div().flex_none().child(
                        svg()
                            .path(LucideIcon::ChevronDown.path())
                            .size(px(12.0))
                            .text_color(rgb(text_color))
                            .group_hover("titlebar-tab-scope", move |icon| {
                                icon.text_color(rgb(hover_text))
                            }),
                    ),
                );

            return Some(bind(trigger, ScopeButton::Pick, tooltip).into_any_element());
        }

        let max_width = available_width.max(0.0).min(320.0);
        let count = format!("{visible}/{total}");
        let show_all_label = i18n.t("sidebar.tab_scope.show_all");
        let show_all_tooltip = show_all_label.clone();
        let capsule_bg = rgba((theme.accent << 8) | if theme.is_light() { 0x14 } else { 0x1f });
        let capsule_border = rgba((theme.accent << 8) | 0x4d);
        let primary_hover_bg = rgba((theme.accent << 8) | 0x24);
        let divider_bg = rgba((theme.accent << 8) | 0x38);
        let clear_hover_bg = rgba((theme.accent << 8) | 0x2e);
        let hover_text = theme.text;

        let primary = bind(
            div()
                .id("titlebar-tab-scope")
                .h_full()
                .px(px(8.0))
                .gap(px(5.0))
                .flex()
                .items_center()
                .min_w(px(0.0))
                .flex_auto()
                .cursor_pointer()
                .hover(move |segment| segment.bg(primary_hover_bg))
                .child(div().flex_none().child(icon(
                    LucideIcon::ListFilter,
                    12.0,
                    rgb(theme.accent),
                )))
                .child(
                    div()
                        .flex_initial()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(tokens.metrics.ui_text_2xs))
                        .text_color(rgb(theme.text_muted))
                        .child(count),
                )
                .child(div().flex_none().child(icon(
                    LucideIcon::ChevronDown,
                    12.0,
                    rgb(theme.text_muted),
                ))),
            ScopeButton::Pick,
            tooltip,
        );

        Some(
            div()
                .id("titlebar-tab-scope-capsule")
                .debug_selector(|| "titlebar-tab-scope-capsule".into())
                .h(px(pill_height))
                .max_w(px(max_width))
                .min_w(px(0.0))
                .flex_none()
                .mx(px(tokens.spacing.two))
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(capsule_border)
                .bg(capsule_bg)
                .overflow_hidden()
                .occlude()
                .flex()
                .flex_row()
                .items_center()
                .child(primary)
                .child(div().w(px(1.0)).h(px(12.0)).flex_none().bg(divider_bg))
                .child(bind(
                    div()
                        .group("titlebar-show-all-tabs")
                        .id("titlebar-show-all-tabs")
                        .debug_selector(|| "titlebar-show-all-tabs".into())
                        .role(gpui::Role::Button)
                        .aria_label(show_all_label)
                        .h_full()
                        .w(px(22.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |segment| segment.bg(clear_hover_bg))
                        .child(
                            svg()
                                .path(LucideIcon::X.path())
                                .size(px(12.0))
                                .text_color(rgb(theme.text_muted))
                                .group_hover("titlebar-show-all-tabs", move |icon| {
                                    icon.text_color(rgb(hover_text))
                                }),
                        ),
                    ScopeButton::Clear,
                    show_all_tooltip,
                ))
                .into_any_element(),
        )
    }
}
