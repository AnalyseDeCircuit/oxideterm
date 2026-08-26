use super::*;

const TITLEBAR_CONTROL_ICON_SIZE: f32 = 12.0;

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
        gpui::WindowControlArea::Close => window.request_close(cx),
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

        div()
            .w_full()
            .h(px(self.tokens.metrics.titlebar_height))
            .flex()
            .flex_row()
            .items_center()
            .bg(self.workspace_chrome_background(titlebar_bg))
            .border_b_1()
            .border_color(rgb(titlebar_border))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(text_color))
            .when(cfg!(target_os = "linux"), |bar| {
                bar.child(self.render_client_titlebar_controls(
                    button_layout.left,
                    supported_controls,
                    titlebar_bg,
                    text_color,
                    window.is_maximized(),
                    cx,
                ))
            })
            .child(self.render_window_drag_region("workspace-titlebar-drag-region", cx))
            .when(
                cfg!(any(target_os = "windows", target_os = "linux")),
                |bar| {
                    bar.child(self.render_client_titlebar_controls(
                        button_layout.right,
                        supported_controls,
                        titlebar_bg,
                        text_color,
                        window.is_maximized(),
                        cx,
                    ))
                },
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
            .w(px(46.0))
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
