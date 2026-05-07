impl WorkspaceApp {
    pub(super) fn persist_sidebar_settings(&mut self) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.sidebar_collapsed;
        self.settings_store.settings_mut().sidebar_ui.width = self.sidebar_width.round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section =
            self.active_sidebar_section.as_settings_key().to_string();
        let _ = self.settings_store.save();
    }

    pub(super) fn set_sidebar_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        self.active_sidebar_section = section;
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

    pub(super) fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.sidebar_width = width.clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        cx.notify();
    }

    pub(super) fn start_sidebar_resize(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.sidebar_resizing = true;
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    pub(super) fn update_sidebar_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.sidebar_resizing {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    pub(super) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            self.persist_sidebar_settings();
            cx.notify();
        }
    }
}
