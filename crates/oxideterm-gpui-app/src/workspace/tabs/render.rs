use super::helpers::{
    WELCOME_PAGE_PADDING, WelcomeRecentConnection, WelcomeRecentKind, WelcomeRecentTarget,
    effective_shortcut_label, welcome_layout_is_compact, welcome_recent_connections,
};
use super::*;
use oxideterm_gpui_shell::tabs::{
    geometry::{TabStripItemKind, TabStripState},
    strip::{TabStripFrame, TabVisual},
};

use gpui::StatefulInteractiveElement;

const TAB_RECONNECT_PHASES: [ReconnectPhase; 8] = [
    ReconnectPhase::Snapshot,
    ReconnectPhase::GracePeriod,
    ReconnectPhase::SshConnect,
    ReconnectPhase::AwaitTerminal,
    ReconnectPhase::RestoreForwards,
    ReconnectPhase::ResumeTransfers,
    ReconnectPhase::RestoreIde,
    ReconnectPhase::Verify,
];

fn tab_reconnect_phase_text(job: &ReconnectProgress) -> String {
    let label = reconnect_phase_label(&job.status);
    if job.attempt > 1 {
        format!("{label} {}/{}", job.attempt, job.max_attempts)
    } else {
        label.to_string()
    }
}

#[derive(Clone, Copy)]
enum WelcomeToolAction {
    NewConnection,
    LocalTerminal,
    ImportConnections,
    SessionManager,
    CloudSync,
}

fn tab_kind_icon(
    workspace: &WorkspaceApp,
    kind: &TabKind,
    cx: &Context<WorkspaceApp>,
) -> LucideIcon {
    match kind {
        TabKind::Workspace => LucideIcon::AppWindow,
        TabKind::LocalTerminal => LucideIcon::Square,
        TabKind::SshTerminal => LucideIcon::Terminal,
        TabKind::MoshTerminal => LucideIcon::Terminal,
        TabKind::FileManager => LucideIcon::FolderOpen,
        TabKind::Graphics | TabKind::RemoteDesktop => LucideIcon::Monitor,
        TabKind::Runtime | TabKind::ConnectionPool => LucideIcon::Gauge,
        TabKind::Topology => LucideIcon::Network,
        TabKind::NotificationCenter => LucideIcon::Bell,
        TabKind::Sftp => LucideIcon::FolderInput,
        TabKind::Ide => LucideIcon::Code2,
        TabKind::Forwards => LucideIcon::ArrowLeftRight,
        TabKind::SessionManager => LucideIcon::LayoutList,
        TabKind::PluginManager => LucideIcon::Puzzle,
        TabKind::Plugin { plugin_id, tab_id } => workspace
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .tab_contribution(plugin_id, tab_id)
            .map(|contribution| LucideIcon::from_plugin_name(&contribution.definition.icon))
            .unwrap_or(LucideIcon::Puzzle),
        TabKind::CloudSync => LucideIcon::Cloud,
        TabKind::Knowledge => LucideIcon::BookOpen,
        TabKind::Settings => LucideIcon::Settings,
        TabKind::Onboarding => LucideIcon::Sparkles,
    }
}

impl WorkspaceApp {
    fn render_tab_bar_icon(
        &self,
        kind: &TabKind,
        icon_color: Rgba,
        cx: &Context<Self>,
    ) -> AnyElement {
        let size = self.tokens.metrics.tab_icon_size;
        if *kind == TabKind::Onboarding {
            let icon_source = crate::app_icon::app_icon_trimmed_source_sized(
                self.settings_store.settings().appearance.app_icon,
                size,
            );
            let radius = (size * 0.22).round().max(2.5);
            return div()
                .size(px(size))
                .relative()
                .flex_none()
                .rounded(px(radius))
                .overflow_hidden()
                .child(
                    gpui::img(icon_source)
                        .size(px(size))
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(radius))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x66)),
                )
                .into_any_element();
        }
        Self::render_lucide_icon(tab_kind_icon(self, kind, cx), size, icon_color)
    }

    pub(super) fn tab_reconnect_controls_width(
        &self,
        state: &TabStripState,
        job: &ReconnectProgress,
        window: &Window,
    ) -> f32 {
        let phases_width =
            TAB_RECONNECT_PHASES.len() as f32 * 4.0 + (TAB_RECONNECT_PHASES.len() - 1) as f32 * 2.0;
        12.0 + phases_width
            + state.text_width(
                &tab_reconnect_phase_text(job),
                self.tokens.metrics.ui_text_2xs,
                settings_ui_font_family(&self.settings_store.settings().appearance.ui_font_family),
                window,
            )
            + self.tokens.metrics.tab_close_button_size
            + 3.0 * 4.0
    }

    fn render_skip_future_ssh_close_confirmations(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                checkbox(
                    &self.tokens,
                    "skip-ssh-close-confirmation",
                    self.i18n.t("common.actions.do_not_ask_again"),
                    self.skip_future_ssh_close_confirmations,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.toggle_skip_future_ssh_close_confirmations(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn tab_strip_frame(
        &mut self,
        state: &TabStripState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> TabStripFrame {
        let layout = self.measure_tab_strip(state, window, cx);
        let active = self.active_tab_id(cx);
        let theme = self.tokens.ui;
        let host = self.tab_host.read(cx);
        let bells = host.bell_highlighted_tab_ids();
        let unread = host.unread_terminal_output_tab_ids();
        let locked = self.onboarding_disclaimer_locked();
        let visuals = layout
            .items
            .iter()
            .map(|item| {
                if let TabStripItemKind::Exiting(index) = item.kind {
                    let exit = &state.exiting_tabs[index];
                    return TabVisual {
                        title: exit.title.clone(),
                        icon: self.render_tab_bar_icon(
                            &exit.kind,
                            TabVisual::color(&self.tokens, exit.was_active, false, false),
                            cx,
                        ),
                        status: None,
                        bell: false,
                        unread: false,
                        closeable: false,
                        draggable: false,
                        detachable: false,
                    };
                }
                let tab = self
                    .tab_by_id(item.tab_id, cx)
                    .expect("measured live or returning tab");
                let title = self.tab_display_title(tab);
                let bell = bells.contains(&tab.id);
                let unread = unread.contains(&tab.id);
                let color = if item.kind == TabStripItemKind::Returning {
                    rgba((theme.accent << 8) | 0xcc)
                } else {
                    TabVisual::color(&self.tokens, Some(tab.id) == active, bell, unread)
                };
                let icon = self.render_tab_bar_icon(&tab.kind, color, cx);
                let onboarding = tab.kind == TabKind::Onboarding;
                let status = self
                    .reconnect_node_id_for_tab(tab, cx)
                    .and_then(|node| {
                        self.workspace_runtime
                            .read(cx)
                            .reconnect_active_progress(&node)
                            .map(|job| (node, job))
                    })
                    .map(|(node, job)| self.render_tab_reconnect_indicator(&job, node, cx));
                TabVisual {
                    title,
                    icon,
                    status,
                    bell,
                    unread,
                    closeable: !onboarding || !locked,
                    draggable: !locked && !onboarding,
                    detachable: !onboarding,
                }
            })
            .collect();
        let zen = self.settings_store.settings().sidebar_ui.zen_mode;
        let quick_actions = zen.then(|| {
            let label = self.i18n.t("zen_mode.exit");
            let shortcut = effective_shortcut_label(
                "app.zenMode",
                &self.settings_store.settings().keybindings.overrides,
            );
            let tokens = self.tokens;
            let workspace = cx.entity().downgrade();
            oxideterm_gpui_ui::button::icon_button(
                &tokens,
                Self::render_lucide_icon(
                    LucideIcon::Eye,
                    tokens.metrics.sidebar_action_icon_size,
                    rgb(theme.text_muted),
                ),
                oxideterm_gpui_ui::IconButtonOptions::compact(self.zen_exit_tabbar_width()),
            )
            .id("zen-exit-tabbar-button")
            .debug_selector(|| "zen-exit-tabbar-button".into())
            .flex_none()
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
                let _ = workspace.update(cx, |this, cx| this.toggle_zen_mode(cx));
            })
            .tooltip(move |_, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(
                    tokens,
                    label.clone(),
                    shortcut.clone(),
                    cx,
                )
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle_zen_mode(cx)))
            .into_any_element()
        });
        TabStripFrame {
            tokens: self.tokens,
            layout,
            visuals,
            active,
            supports_detach: true,
            background: self.workspace_chrome_background(theme.shell_bg()),
            divider: self.workspace_chrome_divider(),
            active_background: self.workspace_tab_background(true),
            idle_background: self.workspace_tab_background(false),
            translucent: self.window_background_preferences().is_some(),
            leading_margin: if zen { self.tabbar_left_x(window) } else { 0. },
            filler: Some(self.render_window_drag_region("workspace-tabbar-drag-region", cx)),
            quick_actions,
            quick_actions_width: self.zen_exit_tabbar_width(),
            new_tab_label: (!locked).then(|| self.i18n.t("menu.new_terminal")),
        }
    }
    fn workspace_tab_background(&self, active: bool) -> Rgba {
        if !active && self.window_background_preferences().is_some() {
            // The strip owns the idle fill; only the selected tab adds a surface.
            rgba(0x00000000)
        } else {
            self.workspace_chrome_background(if active {
                self.tokens.ui.shell_active_tab_bg()
            } else {
                self.tokens.ui.shell_bg()
            })
        }
    }

    pub(in crate::workspace) fn render_node_disconnect_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return div().into_any_element();
        };
        let WorkspaceOverlayConfirmKind::NodeDisconnect { display_name, .. } = &snapshot.kind
        else {
            return div().into_any_element();
        };
        let title = self
            .i18n
            .t("common.confirm.disconnect_node")
            .replace("{{name}}", display_name);
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "node-disconnect-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(title).into_any_element(),
                description: Some(self.render_skip_future_ssh_close_confirmations(cx)),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                this.cancel_node_disconnect_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_node_disconnect_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(in crate::workspace) fn render_tab_close_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.tab_host.read(cx).close_confirm_snapshot() else {
            return div().into_any_element();
        };
        let (title_key, description_key, other_tab_count) = match &snapshot.confirm {
            TabCloseConfirm::ApplicationQuit => ("menu.quit", Some("menu.confirm_quit"), None),
            TabCloseConfirm::Single { .. } | TabCloseConfirm::SshPane { .. } => (
                "tabbar.confirm_close_terminal_title",
                Some("tabbar.confirm_close_terminal_desc"),
                None,
            ),
            TabCloseConfirm::LocalChildPane { .. }
            | TabCloseConfirm::LocalChildProcess { .. }
            | TabCloseConfirm::LocalChildProcessBatch { .. } => {
                ("tabbar.child_process_warning", None, None)
            }
            TabCloseConfirm::Other { tab_ids } => (
                "tabbar.confirm_close_other_title",
                Some("tabbar.confirm_close_other_desc"),
                Some(tab_ids.len()),
            ),
        };
        let description = match (description_key, other_tab_count) {
            (Some(key), Some(count)) => self.i18n.t(key).replace("{{count}}", &count.to_string()),
            (Some(key), None) => self.i18n.t(key),
            (None, _) => String::new(),
        };
        let is_ssh_confirmation = matches!(
            &snapshot.confirm,
            TabCloseConfirm::Single { .. }
                | TabCloseConfirm::SshPane { .. }
                | TabCloseConfirm::Other { .. }
        );
        let description = if is_ssh_confirmation {
            Some(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .when(!description.is_empty(), |body| body.child(description))
                    .child(self.render_skip_future_ssh_close_confirmations(cx))
                    .into_any_element(),
            )
        } else {
            (!description.is_empty()).then(|| div().child(description).into_any_element())
        };
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "tab-close-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                this.cancel_tab_close_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_tab_close_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(super) fn reconnect_node_id_for_tab(&self, tab: &Tab, cx: &App) -> Option<NodeId> {
        match tab.kind {
            TabKind::SshTerminal | TabKind::LocalTerminal => {
                if let Some(active_pane_id) = tab.active_pane_id
                    && let Some(session_id) = tab
                        .root_pane
                        .as_ref()
                        .and_then(|root| root.session_id_for_pane(active_pane_id))
                    && let Some(node_id) = self
                        .workspace_runtime
                        .read(cx)
                        .ssh_terminal_node_id(session_id)
                {
                    return Some(node_id);
                }
                let mut session_ids = Vec::new();
                if let Some(root) = tab.root_pane.as_ref() {
                    root.collect_session_ids(&mut session_ids);
                }
                session_ids
                    .into_iter()
                    .filter_map(|session_id| {
                        self.workspace_runtime
                            .read(cx)
                            .ssh_terminal_node_id(session_id)
                    })
                    .find(|node_id| self.has_active_reconnect_job(node_id, cx))
                    .or_else(|| {
                        tab.root_pane.as_ref().and_then(|root| {
                            let mut session_ids = Vec::new();
                            root.collect_session_ids(&mut session_ids);
                            session_ids.first().and_then(|session_id| {
                                self.workspace_runtime
                                    .read(cx)
                                    .ssh_terminal_node_id(*session_id)
                            })
                        })
                    })
            }
            TabKind::Sftp => self
                .sftp_tab_nodes
                .get(&tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            TabKind::Forwards => self
                .forwarding
                .read(cx)
                .node_for_tab(tab.id)
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            TabKind::Ide => self
                .ide_workspace
                .read(cx)
                .node_for_tab(tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            _ => None,
        }
    }

    fn render_tab_reconnect_indicator(
        &self,
        job: &ReconnectProgress,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let phase_text = tab_reconnect_phase_text(job);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(theme.warning))
            .child(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(theme.warning),
            ))
            .child(self.render_reconnect_phase_strip(job))
            .child(div().whitespace_nowrap().child(phase_text))
            .child(
                div()
                    .id((
                        gpui::ElementId::from("tabbar-cancel-reconnect"),
                        node_id.0.clone(),
                    ))
                    .size(px(self.tokens.metrics.tab_close_button_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .cursor_pointer()
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        self.tokens.metrics.tab_close_icon_size,
                        rgb(theme.warning),
                    ))
                    .on_mouse_move(cx.listener({
                        let label = self.i18n.t("sessions.tree.actions.cancel_reconnect");
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                "tabbar-cancel-reconnect",
                                label.clone(),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        }
                    }))
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            // The reconnect tooltip is mounted at the workspace
                            // root, so the tab button owns the explicit leave
                            // clear just like a browser TooltipTrigger.
                            this.clear_workspace_tooltip("tabbar-cancel-reconnect", cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.cancel_reconnect_for_node(&node_id, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_reconnect_phase_strip(&self, job: &ReconnectProgress) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(TAB_RECONNECT_PHASES.into_iter().map(|phase| {
                let result = job
                    .phase_history
                    .iter()
                    .rev()
                    .find(|event| event.phase == phase)
                    .map(|event| event.result);
                let color = match result {
                    Some(PhaseResult::Ok) => self.tokens.ui.success,
                    Some(PhaseResult::Failed) => self.tokens.ui.error,
                    Some(PhaseResult::Skipped) => self.tokens.ui.text_muted,
                    Some(PhaseResult::Running) => self.tokens.ui.warning,
                    None => self.tokens.ui.border,
                };
                div()
                    .size(px(4.0))
                    .rounded_full()
                    .bg(rgb(color))
                    .into_any_element()
            }))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_empty_workspace(
        &self,
        available_width: f32,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const WELCOME_CONTENT_MAX_WIDTH: f32 = 860.0;

        let theme = self.tokens.ui;
        let compact = welcome_layout_is_compact(available_width);
        div()
            .size_full()
            // The enclosing `tab_canvas_surface` owns the uniform base tint when
            // a background is active so cards below retain their panel vibrancy step.
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text_muted))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .child(
                div()
                    .id("welcome-workspace-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(px(WELCOME_PAGE_PADDING))
                            .pt(px(28.0))
                            .pb(px(48.0))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(WELCOME_CONTENT_MAX_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(22.0))
                                    .child(self.render_welcome_header())
                                    .child(self.render_welcome_workbench(
                                        compact,
                                        has_background,
                                        cx,
                                    ))
                                    .child(self.render_welcome_shortcuts(has_background)),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn welcome_main_content_width(&self, window: &Window, _: &App) -> f32 {
        let settings = self.settings_store.settings();
        let mut available_width = f32::from(window.viewport_size().width);
        if !settings.sidebar_ui.zen_mode {
            available_width -= self.tokens.metrics.activity_bar_width;
            if !self.rails.left.collapsed() {
                available_width -= self.sidebar_panel_width();
            }
            available_width -= self.context_sidebar_occupied_width();
        }
        available_width.max(self.tokens.metrics.min_main_width)
    }

    fn render_welcome_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .pb(px(18.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .flex()
            .flex_col()
            .items_start()
            .child(self.render_welcome_brand())
            .into_any_element()
    }

    fn render_welcome_brand(&self) -> AnyElement {
        let theme = self.tokens.ui;
        let wordmark_size = self.tokens.metrics.welcome_wordmark_font_size;
        let icon_source = crate::app_icon::app_icon_trimmed_source_sized(
            self.settings_store.settings().appearance.app_icon,
            WELCOME_BRAND_BADGE_SIZE,
        );
        // Trimmed icons start right at the squircle edge so x=0 aligns with the
        // workbench columns below, while the subtle border and card shadow keep
        // white squircle variants legible against light canvases.
        let badge = oxideterm_gpui_ui::theme_card_surface_shadow(
            div()
                .size(px(WELCOME_BRAND_BADGE_SIZE))
                .relative()
                .flex_none()
                .rounded(px(WELCOME_BRAND_BADGE_RADIUS))
                .child(
                    gpui::img(icon_source)
                        .size(px(WELCOME_BRAND_BADGE_SIZE))
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(WELCOME_BRAND_BADGE_RADIUS))
                        .border_1()
                        .border_color(rgba((theme.border << 8) | 0x80)),
                ),
            &self.tokens,
        );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(14.0))
            .child(badge)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_size(px(wordmark_size))
                    .line_height(px(wordmark_size + 4.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("layout.empty.title")),
            )
            .into_any_element()
    }

    fn render_welcome_workbench(
        &self,
        compact: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        welcome_workbench_layout(
            compact,
            self.render_welcome_recent_connections(has_background, cx),
            self.render_welcome_guidance(has_background, cx),
        )
        .into_any_element()
    }

    fn render_welcome_recent_connections(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const RECENT_CONNECTION_LIMIT: usize = 4;

        let theme = self.tokens.ui;
        let recent = welcome_recent_connections(&self.connection_store, RECENT_CONNECTION_LIMIT);
        let saved_count = self.connection_store.connections().len()
            + self.connection_store.serial_profiles().len()
            + self.connection_store.telnet_profiles().len()
            + self.connection_store.mosh_profiles().len()
            + self.connection_store.remote_desktop_profiles().len();
        let count_label = self
            .i18n
            .t("layout.empty.saved_count")
            .replace("{{count}}", &saved_count.to_string());
        let mut surface = div().flex().flex_col().gap(px(10.0)).child(
            div()
                .w_full()
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("layout.empty.recent_connections")),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(count_label),
                ),
        );

        let card_alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let container = div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(oxideterm_gpui_ui::color_for_background(
                welcome_card_background(theme),
                has_background,
                card_alpha,
            ))
            .overflow_hidden();

        if recent.is_empty() {
            surface = surface.child(container.child(self.render_welcome_recent_empty(cx)));
        } else {
            let count = recent.len();
            let mut list = div().w_full().flex().flex_col();
            for (index, connection) in recent.into_iter().enumerate() {
                if index > 0 {
                    list = list.child(div().h(px(1.0)).bg(rgba((theme.border << 8) | 0x45)));
                }
                list = list.child(self.render_welcome_recent_row(
                    connection,
                    index == 0,
                    index + 1 == count,
                    has_background,
                    cx,
                ));
            }
            surface = surface.child(container.child(list));
        }

        surface
    }

    fn render_welcome_recent_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .px(px(16.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .items_start()
            .justify_center()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("layout.empty.recent_connections_empty")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.accent))
                    .cursor_pointer()
                    .hover(move |row| row.text_color(rgb(theme.accent_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::LayoutList,
                        14.0,
                        rgb(theme.accent),
                    ))
                    .child(self.i18n.t("layout.empty.open_session_manager"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.open_session_manager_tab(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_recent_row(
        &self,
        connection: WelcomeRecentConnection,
        is_first: bool,
        is_last: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let metrics = welcome_workbench_metrics(&self.tokens);
        let card_alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let inner_radius =
            oxideterm_gpui_ui::modal::rounded_shell_child_radius(self.tokens.radii.lg);
        let target = connection.target.clone();
        div()
            .w_full()
            .min_w_0()
            .h(px(metrics.recent_row_height))
            .px(px(12.0))
            // GPUI overflow_hidden masks to a rectangle; round edge rows to match the outer card.
            .when(is_first, |row| row.rounded_t(px(inner_radius)))
            .when(is_last, |row| row.rounded_b(px(inner_radius)))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .hover(move |row| {
                row.bg(oxideterm_gpui_ui::color_for_background(
                    theme.bg_hover,
                    has_background,
                    0x80,
                ))
            })
            .child(
                div()
                    .size(px(28.0))
                    .flex_none()
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x66))
                    .bg(oxideterm_gpui_ui::color_for_background(
                        theme.bg_sunken,
                        has_background,
                        card_alpha,
                    ))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        self.welcome_recent_icon(connection.kind),
                        14.0,
                        rgb(theme.text_secondary),
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .line_height(px(metrics.sm_line_height))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(connection.name),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(metrics.xs_line_height))
                            .text_color(rgb(theme.text_muted))
                            .child(connection.subtitle),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .rounded(px(self.tokens.radii.xs))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x66))
                            .bg(oxideterm_gpui_ui::color_for_background(
                                theme.bg_sunken,
                                has_background,
                                card_alpha,
                            ))
                            .px(px(6.0))
                            .py(px(1.0))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(self.welcome_recent_kind_label(connection.kind))),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        13.0,
                        rgb(theme.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.open_welcome_recent_target(target.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_welcome_guidance(&self, has_background: bool, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.tokens.ui;
        let metrics = welcome_workbench_metrics(&self.tokens);
        let card_alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let primary_actions = [
            (
                LucideIcon::Plus,
                "layout.empty.new_connection",
                "layout.empty.new_connection_hint",
                WelcomeToolAction::NewConnection,
                true,
            ),
            (
                LucideIcon::Terminal,
                "layout.empty.new_local_terminal",
                "layout.empty.new_local_terminal_hint",
                WelcomeToolAction::LocalTerminal,
                false,
            ),
        ];
        let utility_actions = [
            (
                LucideIcon::Download,
                "layout.empty.import_connections",
                "layout.empty.import_connections_hint",
                WelcomeToolAction::ImportConnections,
            ),
            (
                LucideIcon::LayoutList,
                "layout.empty.open_session_manager",
                "layout.empty.open_session_manager_hint",
                WelcomeToolAction::SessionManager,
            ),
            (
                LucideIcon::Cloud,
                "plugin.cloud_sync.panel_title",
                "layout.empty.cloud_sync_hint",
                WelcomeToolAction::CloudSync,
            ),
        ];

        let mut utility_list = div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(oxideterm_gpui_ui::color_for_background(
                welcome_card_background(theme),
                has_background,
                card_alpha,
            ))
            .overflow_hidden()
            .flex()
            .flex_col();
        let utility_count = utility_actions.len();
        for (index, (icon, title_key, desc_key, action)) in utility_actions.into_iter().enumerate()
        {
            if index > 0 {
                utility_list =
                    utility_list.child(div().h(px(1.0)).bg(rgba((theme.border << 8) | 0x45)));
            }
            utility_list = utility_list.child(self.render_welcome_utility_row(
                icon,
                title_key,
                desc_key,
                action,
                index == 0,
                index + 1 == utility_count,
                has_background,
                cx,
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div().w_full().h(px(20.0)).flex().items_center().child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("layout.empty.get_started")),
                ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(metrics.guidance_stack_gap))
                    .child(div().w_full().grid().grid_cols(2).gap(px(10.0)).children(
                        primary_actions.into_iter().map(
                            |(icon, title_key, desc_key, action, primary)| {
                                self.render_welcome_primary_tile(
                                    icon,
                                    title_key,
                                    desc_key,
                                    action,
                                    primary,
                                    has_background,
                                    cx,
                                )
                            },
                        ),
                    ))
                    .child(utility_list),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_welcome_primary_tile(
        &self,
        icon: LucideIcon,
        title_key: &str,
        description_key: &str,
        action: WelcomeToolAction,
        primary: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let metrics = welcome_workbench_metrics(&self.tokens);
        let card_alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        let card = div()
            .w_full()
            .min_w_0()
            .h(px(metrics.primary_tile_height))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(if primary {
                rgba((theme.accent << 8) | 0x80)
            } else {
                rgba((theme.border << 8) | 0x80)
            })
            .bg(oxideterm_gpui_ui::color_for_background(
                welcome_card_background(theme),
                has_background,
                card_alpha,
            ))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .hover(move |tile| {
                tile.bg(oxideterm_gpui_ui::color_for_background(
                    theme.bg_hover,
                    has_background,
                    0x80,
                ))
            })
            .cursor_pointer()
            .child(
                div()
                    .size(px(28.0))
                    .flex_none()
                    .rounded(px(self.tokens.radii.sm))
                    .when(!primary, |badge| {
                        badge
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x66))
                            .bg(oxideterm_gpui_ui::color_for_background(
                                theme.bg_sunken,
                                has_background,
                                card_alpha,
                            ))
                    })
                    .when(primary, |badge| badge.bg(rgb(theme.accent)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        icon,
                        15.0,
                        rgb(if primary {
                            theme.accent_text
                        } else {
                            theme.text
                        }),
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(metrics.sm_line_height))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t(title_key)),
            );

        self.bind_welcome_tool_action(card, title_key, description_key, action, cx)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_welcome_utility_row(
        &self,
        icon: LucideIcon,
        title_key: &str,
        description_key: &str,
        action: WelcomeToolAction,
        is_first: bool,
        is_last: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let metrics = welcome_workbench_metrics(&self.tokens);
        let inner_radius =
            oxideterm_gpui_ui::modal::rounded_shell_child_radius(self.tokens.radii.lg);
        let row = div()
            .w_full()
            .min_w_0()
            .h(px(metrics.utility_row_height))
            .px(px(12.0))
            .when(is_first, |row| row.rounded_t(px(inner_radius)))
            .when(is_last, |row| row.rounded_b(px(inner_radius)))
            .flex()
            .items_center()
            .gap(px(10.0))
            .hover(move |row| {
                row.bg(oxideterm_gpui_ui::color_for_background(
                    theme.bg_hover,
                    has_background,
                    0x80,
                ))
            })
            .cursor_pointer()
            .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(metrics.sm_line_height))
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t(title_key)),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                13.0,
                rgb(theme.text_muted),
            ));

        self.bind_welcome_tool_action(row, title_key, description_key, action, cx)
    }

    fn bind_welcome_tool_action(
        &self,
        element: gpui::Div,
        title_key: &str,
        description_key: &str,
        action: WelcomeToolAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tooltip_id = format!("welcome-action-{title_key}");
        element
            .id(tooltip_id.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let tooltip_id = tooltip_id.clone();
                    move |this, _event, window, cx| {
                        this.clear_workspace_tooltip(&tooltip_id, cx);
                        match action {
                            WelcomeToolAction::NewConnection => {
                                this.open_new_connection_form(window, cx)
                            }
                            WelcomeToolAction::LocalTerminal => {
                                let _ = this.create_local_terminal_tab(window, cx);
                            }
                            WelcomeToolAction::ImportConnections => {
                                this.open_connection_importers_settings(window, cx)
                            }
                            WelcomeToolAction::SessionManager => {
                                this.open_session_manager_tab(window, cx)
                            }
                            WelcomeToolAction::CloudSync => this.open_cloud_sync_tab(window, cx),
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_move(cx.listener({
                let tooltip_id = tooltip_id.clone();
                let description = self.i18n.t(description_key);
                move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id.clone(),
                        description.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(&tooltip_id, cx);
                }
            }))
            .into_any_element()
    }

    fn welcome_recent_icon(&self, kind: WelcomeRecentKind) -> LucideIcon {
        match kind {
            WelcomeRecentKind::Ssh => LucideIcon::Server,
            WelcomeRecentKind::Serial => LucideIcon::Radio,
            WelcomeRecentKind::Telnet => LucideIcon::Terminal,
            WelcomeRecentKind::Mosh => LucideIcon::Wifi,
            WelcomeRecentKind::Rdp | WelcomeRecentKind::Vnc => LucideIcon::Monitor,
        }
    }

    fn welcome_recent_kind_label(&self, kind: WelcomeRecentKind) -> &'static str {
        match kind {
            WelcomeRecentKind::Ssh => "terminal.typeSsh",
            WelcomeRecentKind::Serial => "modals.new_connection.transport_serial",
            WelcomeRecentKind::Telnet => "modals.new_connection.transport_telnet",
            WelcomeRecentKind::Mosh => "terminal.typeMosh",
            WelcomeRecentKind::Rdp => "modals.new_connection.transport_rdp",
            WelcomeRecentKind::Vnc => "modals.new_connection.transport_vnc",
        }
    }

    fn open_welcome_recent_target(
        &mut self,
        target: WelcomeRecentTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            WelcomeRecentTarget::Ssh(id) => self.open_saved_connection(&id, window, cx),
            WelcomeRecentTarget::Serial(id) => self.open_saved_serial_profile(&id, window, cx),
            WelcomeRecentTarget::Telnet(id) => self.open_saved_telnet_profile(&id, window, cx),
            WelcomeRecentTarget::Mosh(id) => self.open_saved_mosh_profile(&id, window, cx),
            WelcomeRecentTarget::RemoteDesktop(id) => {
                self.open_saved_remote_desktop_profile(&id, window, cx)
            }
        }
    }

    fn render_welcome_shortcuts(&self, has_background: bool) -> AnyElement {
        const WELCOME_SHORTCUTS: [(&str, &str); 6] = [
            ("app.commandPalette", "command_palette.title"),
            ("app.newConnection", "layout.empty.new_connection"),
            ("app.newTerminal", "layout.empty.new_local_terminal"),
            ("app.settings", "command_palette.cmd_settings"),
            ("app.toggleSidebar", "command_palette.cmd_toggle_sidebar"),
            ("app.showShortcuts", "layout.empty.keyboard_shortcuts"),
        ];

        let theme = self.tokens.ui;
        let overrides = &self.settings_store.settings().keybindings.overrides;
        div()
            .pt(px(16.0))
            .w_full()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_start()
            .gap_x(px(18.0))
            .gap_y(px(8.0))
            // Invalid registry entries are omitted instead of showing a shortcut that cannot fire.
            .children(
                WELCOME_SHORTCUTS
                    .into_iter()
                    .filter_map(|(action_id, label_key)| {
                        effective_shortcut_label(action_id, overrides)
                            .map(|key| self.render_welcome_shortcut(key, label_key, has_background))
                    }),
            )
            .into_any_element()
    }

    fn render_welcome_shortcut(
        &self,
        key: String,
        label_key: &str,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let card_alpha = oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens);
        // Window imagery needs the stronger semantic text color to remain legible.
        let label_color = if has_background {
            theme.text
        } else {
            theme.text_muted
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(label_color))
            .child(
                div()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x80))
                    .bg(oxideterm_gpui_ui::color_for_background(
                        welcome_keycap_background(theme),
                        has_background,
                        card_alpha,
                    ))
                    .px(px(5.0))
                    .py(px(1.5))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .line_height(px(14.0))
                    .text_color(rgb(theme.text))
                    .child(key),
            )
            .child(self.i18n.t(label_key))
            .into_any_element()
    }
}

const WELCOME_BRAND_BADGE_SIZE: f32 = 42.0;
const WELCOME_BRAND_BADGE_RADIUS: f32 = 9.5;

fn welcome_card_background(theme: oxideterm_theme::AppUiColors) -> u32 {
    if theme.is_light() {
        theme.bg_panel
    } else {
        theme.bg_card
    }
}

fn welcome_keycap_background(theme: oxideterm_theme::AppUiColors) -> u32 {
    if theme.is_light() {
        theme.bg_sunken
    } else {
        theme.bg_card
    }
}

#[derive(Clone, Copy, Debug)]
struct WelcomeWorkbenchMetrics {
    sm_line_height: f32,
    xs_line_height: f32,
    recent_row_height: f32,
    utility_row_height: f32,
    guidance_stack_gap: f32,
    primary_tile_height: f32,
}

fn welcome_workbench_metrics(tokens: &oxideterm_theme::ThemeTokens) -> WelcomeWorkbenchMetrics {
    // Explicit row and line heights keep CJK and ASCII labels at identical box
    // heights and align the full 4-row recent card bottom edge with the right
    // guidance stack without forcing `< 4` recent rows to stretch.
    let sm_line_height = (tokens.metrics.ui_text_sm + 5.0).round();
    let xs_line_height = (tokens.metrics.ui_text_xs + 4.0).round();
    let recent_row_height = (sm_line_height + 2.0 + xs_line_height + 16.0).max(48.0);
    let utility_row_height = sm_line_height + 26.0;
    let guidance_stack_gap = 12.0;
    let full_recent_card_height = 4.0 * recent_row_height + 5.0;
    let utility_card_height = 3.0 * utility_row_height + 4.0;
    let primary_tile_height =
        (full_recent_card_height - guidance_stack_gap - utility_card_height).max(52.0);
    WelcomeWorkbenchMetrics {
        sm_line_height,
        xs_line_height,
        recent_row_height,
        utility_row_height,
        guidance_stack_gap,
        primary_tile_height,
    }
}

fn welcome_workbench_layout(compact: bool, recent: gpui::Div, guidance: gpui::Div) -> gpui::Div {
    let container = div()
        .w_full()
        .min_w_0()
        .grid()
        .grid_cols(if compact { 1 } else { 2 })
        .items_start()
        .gap(px(if compact { 22.0 } else { 28.0 }));
    if compact {
        container
            .child(guidance.w_full().min_w_0())
            .child(recent.w_full().min_w_0())
    } else {
        container
            .child(recent.w_full().min_w_0())
            .child(guidance.w_full().min_w_0())
    }
}

#[cfg(test)]
mod welcome_layout_tests {
    use super::*;
    use gpui::{Render, TestAppContext, size};

    struct WelcomeLayout;
    impl Render for WelcomeLayout {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().px(px(WELCOME_PAGE_PADDING)).child(
                welcome_workbench_layout(
                    welcome_layout_is_compact(f32::from(window.viewport_size().width)),
                    div()
                        .child(div().h(px(100.0)))
                        .debug_selector(|| "recent".into()),
                    div()
                        .child(div().h(px(120.0)))
                        .debug_selector(|| "guidance".into()),
                )
                .max_w(px(860.0))
                .debug_selector(|| "workbench".into()),
            )
        }
    }

    struct WelcomeFullWorkbenchFixture {
        tokens: oxideterm_theme::ThemeTokens,
        recent_count: usize,
    }

    impl Render for WelcomeFullWorkbenchFixture {
        fn render(&mut self, _window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let metrics = welcome_workbench_metrics(&self.tokens);
            let sample_rows = [
                ("家里的主机本地", "lipsc@192.168.31.196:22"),
                ("rdp://192.168.31.196:3389", "lipsc@192.168.31.196:3389"),
                ("家里的主机", "lipsc@100.118.61.75:22"),
                ("lipsc(tailscale)", "lipsc@100.118.61.75:22"),
            ];
            let mut recent_list = div().w_full().flex().flex_col();
            for (index, (name, subtitle)) in
                sample_rows.into_iter().take(self.recent_count).enumerate()
            {
                if index > 0 {
                    recent_list = recent_list.child(div().h(px(1.0)));
                }
                recent_list = recent_list.child(
                    div()
                        .w_full()
                        .h(px(metrics.recent_row_height))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .line_height(px(metrics.sm_line_height))
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .line_height(px(metrics.xs_line_height))
                                        .child(subtitle),
                                ),
                        ),
                );
            }
            let recent_col = div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(div().w_full().h(px(20.0)))
                .child(
                    div()
                        .w_full()
                        .rounded(px(self.tokens.radii.lg))
                        .border_1()
                        .overflow_hidden()
                        .child(recent_list)
                        .debug_selector(|| "recent-card".into()),
                )
                .debug_selector(|| "recent-col".into());

            let mut utility_list = div()
                .w_full()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .overflow_hidden()
                .flex()
                .flex_col();
            for (index, label) in ["导入连接", "会话管理器", "Oxide 云同步"]
                .into_iter()
                .enumerate()
            {
                if index > 0 {
                    utility_list = utility_list.child(div().h(px(1.0)));
                }
                utility_list = utility_list.child(
                    div()
                        .w_full()
                        .h(px(metrics.utility_row_height))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .line_height(px(metrics.sm_line_height))
                                .child(label),
                        ),
                );
            }
            let guidance_col = div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(div().w_full().h(px(20.0)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(metrics.guidance_stack_gap))
                        .child(
                            div()
                                .w_full()
                                .grid()
                                .grid_cols(2)
                                .gap(px(10.0))
                                .child(div().w_full().h(px(metrics.primary_tile_height)).border_1())
                                .child(
                                    div().w_full().h(px(metrics.primary_tile_height)).border_1(),
                                ),
                        )
                        .child(utility_list),
                )
                .debug_selector(|| "guidance-col".into());

            div().size_full().px(px(WELCOME_PAGE_PADDING)).child(
                div()
                    .w_full()
                    .max_w(px(860.0))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(22.0))
                    .child(
                        div().w_full().pb(px(18.0)).border_b_1().child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(14.0))
                                .child(
                                    div()
                                        .size(px(WELCOME_BRAND_BADGE_SIZE))
                                        .flex_none()
                                        .debug_selector(|| "brand-badge".into()),
                                )
                                .child(div().child("OxideTerm")),
                        ),
                    )
                    .child(welcome_workbench_layout(false, recent_col, guidance_col)),
            )
        }
    }

    #[gpui::test]
    fn welcome_sections_resize_without_stretching(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| WelcomeLayout);
        // Exercise side-by-side desktop and stacked compact widths including reverse resize.
        for (width, compact) in [
            (1000.0, false),
            (900.0, false),
            (870.0, false),
            (848.0, false),
            (847.0, true),
            (800.0, true),
            (420.0, true),
            (870.0, false),
        ] {
            cx.simulate_resize(size(px(width), px(800.0)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let frame = cx.debug_bounds("workbench").unwrap();
            let recent = cx.debug_bounds("recent").unwrap();
            let guidance = cx.debug_bounds("guidance").unwrap();
            assert_eq!(recent.size.height, px(100.0), "width={width}");
            assert_eq!(guidance.size.height, px(120.0), "width={width}");
            if compact {
                assert_eq!(guidance.origin.x, frame.origin.x, "width={width}");
                assert_eq!(recent.origin.x, frame.origin.x, "width={width}");
                assert_eq!(guidance.size.width, frame.size.width, "width={width}");
                assert_eq!(recent.size.width, frame.size.width, "width={width}");
                assert_eq!(
                    recent.origin.y - guidance.bottom(),
                    px(22.0),
                    "width={width}"
                );
            } else {
                assert_eq!(recent.origin.x, frame.origin.x, "width={width}");
                assert_eq!(guidance.origin.y, recent.origin.y, "width={width}");
                assert_eq!(recent.size.width, guidance.size.width, "width={width}");
                assert_eq!(
                    guidance.origin.x - recent.right(),
                    px(28.0),
                    "width={width}"
                );
            }
        }
    }

    #[gpui::test]
    fn welcome_four_recent_rows_and_brand_align_across_font_scales(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| WelcomeFullWorkbenchFixture {
            tokens: oxideterm_theme::default_tokens(),
            recent_count: 4,
        });
        cx.simulate_resize(size(px(1000.0), px(800.0)));

        for scale in [12.0 / 14.0, 1.0, 18.0 / 14.0] {
            view.update(cx, |fixture, cx| {
                let mut tokens = oxideterm_theme::default_tokens();
                tokens.apply_ui_font_scale(scale);
                fixture.tokens = tokens;
                fixture.recent_count = 4;
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let brand_badge = cx.debug_bounds("brand-badge").unwrap();
            let recent_col = cx.debug_bounds("recent-col").unwrap();
            let guidance_col = cx.debug_bounds("guidance-col").unwrap();
            assert_eq!(
                brand_badge.origin.x, recent_col.origin.x,
                "brand badge left edge should align with recent column at scale={scale}"
            );
            assert_eq!(
                brand_badge.size.width,
                px(WELCOME_BRAND_BADGE_SIZE),
                "scale={scale}"
            );
            assert_eq!(
                recent_col.bottom(),
                guidance_col.bottom(),
                "4-row recent column and guidance column bottoms should match at scale={scale}"
            );
        }

        // Fewer than 4 recent rows must still shrink naturally instead of stretching.
        view.update(cx, |fixture, cx| {
            fixture.tokens = oxideterm_theme::default_tokens();
            fixture.recent_count = 2;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let recent_col = cx.debug_bounds("recent-col").unwrap();
        let guidance_col = cx.debug_bounds("guidance-col").unwrap();
        assert!(recent_col.bottom() < guidance_col.bottom());
    }
}
