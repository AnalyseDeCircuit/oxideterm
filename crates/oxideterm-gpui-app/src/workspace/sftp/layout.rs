use super::*;

fn adjusted_sftp_pane_ratio(start_ratio: f32, delta_x: f32, viewport_width: f32) -> f32 {
    // Store the split as a ratio so it remains useful when the window or sidebars resize.
    let delta_ratio = delta_x / viewport_width.max(1.0);
    (start_ratio + delta_ratio).clamp(SFTP_PANE_SPLIT_MIN_RATIO, SFTP_PANE_SPLIT_MAX_RATIO)
}

fn adjusted_sftp_queue_height(start_height: f32, delta_y: f32, viewport_height: f32) -> f32 {
    // Moving the divider upward grows the queue, so vertical pointer delta is inverted.
    let max_height = (viewport_height * SFTP_QUEUE_MAX_VIEWPORT_RATIO).max(SFTP_QUEUE_MIN_HEIGHT);
    (start_height - delta_y).clamp(SFTP_QUEUE_MIN_HEIGHT, max_height)
}

impl WorkspaceApp {
    fn sftp_pane_layout_width(&self, window: &Window, cx: &App) -> f32 {
        let zen_mode = self.settings_store.settings().sidebar_ui.zen_mode;
        let mut width = f32::from(window.viewport_size().width);
        if !zen_mode {
            width -= self.tokens.metrics.activity_bar_width;
            if self.sidebar_rendered {
                width -= self.sidebar_panel_width();
            }
            if self.context_sidebar_rendered {
                width -= self.ai_entity.read(cx).chat_ui().sidebar_width;
            }
        }
        // The split ratio is applied inside the SFTP root padding.
        (width - SFTP_ROOT_PADDING * 2.0).max(1.0)
    }

    pub(in crate::workspace) fn sftp_queue_height_for_window(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        adjusted_sftp_queue_height(
            self.sftp_view.read(cx).queue_height,
            0.0,
            f32::from(window.viewport_size().height),
        )
    }

    pub(in crate::workspace) fn start_sftp_pane_resize(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.pane_resize_drag = Some(SftpPaneResizeDrag {
                start_cursor_x: event.position.x,
                start_ratio: sftp.pane_split_ratio,
            });
            cx.notify();
        });
    }

    pub(in crate::workspace) fn update_sftp_pane_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.sftp_view.read(cx).pane_resize_drag else {
            return;
        };
        if !event.dragging() {
            // A lost mouse-up must not leave the full-window capture layer active.
            self.finish_sftp_pane_resize(cx);
            return;
        }
        let next_ratio = adjusted_sftp_pane_ratio(
            drag.start_ratio,
            f32::from(event.position.x - drag.start_cursor_x),
            self.sftp_pane_layout_width(window, cx),
        );
        self.sftp_view.update(cx, |sftp, cx| {
            if (next_ratio - sftp.pane_split_ratio).abs() >= f32::EPSILON {
                sftp.pane_split_ratio = next_ratio;
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn finish_sftp_pane_resize(&mut self, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            if sftp.pane_resize_drag.take().is_some() {
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn reset_sftp_pane_split(&mut self, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            let ratio_changed =
                (sftp.pane_split_ratio - SFTP_PANE_SPLIT_DEFAULT_RATIO).abs() >= f32::EPSILON;
            let drag_cleared = sftp.pane_resize_drag.take().is_some();
            if ratio_changed || drag_cleared {
                sftp.pane_split_ratio = SFTP_PANE_SPLIT_DEFAULT_RATIO;
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn start_sftp_queue_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let current_height = self.sftp_queue_height_for_window(window, cx);
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.queue_height = current_height;
            sftp.queue_resize_drag = Some(SftpQueueResizeDrag {
                start_cursor_y: event.position.y,
                start_height: current_height,
            });
            cx.notify();
        });
    }

    pub(in crate::workspace) fn update_sftp_queue_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.sftp_view.read(cx).queue_resize_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_sftp_queue_resize(cx);
            return;
        }
        let next_height = adjusted_sftp_queue_height(
            drag.start_height,
            f32::from(event.position.y - drag.start_cursor_y),
            f32::from(window.viewport_size().height),
        );
        self.sftp_view.update(cx, |sftp, cx| {
            if (next_height - sftp.queue_height).abs() >= f32::EPSILON {
                sftp.queue_height = next_height;
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn finish_sftp_queue_resize(&mut self, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            if sftp.queue_resize_drag.take().is_some() {
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn reset_sftp_queue_height(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let default_height = adjusted_sftp_queue_height(
            SFTP_QUEUE_DEFAULT_HEIGHT,
            0.0,
            f32::from(window.viewport_size().height),
        );
        self.sftp_view.update(cx, |sftp, cx| {
            let height_changed = (sftp.queue_height - default_height).abs() >= f32::EPSILON;
            let drag_cleared = sftp.queue_resize_drag.take().is_some();
            if height_changed || drag_cleared {
                sftp.queue_height = default_height;
                cx.notify();
            }
        });
    }
}
