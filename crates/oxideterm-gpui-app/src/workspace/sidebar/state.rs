impl WorkspaceApp {
    pub(super) fn persist_sidebar_settings(&mut self) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.sidebar_collapsed;
        self.settings_store.settings_mut().sidebar_ui.width = self.sidebar_width.round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section =
            self.effective_sidebar_panel_section()
                .as_settings_key()
                .to_string();
        let _ = self.settings_store.save();
    }

    pub(super) fn ai_sidebar_visible(&self) -> bool {
        self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::Assistant
            && self.settings_store.settings().ai.enabled
    }

    pub(super) fn context_sidebar_visible(&self) -> bool {
        let settings = self.settings_store.settings();
        if settings.sidebar_ui.ai_sidebar_collapsed || settings.sidebar_ui.zen_mode {
            return false;
        }
        match self.active_context_sidebar_panel {
            ContextSidebarPanel::Assistant => settings.ai.enabled,
            ContextSidebarPanel::HostTools => true,
        }
    }

    pub(super) fn set_sidebar_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        self.clear_ai_sidebar_keyboard_focus();
        self.active_sidebar_section = section;
        if section == SidebarSection::Extensions {
            self.bootstrap_native_plugin_runtime(cx);
        }
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_resizing = false;
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn sidebar_panel_width(&self) -> f32 {
        (self.sidebar_width - self.tokens.metrics.activity_bar_width).max(0.0)
    }

    pub(super) fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) -> bool {
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

    pub(super) fn start_sidebar_resize(
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

    pub(super) fn update_sidebar_resize(
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

    pub(super) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            self.persist_sidebar_settings();
            cx.notify();
        }
    }

    fn sidebar_width_from_cursor(&self, cursor_x: Pixels, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        f32::from(cursor_x).clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens
                .metrics
                .sidebar_max_width
                .min(window_width.max(self.tokens.metrics.sidebar_min_width)),
        )
    }

    pub(super) fn toggle_ai_sidebar(&mut self, cx: &mut Context<Self>) -> bool {
        if self.ai_sidebar_visible() {
            self.collapse_context_sidebar(cx);
            return true;
        }
        self.open_context_sidebar_panel(ContextSidebarPanel::Assistant, cx)
    }

    pub(super) fn open_context_sidebar_panel(
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

    pub(super) fn collapse_context_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = true;
        self.ai_sidebar_resizing = false;
        self.clear_ai_sidebar_keyboard_focus();
        self.close_ai_sidebar_popovers();
        let _ = self.settings_store.save();
        cx.notify();
    }

    pub(super) fn set_ai_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) -> bool {
        let next_width = width.clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH);
        if (next_width - self.ai_sidebar_width).abs() < f32::EPSILON {
            return false;
        }
        // Same repaint contract as the main sidebar: pointer capture may keep
        // sending moves after the width is clamped at a boundary.
        self.ai_sidebar_width = next_width;
        cx.notify();
        true
    }

    pub(super) fn start_ai_sidebar_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let was_resizing = self.ai_sidebar_resizing;
        self.ai_sidebar_resizing = true;
        // Mirror the browser sidebar: the first press updates the width from
        // the pointer position so a resize drag is visible before the next move.
        let width_changed =
            self.set_ai_sidebar_width(self.ai_sidebar_width_from_cursor(event.position.x, window), cx);
        if !was_resizing && !width_changed {
            cx.notify();
        }
    }

    pub(super) fn update_ai_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_sidebar_resizing {
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
        self.set_ai_sidebar_width(self.ai_sidebar_width_from_cursor(event.position.x, window), cx);
    }

    pub(super) fn finish_ai_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.ai_sidebar_resizing {
            self.ai_sidebar_resizing = false;
            self.settings_store
                .settings_mut()
                .sidebar_ui
                .ai_sidebar_width = self.ai_sidebar_width.round() as i64;
            let _ = self.settings_store.save();
            cx.notify();
        }
    }

    fn ai_sidebar_width_from_cursor(&self, cursor_x: Pixels, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        (window_width - f32::from(cursor_x)).clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH)
    }
}
