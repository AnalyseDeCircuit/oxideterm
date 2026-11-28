use super::*;
use crate::appearance_page::CoreAppearancePage;
use oxideterm_settings::{
    BackgroundScope, MAX_TERMINAL_BACKGROUND_OPACITY, MIN_TERMINAL_BACKGROUND_OPACITY,
};

fn scope_row(
    page: &CoreAppearancePage<'_>,
    owner: &Entity<PageState>,
    scope: BackgroundScope,
) -> AnyElement {
    use oxideterm_gpui_ui::segmented_control::{
        SegmentedControlOptions, segmented_control, segmented_control_item,
    };
    let active = usize::from(scope == BackgroundScope::Window);
    let options = [
        (
            BackgroundScope::Content,
            "settings_view.terminal.bg_scope_content",
        ),
        (
            BackgroundScope::Window,
            "settings_view.terminal.bg_scope_window",
        ),
    ];
    let items = options
        .into_iter()
        .enumerate()
        .map(|(i, (target, key))| {
            let owner = owner.downgrade();
            segmented_control_item(page.tokens, page.i18n.t(key), i == active)
                .id(("browser-wallpaper-scope", i))
                .role(gpui::Role::Button)
                .aria_label(page.i18n.t(key))
                .on_click(move |_, _, cx| {
                    if target != scope {
                        let _ = owner.update(cx, |state, cx| {
                            state.settings.terminal.background_scope = target;
                            state.changed(cx);
                        });
                    }
                })
                .into_any_element()
        })
        .collect();
    page.appearance_row(
        "settings_view.terminal.bg_scope",
        "",
        segmented_control(
            page.tokens,
            "browser-wallpaper-scope",
            SegmentedControlOptions::new(active, active, 2).compact(230.),
            items,
        )
        .into_any_element(),
    )
}

pub(super) fn card(
    page: &CoreAppearancePage<'_>,
    owner: &Entity<PageState>,
    cx: &App,
) -> AnyElement {
    let state = owner.read(cx);
    let Some(sources) = state.wallpaper_gallery.as_ref() else {
        return div().into_any_element();
    };
    let terminal = &state.settings.terminal;
    let selected = WallpaperId::selected(terminal);
    let builtins = BundledWallpaper::ALL.into_iter().map(|b| WallpaperTile {
        id: WallpaperId::Bundled(b),
        name: b.filename().into(),
        format_key: if b == BundledWallpaper::Ambient {
            "settings_view.terminal.bg_format_png"
        } else {
            "settings_view.terminal.bg_format_webp"
        },
        thumbnail: (sources.source)(b),
    });
    let thumbnails = builtins
        .chain(state.wallpaper_tiles.iter().cloned())
        .map(|tile| {
            let id = tile.id;
            let active = Some(id) == selected;
            let pick = sources.request.clone();
            let remove = sources.request.clone();
            let color = rgb(page.tokens.ui.text_muted);
            crate::settings_background_thumbnail_frame(
                page.tokens,
                &tile.name,
                tile.thumbnail,
                active,
                page.i18n.t("settings_view.terminal.bg_active"),
                page.i18n.t(tile.format_key),
                move || super::nav::render_icon("lucide/image.svg", 20., color),
            )
            .id(gpui::SharedString::from(id.key()))
            .role(gpui::Role::Button)
            .aria_label(tile.name)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pick(WallpaperAction::Select(id), cx);
                cx.stop_propagation();
            })
            .when(
                matches!(id, WallpaperId::Imported(_)) && !state.gallery_busy,
                |thumb| {
                    thumb.child(
                        crate::settings_background_thumbnail_remove_button(
                            page.tokens,
                            super::nav::render_icon("lucide/x.svg", 12., rgb(page.tokens.ui.text)),
                        )
                        .id((gpui::ElementId::from("remove-wallpaper"), id.key()))
                        .role(gpui::Role::Button)
                        .aria_label(page.i18n.t("settings_view.terminal.bg_clear"))
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_, _, cx| {
                                remove(WallpaperAction::Remove(id), cx);
                                cx.stop_propagation();
                            },
                        ),
                    )
                },
            )
            .into_any_element()
        })
        .collect();
    let pick = sources.request.clone();
    let clear = sources.request.clone();
    let actions = div()
        .flex()
        .gap(px(8.))
        .child(
            page.appearance_action_button(
                "lucide/plus.svg",
                page.i18n.t("settings_view.terminal.bg_add"),
                move |_, _, cx| {
                    pick(WallpaperAction::Pick, cx);
                },
            )
            .when(state.gallery_busy, |b| b.opacity(0.5)),
        )
        .when(
            !state.wallpaper_tiles.is_empty() && !state.gallery_busy,
            |a| {
                a.child(
                    crate::settings_background_clear_all_button(
                        page.tokens,
                        page.i18n.t("settings_view.terminal.bg_clear_all"),
                        super::nav::render_icon(
                            "lucide/trash-2.svg",
                            14.,
                            rgb(page.tokens.ui.error),
                        ),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        clear(WallpaperAction::Clear, cx);
                    }),
                )
            },
        );
    let mut rows = vec![crate::settings_background_gallery(
        page.tokens,
        page.i18n.t("settings_view.terminal.bg_gallery"),
        actions.into_any_element(),
        crate::settings_background_thumbnails_layout(thumbnails),
    )];
    if let Some(error) = state.gallery_error {
        let request = sources.request.clone();
        rows.push(
            page.appearance_row(
                error,
                "",
                page.appearance_action_button(
                    "lucide/refresh-cw.svg",
                    page.i18n.t("ide.retry"),
                    move |_, _, cx| request(WallpaperAction::Reload, cx),
                )
                .into_any_element(),
            ),
        );
    }
    if selected.is_some() {
        let toggle_owner = owner.downgrade();
        rows.push(
            page.appearance_row(
                "settings_view.terminal.bg_enabled",
                "",
                oxideterm_gpui_ui::switch::switch(
                    page.tokens,
                    "browser-background-enabled",
                    String::new(),
                    terminal.background_enabled,
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let _ = toggle_owner.update(cx, |s, cx| {
                        s.settings.terminal.background_enabled =
                            !s.settings.terminal.background_enabled;
                        s.changed(cx);
                    });
                })
                .into_any_element(),
            ),
        );
        rows.push(page.appearance_row(
            "settings_view.terminal.bg_opacity",
            "",
            page.appearance_slider_value_control(
                SettingsSlider::AppearanceBackgroundOpacity,
                SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                (MIN_TERMINAL_BACKGROUND_OPACITY * 100.) as f32,
                (MAX_TERMINAL_BACKGROUND_OPACITY * 100.) as f32,
                (terminal.background_opacity * 100.).round() as f32,
                "%",
                cx,
            ),
        ));
        rows.push(scope_row(page, owner, terminal.background_scope));
        if terminal.background_scope == BackgroundScope::Content {
            let pills = crate::background_tab_options()
                .iter()
                .filter(|(key, _, _)| (sources.supports_tab)(key))
                .map(|&(key, label, icon)| {
                    let active = terminal.background_enabled_tabs.iter().any(|t| t == key);
                    let owner = owner.downgrade();
                    let color = if active {
                        page.tokens.ui.accent
                    } else {
                        page.tokens.ui.text_muted
                    };
                    crate::settings_background_tab_pill(
                        page.tokens,
                        state.i18n.t(label),
                        super::nav::render_icon(
                            crate::appearance::background_tab_icon(icon),
                            page.tokens.metrics.settings_background_tab_icon_size,
                            rgb(color),
                        ),
                        active,
                    )
                    .id((gpui::ElementId::from("browser-wallpaper-tab"), key))
                    .role(gpui::Role::Button)
                    .aria_label(state.i18n.t(label))
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |state, cx| {
                            let tabs = &mut state.settings.terminal.background_enabled_tabs;
                            if let Some(i) = tabs.iter().position(|tab| tab == key) {
                                tabs.remove(i);
                            } else {
                                tabs.push(key.to_owned());
                            }
                            state.changed(cx);
                        });
                    })
                    .into_any_element()
                })
                .collect();
            rows.push(crate::settings_background_tabs_section(
                page.tokens,
                state.i18n.t("settings_view.terminal.bg_tabs"),
                state.i18n.t("settings_view.terminal.bg_tabs_hint"),
                pills,
            ));
        }
    }
    page.appearance_card(state.i18n.t("settings_view.terminal.bg_title"), None, rows)
}
