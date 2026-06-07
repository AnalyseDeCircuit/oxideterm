impl WorkspaceApp {
    pub(super) fn render_window_drag_region(
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
            // Linux X11/Wayland do not consistently consume GPUI
            // WindowControlArea hit tests, so also start moving from a normal
            // client-side mouse event. Windows/macOS tolerate the duplicate path.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, window, cx| {
                    window.start_window_move();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_window_drag_content_region(
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
            .items_center()
            // Only use this for non-interactive top-chrome title content. Do not
            // wrap buttons, tabs, resize handles, terminal content, or inputs.
            .window_control_area(gpui::WindowControlArea::Drag)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, window, cx| {
                    window.start_window_move();
                    cx.stop_propagation();
                }),
            )
            .child(content)
            .into_any_element()
    }

    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        // Tauri does not draw a separate accent-tinted top strip; its transparent
        // macOS chrome sits over the app root background. Native still needs this
        // drag area for traffic lights, so keep it visually merged with theme.bg.
        let titlebar_bg = theme.bg;
        let titlebar_border = theme.border;
        let text_color = readable_color(titlebar_bg, theme.text_muted, theme.text);

        div()
            .h(px(self.tokens.metrics.titlebar_height))
            .flex()
            .flex_row()
            .items_center()
            .bg(rgb(titlebar_bg))
            .border_b_1()
            .border_color(rgb(titlebar_border))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(text_color))
            .child(self.render_window_drag_region("workspace-titlebar-drag-region", cx))
            .when(
                cfg!(any(target_os = "windows", target_os = "linux")),
                |bar| bar.child(self.render_client_titlebar_controls(titlebar_bg, text_color, cx)),
            )
            .into_any_element()
    }

    fn render_client_titlebar_controls(
        &self,
        titlebar_bg: u32,
        text_color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h_full()
            .flex()
            .flex_row()
            .child(self.client_titlebar_button(
                "−",
                gpui::WindowControlArea::Min,
                titlebar_button_hover(titlebar_bg),
                text_color,
                cx,
            ))
            .child(self.client_titlebar_button(
                "□",
                gpui::WindowControlArea::Max,
                titlebar_button_hover(titlebar_bg),
                text_color,
                cx,
            ))
            .child(self.client_titlebar_button(
                "×",
                gpui::WindowControlArea::Close,
                0xc42b1c,
                0xffffff,
                cx,
            ))
            .into_any_element()
    }

    fn client_titlebar_button(
        &self,
        glyph: &'static str,
        control_area: gpui::WindowControlArea,
        hover_bg: u32,
        text_color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let use_windows_native_caption_hit_test = cfg!(target_os = "windows")
            && matches!(
                control_area,
                gpui::WindowControlArea::Min | gpui::WindowControlArea::Max
            );

        div()
            .w(px(46.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.0))
            .text_color(rgb(text_color))
            .hover(move |button| button.bg(rgb(hover_bg)))
            // Windows keeps maximize/restore behavior in native non-client
            // HTMAXBUTTON handling; gpui::Window::zoom_window only maximizes.
            // Keep this limited to Min/Max so the Close button still follows
            // the pure GPUI fallback path and drag regions stay isolated.
            .when(use_windows_native_caption_hit_test, |button| {
                button.window_control_area(control_area)
            })
            // Keep a GPUI fallback for Linux and for any platform path that does
            // not consume the native caption hit test.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event, window, cx| {
                    match control_area {
                        gpui::WindowControlArea::Min => window.minimize_window(),
                        gpui::WindowControlArea::Max => {
                            if window.is_fullscreen() {
                                window.toggle_fullscreen();
                            } else {
                                window.zoom_window();
                            }
                        }
                        gpui::WindowControlArea::Close => window.remove_window(),
                        gpui::WindowControlArea::Drag => {}
                    }
                    cx.stop_propagation();
                }),
            )
            .child(glyph)
            .into_any_element()
    }
}
