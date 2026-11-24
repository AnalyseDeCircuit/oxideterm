use super::*;

fn should_collapse_context_sidebar_panel(
    sidebar_visible: bool,
    active_panel: ContextSidebarPanel,
    requested_panel: ContextSidebarPanel,
) -> bool {
    sidebar_visible && active_panel == requested_panel
}

fn should_collapse_primary_sidebar_section(
    sidebar_collapsed: bool,
    visible_section: SidebarSection,
    requested_section: SidebarSection,
) -> bool {
    !sidebar_collapsed && visible_section == requested_section
}

pub(in crate::workspace) fn context_sidebar_panel_visible(
    sidebar_collapsed: bool,
    zen_mode: bool,
    ai_enabled: bool,
    active_panel: ContextSidebarPanel,
) -> bool {
    if sidebar_collapsed || zen_mode {
        return false;
    }

    // Host Tools shares the companion sidebar shell, but its visibility must
    // remain independent from the optional AI feature.
    match active_panel {
        ContextSidebarPanel::Assistant => ai_enabled,
        ContextSidebarPanel::HostTools => true,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn set_sidebar_collapsed_with_motion(
        &mut self,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) {
        let was_collapsed = self.rails.left.collapsed();
        let generation = self.rails.left.set_collapsed(collapsed, &self.tokens);
        if !collapsed {
            if was_collapsed && self.session_manager_sidebar_visible() {
                // Defer SSH config scan until the drawer expand spring settles so disk scans never drop frames.
                let settle_delay = oxideterm_gpui_ui::motion::settle_duration(
                    &self.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                );
                cx.spawn(async move |weak, cx| {
                    if !settle_delay.is_zero() {
                        Timer::after(settle_delay).await;
                    }
                    let _ = weak.update(cx, |this, cx| {
                        if !this.rails.left.collapsed()
                            && this.rails.left.generation() == generation
                            && this.session_manager_sidebar_visible()
                        {
                            this.refresh_session_manager_ssh_config_hosts(cx);
                        }
                    });
                })
                .detach();
            }
            return;
        }
        if self.effective_sidebar_panel_section() == SidebarSection::Connections {
            self.close_session_manager_sidebar_search(cx);
        }
        self.schedule_rail_unmount(RailSide::Left, generation, cx);
    }

    fn schedule_rail_unmount(&mut self, side: RailSide, generation: u64, cx: &mut Context<Self>) {
        if !self.rails.rail_mut(side).mounted() {
            return;
        }
        let delay = oxideterm_gpui_ui::motion::exit_settle_duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.rails.rail_mut(side).finish_collapse(generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn persist_sidebar_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.rails.left.collapsed();
        self.settings_store.settings_mut().sidebar_ui.width =
            self.rails.left.width().round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section = self
            .effective_sidebar_panel_section()
            .as_settings_key()
            .to_string();
        self.persist_sidebar_settings_store(cx);
    }

    fn persist_sidebar_settings_store(&mut self, cx: &mut Context<Self>) {
        if self.settings_store.save().is_ok() {
            // Internal writes advance the Entity-owned watcher before its next tick.
            self.settings_workspace.update(cx, |settings, _cx| {
                settings.acknowledge_external_store_state()
            });
        }
    }

    pub(in crate::workspace) fn ai_sidebar_visible(&self) -> bool {
        self.context_sidebar_visible()
            && self.rails.context_panel == ContextSidebarPanel::Assistant
            && self.settings_store.settings().ai.enabled
    }

    pub(in crate::workspace) fn context_sidebar_visible(&self) -> bool {
        let settings = self.settings_store.settings();
        context_sidebar_panel_visible(
            self.rails.right.collapsed(),
            settings.sidebar_ui.zen_mode,
            settings.ai.enabled,
            self.rails.context_panel,
        )
    }

    pub(in crate::workspace) fn context_sidebar_occupied_width(&self) -> f32 {
        if self.settings_store.settings().sidebar_ui.zen_mode {
            return 0.0;
        }
        self.tokens.metrics.activity_bar_width
            + if self.context_sidebar_visible() {
                self.rails.right.width()
            } else {
                0.0
            }
    }

    pub(in crate::workspace) fn sidebar_hosts_embedded_sftp(&self) -> bool {
        // The embedded SFTP surface stacks below whichever navigator panel is
        // already visible: Active Sessions or the Session Manager sidebar form.
        matches!(
            self.effective_sidebar_panel_section(),
            SidebarSection::Sessions | SidebarSection::Connections
        )
    }

    pub(in crate::workspace) fn session_manager_sidebar_visible(&self) -> bool {
        !self.rails.left.collapsed()
            && !self.settings_store.settings().sidebar_ui.zen_mode
            && self.effective_sidebar_panel_section() == SidebarSection::Connections
    }

    pub(in crate::workspace) fn set_sidebar_section(
        &mut self,
        section: SidebarSection,
        cx: &mut Context<Self>,
    ) {
        self.clear_ai_sidebar_keyboard_focus(cx);
        let was_visible = self.session_manager_sidebar_visible();
        let was_collapsed = self.rails.left.collapsed();
        if let Some(transition_gen) = self.rails.select_section(section, &self.tokens) {
            let settle_delay = oxideterm_gpui_ui::motion::settle_duration(
                &self.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            );
            cx.spawn(async move |weak, cx| {
                Timer::after(settle_delay).await;
                let _ = weak.update(cx, |this, cx| {
                    if this.rails.finish_section_transition(transition_gen) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if section == SidebarSection::Extensions {
            self.bootstrap_native_plugin_runtime(cx);
        }
        if self.rails.left.collapsed() {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        // Reopening a collapsed panel already refreshes in the expand path.
        if !was_visible && !was_collapsed && self.session_manager_sidebar_visible() {
            self.refresh_session_manager_ssh_config_hosts(cx);
        }
        if matches!(
            self.rails.section,
            SidebarSection::Sessions | SidebarSection::Connections
        ) {
            self.activate_embedded_sftp_sidebar_if_visible(cx);
        }
        if section != SidebarSection::Connections {
            self.close_session_manager_sidebar_search(cx);
            self.connections.session_manager.update(cx, |manager, cx| {
                manager.set_sidebar_sort_open(false, cx);
            });
        }
        self.persist_sidebar_settings(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_sidebar_section(
        &mut self,
        section: SidebarSection,
        cx: &mut Context<Self>,
    ) {
        // Activity-bar panel buttons are symmetric toggles: selecting another
        // panel opens it, while selecting the visible panel hides the sidebar.
        if should_collapse_primary_sidebar_section(
            self.rails.left.collapsed(),
            self.effective_sidebar_panel_section(),
            section,
        ) {
            self.toggle_sidebar(cx);
        } else {
            self.set_sidebar_section(section, cx);
        }
    }

    pub(in crate::workspace) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.onboarding_disclaimer_locked() {
            return;
        }
        self.set_sidebar_collapsed_with_motion(!self.rails.left.collapsed(), cx);
        self.rails.left.end_resize();
        self.persist_sidebar_settings(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn sidebar_panel_width(&self) -> f32 {
        self.rails.left.panel_width(&self.tokens)
    }

    fn rail_cursor_width(&self, side: RailSide, x: Pixels, window: &Window) -> f32 {
        let viewport = f32::from(window.viewport_size().width);
        let activity = self.tokens.metrics.activity_bar_width;
        match side {
            RailSide::Left => clamp_responsive_sidebar_width(
                side.width_at(f32::from(x), viewport, activity),
                viewport,
                self.tokens.metrics.sidebar_min_width,
                self.tokens.metrics.sidebar_max_width,
            ),
            RailSide::Right => ai_sidebar_width_from_cursor_value(f32::from(x), viewport, activity),
        }
    }

    pub(in crate::workspace) fn start_rail_resize(
        &mut self,
        side: RailSide,
        x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let width = self.rail_cursor_width(side, x, window);
        self.rails.rail_mut(side).begin_resize(width, &self.tokens);
        cx.notify();
    }

    pub(in crate::workspace) fn update_rail_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.rails.resizing_side().is_none() {
            return;
        }
        if !event.dragging() {
            self.finish_rail_resize(cx);
            return;
        }
        self.drag_rail_resize(event.position.x, window, cx);
    }

    pub(in crate::workspace) fn drag_rail_resize(
        &mut self,
        x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(side) = self.rails.resizing_side() else {
            return;
        };
        let width = self.rail_cursor_width(side, x, window);
        if self
            .rails
            .rail_mut(side)
            .set_width(width, true, &self.tokens)
        {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_rail_resize(&mut self, cx: &mut Context<Self>) {
        let Some(side) = self.rails.resizing_side() else {
            return;
        };
        self.rails.rail_mut(side).end_resize();
        match side {
            RailSide::Left => self.persist_sidebar_settings(cx),
            RailSide::Right => {
                self.settings_store
                    .settings_mut()
                    .sidebar_ui
                    .ai_sidebar_width = self.rails.right.width().round() as i64;
                self.persist_sidebar_settings_store(cx);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_embedded_sftp_sidebar_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.embedded_sftp_sidebar_resizing = true;
        self.set_embedded_sftp_sidebar_fraction(event.position.y, window, cx);
    }

    pub(in crate::workspace) fn update_embedded_sftp_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.embedded_sftp_sidebar_resizing {
            return;
        }
        if !event.dragging() {
            // End capture when the platform no longer reports a pressed mouse
            // button, even if the splitter missed the corresponding mouse-up.
            self.finish_embedded_sftp_sidebar_resize(cx);
            return;
        }
        self.set_embedded_sftp_sidebar_fraction(event.position.y, window, cx);
    }

    pub(in crate::workspace) fn finish_embedded_sftp_sidebar_resize(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.embedded_sftp_sidebar_resizing {
            return;
        }
        self.embedded_sftp_sidebar_resizing = false;
        self.persist_sidebar_settings_store(cx);
        cx.notify();
    }

    fn set_embedded_sftp_sidebar_fraction(
        &mut self,
        cursor_y: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // The split lives below the native/custom titlebar and the fixed
        // primary-sidebar header, so convert the window cursor into that body.
        let body_top = self.window_titlebar_height(window) + self.tokens.metrics.tabbar_height;
        let body_height = (f32::from(window.viewport_size().height) - body_top).max(1.0);
        let fraction = ((f32::from(cursor_y) - body_top) / body_height).clamp(
            EMBEDDED_SFTP_MIN_SESSION_FRACTION,
            EMBEDDED_SFTP_MAX_SESSION_FRACTION,
        );
        let current = self.settings_store.settings().sftp.sidebar_session_fraction;
        if (current - fraction).abs() < f32::EPSILON {
            return;
        }
        self.settings_store
            .settings_mut()
            .sftp
            .sidebar_session_fraction = fraction;
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_ai_sidebar(&mut self, cx: &mut Context<Self>) -> bool {
        self.toggle_context_sidebar_panel(ContextSidebarPanel::Assistant, cx)
    }

    pub(in crate::workspace) fn toggle_context_sidebar_panel(
        &mut self,
        panel: ContextSidebarPanel,
        cx: &mut Context<Self>,
    ) -> bool {
        // Clicking the currently visible context panel mirrors an ordinary toggle.
        if should_collapse_context_sidebar_panel(
            self.context_sidebar_visible(),
            self.rails.context_panel,
            panel,
        ) {
            self.collapse_context_sidebar(cx);
            return true;
        }
        self.open_context_sidebar_panel(panel, cx)
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
                cx,
            );
            cx.notify();
            return false;
        }

        self.rails.context_panel = panel;
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = false;
        self.rails.right.set_collapsed(false, &self.tokens);
        if panel == ContextSidebarPanel::Assistant {
            self.ensure_ai_chat_initialized(cx);
            self.bootstrap_ai_mcp_registry(cx);
        } else {
            // Non-AI context panels share the old right-sidebar shell, but must
            // not keep AI-specific focus or floating popovers alive.
            self.close_ai_sidebar_popovers(cx);
        }
        self.sync_host_tools_lifecycle(panel == ContextSidebarPanel::HostTools, cx);
        self.clear_ai_sidebar_keyboard_focus(cx);
        self.persist_sidebar_settings_store(cx);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn collapse_context_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .ai_sidebar_collapsed = true;
        let generation = self.rails.right.set_collapsed(true, &self.tokens);
        self.schedule_rail_unmount(RailSide::Right, generation, cx);
        self.rails.right.end_resize();
        self.sync_host_tools_lifecycle(false, cx);
        self.clear_ai_sidebar_keyboard_focus(cx);
        self.close_ai_sidebar_popovers(cx);
        self.persist_sidebar_settings_store(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn clamp_sidebar_widths_to_viewport(
        &mut self,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) {
        let primary_width = clamp_responsive_sidebar_width(
            self.rails.left.width(),
            viewport_width,
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        let context_width = clamp_responsive_sidebar_width(
            self.rails.right.width(),
            viewport_width,
            AI_SIDEBAR_ABSOLUTE_MIN_WIDTH,
            AI_SIDEBAR_ABSOLUTE_MAX_WIDTH,
        );
        let left_visible = !self.rails.left.collapsed();
        let right_visible = self.context_sidebar_visible();
        let left_changed = self
            .rails
            .left
            .set_width(primary_width, left_visible, &self.tokens);
        let right_changed = self
            .rails
            .right
            .set_width(context_width, right_visible, &self.tokens);
        if left_changed || right_changed {
            cx.notify();
        }
    }
}

pub(in crate::workspace) fn ai_sidebar_width_from_cursor_value(
    cursor_x: f32,
    viewport_width: f32,
    activity_bar_width: f32,
) -> f32 {
    // The context sidebar ends at the right activity rail, so dragging left must
    // increase width and dragging right must decrease width. Keep this math in
    // a pure helper so regressions do not require constructing a GPUI Window.
    clamp_responsive_sidebar_width(
        RailSide::Right.width_at(cursor_x, viewport_width, activity_bar_width),
        viewport_width,
        AI_SIDEBAR_ABSOLUTE_MIN_WIDTH,
        AI_SIDEBAR_ABSOLUTE_MAX_WIDTH,
    )
}

#[cfg(test)]
mod tests {
    use super::ai_sidebar_width_from_cursor_value;

    #[test]
    fn context_sidebar_resize_excludes_the_fixed_right_rail() {
        assert_eq!(
            ai_sidebar_width_from_cursor_value(852.0, 1_200.0, 48.0),
            300.0
        );
        assert_eq!(
            ai_sidebar_width_from_cursor_value(752.0, 1_200.0, 48.0),
            400.0
        );
    }
}
