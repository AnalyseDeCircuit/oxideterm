use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn set_sidebar_collapsed_with_motion(
        &mut self,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_collapsed = collapsed;
        self.sidebar_motion_generation = self.sidebar_motion_generation.wrapping_add(1);
        let generation = self.sidebar_motion_generation;
        if !collapsed {
            self.sidebar_rendered = true;
            return;
        }
        if !self.tokens.motion.enabled {
            self.sidebar_rendered = false;
            return;
        }
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        // Keep the panel mounted until its closing transition completes.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.sidebar_collapsed && this.sidebar_motion_generation == generation {
                    this.sidebar_rendered = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn set_context_sidebar_rendered_with_motion(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.context_sidebar_motion_generation =
            self.context_sidebar_motion_generation.wrapping_add(1);
        let generation = self.context_sidebar_motion_generation;
        if visible {
            self.context_sidebar_rendered = true;
            return;
        }
        if !self.tokens.motion.enabled {
            self.context_sidebar_rendered = false;
            return;
        }
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        // Delayed unmount makes the right sidebar's collapse animation observable.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if !this.context_sidebar_visible()
                    && this.context_sidebar_motion_generation == generation
                {
                    this.context_sidebar_rendered = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn persist_sidebar_settings(&mut self) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.sidebar_collapsed;
        self.settings_store.settings_mut().sidebar_ui.width = self.sidebar_width.round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section = self
            .effective_sidebar_panel_section()
            .as_settings_key()
            .to_string();
        let _ = self.settings_store.save();
    }

    pub(in crate::workspace) fn ai_sidebar_visible(&self) -> bool {
        self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::Assistant
            && self.settings_store.settings().ai.enabled
    }

    pub(in crate::workspace) fn context_sidebar_visible(&self) -> bool {
        let settings = self.settings_store.settings();
        if settings.sidebar_ui.ai_sidebar_collapsed || settings.sidebar_ui.zen_mode {
            return false;
        }
        match self.active_context_sidebar_panel {
            ContextSidebarPanel::Assistant => settings.ai.enabled,
            ContextSidebarPanel::HostTools => true,
        }
    }

    pub(in crate::workspace) fn set_sidebar_section(
        &mut self,
        section: SidebarSection,
        cx: &mut Context<Self>,
    ) {
        self.clear_ai_sidebar_keyboard_focus();
        self.active_sidebar_section = section;
        if section == SidebarSection::Extensions {
            self.bootstrap_native_plugin_runtime(cx);
        }
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.set_sidebar_collapsed_with_motion(!self.sidebar_collapsed, cx);
        self.sidebar_resizing = false;
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(in crate::workspace) fn sidebar_panel_width(&self) -> f32 {
        (self.sidebar_width - self.tokens.metrics.activity_bar_width).max(0.0)
    }

    pub(in crate::workspace) fn set_sidebar_width(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let next_width = width.clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        if (next_width - self.sidebar_width).abs() < f32::EPSILON {
            return false;
        }
        // Resize mousemove is a high-frequency root-capture path. Repaint only
        // when the clamped browser-style sidebar width actually changes.
        self.sidebar_width = next_width;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn start_sidebar_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let was_resizing = self.sidebar_resizing;
        self.sidebar_resizing = true;
        let width_changed =
            self.set_sidebar_width(self.sidebar_width_from_cursor(event.position.x, window), cx);
        if !was_resizing && !width_changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn update_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.sidebar_resizing {
            return;
        }
        if !event.dragging() {
            // Browser resize handles release as soon as the platform reports
            // the button is no longer down, even if GPUI missed mouse-up.
            self.finish_sidebar_resize(cx);
            return;
        }
        // Match the AI sidebar: root-level movement owns the captured drag,
        // and the visible width is derived from the current window cursor.
        self.set_sidebar_width(self.sidebar_width_from_cursor(event.position.x, window), cx);
    }

    pub(in crate::workspace) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            self.persist_sidebar_settings();
            cx.notify();
        }
    }

    pub(in crate::workspace) fn sidebar_width_from_cursor(
        &self,
        cursor_x: Pixels,
        window: &Window,
    ) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        f32::from(cursor_x).clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens
                .metrics
                .sidebar_max_width
                .min(window_width.max(self.tokens.metrics.sidebar_min_width)),
        )
    }

    pub(in crate::workspace) fn toggle_ai_sidebar(&mut self, cx: &mut Context<Self>) -> bool {
        if self.ai_sidebar_visible() {
            self.collapse_context_sidebar(cx);
            return true;
        }
        self.open_context_sidebar_panel(ContextSidebarPanel::Assistant, cx)
    }

    pub(in crate::workspace) fn open_context_sidebar_panel(
        &mut self,
        panel: ContextSidebarPanel,
        cx: &mut Context<Self>,
    ) -> bool {
        if panel == ContextSidebarPanel::Assistant && !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.sidebar.not_enabled_hint"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return false;
        }

        self.active_context_sidebar_panel = panel;
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = false;
        self.set_context_sidebar_rendered_with_motion(true, cx);
        if panel == ContextSidebarPanel::Assistant {
            self.ensure_ai_chat_initialized();
            self.bootstrap_ai_mcp_registry();
        } else {
            // Non-AI context panels share the old right-sidebar shell, but must
            // not keep AI-specific focus or floating popovers alive.
            self.close_ai_sidebar_popovers();
            self.active_context_sidebar_tool = ContextSidebarTool::Monitor;
            self.refresh_connection_monitor_pool_stats();
            self.sync_connection_monitor_selection(cx);
        }
        self.clear_ai_sidebar_keyboard_focus();
        let _ = self.settings_store.save();
        cx.notify();
        true
    }

    pub(in crate::workspace) fn collapse_context_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = true;
        self.set_context_sidebar_rendered_with_motion(false, cx);
        self.ai.chat.sidebar_resizing = false;
        self.clear_ai_sidebar_keyboard_focus();
        self.close_ai_sidebar_popovers();
        let _ = self.settings_store.save();
        cx.notify();
    }

    pub(in crate::workspace) fn set_ai_sidebar_width(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let next_width = width.clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH);
        if (next_width - self.ai.chat.sidebar_width).abs() < f32::EPSILON {
            return false;
        }
        // Same repaint contract as the main sidebar: pointer capture may keep
        // sending moves after the width is clamped at a boundary.
        self.ai.chat.sidebar_width = next_width;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn start_ai_sidebar_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let was_resizing = self.ai.chat.sidebar_resizing;
        self.ai.chat.sidebar_resizing = true;
        // Mirror the browser sidebar: the first press updates the width from
        // the pointer position so a resize drag is visible before the next move.
        let width_changed = self.set_ai_sidebar_width(
            self.ai_sidebar_width_from_cursor(event.position.x, window),
            cx,
        );
        if !was_resizing && !width_changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn update_ai_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai.chat.sidebar_resizing {
            return;
        }
        if !event.dragging() {
            // Keep both sidebars on the same release contract: a missed
            // mouse-up cannot leave the resize state latched.
            self.finish_ai_sidebar_resize(cx);
            return;
        }
        // Continue from the root capture even after the pointer leaves the AI
        // sidebar edge, matching browser resize handles.
        self.set_ai_sidebar_width(
            self.ai_sidebar_width_from_cursor(event.position.x, window),
            cx,
        );
    }

    pub(in crate::workspace) fn finish_ai_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.ai.chat.sidebar_resizing {
            self.ai.chat.sidebar_resizing = false;
            self.settings_store
                .settings_mut()
                .sidebar_ui
                .ai_sidebar_width = self.ai.chat.sidebar_width.round() as i64;
            let _ = self.settings_store.save();
            cx.notify();
        }
    }

    pub(in crate::workspace) fn ai_sidebar_width_from_cursor(
        &self,
        cursor_x: Pixels,
        window: &Window,
    ) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        ai_sidebar_width_from_cursor_value(f32::from(cursor_x), window_width)
    }
}

pub(in crate::workspace) fn ai_sidebar_width_from_cursor_value(
    cursor_x: f32,
    window_width: f32,
) -> f32 {
    // The context sidebar is anchored to the right edge, so dragging left must
    // increase width and dragging right must decrease width. Keep this math in
    // a pure helper so regressions do not require constructing a GPUI Window.
    (window_width - cursor_x).clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH)
}

#[cfg(test)]
mod sidebar_resize_state_tests {
    use super::*;

    #[test]
    pub(in crate::workspace) fn ai_sidebar_width_from_cursor_uses_right_edge_distance() {
        assert_eq!(ai_sidebar_width_from_cursor_value(700.0, 1000.0), 300.0);
    }

    #[test]
    pub(in crate::workspace) fn ai_sidebar_width_from_cursor_clamps_to_sidebar_limits() {
        assert_eq!(
            ai_sidebar_width_from_cursor_value(995.0, 1000.0),
            AI_SIDEBAR_MIN_WIDTH
        );
        assert_eq!(
            ai_sidebar_width_from_cursor_value(0.0, 2000.0),
            AI_SIDEBAR_MAX_WIDTH
        );
    }
}
