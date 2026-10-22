use super::*;
use gpui::{StatefulInteractiveElement, point};
use oxideterm_gpui_ui::select::{
    select_option_action, select_option_highlighted, select_overlay_popup_with_max_height,
    select_separator,
};

const TOOLS: [ContextSidebarTool; 11] = [
    ContextSidebarTool::Monitor,
    ContextSidebarTool::Processes,
    ContextSidebarTool::Gpu,
    ContextSidebarTool::Docker,
    ContextSidebarTool::Services,
    ContextSidebarTool::Logs,
    ContextSidebarTool::Ports,
    ContextSidebarTool::Tmux,
    ContextSidebarTool::Filesystems,
    ContextSidebarTool::Schedules,
    ContextSidebarTool::Packages,
];

const MONITORING_TOOL_COUNT: usize = 4;

impl ContextSidebarTool {
    pub(in crate::workspace) fn label_key(self) -> &'static str {
        match self {
            Self::Monitor => "sidebar.panels.host_monitor",
            Self::Gpu => "sidebar.panels.gpu",
            Self::Processes => "sidebar.panels.processes",
            Self::Services => "sidebar.panels.services",
            Self::Logs => "sidebar.panels.logs",
            Self::Tmux => "sidebar.panels.tmux",
            Self::Docker => "sidebar.panels.docker",
            Self::Ports => "sidebar.panels.ports",
            Self::Schedules => "sidebar.panels.schedules",
            Self::Filesystems => "sidebar.panels.filesystems",
            Self::Packages => "sidebar.panels.packages",
        }
    }

    pub(in crate::workspace) fn icon(self) -> LucideIcon {
        match self {
            Self::Monitor => LucideIcon::Activity,
            Self::Gpu => LucideIcon::Gpu,
            Self::Processes => LucideIcon::ListTree,
            Self::Services => LucideIcon::ServerCog,
            Self::Logs => LucideIcon::FileText,
            Self::Tmux => LucideIcon::SplitSquareHorizontal,
            Self::Docker => LucideIcon::Docker,
            Self::Ports => LucideIcon::Network,
            Self::Schedules => LucideIcon::CalendarClock,
            Self::Filesystems => LucideIcon::HardDrive,
            Self::Packages => LucideIcon::Package,
        }
    }
}

fn visible_tool_count(available_height: f32, row_height: f32, separator_height: f32) -> usize {
    // Reserve More only for overflow, and the second divider only when both groups show.
    (0..=TOOLS.len())
        .rev()
        .find(|&count| {
            let more_height = if count < TOOLS.len() { row_height } else { 0.0 };
            tool_stack_height(count, row_height, separator_height) + more_height <= available_height
        })
        .unwrap_or(0)
}

fn tool_stack_height(count: usize, row_height: f32, separator_height: f32) -> f32 {
    let separators = if count > MONITORING_TOOL_COUNT {
        2.0
    } else {
        1.0
    };
    (count + 1) as f32 * row_height + separators * separator_height
}

fn overflow_tools(visible: usize) -> Vec<ContextSidebarTool> {
    TOOLS[visible..].to_vec()
}

impl WorkspaceApp {
    fn context_tool_separator(&self) -> gpui::Div {
        div()
            .flex_none()
            .w(px(self.tokens.metrics.divider_width))
            .h(px(self.tokens.metrics.divider_height))
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .bg(self.workspace_divider_color(self.tokens.ui.divider, 0xff))
    }

    fn context_tool_count(&self, window: &Window) -> usize {
        let metrics = self.tokens.metrics;
        let lock_height = if self.settings_store.settings().sidebar_ui.show_app_lock_icon {
            metrics.activity_icon_size + metrics.activity_icon_gap + self.tokens.spacing.two * 2.0
        } else {
            0.0
        };
        let available = f32::from(window.viewport_size().height)
            - self.window_titlebar_height(window)
            - metrics.tabbar_height
            - PRIMARY_SIDEBAR_CONTENT_TOP_INSET
            - lock_height;
        visible_tool_count(
            available,
            metrics.activity_icon_size + metrics.activity_icon_gap,
            metrics.divider_height + metrics.activity_icon_gap,
        )
    }

    fn context_tool_active(&self, tool: ContextSidebarTool, cx: &App) -> bool {
        self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            && self.host_tools.read(cx).active_tool() == tool
    }

    fn activate_context_tool(&mut self, tool: ContextSidebarTool, cx: &mut Context<Self>) {
        self.context_tools_menu = None;
        self.clear_all_workspace_tooltips(cx);
        if self.context_tool_active(tool, cx) {
            self.collapse_context_sidebar(cx);
            return;
        }
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.select_sidebar_tool(tool, cx)
        });
        if !self.context_sidebar_visible()
            || self.active_context_sidebar_panel != ContextSidebarPanel::HostTools
        {
            self.open_context_sidebar_panel(ContextSidebarPanel::HostTools, cx);
        }
        cx.notify();
    }

    fn context_tool_button_options(&self, active: bool) -> IconButtonOptions {
        IconButtonOptions {
            radius: ButtonRadius::Md,
            has_background: active,
            background: active.then(|| self.settings_panel_background(self.tokens.ui.bg_panel)),
            border: active.then(|| rgb(self.tokens.ui.border)),
            hover_background: Some(rgb(self.tokens.ui.bg_hover)),
            idle_opacity: 1.0,
            ..IconButtonOptions::compact(self.tokens.metrics.activity_icon_size)
        }
    }

    pub(in crate::workspace) fn render_context_tool_buttons(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.context_tool_count(window);
        let mut buttons = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .child(self.render_activity_icon(SidebarSection::Assistant, LucideIcon::Sparkles, cx))
            .child(self.context_tool_separator());
        for (index, tool) in TOOLS.iter().take(count).copied().enumerate() {
            if index == MONITORING_TOOL_COUNT {
                buttons = buttons.child(self.context_tool_separator());
            }
            let active = self.context_tool_active(tool, cx);
            buttons = buttons.child(
                div()
                    .flex_none()
                    .mb(px(self.tokens.metrics.activity_icon_gap))
                    .child(self.workspace_tooltip_icon_button(
                        tool.icon(),
                        self.tokens.metrics.activity_icon_glyph_size,
                        rgb(if active {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text
                        }),
                        self.context_tool_button_options(active),
                        self.i18n.t(tool.label_key()),
                        "context-tool",
                        true,
                        cx.listener(move |this, _, _, cx| {
                            this.activate_context_tool(tool, cx);
                            cx.stop_propagation();
                        }),
                        cx.entity(),
                    )),
            );
        }
        let overflow = overflow_tools(count);
        if overflow.is_empty() {
            return buttons.into_any_element();
        }
        let active = self.context_tools_menu.is_some()
            || overflow
                .iter()
                .any(|tool| self.context_tool_active(*tool, cx));
        buttons
            .child(
                div()
                    .flex_none()
                    .mb(px(self.tokens.metrics.activity_icon_gap))
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::MoreVertical,
                        self.tokens.metrics.activity_icon_glyph_size,
                        rgb(if active {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text
                        }),
                        self.context_tool_button_options(active),
                        self.i18n.t("sidebar.panels.more_tools"),
                        "context-tools-more",
                        true,
                        cx.listener(move |this, _, window, cx| {
                            let open = this.context_tools_menu.is_none();
                            this.prepare_modal_interaction_boundary(cx);
                            if open {
                                let overflow = overflow_tools(this.context_tool_count(window));
                                if overflow.is_empty() {
                                    return;
                                }
                                let active_tool = this.host_tools.read(cx).active_tool();
                                let index = overflow
                                    .iter()
                                    .position(|tool| *tool == active_tool)
                                    .unwrap_or(0);
                                this.context_tools_menu = Some(overflow[index]);
                                this.context_tools_menu_scroll.scroll_to_item(index);
                                window.focus(&this.focus_handle, cx);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                        cx.entity(),
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_tools_menu(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let highlighted = self.context_tools_menu?;
        if self.settings_store.settings().sidebar_ui.zen_mode {
            self.context_tools_menu = None;
            return None;
        }
        let count = self.context_tool_count(window);
        let overflow = overflow_tools(count);
        if overflow.is_empty() {
            self.context_tools_menu = None;
            return None;
        }
        let highlighted = if overflow.contains(&highlighted) {
            highlighted
        } else {
            overflow[0]
        };
        let max_height = (f32::from(window.viewport_size().height) - 24.0)
            .min(self.tokens.metrics.ui_select_max_height);
        let mut popup = select_overlay_popup_with_max_height(&self.tokens, 220.0, max_height)
            .id("context-tools-menu")
            .track_scroll(&self.context_tools_menu_scroll);
        for (index, tool) in overflow.into_iter().enumerate() {
            let option = select_option_action(
                select_option_highlighted(
                    &self.tokens,
                    self.i18n.t(tool.label_key()),
                    self.context_tool_active(tool, cx),
                    highlighted == tool,
                ),
                false,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.activate_context_tool(tool, cx);
                    cx.stop_propagation();
                }),
            );
            // Keep one scroll item per tool, including its optional group divider.
            popup = popup.child(
                div()
                    .when(index > 0 && count + index == MONITORING_TOOL_COUNT, |row| {
                        row.child(select_separator(&self.tokens))
                    })
                    .child(option),
            );
        }
        let metrics = self.tokens.metrics;
        // The fixed rail has no scroll transform. Derive the trigger position from the same
        // layout inputs in this frame, so resizing cannot leave a cached anchor behind.
        let top = self.window_titlebar_height(window)
            + metrics.tabbar_height
            + PRIMARY_SIDEBAR_CONTENT_TOP_INSET
            + tool_stack_height(
                count,
                metrics.activity_icon_size + metrics.activity_icon_gap,
                metrics.divider_height + metrics.activity_icon_gap,
            );
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.context_tools_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.context_tools_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(deferred(
                    anchored()
                        .anchor(Corner::TopRight)
                        .position(point(
                            window.viewport_size().width - px(metrics.activity_bar_width + 4.0),
                            px(top),
                        ))
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window_with_margin(px(8.0))
                        .child(popup),
                ))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn handle_context_tools_menu_key(
        &mut self,
        key: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(highlighted) = self.context_tools_menu else {
            return false;
        };
        let overflow = overflow_tools(self.context_tool_count(window));
        if overflow.is_empty() {
            self.context_tools_menu = None;
            return false;
        }
        let index = overflow
            .iter()
            .position(|tool| *tool == highlighted)
            .unwrap_or(0);
        let next = match key {
            "escape" | "tab" => {
                self.context_tools_menu = None;
                cx.notify();
                return true;
            }
            "enter" | "space" | " " => {
                self.activate_context_tool(overflow[index], cx);
                return true;
            }
            "arrowdown" | "down" => browser_behavior::browser_select_next_index(
                index,
                overflow.len(),
                browser_behavior::BrowserSelectKeyDirection::Next,
            ),
            "arrowup" | "up" => browser_behavior::browser_select_next_index(
                index,
                overflow.len(),
                browser_behavior::BrowserSelectKeyDirection::Previous,
            ),
            "home" => 0,
            "end" => overflow.len() - 1,
            _ => return true,
        };
        self.context_tools_menu = Some(overflow[next]);
        self.context_tools_menu_scroll.scroll_to_item(next);
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinking_the_rail_moves_only_trailing_tools_into_more() {
        // All tools fit exactly without reserving an unnecessary More button.
        let all_visible = visible_tool_count(498.0, 40.0, 9.0);
        assert_eq!(all_visible, 11);
        assert_eq!(overflow_tools(all_visible), vec![]);
        assert_eq!(visible_tool_count(700.0, 40.0, 9.0), 11);
        // Once the last row no longer fits, More needs a row of its own.
        let almost_all = visible_tool_count(497.0, 40.0, 9.0);
        assert_eq!(almost_all, 9);
        assert_eq!(
            overflow_tools(almost_all),
            vec![ContextSidebarTool::Schedules, ContextSidebarTool::Packages]
        );
        assert_eq!(visible_tool_count(300.0, 40.0, 9.0), 5);
        // A visible management row needs the group divider as well as its own row.
        assert_eq!(visible_tool_count(298.0, 40.0, 9.0), 5);
        assert_eq!(visible_tool_count(297.0, 40.0, 9.0), 4);
        let visible = visible_tool_count(210.0, 40.0, 9.0);
        assert_eq!(
            &TOOLS[..visible],
            &[
                ContextSidebarTool::Monitor,
                ContextSidebarTool::Processes,
                ContextSidebarTool::Gpu
            ]
        );
        assert_eq!(
            overflow_tools(visible),
            vec![
                ContextSidebarTool::Docker,
                ContextSidebarTool::Services,
                ContextSidebarTool::Logs,
                ContextSidebarTool::Ports,
                ContextSidebarTool::Tmux,
                ContextSidebarTool::Filesystems,
                ContextSidebarTool::Schedules,
                ContextSidebarTool::Packages
            ]
        );
        assert_eq!(visible_tool_count(80.0, 40.0, 9.0), 0);
    }
}
