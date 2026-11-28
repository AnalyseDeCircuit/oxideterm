use super::*;
use oxideterm_gpui_connections::session_manager::SessionManagerOpenTarget;
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    modal::rounded_shell_child_radius,
    theme_card_surface_shadow,
    tooltip::tooltip_view,
};
use oxideterm_settings_model::{
    AppearanceSchemeError, apply_appearance_mode, next_appearance_scheme,
};
use oxideterm_theme::AppUiColors;

const WELCOME_CONTENT_MAX_WIDTH: f32 = 860.0;
const WELCOME_PAGE_PADDING: f32 = 24.0;
const WELCOME_COMPACT_LAYOUT_WIDTH: f32 = 800.0;
const WELCOME_BRAND_BADGE_SIZE: f32 = 42.0;
const WELCOME_BRAND_BADGE_RADIUS: f32 = 9.5;
const RECENT_CONNECTION_LIMIT: usize = 4;

fn welcome_card_background(theme: AppUiColors) -> u32 {
    if theme.is_light() {
        theme.bg_panel
    } else {
        theme.bg_card
    }
}

fn welcome_keycap_background(theme: AppUiColors) -> u32 {
    if theme.is_light() {
        theme.bg_sunken
    } else {
        theme.bg_card
    }
}

#[derive(Clone, Copy)]
struct WelcomeWorkbenchMetrics {
    sm_line_height: f32,
    xs_line_height: f32,
    recent_row_height: f32,
    utility_row_height: f32,
    guidance_stack_gap: f32,
    primary_tile_height: f32,
}

fn welcome_workbench_metrics(tokens: &ThemeTokens) -> WelcomeWorkbenchMetrics {
    // Explicit row and line heights keep CJK and ASCII labels at identical box
    // heights and align the full 4-row recent card bottom edge with the right
    // guidance stack without forcing `< 4` recent rows to stretch.
    let sm_line_height = (tokens.metrics.ui_text_sm + 5.0).round();
    let xs_line_height = (tokens.metrics.ui_text_xs + 4.0).round();
    let recent_row_height = (sm_line_height + 2.0 + xs_line_height + 16.0).max(48.0);
    let utility_row_height = sm_line_height + 26.0;
    let guidance_stack_gap = 12.0;
    let full_recent_card_height = 4.0 * recent_row_height + 5.0;
    let utility_card_height = 3.0 * utility_row_height + 4.0;
    let primary_tile_height =
        (full_recent_card_height - guidance_stack_gap - utility_card_height).max(52.0);
    WelcomeWorkbenchMetrics {
        sm_line_height,
        xs_line_height,
        recent_row_height,
        utility_row_height,
        guidance_stack_gap,
        primary_tile_height,
    }
}

pub(super) fn glyph(icon: LucideIcon, size: f32, color: u32) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(rgb(color))
        .into_any_element()
}

impl Workspace {
    pub(crate) fn chrome_divider(&self) -> Rgba {
        self.divider(0x66)
    }
    pub(super) fn divider(&self, opaque: u32) -> Rgba {
        let alpha = if self.wallpaper.is_some() {
            opaque.min(0x26)
        } else {
            opaque
        };
        rgba((self.tokens.ui.border << 8) | alpha)
    }
    pub(super) fn chrome_fill(&self, color: u32) -> Rgba {
        let alpha =
            (self.tokens.metrics.sidebar_vibrancy_alpha.clamp(0., 1.) * 255.).round() as u32;
        oxideterm_gpui_ui::color_for_background(color, self.window_wallpaper().is_some(), alpha)
    }

    pub(crate) fn shortcut(&self, id: &str, cx: &App) -> Option<String> {
        let def = self.actions.iter().find(|d| d.id == id)?;
        effective_combo(
            def,
            &self.settings.read(cx).settings(cx).keybindings.overrides,
            self.side,
        )
        .map(|combo| format_combo_for(&combo, self.side))
    }

    pub(super) fn bind_button(
        &self,
        button: Div,
        id: impl Into<ElementId>,
        label: String,
        shortcut: Option<String>,
        cx: &Context<Self>,
        activate: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let tokens = self.tokens;
        let owner = cx.weak_entity();
        let accessible = activate.clone();
        button
            .id(id)
            .flex_none()
            .role(Role::Button)
            .aria_label(label.clone())
            .tooltip(move |_, cx| tooltip_view(tokens, label.clone(), shortcut.clone(), cx))
            .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                let _ = owner.update(cx, |h, cx| accessible(h, w, cx));
            })
            .on_click(cx.listener(move |h, _, w, cx| {
                activate(h, w, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    pub(super) fn toolbar_button(
        &self,
        action: Action,
        icon: LucideIcon,
        label: &'static str,
        size: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = &self.tokens;
        let shortcut = match action {
            Action::Palette => self.shortcut("app.commandPalette", cx),
            Action::Zen => self.shortcut("app.zenMode", cx),
            Action::Split(SplitDirection::Horizontal) => self.shortcut("split.horizontal", cx),
            Action::Split(SplitDirection::Vertical) => self.shortcut("split.vertical", cx),
            _ => None,
        };
        let button = icon_button(
            t,
            glyph(icon, t.metrics.sidebar_action_icon_size, t.ui.text_muted),
            IconButtonOptions::opaque_toolbar(size, ButtonRadius::Md),
        );
        self.bind_button(
            button,
            label,
            self.i18n.t(label),
            shortcut,
            cx,
            move |h, w, cx| h.action(action, w, cx),
        )
    }

    pub(super) fn titlebar(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let size = t.metrics.titlebar_height - 6.0;
        let action_padding = t.spacing.one / 2.0;
        let scheme = self
            .settings
            .read(cx)
            .settings(cx)
            .appearance
            .active_scheme(dark(window));
        let dark_scheme = scheme.is_some_and(oxideterm_settings::AppearanceScheme::is_dark);
        let actions_width = 3.0 * (size + 2.0 * action_padding);
        let available =
            f32::from(window.viewport_size().width) - 2.0 * (actions_width + t.spacing.two);
        let scope = self.scope_trigger(available, cx);
        let show_scope = scope.is_some();
        let actions = div()
            .flex_none()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_none()
                    .px(px(action_padding))
                    .child(self.toolbar_button(
                        Action::Palette,
                        LucideIcon::Search,
                        Action::Palette.key(),
                        size,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(action_padding))
                    .child(self.toolbar_button(
                        Action::Zen,
                        LucideIcon::EyeOff,
                        Action::Zen.key(),
                        size,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(action_padding))
                    .child(self.toolbar_button(
                        Action::Theme,
                        if dark_scheme {
                            LucideIcon::Moon
                        } else {
                            LucideIcon::Sun
                        },
                        match scheme {
                            None => "settings_view.appearance.configure_schemes",
                            Some(_) if dark_scheme => "sidebar.tooltips.switch_to_light",
                            Some(_) => "sidebar.tooltips.switch_to_dark",
                        },
                        size,
                        cx,
                    )),
            );

        // Match desktop's native titlebar: no top-left brand logo or wordmark,
        // keeping the top chrome clean and centered around workspace actions.
        div()
            .flex_none()
            .w_full()
            .h(px(t.metrics.titlebar_height))
            .flex()
            .flex_row()
            .items_center()
            .bg(self.chrome_fill(t.ui.shell_bg()))
            .border_b_1()
            .border_color(self.chrome_divider())
            .text_size(px(t.metrics.titlebar_label_font_size))
            .text_color(rgb(t.ui.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .px_2()
                    .gap(px(t.spacing.one))
                    .when_some(self.gateway_host.clone(), |bar, host| {
                        bar.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .rounded(px(t.radii.sm))
                                .bg(rgb(t.ui.shell_active_bg()))
                                .px_2()
                                .py(px(1.0))
                                .child(host),
                        )
                    })
                    .when(!self.demo && !self.ready, |bar| {
                        bar.child(self.i18n.t(if self.connecting {
                            "common.status_connecting"
                        } else {
                            "connections.state.disconnected"
                        }))
                    }),
            )
            .children(scope)
            .child(
                div()
                    .when(show_scope, |right| right.flex_1().min_w(px(actions_width)))
                    .flex()
                    .justify_end()
                    .child(actions),
            )
            .into_any_element()
    }

    pub(super) fn toggle_theme(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx).settings(cx);
        let system_dark = dark(w);
        match next_appearance_scheme(settings, system_dark) {
            Ok(scheme) => self.settings.update(cx, |s, cx| {
                s.edit_preferences(cx, |p| {
                    apply_appearance_mode(p, Some(scheme));
                })
            }),
            Err(e) => {
                let message = match e {
                    AppearanceSchemeError::Identical => {
                        self.i18n.t("settings_view.appearance.schemes_identical")
                    }
                    AppearanceSchemeError::Unavailable(scheme) => self
                        .i18n
                        .t("settings_view.appearance.scheme_unavailable")
                        .replace(
                            "{{scheme}}",
                            &self.i18n.t(if scheme.is_dark() {
                                "settings_view.appearance.system_dark"
                            } else {
                                "settings_view.appearance.system_light"
                            }),
                        ),
                    AppearanceSchemeError::Unassigned => {
                        self.i18n.t("settings_view.appearance.legacy_scheme_hint")
                    }
                };
                self.notifications_notice(message, cx);
                self.settings.update(cx, |s, cx| {
                    s.open_section(oxideterm_gpui_settings_view::SettingsTab::Appearance, cx)
                });
                self.action(Action::Settings, w, cx);
            }
        }
    }

    pub(super) fn toggle_zen(&mut self, cx: &mut Context<Self>) {
        let zen = !self.settings.read(cx).settings(cx).sidebar_ui.zen_mode;
        self.settings.update(cx, |s, cx| {
            s.edit_preferences(cx, |p| p.sidebar_ui.zen_mode = zen)
        });
        self.rails.set_zen_mode(zen, true, &self.tokens);
        self.zen_exit.reset();
        self.changed(cx);
    }

    pub(super) fn zen_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.zen_exit
                .reveal(self.settings.read(cx).settings(cx).sidebar_ui.zen_mode);
        } else if let Some(generation) = self.zen_exit.schedule_hide() {
            at_deadline(Instant::now() + zen::HIDE_DELAY, cx, move |h, _, cx| {
                if h.zen_exit.apply_hide(generation) {
                    cx.notify();
                }
            })
            .detach();
        }
    }

    pub(super) fn strip_actions(&self, cx: &Context<Self>) -> (Option<AnyElement>, f32) {
        // Match desktop tab strip: only render the exit-zen control here while
        // zen mode is active; normal mode shows only tabs and the `+` button.
        let zen = self.settings.read(cx).settings(cx).sidebar_ui.zen_mode;
        if !zen {
            return (None, 0.0);
        }
        let size = self.tokens.metrics.tabbar_height;
        (
            Some(self.toolbar_button(Action::Zen, LucideIcon::Eye, "zen_mode.exit", size, cx)),
            size,
        )
    }

    pub(crate) fn occupied_rail_width(&self, cx: &App) -> f32 {
        if self.settings.read(cx).settings(cx).sidebar_ui.zen_mode {
            return 0.;
        }
        let bar = self.tokens.metrics.activity_bar_width;
        let left = if self.rails.left.collapsed() {
            bar
        } else {
            bar + self.rails.left.panel_width(&self.tokens)
        };
        let right = if self.rails.right.collapsed() {
            bar
        } else {
            bar + self.rails.right.panel_width(&self.tokens)
        };
        left + right
    }

    fn welcome_available_width(&self, w: &Window, cx: &App) -> f32 {
        (f32::from(w.viewport_size().width) - self.occupied_rail_width(cx))
            .max(self.tokens.metrics.min_main_width)
    }

    pub(super) fn empty_workspace(&self, w: &Window, cx: &Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let available = self.welcome_available_width(w, cx);
        let compact = available - 2.0 * WELCOME_PAGE_PADDING < WELCOME_COMPACT_LAYOUT_WIDTH;
        let recent = self.render_welcome_recent_connections(cx);
        let guidance = self.render_welcome_guidance(cx);
        let workbench = if let Some(guidance) = guidance {
            let grid = div()
                .w_full()
                .min_w_0()
                .grid()
                .grid_cols(if compact { 1 } else { 2 })
                .items_start()
                .gap(px(if compact { 22.0 } else { 28.0 }));
            if compact {
                grid.child(guidance.w_full().min_w_0())
                    .child(recent.w_full().min_w_0())
            } else {
                grid.child(recent.w_full().min_w_0())
                    .child(guidance.w_full().min_w_0())
            }
        } else {
            recent.w_full()
        };

        div()
            .size_full()
            .when(self.window_wallpaper().is_none(), |view| {
                view.bg(rgb(theme.bg))
            })
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .id("welcome-workspace-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(px(WELCOME_PAGE_PADDING))
                            .pt(px(28.0))
                            .pb(px(48.0))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(WELCOME_CONTENT_MAX_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(22.0))
                                    .child(self.render_welcome_header(cx))
                                    .child(workbench)
                                    .child(self.render_welcome_shortcuts(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_header(&self, cx: &App) -> AnyElement {
        let theme = self.tokens.ui;
        let wordmark_size = self.tokens.metrics.welcome_wordmark_font_size;
        let variant = self.settings.read(cx).settings(cx).appearance.app_icon;
        let scaled_icon = WELCOME_BRAND_BADGE_SIZE * crate::icon::APP_ICON_CANVAS_TO_SQUIRCLE_SCALE;
        let badge = theme_card_surface_shadow(
            div()
                .size(px(WELCOME_BRAND_BADGE_SIZE))
                .relative()
                .flex_none()
                .overflow_hidden()
                .rounded(px(WELCOME_BRAND_BADGE_RADIUS))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    img(crate::icon::app_icon_source(variant))
                        .size(px(scaled_icon))
                        .flex_none()
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(WELCOME_BRAND_BADGE_RADIUS))
                        .border_1()
                        .border_color(rgba((theme.border << 8) | 0x80)),
                ),
            &self.tokens,
        );

        div()
            .w_full()
            .pb(px(18.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .flex()
            .flex_col()
            .items_start()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(badge)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(wordmark_size))
                            .line_height(px(wordmark_size + 4.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("layout.empty.title")),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_recent_connections(&self, cx: &Context<Self>) -> Div {
        let theme = self.tokens.ui;
        let has_background = self.window_wallpaper().is_some();
        let alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let fill =
            move |color| oxideterm_gpui_ui::color_for_background(color, has_background, alpha);
        let metrics = welcome_workbench_metrics(&self.tokens);
        let inner_radius = rounded_shell_child_radius(self.tokens.radii.lg);
        let saved = &self.catalog_data.items;
        let mut recent: Vec<_> = saved
            .iter()
            .filter_map(|row| {
                let timestamp = row.last_used.as_deref()?;
                Some((timestamp, row))
            })
            .collect();
        recent.sort_by(|a, b| b.0.cmp(a.0).then_with(|| a.1.id().cmp(b.1.id())));
        recent.truncate(RECENT_CONNECTION_LIMIT);
        let count_label = self
            .i18n
            .t("layout.empty.saved_count")
            .replace("{{count}}", &saved.len().to_string());

        let container = div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(fill(welcome_card_background(theme)))
            .overflow_hidden();

        let empty_action = if self.demo {
            Action::Catalog
        } else {
            Action::Ble
        };
        let body = if recent.is_empty() {
            container.child(
                div()
                    .w_full()
                    .px(px(16.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .items_start()
                    .justify_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("layout.empty.recent_connections_empty")),
                    )
                    .when(self.action_available(empty_action, cx), |col| {
                        col.child(
                            div()
                                .id("welcome-recent-open-manager")
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.accent))
                                .cursor_pointer()
                                .hover(move |row| row.text_color(rgb(theme.accent_hover)))
                                .child(glyph(
                                    if self.demo {
                                        LucideIcon::LayoutList
                                    } else {
                                        LucideIcon::Bluetooth
                                    },
                                    14.0,
                                    theme.accent,
                                ))
                                .child(self.i18n.t(if self.demo {
                                    "layout.empty.open_session_manager"
                                } else {
                                    "modals.new_connection.transport_ble"
                                }))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |h, _, w, cx| {
                                        h.action(empty_action, w, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            )
        } else {
            let count = recent.len();
            let mut list = div().w_full().flex().flex_col();
            for (index, (_, row)) in recent.into_iter().enumerate() {
                let node_id = row.id().to_owned();
                if index > 0 {
                    list = list.child(div().h(px(1.0)).bg(rgba((theme.border << 8) | 0x45)));
                }
                let is_first = index == 0;
                let is_last = index + 1 == count;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("welcome-recent-{node_id}")))
                        .w_full()
                        .min_w_0()
                        .h(px(metrics.recent_row_height))
                        .px(px(12.0))
                        .when(is_first, |r| r.rounded_t(px(inner_radius)))
                        .when(is_last, |r| r.rounded_b(px(inner_radius)))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .cursor_pointer()
                        .hover(move |r| {
                            r.bg(oxideterm_gpui_ui::color_for_background(
                                theme.bg_hover,
                                has_background,
                                0x80,
                            ))
                        })
                        .child({
                            let is_ble = crate::ble::is_ble_node(&node_id);
                            div()
                                .size(px(28.0))
                                .flex_none()
                                .rounded(px(self.tokens.radii.sm))
                                .border_1()
                                .border_color(rgba((theme.border << 8) | 0x66))
                                .bg(fill(theme.bg_sunken))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(glyph(
                                    if is_ble {
                                        LucideIcon::Bluetooth
                                    } else if matches!(
                                        row.target,
                                        SessionManagerOpenTarget::StandaloneSftp(_)
                                    ) {
                                        LucideIcon::FolderSync
                                    } else {
                                        LucideIcon::Server
                                    },
                                    14.0,
                                    theme.text_secondary,
                                ))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .line_height(px(metrics.sm_line_height))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .child(row.name.clone()),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .line_height(px(metrics.xs_line_height))
                                        .text_color(rgb(theme.text_muted))
                                        .child(row.subtitle.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .rounded(px(self.tokens.radii.xs))
                                        .border_1()
                                        .border_color(rgba((theme.border << 8) | 0x66))
                                        .bg(fill(theme.bg_sunken))
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .font_family("Lilex")
                                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(if crate::ble::is_ble_node(&node_id) {
                                            "BLE".to_string()
                                        } else if matches!(
                                            row.target,
                                            SessionManagerOpenTarget::StandaloneSftp(_)
                                        ) {
                                            self.i18n.t("sidebar.panels.sftp")
                                        } else {
                                            self.i18n.t("terminal.typeSsh")
                                        }),
                                )
                                .child(glyph(LucideIcon::ChevronRight, 13.0, theme.text_muted)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |h, _, w, cx| {
                                h.connect_catalog_item(&node_id, w, cx);
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
            container.child(list)
        };

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .w_full()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("layout.empty.recent_connections")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(count_label),
                    ),
            )
            .child(body)
    }

    fn render_welcome_guidance(&self, cx: &Context<Self>) -> Option<Div> {
        let theme = self.tokens.ui;
        let has_background = self.window_wallpaper().is_some();
        let alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let fill =
            move |color| oxideterm_gpui_ui::color_for_background(color, has_background, alpha);
        let metrics = welcome_workbench_metrics(&self.tokens);
        let inner_radius = rounded_shell_child_radius(self.tokens.radii.lg);

        let primary_actions = [
            (
                LucideIcon::Plus,
                "layout.empty.new_connection",
                "layout.empty.new_connection_hint",
                Action::NewConnection,
                true,
            ),
            (
                LucideIcon::Terminal,
                if self.demo {
                    "layout.empty.new_local_terminal"
                } else {
                    "menu.new_terminal"
                },
                if self.demo {
                    "layout.empty.new_local_terminal_hint"
                } else {
                    "onboarding.gateway_terminal_desc"
                },
                Action::Terminal,
                false,
            ),
        ];

        let utility_actions = [
            (
                LucideIcon::FolderInput,
                "layout.empty.import_connections",
                "settings_view.connections.ssh_config.pick_file",
                Action::ImportConnections,
            ),
            (
                LucideIcon::LayoutList,
                "layout.empty.open_session_manager",
                "layout.empty.open_session_manager_hint",
                Action::Catalog,
            ),
            (
                LucideIcon::Cloud,
                "plugin.cloud_sync.panel_title",
                "plugin.cloud_sync.browser_preview",
                Action::CloudSync,
            ),
        ];

        let mut utility_list = div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(fill(welcome_card_background(theme)))
            .overflow_hidden()
            .flex()
            .flex_col();
        let utility_actions: Vec<_> = utility_actions
            .into_iter()
            .filter(|(_, _, _, a)| self.action_available(*a, cx))
            .collect();
        let primary_actions: Vec<_> = primary_actions
            .into_iter()
            .filter(|(_, _, _, action, _)| self.action_available(*action, cx))
            .collect();
        if primary_actions.is_empty() && utility_actions.is_empty() {
            return None;
        }
        let primary_columns = primary_actions.len().max(1);
        let utility_count = utility_actions.len();
        for (index, (icon, title_key, hint_key, action)) in utility_actions.into_iter().enumerate()
        {
            if index > 0 {
                utility_list =
                    utility_list.child(div().h(px(1.0)).bg(rgba((theme.border << 8) | 0x45)));
            }
            let is_first = index == 0;
            let is_last = index + 1 == utility_count;
            let row = div()
                .w_full()
                .min_w_0()
                .h(px(metrics.utility_row_height))
                .px(px(12.0))
                .when(is_first, |r| r.rounded_t(px(inner_radius)))
                .when(is_last, |r| r.rounded_b(px(inner_radius)))
                .flex()
                .items_center()
                .gap(px(10.0))
                .cursor_pointer()
                .hover(move |r| {
                    r.bg(oxideterm_gpui_ui::color_for_background(
                        theme.bg_hover,
                        has_background,
                        0x80,
                    ))
                })
                .child(glyph(icon, 14.0, theme.text_muted))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .line_height(px(metrics.sm_line_height))
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t(title_key)),
                )
                .child(glyph(LucideIcon::ChevronRight, 13.0, theme.text_muted));
            utility_list = utility_list.child(self.bind_button(
                row,
                SharedString::from(format!("welcome-utility-{title_key}")),
                self.i18n.t(hint_key),
                None,
                cx,
                move |h, w, cx| h.action(action, w, cx),
            ));
        }

        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div().w_full().h(px(20.0)).flex().items_center().child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("layout.empty.get_started")),
                    ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(metrics.guidance_stack_gap))
                        .child(
                            div()
                                .w_full()
                                .grid()
                                .grid_cols(primary_columns as u16)
                                .gap(px(10.0))
                                .children(primary_actions.into_iter().map(
                                    |(icon, title_key, hint_key, action, primary)| {
                                        let tile = div()
                                            .w_full()
                                            .min_w_0()
                                            .h(px(metrics.primary_tile_height))
                                            .rounded(px(self.tokens.radii.lg))
                                            .border_1()
                                            .border_color(if primary {
                                                rgba((theme.accent << 8) | 0x80)
                                            } else {
                                                rgba((theme.border << 8) | 0x80)
                                            })
                                            .bg(fill(welcome_card_background(theme)))
                                            .px(px(12.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(10.0))
                                            .cursor_pointer()
                                            .hover(move |tile| {
                                                tile.bg(oxideterm_gpui_ui::color_for_background(
                                                    theme.bg_hover,
                                                    has_background,
                                                    0x80,
                                                ))
                                            })
                                            .child(
                                                div()
                                                    .size(px(28.0))
                                                    .flex_none()
                                                    .rounded(px(self.tokens.radii.sm))
                                                    .when(!primary, |badge| {
                                                        badge
                                                            .border_1()
                                                            .border_color(rgba(
                                                                (theme.border << 8) | 0x66,
                                                            ))
                                                            .bg(fill(theme.bg_sunken))
                                                    })
                                                    .when(primary, |badge| {
                                                        badge.bg(rgb(theme.accent))
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(glyph(
                                                        icon,
                                                        15.0,
                                                        if primary {
                                                            theme.accent_text
                                                        } else {
                                                            theme.text
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .truncate()
                                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                                    .line_height(px(metrics.sm_line_height))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(theme.text))
                                                    .child(self.i18n.t(title_key)),
                                            );
                                        self.bind_button(
                                            tile,
                                            SharedString::from(format!(
                                                "welcome-primary-{title_key}"
                                            )),
                                            self.i18n.t(hint_key),
                                            None,
                                            cx,
                                            move |h, w, cx| h.action(action, w, cx),
                                        )
                                    },
                                )),
                        )
                        .when(utility_count > 0, |stack| stack.child(utility_list)),
                ),
        )
    }

    fn render_welcome_shortcuts(&self, cx: &App) -> AnyElement {
        const WELCOME_SHORTCUTS: [(&str, &str); 6] = [
            ("app.commandPalette", "command_palette.title"),
            ("app.newConnection", "layout.empty.new_connection"),
            ("app.newTerminal", "layout.empty.new_local_terminal"),
            ("app.settings", "command_palette.cmd_settings"),
            ("app.toggleSidebar", "command_palette.cmd_toggle_sidebar"),
            ("app.showShortcuts", "layout.empty.keyboard_shortcuts"),
        ];

        let theme = self.tokens.ui;
        let has_background = self.window_wallpaper().is_some();
        let alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let fill =
            move |color| oxideterm_gpui_ui::color_for_background(color, has_background, alpha);
        div()
            .pt(px(16.0))
            .w_full()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_start()
            .gap_x(px(18.0))
            .gap_y(px(8.0))
            .children(
                WELCOME_SHORTCUTS
                    .into_iter()
                    .filter_map(|(action_id, label_key)| {
                        let label_key = match action_id {
                            "app.newTerminal" if !self.ready => return None,
                            "app.newTerminal" if !self.demo => "menu.new_terminal",
                            _ => label_key,
                        };
                        self.shortcut(action_id, cx).map(|key| {
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(if has_background {
                                    theme.text
                                } else {
                                    theme.text_muted
                                }))
                                .child(
                                    div()
                                        .rounded(px(self.tokens.radii.xs))
                                        .border_1()
                                        .border_color(rgba((theme.border << 8) | 0x80))
                                        .bg(fill(welcome_keycap_background(theme)))
                                        .px(px(5.0))
                                        .py(px(1.5))
                                        .font_family("Lilex")
                                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                                        .line_height(px(14.0))
                                        .text_color(rgb(theme.text))
                                        .child(key),
                                )
                                .child(self.i18n.t(label_key))
                        })
                    }),
            )
            .into_any_element()
    }
}
