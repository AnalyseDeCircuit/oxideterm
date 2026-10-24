use super::*;
use oxideterm_gpui_ui::button::ButtonVariant;

const TITLEBAR_CONTROL_ICON_SIZE: f32 = 12.0;
const TITLEBAR_CONTROL_WIDTH: f32 = 46.0;

pub(in crate::workspace) fn client_titlebar_button_layout(cx: &App) -> gpui::WindowButtonLayout {
    #[cfg(target_os = "linux")]
    {
        return cx
            .button_layout()
            .unwrap_or_else(gpui::WindowButtonLayout::linux_default);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = cx;
        gpui::WindowButtonLayout {
            left: [None; gpui::MAX_BUTTONS_PER_SIDE],
            right: [
                Some(gpui::WindowButton::Minimize),
                Some(gpui::WindowButton::Maximize),
                Some(gpui::WindowButton::Close),
            ],
        }
    }
}

fn window_button_supported(button: gpui::WindowButton, controls: gpui::WindowControls) -> bool {
    match button {
        gpui::WindowButton::Minimize => controls.minimize,
        gpui::WindowButton::Maximize => controls.maximize,
        gpui::WindowButton::Close => true,
    }
}

fn visible_window_buttons(
    buttons: [Option<gpui::WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
    controls: gpui::WindowControls,
) -> impl Iterator<Item = gpui::WindowButton> {
    buttons
        .into_iter()
        .flatten()
        .filter(move |button| window_button_supported(*button, controls))
}

fn activate_client_titlebar_control(
    control_area: gpui::WindowControlArea,
    window: &mut Window,
    cx: &mut App,
) {
    match control_area {
        gpui::WindowControlArea::Min => window.minimize_window(),
        gpui::WindowControlArea::Max => {
            if window.is_fullscreen() {
                window.toggle_fullscreen();
            } else {
                window.zoom_window();
            }
        }
        gpui::WindowControlArea::Close => {
            // The close guard updates the same workspace leased by the button listener.
            window.defer(cx, |window, cx| window.request_close(cx));
        }
        gpui::WindowControlArea::Drag => {}
    }
}

pub(in crate::workspace) fn handle_window_drag_mouse_down(
    event: &MouseDownEvent,
    window: &Window,
) -> bool {
    if event.click_count == 2 {
        if cfg!(target_os = "macos") {
            // AppKit applies the user's configured titlebar double-click action.
            window.titlebar_double_click();
            return true;
        }
        if cfg!(target_os = "linux") && window.window_controls().maximize && window.is_resizable() {
            window.zoom_window();
            return true;
        }
    }

    window.start_window_move();
    false
}

fn window_titlebar_visibility(
    is_linux: bool,
    is_fullscreen: bool,
    show_window_titlebar: bool,
) -> bool {
    !is_fullscreen && (!is_linux || show_window_titlebar)
}

#[derive(Clone, Copy)]
pub(in crate::workspace) enum ClientTitlebarIcon {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl ClientTitlebarIcon {
    fn for_button(button: gpui::WindowButton, is_maximized: bool) -> Self {
        match button {
            gpui::WindowButton::Minimize => Self::Minimize,
            gpui::WindowButton::Maximize if is_maximized => Self::Restore,
            gpui::WindowButton::Maximize => Self::Maximize,
            gpui::WindowButton::Close => Self::Close,
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Minimize => "window-controls/minimize.svg",
            Self::Maximize => "window-controls/maximize.svg",
            Self::Restore => "window-controls/restore.svg",
            Self::Close => "window-controls/close.svg",
        }
    }

    fn ids(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Minimize => (
                "titlebar-control-minimize",
                "titlebar-control-minimize-icon",
                "titlebar-control-minimize-group",
            ),
            Self::Maximize => (
                "titlebar-control-maximize",
                "titlebar-control-maximize-icon",
                "titlebar-control-maximize-group",
            ),
            Self::Restore => (
                "titlebar-control-restore",
                "titlebar-control-restore-icon",
                "titlebar-control-restore-group",
            ),
            Self::Close => (
                "titlebar-control-close",
                "titlebar-control-close-icon",
                "titlebar-control-close-group",
            ),
        }
    }

    fn accessibility_key(self) -> &'static str {
        match self {
            Self::Minimize => "common.window_controls.minimize",
            Self::Maximize => "common.window_controls.maximize",
            Self::Restore => "common.window_controls.restore",
            Self::Close => "common.window_controls.close",
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn window_titlebar_visible(&self, window: &Window) -> bool {
        !self.settings_store.settings().sidebar_ui.zen_mode
            && (window.is_fullscreen() || self.window_titlebar_controls_visible(window))
    }

    pub(in crate::workspace) fn window_titlebar_controls_visible(&self, window: &Window) -> bool {
        window_titlebar_visibility(
            cfg!(target_os = "linux"),
            window.is_fullscreen(),
            self.settings_store
                .settings()
                .appearance
                .show_window_titlebar,
        )
    }

    pub(in crate::workspace) fn window_titlebar_height(&self, window: &Window) -> f32 {
        if self.window_titlebar_visible(window) {
            self.tokens.metrics.titlebar_height
        } else {
            0.0
        }
    }

    pub(in crate::workspace) fn render_window_drag_region(
        &self,
        element_id: impl Into<gpui::ElementId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(element_id)
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            // Keep window dragging limited to inert top-chrome filler. Caption
            // buttons, tabs, resize handles, terminal content, and input fields
            // must stay outside or normal app interaction will be stolen.
            .window_control_area(gpui::WindowControlArea::Drag)
            // Drag-only chrome can be empty or text-only, so force a concrete
            // mouse hitbox for client-decoration hit testing on every platform.
            .occlude()
            // Windows consumes this through non-client HTCAPTION handling. A
            // handled GPUI mouse-down would suppress the native move operation.
            .when(!cfg!(target_os = "windows"), |region| {
                region.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, event, window, cx| {
                        handle_window_drag_mouse_down(event, window);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_window_drag_content_region(
        &self,
        element_id: impl Into<gpui::ElementId>,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(element_id)
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            // Only use this for non-interactive top-chrome title content. Do not
            // wrap buttons, tabs, resize handles, terminal content, or inputs.
            .window_control_area(gpui::WindowControlArea::Drag)
            .occlude()
            // Windows titlebar movement is owned by HTCAPTION; keep the manual
            // compositor move path for platforms where GPUI exposes it.
            .when(!cfg!(target_os = "windows"), |region| {
                region.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, event, window, cx| {
                        handle_window_drag_mouse_down(event, window);
                        cx.stop_propagation();
                    }),
                )
            })
            .child(content)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_title_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Tauri does not draw a separate accent-tinted top strip; its transparent
        // macOS chrome sits over the app root background. Native still needs this
        // drag area for traffic lights, so keep it visually merged with theme.bg.
        let titlebar_bg = theme.bg;
        let titlebar_border = theme.border;
        let text_color = readable_color(titlebar_bg, theme.text_muted, theme.text);
        let button_layout = client_titlebar_button_layout(cx);
        let supported_controls = window.window_controls();
        let show_client_controls = cfg!(any(target_os = "windows", target_os = "linux"))
            && self.window_titlebar_controls_visible(window);
        let show_native_controls = cfg!(target_os = "macos") && !window.is_fullscreen();
        let action_button_size = self.tokens.metrics.titlebar_height - 6.0;
        let action_button_padding = self.tokens.spacing.one / 2.0;
        let action_button_width = action_button_size + action_button_padding * 2.0;
        let left_controls_width = if show_native_controls {
            72.0
        } else if show_client_controls {
            visible_window_buttons(button_layout.left, supported_controls).count() as f32
                * TITLEBAR_CONTROL_WIDTH
        } else {
            0.0
        };
        let right_controls_width = if show_client_controls {
            visible_window_buttons(button_layout.right, supported_controls).count() as f32
                * TITLEBAR_CONTROL_WIDTH
        } else {
            0.0
        };
        let background_count = self.background_task_count(cx);
        let background_count_label = background_count.to_string();
        let background_width = if background_count > 0 {
            action_button_width
                + background_count_label.len() as f32 * self.tokens.metrics.titlebar_label_font_size
        } else {
            0.0
        };
        let scope_filtered = *self.tab_host.read(cx).scope() != tabs::TabScope::All;
        let show_all_label = self.i18n.t("sidebar.tab_scope.show_all");
        let show_all_width = if scope_filtered {
            self.tab_text_width(&show_all_label, self.tokens.metrics.ui_text_sm, window)
                + self.tokens.metrics.ui_control_padding_x * 2.0
        } else {
            0.0
        };
        let side_min_width = left_controls_width.max(
            right_controls_width + action_button_width * 3.0 + background_width + show_all_width,
        );
        let scope_available_width = f32::from(window.viewport_size().width)
            - side_min_width * 2.0
            - self.tokens.spacing.two * 2.0;
        let background_label = self
            .i18n
            .t("sidebar.background_tasks.summary")
            .replace("{{count}}", &background_count_label);
        let background_workspace = cx.entity().downgrade();
        let zen_label = self.i18n.t("sidebar.actions.enter_zen");
        let zen_tooltip = tabs::effective_shortcut_label(
            "app.zenMode",
            &self.settings_store.settings().keybindings.overrides,
        )
        .map_or_else(
            || zen_label.clone(),
            |shortcut| format!("{zen_label} ({shortcut})"),
        );
        let zen_workspace = cx.entity().downgrade();
        let palette_label = self.i18n.t("command_palette.title");
        let palette_tooltip = tabs::effective_shortcut_label(
            "app.commandPalette",
            &self.settings_store.settings().keybindings.overrides,
        )
        .map_or_else(
            || palette_label.clone(),
            |shortcut| format!("{palette_label} ({shortcut})"),
        );
        let palette_workspace = cx.entity().downgrade();
        let scheme = self
            .settings_store
            .settings()
            .appearance
            .active_scheme(self.system_dark);
        let dark = scheme.is_some_and(oxideterm_settings::AppearanceScheme::is_dark);
        let theme_label = self.i18n.t(match scheme {
            None => "settings_view.appearance.configure_schemes",
            Some(_) if dark => "sidebar.tooltips.switch_to_light",
            Some(_) => "sidebar.tooltips.switch_to_dark",
        });
        let theme_toggle_workspace = cx.entity().downgrade();

        div()
            .w_full()
            .h(px(self.tokens.metrics.titlebar_height))
            .flex()
            .flex_row()
            .items_center()
            .bg(self.workspace_chrome_background(titlebar_bg))
            .border_b_1()
            .border_color(self.workspace_divider_color(titlebar_border, 0xff))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(text_color))
            // Equal side regions keep the selector centered in the window,
            // regardless of which side owns the native caption controls.
            .child(
                div()
                    .flex_1()
                    .min_w(px(side_min_width))
                    .h_full()
                    .flex()
                    .when(show_native_controls, |side| {
                        side.child(div().w(px(72.0)).h_full().flex_none())
                    })
                    .when(show_client_controls && cfg!(target_os = "linux"), |side| {
                        side.child(self.render_client_titlebar_controls(
                            button_layout.left,
                            supported_controls,
                            titlebar_bg,
                            text_color,
                            window.is_maximized(),
                            cx,
                        ))
                    })
                    .child(self.render_window_drag_region("workspace-titlebar-drag-left", cx)),
            )
            .child(self.render_tab_scope_trigger(window, scope_available_width, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(side_min_width))
                    .h_full()
                    .flex()
                    .items_center()
                    .when(scope_filtered, |side| {
                        let workspace = cx.entity().downgrade();
                        side.child(
                            oxideterm_gpui_ui::button::button_with(
                                &self.tokens,
                                show_all_label.clone(),
                                oxideterm_gpui_ui::button::ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            )
                            .id("titlebar-show-all-tabs")
                            .w(px(show_all_width))
                            .h(px(action_button_size))
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .role(gpui::Role::Button)
                            .aria_label(show_all_label.clone())
                            .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                                let _ = workspace.update(cx, |this, cx| {
                                    this.select_tab_scope(tabs::TabScope::All, window, cx);
                                });
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.select_tab_scope(tabs::TabScope::All, window, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                    })
                    .child(self.render_window_drag_region("workspace-titlebar-drag-right", cx))
                    .when(background_count > 0, |side| {
                        side.child(
                            div()
                                .id("titlebar-background-tasks-button")
                                .flex_none()
                                .flex()
                                .items_center()
                                .px(px(action_button_padding))
                                .role(gpui::Role::Button)
                                .aria_label(background_label.clone())
                                .cursor_pointer()
                                .on_a11y_action(
                                    gpui::AccessibleAction::Click,
                                    move |_, window, cx| {
                                        let _ = background_workspace.update(cx, |this, cx| {
                                            this.open_background_task_picker(window, cx);
                                        });
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.open_background_task_picker(window, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(self.workspace_tooltip_icon_button(
                                    LucideIcon::ListChecks,
                                    self.tokens.metrics.sidebar_action_icon_size,
                                    rgb(text_color),
                                    IconButtonOptions::opaque_toolbar(
                                        action_button_size,
                                        ButtonRadius::Md,
                                    ),
                                    background_label.clone(),
                                    "titlebar-background-tasks",
                                    true,
                                    cx.listener(move |this, _, window, cx| {
                                        this.clear_workspace_tooltip(&background_label, cx);
                                        this.open_background_task_picker(window, cx);
                                        cx.stop_propagation();
                                    }),
                                    cx.entity(),
                                ))
                                .child(background_count_label),
                        )
                    })
                    .child(
                        div()
                            .id("titlebar-command-palette-button")
                            .flex_none()
                            .px(px(action_button_padding))
                            .role(gpui::Role::Button)
                            .aria_label(palette_label)
                            .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                                let _ = palette_workspace.update(cx, |this, cx| {
                                    this.open_command_palette(window, cx);
                                });
                            })
                            .child(self.workspace_tooltip_icon_button(
                                LucideIcon::Search,
                                self.tokens.metrics.sidebar_action_icon_size,
                                rgb(text_color),
                                IconButtonOptions::opaque_toolbar(
                                    action_button_size,
                                    ButtonRadius::Md,
                                ),
                                palette_tooltip.clone(),
                                "titlebar-command-palette",
                                true,
                                cx.listener(move |this, _, window, cx| {
                                    this.clear_workspace_tooltip(&palette_tooltip, cx);
                                    this.open_command_palette(window, cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            )),
                    )
                    .child(
                        div()
                            .id("titlebar-zen-button")
                            .flex_none()
                            .px(px(action_button_padding))
                            .role(gpui::Role::Button)
                            .aria_label(zen_label)
                            .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
                                let _ =
                                    zen_workspace.update(cx, |this, cx| this.toggle_zen_mode(cx));
                            })
                            .child(self.workspace_tooltip_icon_button(
                                LucideIcon::EyeOff,
                                self.tokens.metrics.sidebar_action_icon_size,
                                rgb(text_color),
                                IconButtonOptions::opaque_toolbar(
                                    action_button_size,
                                    ButtonRadius::Md,
                                ),
                                zen_tooltip.clone(),
                                "titlebar-zen",
                                true,
                                cx.listener(move |this, _, _, cx| {
                                    this.clear_workspace_tooltip(&zen_tooltip, cx);
                                    this.toggle_zen_mode(cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            )),
                    )
                    .child(
                        div()
                            .id("titlebar-theme-button")
                            .flex_none()
                            .px(px(action_button_padding))
                            .role(gpui::Role::Button)
                            .aria_label(theme_label.clone())
                            .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
                                let _ = theme_toggle_workspace.update(cx, |this, cx| {
                                    this.toggle_titlebar_theme(cx);
                                });
                            })
                            .child(self.workspace_tooltip_icon_button(
                                if dark {
                                    LucideIcon::Moon
                                } else {
                                    LucideIcon::Sun
                                },
                                self.tokens.metrics.sidebar_action_icon_size,
                                rgb(text_color),
                                IconButtonOptions {
                                    disabled: self.theme_transition_task.is_some(),
                                    ..IconButtonOptions::opaque_toolbar(
                                        action_button_size,
                                        ButtonRadius::Md,
                                    )
                                },
                                theme_label.clone(),
                                "titlebar-theme-toggle",
                                true,
                                cx.listener(move |this, _, _, cx| {
                                    this.clear_workspace_tooltip(&theme_label, cx);
                                    this.toggle_titlebar_theme(cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            )),
                    )
                    .when(show_client_controls, |side| {
                        side.child(self.render_client_titlebar_controls(
                            button_layout.right,
                            supported_controls,
                            titlebar_bg,
                            text_color,
                            window.is_maximized(),
                            cx,
                        ))
                    }),
            )
            .when(
                cfg!(target_os = "linux") && supported_controls.window_menu,
                |bar| {
                    bar.on_mouse_down(MouseButton::Right, |event, window, cx| {
                        window.show_window_menu(event.position);
                        cx.stop_propagation();
                    })
                },
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_client_titlebar_controls(
        &self,
        buttons: [Option<gpui::WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
        supported_controls: gpui::WindowControls,
        titlebar_bg: u32,
        text_color: u32,
        is_maximized: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h_full()
            .flex_none()
            .flex()
            .flex_row()
            .children(
                visible_window_buttons(buttons, supported_controls).map(|button| {
                    let icon = ClientTitlebarIcon::for_button(button, is_maximized);
                    let (control_area, hover_bg, hover_text_color) = match button {
                        gpui::WindowButton::Minimize => (
                            gpui::WindowControlArea::Min,
                            titlebar_button_hover(titlebar_bg),
                            text_color,
                        ),
                        gpui::WindowButton::Maximize => (
                            gpui::WindowControlArea::Max,
                            titlebar_button_hover(titlebar_bg),
                            text_color,
                        ),
                        gpui::WindowButton::Close => {
                            (gpui::WindowControlArea::Close, 0xc42b1c, 0xffffff)
                        }
                    };
                    self.client_titlebar_button(
                        icon,
                        control_area,
                        hover_bg,
                        text_color,
                        hover_text_color,
                        cx,
                    )
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn client_titlebar_button(
        &self,
        icon: ClientTitlebarIcon,
        control_area: gpui::WindowControlArea,
        hover_bg: u32,
        text_color: u32,
        hover_text_color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let use_native_caption_hit_test = cfg!(target_os = "windows");
        let (button_id, icon_id, group_id) = icon.ids();
        let accessibility_label = self.i18n.t(icon.accessibility_key());
        let pressed_bg = mix_rgb(hover_bg, 0x000000, 0.18);

        div()
            .group(group_id)
            .id(button_id)
            .role(gpui::Role::Button)
            .aria_label(accessibility_label)
            .occlude()
            .w(px(TITLEBAR_CONTROL_WIDTH))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(text_color))
            // The close icon stays theme-readable at rest and turns white
            // only against its destructive hover background.
            .hover(move |button| button.bg(rgb(hover_bg)).text_color(rgb(hover_text_color)))
            .active(move |button| button.bg(rgb(pressed_bg)).text_color(rgb(hover_text_color)))
            // Native Windows caption hit testing routes pointer movement through
            // WM_NCMOUSEMOVE. Force a view refresh so moving directly from one
            // caption button to the next cannot leave the previous hover paint.
            .when(use_native_caption_hit_test, |button| {
                button.on_mouse_move(cx.listener(|_this, _event, _window, cx| cx.notify()))
            })
            // Windows owns caption buttons through non-client HT* hit testing;
            // stopping the GPUI mouse event would prevent minimize/restore.
            .when(use_native_caption_hit_test, |button| {
                button.window_control_area(control_area).on_a11y_action(
                    gpui::AccessibleAction::Click,
                    move |_data, window, cx| {
                        activate_client_titlebar_control(control_area, window, cx);
                    },
                )
            })
            // Keep a GPUI fallback for platforms where titlebar buttons are
            // rendered client-side without native caption hit testing.
            .when(!use_native_caption_hit_test, |button| {
                button.on_click(cx.listener(move |_this, _event, window, cx| {
                    activate_client_titlebar_control(control_area, window, cx);
                    cx.stop_propagation();
                }))
            })
            .child(
                svg()
                    .path(icon.path())
                    .size(px(TITLEBAR_CONTROL_ICON_SIZE))
                    .text_color(rgb(text_color))
                    .group_hover(group_id, move |icon| icon.text_color(rgb(hover_text_color)))
                    .id(icon_id),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    struct CloseGuardView {
        allow_close: bool,
        requests: Rc<Cell<usize>>,
    }

    impl gpui::Render for CloseGuardView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    #[gpui::test]
    fn client_close_waits_for_owner_update_and_honors_the_close_guard(
        cx: &mut gpui::TestAppContext,
    ) {
        let requests = Rc::new(Cell::new(0));
        let view_requests = requests.clone();
        let handle = cx.add_window(move |window, cx| {
            let owner: gpui::Entity<CloseGuardView> = cx.entity();
            window.on_window_should_close(cx, move |_, cx| {
                owner.update(cx, |owner, _| {
                    owner.requests.set(owner.requests.get() + 1);
                    owner.allow_close
                })
            });
            CloseGuardView {
                allow_close: false,
                requests: view_requests,
            }
        });
        handle
            .update(cx, |_, window, cx| {
                activate_client_titlebar_control(gpui::WindowControlArea::Close, window, cx);
                assert_eq!(
                    requests.get(),
                    0,
                    "close guard must run after the owner update"
                );
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(requests.get(), 1);
        handle
            .update(cx, |owner, window, cx| {
                owner.allow_close = true;
                activate_client_titlebar_control(gpui::WindowControlArea::Close, window, cx);
                assert_eq!(requests.get(), 1);
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(requests.get(), 2);
        assert!(cx.read(|cx| {
            cx.windows()
                .iter()
                .all(|window| window.window_id() != handle.window_id())
        }));
    }

    #[test]
    fn window_control_capabilities_filter_optional_buttons_only() {
        let controls = gpui::WindowControls {
            fullscreen: true,
            maximize: false,
            minimize: false,
            window_menu: true,
        };

        assert!(!window_button_supported(
            gpui::WindowButton::Minimize,
            controls
        ));
        assert!(!window_button_supported(
            gpui::WindowButton::Maximize,
            controls
        ));
        assert!(window_button_supported(gpui::WindowButton::Close, controls));
    }

    #[test]
    fn visible_window_controls_preserve_desktop_order() {
        let configured = [
            Some(gpui::WindowButton::Close),
            Some(gpui::WindowButton::Maximize),
            Some(gpui::WindowButton::Minimize),
        ];
        let controls = gpui::WindowControls {
            fullscreen: true,
            maximize: false,
            minimize: true,
            window_menu: true,
        };

        assert_eq!(
            visible_window_buttons(configured, controls).collect::<Vec<_>>(),
            vec![gpui::WindowButton::Close, gpui::WindowButton::Minimize]
        );
    }
}
