impl WorkspaceApp {
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
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    // Keep drag hitboxes away from caption buttons; GPUI
                    // returns the first matching window-control area by paint order.
                    .window_control_area(gpui::WindowControlArea::Drag)
                    // The drag filler is visually empty, so force a concrete
                    // mouse hitbox for client-decoration hit testing.
                    .occlude()
                    // Linux X11/Wayland do not currently consume GPUI
                    // WindowControlArea hit tests, so also start moving from a
                    // normal client-side mouse event.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_this, _event, window, cx| {
                            window.start_window_move();
                            cx.stop_propagation();
                        }),
                    ),
            )
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
        div()
            .w(px(46.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.0))
            .text_color(rgb(text_color))
            .hover(move |button| button.bg(rgb(hover_bg)))
            // Caption buttons are pure GPUI client controls. Keeping
            // WindowControlArea here can route clicks into platform non-client
            // handling before GPUI receives the fallback mouse event.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event, window, cx| {
                    match control_area {
                        gpui::WindowControlArea::Min => window.minimize_window(),
                        gpui::WindowControlArea::Max => window.zoom_window(),
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
