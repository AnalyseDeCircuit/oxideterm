impl WorkspaceApp {
    pub(super) fn persist_sidebar_settings(&mut self) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.sidebar_collapsed;
        self.settings_store.settings_mut().sidebar_ui.width = self.sidebar_width.round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section =
            self.active_sidebar_section.as_settings_key().to_string();
        let _ = self.settings_store.save();
    }

    pub(super) fn ai_sidebar_visible(&self) -> bool {
        let settings = self.settings_store.settings();
        settings.ai.enabled
            && !settings.sidebar_ui.ai_sidebar_collapsed
            && !settings.sidebar_ui.zen_mode
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

    pub(super) fn start_sidebar_resize(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let was_resizing = self.sidebar_resizing;
        self.sidebar_resizing = true;
        let width_changed = self.set_sidebar_width(f32::from(event.position.x), cx);
        if !was_resizing && !width_changed {
            cx.notify();
        }
    }

    pub(super) fn update_sidebar_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.sidebar_resizing {
            return;
        }
        // The root view acts as our pointer-capture owner. GPUI may report no
        // element-local pressed button once the cursor leaves the narrow resize
        // handle, so the active resize flag is the browser-style capture source
        // of truth until root mouse-up finishes it.
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    pub(super) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            self.persist_sidebar_settings();
            cx.notify();
        }
    }

    pub(super) fn toggle_ai_sidebar(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.sidebar.not_enabled_hint"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return false;
        }
        let collapsed = !self.settings_store.settings().sidebar_ui.ai_sidebar_collapsed;
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = collapsed;
        if !collapsed {
            self.ensure_ai_chat_initialized();
            self.bootstrap_ai_mcp_registry();
        }
        self.clear_ai_sidebar_keyboard_focus();
        let _ = self.settings_store.save();
        cx.notify();
        true
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
        _event: &MouseDownEvent,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_sidebar_resizing {
            self.ai_sidebar_resizing = true;
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
