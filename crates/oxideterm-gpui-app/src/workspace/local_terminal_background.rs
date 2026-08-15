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
        let Some(active_index) = self.active_tab_index(cx) else {
            return;
        };
        if self.tabs(cx)[active_index].kind != TabKind::LocalTerminal {
            return;
        }
        let Some(active_pane_id) = self.tabs(cx)[active_index].active_pane_id else {
            return;
        };
        let Some(root_pane) = self.tabs(cx)[active_index].root_pane.as_ref().cloned() else {
            return;
        };
        let Some(session_id) = root_pane.session_id_for_pane(active_pane_id) else {
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&active_pane_id).cloned() else {
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
            cx,
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
            self.ai_runtime_context.update(cx, |runtime, _cx| {
                runtime.revoke_terminal_session(*session_id);
            });
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
            cx,
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
        if let Some(session_id) = self.tabs(cx)[active_index]
            .root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(active_pane_id))
        {
            self.release_public_mcp_terminal_for_closed_session(session_id, cx);
        }
        self.remove_terminal_pane(&active_pane_id, cx);

        let tab_id = self.tabs(cx)[active_index].id;
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.close_pane(tab_id, active_pane_id);
        });
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
    }

    fn remove_local_terminal_tab_at_index_without_shutdown(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(TabRemovalTransition {
            tab,
            mount_cleanup,
            previous_active_tab_id,
            next_active_tab_id,
        }) = self
            .tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_tab_at(index))
        else {
            return;
        };
        self.apply_tab_mount_cleanup(mount_cleanup, Some(window), cx);
        let mut pane_ids = Vec::new();
        let mut session_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_pane_ids(&mut pane_ids);
            root_pane.collect_session_ids(&mut session_ids);
        }
        for session_id in session_ids {
            self.release_public_mcp_terminal_for_closed_session(session_id, cx);
        }
        for pane_id in pane_ids {
            self.remove_terminal_pane(&pane_id, cx);
        }
        self.apply_main_window_active_tab_change(previous_active_tab_id, next_active_tab_id, cx);
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = self.active_tab(cx).is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
        });
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
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
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let title = detached.title.clone();
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let session = detached.session.clone();
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(session, preferences, window, cx)
                .expect("failed to resume detached local terminal pane")
        });

        self.register_terminal_pane(pane_id, detached.session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::LocalTerminal,
                title: title.clone(),
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, detached.session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, detached.session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        if self.detached_local_terminals.is_empty() {
            self.detached_local_terminals_popover_open = false;
        }
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        self.push_command_palette_toast(
            self.i18n.t("local_shell.toast.attached"),
            Some(title),
            TerminalNoticeVariant::Success,
            cx,
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
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.revoke_terminal_session(session_id);
        });
        if self.detached_local_terminals.is_empty() {
            self.detached_local_terminals_popover_open = false;
        }
        cx.notify();
    }

    pub(super) fn visible_local_terminal_session_count(&self, cx: &App) -> usize {
        self.tabs(cx)
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
        self.sync_detached_local_terminal_order();
        self.sync_detached_local_terminal_list_state();
        let list_height = (self.detached_local_terminal_order.len() as f32
            * DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT)
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

        Some(popover.into_any_element())
    }

    fn sync_detached_local_terminal_order(&mut self) {
        let mut sessions = self
            .detached_local_terminals
            .values()
            .map(|session| (session.session_id, session.detached_at))
            .collect::<Vec<_>>();
        sessions.sort_by_key(|(_, detached_at)| *detached_at);
        self.detached_local_terminal_order = sessions
            .into_iter()
            .map(|(session_id, _)| session_id)
            .collect();
    }

    fn sync_detached_local_terminal_list_state(&mut self) {
        let signatures = self
            .detached_local_terminal_order
            .iter()
            .filter_map(|session_id| self.detached_local_terminals.get(session_id))
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
        let Some(session) = self
            .detached_local_terminal_order
            .get(index)
            .and_then(|session_id| self.detached_local_terminals.get(session_id))
            .cloned()
        else {
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
            .buffer_line_count()
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
