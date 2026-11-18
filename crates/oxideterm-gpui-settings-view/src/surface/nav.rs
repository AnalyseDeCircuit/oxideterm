use super::*;
use crate::SettingsTabIcon;
use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, Rgba, ScrollHandle, svg};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    entity_row::{EntityListRowOptions, entity_list_row},
    scroll::vertical_scrollbar_layer,
    separator::{SeparatorOrientation, separator},
};

const SETTINGS_NAVIGATION_ID: &str = "settings-navigation";

pub(crate) fn render_icon(icon: &'static str, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon)
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

fn panel_background(frame: &SettingsSurfaceFrame, color: u32) -> Rgba {
    if frame.has_background {
        rgba((color << 8) | oxideterm_gpui_ui::theme_glass_card_background_alpha(&frame.tokens))
    } else {
        rgb(color)
    }
}

pub(crate) fn tab_icon(tab: SettingsTab) -> &'static str {
    match tab.icon() {
        SettingsTabIcon::BookOpen => "lucide/book-open.svg",
        SettingsTabIcon::Code2 => "lucide/code-2.svg",
        SettingsTabIcon::HardDrive => "lucide/hard-drive.svg",
        SettingsTabIcon::HelpCircle => "lucide/help-circle.svg",
        SettingsTabIcon::Key => "lucide/key.svg",
        SettingsTabIcon::Keyboard => "lucide/keyboard.svg",
        SettingsTabIcon::Monitor => "lucide/monitor.svg",
        SettingsTabIcon::Network => "lucide/network.svg",
        SettingsTabIcon::Shield => "lucide/shield.svg",
        SettingsTabIcon::Sparkles => "lucide/sparkles.svg",
        SettingsTabIcon::Square => "lucide/square.svg",
        SettingsTabIcon::Terminal => "lucide/terminal.svg",
        SettingsTabIcon::WifiOff => "lucide/wifi-off.svg",
    }
}

fn settings_nav_item_index(groups: &[Vec<SettingsTab>], tab: SettingsTab) -> Option<usize> {
    let mut item_index = 0;
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            item_index += 1;
        }
        for candidate in group {
            if *candidate == tab {
                return Some(item_index);
            }
            item_index += 1;
        }
    }
    None
}

fn settings_nav_vertical_offset(
    scroll_handle: &ScrollHandle,
    source_index: usize,
    target_index: usize,
) -> Option<f32> {
    let source_bounds = scroll_handle.bounds_for_item(source_index)?;
    let target_bounds = scroll_handle.bounds_for_item(target_index)?;
    Some(f32::from(source_bounds.origin.y - target_bounds.origin.y))
}

impl SettingsSurfaceView {
    fn action(
        &self,
        request: impl Fn() -> SettingsSurfaceRequest + 'static,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let dispatch = self.bindings.dispatch.clone();
        move |_, window, cx| {
            dispatch(request(), window, cx);
            cx.stop_propagation();
        }
    }

    fn nav_button(
        &self,
        frame: &SettingsSurfaceFrame,
        icon: &'static str,
        icon_size: f32,
        icon_color: Rgba,
        options: IconButtonOptions,
        tooltip: String,
        id: &'static str,
        flex_none: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let move_dispatch = self.bindings.dispatch.clone();
        let leave_dispatch = self.bindings.dispatch.clone();
        let moved = tooltip.clone();
        icon_button(
            &frame.tokens,
            render_icon(icon, icon_size, icon_color),
            options,
        )
        .id((gpui::ElementId::from(id), tooltip.clone()))
        .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
            move_dispatch(
                SettingsSurfaceRequest::Tooltip(moved.clone(), Some(event.position)),
                window,
                cx,
            );
        })
        .on_hover(move |hovering: &bool, window, cx| {
            if !hovering {
                leave_dispatch(
                    SettingsSurfaceRequest::Tooltip(tooltip.clone(), None),
                    window,
                    cx,
                );
            }
        })
        .on_mouse_down(MouseButton::Left, on_click)
        .when(flex_none, |button| button.flex_none())
        .into_any_element()
    }

    pub(super) fn render_settings_nav(
        &self,
        frame: &SettingsSurfaceFrame,
        cx: &mut Context<SettingsSurfaceView>,
    ) -> AnyElement {
        let theme = frame.tokens.ui;
        let settings_search_open = self.settings.read(cx).settings_search_open();
        let settings_nav_scroll = self.nav_scroll.clone();
        let settings_nav_width = frame.tokens.metrics.settings_nav_width;
        let navigation_groups = &frame.groups;
        let mut nav = div()
            .w(px(settings_nav_width))
            .min_w(px(settings_nav_width))
            .h_full()
            .flex_none()
            // Mirrors Tauri's `min-h-0` settings sidebar contract: the title
            // stays fixed and the tab list owns vertical overflow instead of
            // forcing the sidebar to grow with every added settings category.
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .pb_4()
            .bg(panel_background(frame, theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border));

        nav = nav.child(
            div()
                .flex_none()
                .h(px(48.0))
                .px(px(20.0))
                .when(!settings_search_open, |header| header.mb(px(12.0)))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(theme.border))
                .text_size(px(frame.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text_heading))
                .child(frame.i18n.t("settings_view.title"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            self.nav_button(
                                frame,
                                "lucide/search.svg",
                                15.0,
                                rgb(if settings_search_open {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }),
                                IconButtonOptions {
                                    background: settings_search_open
                                        .then(|| panel_background(frame, theme.bg_active)),
                                    hover_background: Some(rgb(theme.bg_hover)),
                                    ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                },
                                frame.i18n.t("settings_view.search.open"),
                                "settings-search",
                                true,
                                self.action(|| SettingsSurfaceRequest::ToggleSearch),
                            ),
                        )
                        .child(self.nav_button(
                            frame,
                            "lucide/list-tree.svg",
                            15.0,
                            rgb(theme.text_muted),
                            IconButtonOptions {
                                hover_background: Some(rgb(theme.bg_hover)),
                                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                            },
                            frame.i18n.t("settings_view.navigation_editor.open"),
                            "settings-navigation-editor",
                            true,
                            self.action(|| SettingsSurfaceRequest::OpenNavigationEditor),
                        )),
                ),
        );

        if settings_search_open {
            return nav
                .child(self.render_settings_search_panel(frame, cx))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-nav-scroll")
            .size_full()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&settings_nav_scroll)
            .px_3()
            .flex()
            .flex_col();

        for (group_index, group) in navigation_groups.iter().enumerate() {
            if group_index > 0 {
                list = list.child(
                    div()
                        .flex_none()
                        .py_2()
                        .child(separator(&frame.tokens, SeparatorOrientation::Horizontal)),
                );
            }
            for tab in group {
                list = list.child(self.render_settings_nav_item(
                    frame,
                    *tab,
                    navigation_groups,
                    &settings_nav_scroll,
                    cx,
                ));
            }
        }

        nav.child(div().flex_1().min_h(px(0.0)).relative().child(list).child(
            vertical_scrollbar_layer("settings-nav-scrollbar", &settings_nav_scroll),
        ))
        .into_any_element()
    }

    pub fn render_settings_nav_item(
        &self,
        frame: &SettingsSurfaceFrame,
        tab: SettingsTab,
        navigation_groups: &[Vec<SettingsTab>],
        settings_nav_scroll: &ScrollHandle,
        cx: &mut Context<SettingsSurfaceView>,
    ) -> AnyElement {
        let theme = frame.tokens.ui;
        let active = self.settings.read(cx).route_snapshot().active_tab == tab;
        let nav_item_index = settings_nav_item_index(navigation_groups, tab);
        let navigation_groups_for_click = navigation_groups.to_vec();
        let transition_scroll_handle = settings_nav_scroll.clone();
        let tokens = frame.tokens;
        let dispatch = self.bindings.dispatch.clone();
        let selection_surface = active.then(|| {
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(frame.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(panel_background(frame, theme.bg_panel));
            let surface = oxideterm_gpui_ui::theme_card_surface_shadow(surface, &frame.tokens);

            match nav_item_index {
                Some(index) => self.nav_motion.render_vertical(
                    SETTINGS_NAVIGATION_ID,
                    index,
                    &frame.tokens,
                    surface,
                ),
                None => surface.into_any_element(),
            }
        });
        div()
            // Keep selection and hover surfaces stable when the settings
            // window has spare vertical space.
            .h(px(frame.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .mb(px(4.0))
            .px_3()
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(frame.tokens.radii.md))
            .bg(rgba(0x00000000))
            .text_size(px(frame.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(if active {
                theme.text_heading
            } else {
                theme.text
            }))
            .cursor_pointer()
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(rgba((theme.bg_hover << 8) | 0x80))
                }
            })
            .when_some(selection_surface, |item, surface| item.child(surface))
            .child(
                div()
                    .flex_none()
                    .child(render_icon(tab_icon(tab), 18.0, rgb(theme.accent))),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .child(frame.i18n.t(tab.label_key())),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    let active_tab = this.settings.read(cx).route_snapshot().active_tab;
                    if active_tab != tab
                        && let Some(source) =
                            settings_nav_item_index(&navigation_groups_for_click, active_tab)
                        && let Some(target) =
                            settings_nav_item_index(&navigation_groups_for_click, tab)
                    {
                        let offset =
                            settings_nav_vertical_offset(&transition_scroll_handle, source, target);
                        this.animate_route(SETTINGS_NAVIGATION_ID, &tokens, target, offset, cx);
                    }
                    this.settings
                        .update(cx, |settings, cx| settings.set_active_tab(tab, cx));
                    dispatch(SettingsSurfaceRequest::SelectedTab(tab), window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub fn render_settings_search_panel(
        &self,
        frame: &SettingsSurfaceFrame,
        cx: &mut Context<SettingsSurfaceView>,
    ) -> AnyElement {
        let settings = self.settings.read(cx);
        let query = settings.settings_search_query();
        let input = (self.bindings.search_field)(
            query,
            settings.settings_entity_focused_input() == Some(crate::SettingsInput::SettingsSearch),
            cx,
        );
        // The navigation surface already owns its image tint.
        let search_input = div()
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(36.0))
            .px_3()
            .flex()
            .items_center()
            .gap(px(frame.tokens.spacing.one))
            .border_b_1()
            .border_color(frame.divider)
            .bg(rgba(0x00000000))
            .children(input)
            .when(!query.is_empty(), |input| {
                input.child(self.nav_button(
                    frame,
                    "lucide/x.svg",
                    13.0,
                    rgb(frame.tokens.ui.text_muted),
                    IconButtonOptions {
                        hover_background: Some(rgb(frame.tokens.ui.bg_hover)),
                        ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                    },
                    frame.i18n.t("settings_view.search.clear"),
                    "settings-search-clear",
                    true,
                    self.action(|| SettingsSurfaceRequest::ClearSearch),
                ))
            });
        let cached = self.search_filter.results(&frame.i18n);
        let results = cached
            .iter()
            .filter(|result| {
                result.section != SettingsSectionId::KnowledgeDocuments
                    || (self.bindings.knowledge_available)(cx)
            })
            .collect::<Vec<_>>();
        let result_scroll = self.search_scroll.clone();
        let mut result_list = div()
            .id("settings-search-results-scroll")
            .size_full()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&result_scroll)
            .pt_3()
            .px_2()
            .pb_3()
            .flex()
            .flex_col()
            .gap(px(2.0));

        if query.trim().is_empty() || results.is_empty() {
            let message_key = if query.trim().is_empty() {
                "settings_view.search.hint"
            } else {
                "settings_view.search.no_results"
            };
            result_list = result_list.child(
                div()
                    .flex_1()
                    .min_h(px(120.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_size(px(frame.tokens.metrics.ui_text_sm))
                    .text_color(rgb(frame.tokens.ui.text_muted))
                    .child(render_icon(
                        "lucide/search.svg",
                        20.0,
                        rgb(frame.tokens.ui.text_muted),
                    ))
                    .child(frame.i18n.t(message_key)),
            );
        } else {
            for result in results {
                let tab = result.tab;
                let action_result = result.clone();
                let row = entity_list_row(
                    &frame.tokens,
                    EntityListRowOptions::new()
                        .compact()
                        .has_background_image(frame.has_background),
                    Some(
                        render_icon(tab_icon(tab), 16.0, rgb(frame.tokens.ui.accent))
                            .into_any_element(),
                    ),
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(frame.tokens.metrics.ui_text_sm))
                        .text_color(rgb(frame.tokens.ui.text))
                        .child(result.label.clone())
                        .into_any_element(),
                    Some(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(frame.tokens.metrics.ui_text_xs))
                            .text_color(rgb(frame.tokens.ui.text_muted))
                            .child(result.breadcrumb.clone())
                            .into_any_element(),
                    ),
                    Vec::new(),
                    Vec::new(),
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    self.action(move || {
                        SettingsSurfaceRequest::SearchResult(action_result.clone())
                    }),
                );
                result_list = result_list.child(row);
            }
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(search_input)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(result_list)
                    .child(vertical_scrollbar_layer(
                        "settings-search-results-scrollbar",
                        &result_scroll,
                    )),
            )
            .into_any_element()
    }
}
