use super::*;
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;

const DETACHED_TERMINAL_POPOVER_WIDTH: f32 = 256.0; // Tauri w-64.
const DETACHED_TERMINAL_POPOVER_MAX_HEIGHT: f32 = 192.0; // Tauri max-h-48.
const DETACHED_TERMINAL_POPOVER_ALPHA: u32 = 0xf2; // Tauri elevated surface.
const DETACHED_TERMINAL_HEADER_ALPHA: u32 = 0x99; // Tauri subtle header.
const DETACHED_TERMINAL_HOVER_ALPHA: u32 = 0x99; // Tauri hover:bg-theme-bg-hover.
const DETACHED_TERMINAL_AMBER: u32 = 0xf59e0b; // Tailwind amber-500.

impl WorkspaceApp {
    pub(super) fn detach_active_local_terminal_from_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        if self.tabs[active_index].kind != TabKind::LocalTerminal {
            return;
        }
        let Some(active_pane_id) = self.tabs[active_index].active_pane_id else {
            return;
        };
        let Some(root_pane) = self.tabs[active_index].root_pane.as_ref().cloned() else {
            return;
        };
        let Some(session_id) = root_pane.session_id_for_pane(active_pane_id) else {
            return;
        };
        let Some(pane) = self.panes.get(&active_pane_id).cloned() else {
            return;
        };

        let pane_ref = pane.read(cx);
        let title = pane_ref.title().to_string();
        let session = pane_ref.shared_session();
        let buffer_lines = pane_ref.buffer_line_count();
        self.detached_local_terminals.insert(
            session_id,
            DetachedLocalTerminalSession {
                session_id,
                title: title.clone(),
                session,
                detached_at: Instant::now(),
                buffer_lines,
            },
        );

        if root_pane.pane_count() <= 1 {
            self.remove_local_terminal_tab_at_index_without_shutdown(active_index, window, cx);
        } else {
            self.remove_active_local_pane_without_shutdown(
                active_index,
                active_pane_id,
                window,
                cx,
            );
        }

        self.detached_local_terminals_popover_open = true;
        self.push_command_palette_toast(
            self.i18n.t("local_shell.toast.detached"),
            Some(self.i18n_replace("local_shell.toast.detached_desc", &[("shell", title)])),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn cleanup_dead_local_terminal_sessions_from_palette(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let stale_sessions = self
            .detached_local_terminals
            .iter()
            .filter_map(|(session_id, session)| {
                let lifecycle = session.session.lock().lifecycle();
                (!lifecycle.is_running()).then_some(*session_id)
            })
            .collect::<Vec<_>>();
        for session_id in &stale_sessions {
            self.detached_local_terminals.remove(session_id);
        }
        if self.detached_local_terminals.is_empty() {
            self.detached_local_terminals_popover_open = false;
        }
        self.push_command_palette_toast(
            self.i18n_replace(
                "command_palette.cleanup_result",
                &[("count", stale_sessions.len().to_string())],
            ),
            None,
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    fn remove_active_local_pane_without_shutdown(
        &mut self,
        active_index: usize,
        active_pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_terminal_pane(&active_pane_id);

        let tab = &mut self.tabs[active_index];
        let Some(root_pane) = tab.root_pane.as_mut() else {
            return;
        };
        if let Some(next_active) = root_pane.close_pane(active_pane_id) {
            if let Some(replacement) = root_pane.single_child_replacement() {
                tab.root_pane = Some(replacement);
            }
            tab.active_pane_id = Some(next_active);
        }
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
    }

    fn remove_local_terminal_tab_at_index_without_shutdown(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_active_tab_id = self.main_window_tabs.active_tab_id;
        let removed_was_active = self.tabs.get(index).map(|tab| tab.id) == old_active_tab_id;
        let tab = self.tabs.remove(index);
        let mut pane_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_pane_ids(&mut pane_ids);
        }
        for pane_id in pane_ids {
            self.remove_terminal_pane(&pane_id);
        }
        self.main_window_tabs.active_tab_id = if self.tabs.is_empty() {
            None
        } else if !removed_was_active
            && old_active_tab_id.is_some_and(|tab_id| self.tabs.iter().any(|tab| tab.id == tab_id))
        {
            old_active_tab_id
        } else {
            Some(self.tabs[index.min(self.tabs.len() - 1)].id)
        };
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
    }

    pub(super) fn attach_detached_local_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(detached) = self.detached_local_terminals.remove(&session_id) else {
            return;
        };
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let title = detached.title.clone();
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let session = detached.session.clone();
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(session, preferences, window, cx)
                .expect("failed to resume detached local terminal pane")
        });

        self.register_terminal_pane(pane_id, detached.session_id, pane.clone(), cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: title.clone(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, detached.session_id)),
            active_pane_id: Some(pane_id),
        });
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        if self.detached_local_terminals.is_empty() {
            self.detached_local_terminals_popover_open = false;
        }
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        self.push_command_palette_toast(
            self.i18n.t("local_shell.toast.attached"),
            Some(title),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn kill_detached_local_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        if let Some(detached) = self.detached_local_terminals.remove(&session_id) {
            detached.session.lock().shutdown();
        }
        if self.detached_local_terminals.is_empty() {
            self.detached_local_terminals_popover_open = false;
        }
        cx.notify();
    }

    pub(super) fn visible_local_terminal_session_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|tab| tab.kind == TabKind::LocalTerminal)
            .map(|tab| tab.root_pane.as_ref().map_or(0, PaneNode::pane_count))
            .sum()
    }

    pub(super) fn render_detached_local_terminals_popover(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.detached_local_terminals_popover_open || self.detached_local_terminals.is_empty() {
            return None;
        }
        let theme = self.tokens.ui;
        let sessions = self.sorted_detached_local_terminal_sessions();
        self.sync_detached_local_terminal_list_state(&sessions);
        let list_height = (sessions.len() as f32 * DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT)
            .min(DETACHED_TERMINAL_POPOVER_MAX_HEIGHT);
        let state = self.detached_local_terminal_list_state.clone();
        let spec = self.detached_local_terminal_list_spec();
        let workspace = cx.entity();
        let list = div()
            .id("detached-local-terminals-scroll")
            .h(px(list_height))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_detached_local_terminal_list_item(index, cx)
                    })
                },
            ));

        let popover = div()
            .absolute()
            .left(px(self.tokens.metrics.activity_bar_width))
            .bottom(px(4.0))
            .w(px(DETACHED_TERMINAL_POPOVER_WIDTH))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba(
                (theme.bg_elevated << 8) | DETACHED_TERMINAL_POPOVER_ALPHA,
            ))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    // Browser popovers clip the painted header through the
                    // parent radius; GPUI must align to the inner border curve.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                    .bg(rgba((theme.bg << 8) | DETACHED_TERMINAL_HEADER_ALPHA))
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("local_shell.background.title")),
                    )
                    .child(
                        div()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.detached_local_terminals_popover_open = false;
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(list);

        Some(oxideterm_gpui_ui::motion::slide_fade_in_y(
            &self.tokens,
            "detached-local-terminals-popover-enter",
            popover,
            4.0,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        ))
    }

    fn sorted_detached_local_terminal_sessions(&self) -> Vec<DetachedLocalTerminalSession> {
        let mut sessions = self
            .detached_local_terminals
            .values()
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.detached_at.cmp(&right.detached_at));
        sessions
    }

    fn sync_detached_local_terminal_list_state(
        &mut self,
        sessions: &[DetachedLocalTerminalSession],
    ) {
        let signatures = sessions
            .iter()
            .map(detached_local_terminal_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.detached_local_terminal_list_state,
            &mut self.detached_local_terminal_list_cache.borrow_mut(),
            "detached-local-terminals",
            &signatures,
            self.detached_local_terminal_list_spec(),
        );
    }

    fn detached_local_terminal_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT),
            DETACHED_LOCAL_TERMINAL_LIST_OVERSCAN,
        )
    }

    fn render_detached_local_terminal_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sessions = self.sorted_detached_local_terminal_sessions();
        let Some(session) = sessions.get(index).cloned() else {
            return div().into_any_element();
        };
        self.render_detached_local_terminal_row(session, cx)
    }

    fn render_detached_local_terminal_row(
        &self,
        session: DetachedLocalTerminalSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let session_id = session.session_id;
        let buffer_lines = session
            .session
            .lock()
            .snapshot()
            .lines
            .len()
            .max(session.buffer_lines);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .hover(|row| row.bg(rgba((theme.bg_hover << 8) | DETACHED_TERMINAL_HOVER_ALPHA)))
            .child(Self::render_lucide_icon(
                LucideIcon::Square,
                14.0,
                rgb(DETACHED_TERMINAL_AMBER),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(rgb(theme.text))
                            .truncate()
                            .child(session.title),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Clock,
                                        10.0,
                                        rgb(theme.text_muted),
                                    ))
                                    .child(format_duration(session.detached_at.elapsed())),
                            )
                            .child(format!(
                                "{} {}",
                                buffer_lines,
                                self.i18n.t("local_shell.background.lines")
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgba((theme.accent << 8) | 0x33)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Play,
                                12.0,
                                rgb(theme.accent),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.attach_detached_local_terminal_session(
                                        session_id, window, cx,
                                    );
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgba(0xef444433)))
                            .child(Self::render_lucide_icon(LucideIcon::X, 12.0, rgb(0xf87171)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.kill_detached_local_terminal_session(session_id, cx);
                                }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn detached_local_terminal_signature(session: &DetachedLocalTerminalSession) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Session id is the stable row key. Title and retained buffer count affect
    // row content, while elapsed time only changes text in-place and keeps the
    // same row height.
    session.session_id.hash(&mut hasher);
    session.title.hash(&mut hasher);
    session.buffer_lines.hash(&mut hasher);
    hasher.finish()
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
