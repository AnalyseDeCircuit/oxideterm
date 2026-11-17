use super::*;

impl HostToolsEntity {
    // ---- panel ----

    /// The provider-tab branch of the multiplexer tool: one header, one search
    /// box, one status row and the session tree for the selected connection.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_mux_panel(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        search_ime: &HostToolsPlainTextImeFrame,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let same_host = self.host_tmux.mux.host == selected_id;
        let snapshot = same_host
            .then(|| self.host_tmux.mux.snapshot.clone())
            .flatten();
        let last_refreshed_ms = same_host
            .then_some(self.host_tmux.mux.sessions_refreshed_ms)
            .flatten();
        // One search pass for the whole panel: the query is normalized once and
        // every listed subtree is walked once, so the status-row count and the
        // list below read the same match answers instead of re-testing the same
        // rows for every render decision.
        let search = MuxSearchPass::new(
            &self.ui.host_tmux_search_query,
            snapshot.as_ref(),
            &self.host_tmux.mux.details,
        );
        let visible_sessions: Vec<&MuxSession> = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .sessions
                    .iter()
                    .filter(|session| {
                        search
                            .matches_session(session, self.host_tmux.mux.details.get(&session.name))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let header = div()
            .flex_none()
            .w_full()
            .min_w_0()
            .px_3()
            .pt_3()
            .pb_2()
            .flex()
            .flex_col()
            .gap_2()
            .border_b_1()
            .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .child(self.render_connection_switcher(
                connections,
                selected_id,
                self.host_tmux.mux.snapshot_running.is_some(),
                tokens,
                mono_font_family.clone(),
                selectable_text,
                cx,
            ))
            .child(self.render_mux_tabs(cx, tokens))
            .child(self.render_host_tmux_search(search_ime, tokens, i18n, cx))
            .child(self.render_mux_status_row(
                selected_id,
                visible_sessions.len(),
                last_refreshed_ms,
                terminal_available,
                tokens,
                i18n,
                cx,
            ));
        let body = self.render_mux_body(
            selected_id,
            snapshot.as_ref(),
            &visible_sessions,
            &search,
            terminal_available,
            tokens,
            i18n,
            mono_font_family,
            selectable_text,
            cx,
        );

        div()
            .id("host-mux-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(body)
            .into_any_element()
    }

    pub(crate) fn render_mux_tabs(
        &self,
        cx: &mut Context<Self>,
        tokens: &ThemeTokens,
    ) -> AnyElement {
        let mut tabs = oxideterm_gpui_ui::tabs::tabs_list(tokens);
        for provider in [None, Some(Multiplexer::Zellij), Some(Multiplexer::Herdr)] {
            let label = match provider {
                // The tab bar labels each provider by its display name, which
                // starts with a capital like the sidebar entry; the tool itself
                // keeps the collective name.
                None => "Tmux",
                Some(provider) => provider.name(),
            };
            tabs = tabs.child(
                oxideterm_gpui_ui::tabs::tabs_trigger(
                    tokens,
                    label.into(),
                    self.host_tmux.mux.provider == provider,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |tools, _, _, cx| {
                        if tools.host_tmux.mux.provider != provider {
                            tools.host_tmux.mux.clear_host();
                            tools.host_tmux.mux.provider = provider;
                            // A dialog or viewer opened for the previous
                            // provider describes objects this one does not
                            // have, so both close with the host state.
                            tools.dismiss_mux_input_dialog(cx);
                            tools.dismiss_mux_read_dialog(cx);
                            tools.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        tabs.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_status_row(
        &self,
        host: &str,
        visible_sessions: usize,
        last_refreshed_ms: Option<u64>,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let left = match last_refreshed_ms {
            Some(ms) => format!(
                "{} {} · {} {}",
                visible_sessions,
                i18n.t("sidebar.multiplexer.sessions_title"),
                i18n.t("sidebar.multiplexer.last_refreshed"),
                format_mux_clock(ms)
            ),
            None => format!(
                "{} {}",
                visible_sessions,
                i18n.t("sidebar.multiplexer.sessions_title")
            ),
        };
        let windows = self.mux_windows(host);
        // A workspace needs a session to live in; the expanded one, otherwise
        // the first listed session.
        let context_session = self.mux_action_session();
        let new_workspace_supported = match (self.mux_provider(), context_session.as_deref()) {
            (Some(provider), Some(session)) => self.mux_verb_supported(
                provider,
                windows,
                session,
                None,
                MuxVerbKind::CreateWorkspace,
            ),
            _ => false,
        };
        let default_attach = self
            .mux_provider()
            .and_then(|provider| provider.attach_command(windows, None));
        let host = host.to_owned();
        let attach_host = host.clone();
        let new_workspace_label = i18n.t("sidebar.multiplexer.new_workspace");
        let open_label = i18n.t("sidebar.multiplexer.open");
        let attach_title = i18n.t("sidebar.multiplexer.attach_title");
        let attach_opened = i18n.t("sidebar.multiplexer.toast.attach_opened");
        let attach_missing = i18n.t("sidebar.multiplexer.toast.connection_missing");
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().child(left))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .when(new_workspace_supported, |row| {
                        row.child(host_tools_tooltip_icon_button(
                            tokens,
                            "lucide/plus.svg",
                            13.0,
                            rgb(theme.text),
                            oxideterm_gpui_ui::button::IconButtonOptions {
                                size: 24.0,
                                disabled: self.mux_action_running(),
                                has_background: true,
                                background: Some(rgb(theme.bg_hover)),
                                hover_background: Some(rgb(theme.bg_panel)),
                                idle_opacity: 1.0,
                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                            },
                            new_workspace_label,
                            "host-mux-new-workspace",
                            true,
                            cx.listener(move |tools, _event, window, cx| {
                                let Some(session) = tools.mux_action_session() else {
                                    cx.emit(HostToolsEvent::ShowNotice(
                                        HostToolsNotice::MuxConnectionMissing,
                                    ));
                                    return;
                                };
                                tools.open_mux_input_dialog(
                                    MuxInputDialogKind::NewWorkspace,
                                    &session,
                                    "",
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/terminal.svg",
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: default_attach.is_none() || !terminal_available,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        open_label,
                        "host-mux-open",
                        true,
                        cx.listener(move |tools, _event, window, cx| {
                            let Some(command) = default_attach.clone() else {
                                return;
                            };
                            tools.dispatch_mux_terminal(
                                &attach_host,
                                command,
                                attach_title.clone(),
                                attach_opened.clone(),
                                attach_missing.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.host_tmux.mux.snapshot_running.is_some(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.multiplexer.actions.refresh"),
                        "host-mux-refresh",
                        true,
                        cx.listener(move |tools, _event, _window, cx| {
                            tools.refresh_mux_sessions(
                                host.clone(),
                                HostSnapshotFeedback::Toast,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_body(
        &self,
        host: &str,
        snapshot: Option<&MuxSnapshot>,
        sessions: &[&MuxSession],
        search: &MuxSearchPass,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let center = |icon: &'static str, color: u32, text: String, cx: &mut Context<Self>| {
            host_tools_center_state(icon, color, text, selectable_text, cx)
        };
        let Some(snapshot) = snapshot else {
            return center(
                "lucide/bot.svg",
                theme.text_muted,
                i18n.t("sidebar.multiplexer.sampling"),
                cx,
            );
        };
        match &snapshot.status {
            MuxStatus::Unavailable => {
                return center(
                    "lucide/bot.svg",
                    theme.text_muted,
                    i18n.t("sidebar.multiplexer.unavailable"),
                    cx,
                );
            }
            MuxStatus::ServerNotRunning => {
                return center(
                    "lucide/bot.svg",
                    theme.text_muted,
                    i18n.t("sidebar.multiplexer.server_not_running"),
                    cx,
                );
            }
            // The provider's own message is bounded by the model; anything else
            // falls back to the localized failure text.
            MuxStatus::Error(Some(message)) => {
                return center(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.multiplexer.error")
                        .replace("{{error}}", message),
                    cx,
                );
            }
            MuxStatus::Error(None) => {
                return center(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.multiplexer.failed"),
                    cx,
                );
            }
            MuxStatus::Available => {}
        }
        // The panel filtered these sessions through this same pass, so the
        // count in the status row and the list here cannot disagree.
        if sessions.is_empty() {
            // A query that hides every session must not read as "no sessions
            // exist", so the empty state names the search instead.
            let empty_key = if !search.is_active() {
                "sidebar.multiplexer.empty"
            } else {
                "sidebar.multiplexer.search_empty"
            };
            return center("lucide/bot.svg", theme.text_muted, i18n.t(empty_key), cx);
        }
        let windows = self.mux_windows(host);
        let mut list = div()
            .id("host-mux-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(self.render_mux_sessions_header(windows, tokens, i18n, cx));
        for &session in sessions {
            list = list.child(self.render_mux_session_row(
                host,
                session,
                search,
                terminal_available,
                windows,
                tokens,
                i18n,
                mono_font_family.clone(),
                cx,
            ));
        }
        list.into_any_element()
    }

    /// Only a provider with a non-interactive create-session verb gets the
    /// header button; `new_session_command` answers that with a fixed name.
    fn render_mux_sessions_header(
        &self,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let provider = self.mux_provider()?;
        if provider
            .new_session_command(windows, MUX_VERB_PROBE)
            .is_none()
        {
            return None;
        }
        let theme = tokens.ui;
        Some(
            div()
                .w_full()
                .px_3()
                .pt_3()
                .pb_1()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                // The status row above already counts the sessions, so this
                // row carries only the create affordance.
                .child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/plus.svg",
                    11.0,
                    rgb(theme.text),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 18.0,
                        has_background: true,
                        background: Some(rgb(theme.bg_hover)),
                        hover_background: Some(rgb(theme.bg_panel)),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(18.0)
                    },
                    i18n.t("sidebar.multiplexer.actions.new_session"),
                    "host-mux-new-session",
                    true,
                    cx.listener(move |tools, _event, window, cx| {
                        // A new session defines its own name; no context session.
                        tools.open_mux_input_dialog(
                            MuxInputDialogKind::NewSession,
                            "",
                            "",
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ))
                .into_any_element(),
        )
    }

    pub(super) fn mux_provider(&self) -> Option<Multiplexer> {
        self.host_tmux.mux.provider
    }

    /// Whether the model can build a command for this verb at all. The panel
    /// lays a button out only when an answer exists, so the built line is
    /// dropped here; the probe payload is a fixed literal that survives the
    /// model's grammar check without meaning anything to the provider.
    pub(super) fn mux_verb_supported(
        &self,
        provider: Multiplexer,
        windows: bool,
        session: &str,
        node: Option<&MuxNode>,
        kind: MuxVerbKind,
    ) -> bool {
        provider
            .verb_command(windows, session, node, &mux_probe_verb(kind))
            .is_some()
    }

    pub(super) fn dispatch_mux_terminal(
        &mut self,
        host: &str,
        command: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if host == LOCAL_HOST_CONNECTION_ID {
            window.dispatch_action(
                Box::new(HostToolsWindowRequest::new(
                    HostToolsWindowIntent::OpenLocalMultiplexer {
                        command,
                        title,
                        opened_notice,
                        missing_notice,
                    },
                )),
                cx,
            );
            // The workspace handler owns both copies: it reports the open once
            // the tab exists, so a failed launch is never covered by success.
            return;
        }
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id: host.to_string(),
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }
}

impl HostToolsEntity {
    // ---- session rows ----

    /// Element identity for a multiplexer row. The provider and the owning
    /// session have to be part of it: the provider tabs reuse the same session
    /// names and two trees can hand out the same object id, so a shorter key
    /// would carry hover, focus and expansion state onto a row that is not the
    /// same row.
    pub(super) fn mux_row_identity(
        provider: Multiplexer,
        session: &str,
        node: Option<&MuxNode>,
    ) -> String {
        match node {
            Some(node) => format!(
                "{}:{}:{:?}:{}",
                provider.name(),
                session,
                node.kind,
                node.id
            ),
            None => format!("{}:{}", provider.name(), session),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_session_row(
        &self,
        host: &str,
        session: &MuxSession,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let Some(provider) = self.mux_provider() else {
            return div().into_any_element();
        };
        let details = self.host_tmux.mux.details.get(&session.name);
        // A search reveals the branches it matched without touching the
        // click-managed expansion state.
        let descendant_match = details.is_some_and(|tree| search.matches_inside(tree));
        let expanded = self.host_tmux.mux.session.as_deref() == Some(session.name.as_str())
            || descendant_match;
        // An attach starts a stopped session, so the action stays enabled while
        // only the name dims to read as inactive.
        let name_color = if session.running {
            theme.text
        } else {
            theme.text_muted
        };
        let attach_command = provider.attach_command(windows, Some(&session.name));
        let attach_label = i18n.t("sidebar.multiplexer.actions.session_attach");
        let attach_opened = i18n.t("sidebar.multiplexer.toast.attach_opened");
        let attach_missing = i18n.t("sidebar.multiplexer.toast.connection_missing");
        let attach_title = mux_row_title(provider, &session.name);
        let attach_host = host.to_owned();
        let row_host = host.to_owned();
        let session_name = session.name.clone();
        let header = div()
            .id(host_tools_row_id(
                "host-mux-session",
                host,
                Self::mux_row_identity(provider, &session.name, None),
            ))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(34.0))
                    .pl(px(10.0))
                    .pr_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_none().child(
                            svg()
                                .path(if expanded {
                                    "lucide/chevron-down.svg"
                                } else {
                                    "lucide/chevron-right.svg"
                                })
                                .size(px(11.0))
                                .text_color(rgb(theme.text_muted)),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(name_color))
                            .child(session.name.clone()),
                    )
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/cable.svg",
                        12.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 22.0,
                            disabled: attach_command.is_none() || !terminal_available,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                        },
                        attach_label,
                        "host-mux-session-attach",
                        true,
                        cx.listener(move |tools, _event, window, cx| {
                            let Some(command) = attach_command.clone() else {
                                return;
                            };
                            tools.dispatch_mux_terminal(
                                &attach_host,
                                command,
                                attach_title.clone(),
                                attach_opened.clone(),
                                attach_missing.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .children(self.mux_verb_buttons(
                        &row_host,
                        provider,
                        &session.name,
                        None,
                        session.running,
                        windows,
                        12.0,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let session_name = session.name.clone();
                    move |tools, _event, _window, cx| {
                        tools.toggle_mux_session(&session_name, cx);
                        cx.stop_propagation();
                    }
                }),
            );

        let mut container = div().w_full().min_w_0().flex().flex_col().child(header);
        if expanded {
            container = container.children(self.render_mux_session_children(
                host,
                provider,
                &session_name,
                search,
                terminal_available,
                windows,
                tokens,
                i18n,
                mono_font_family,
                cx,
            ));
        }
        container.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_session_children(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = tokens.ui;
        let muted_child = |text: String| {
            div()
                .w_full()
                .min_w_0()
                .pl(px(45.0))
                .pr_3()
                .py(px(6.0))
                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element()
        };
        if self.host_tmux.mux.detail_failed.contains(session) {
            return vec![muted_child(i18n.t("sidebar.multiplexer.failed"))];
        }
        let Some(tree) = self.host_tmux.mux.details.get(session) else {
            // Expanding requests the subtree, and a refresh re-requests the
            // session that is still open.
            return vec![muted_child(i18n.t("sidebar.multiplexer.sampling"))];
        };
        let roots: Vec<&MuxNode> = tree
            .iter()
            .filter(|node| search.matches_subtree(node))
            .collect();
        if roots.is_empty() {
            // A query that only reached the session name has nothing to show
            // below it, and a blank area reads as a failed capture, so the
            // expansion says the search found no object here. Without a query
            // the session really is empty and keeps its own empty state.
            return if search.is_active() {
                vec![muted_child(
                    i18n.t("sidebar.multiplexer.search_child_empty"),
                )]
            } else {
                vec![muted_child(i18n.t("sidebar.multiplexer.workspace_empty"))]
            };
        }
        roots
            .into_iter()
            .map(|node| {
                self.render_mux_node(
                    host,
                    provider,
                    session,
                    node,
                    0,
                    search,
                    terminal_available,
                    windows,
                    tokens,
                    i18n,
                    mono_font_family.clone(),
                    cx,
                )
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_mux_node(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        node: &MuxNode,
        depth: usize,
        search: &MuxSearchPass,
        terminal_available: bool,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let key = (node.kind, node.id.clone());
        // A match below this row reveals the branch without a click.
        let descendant_match = search.matches_inside(&node.children);
        let expanded = self.host_tmux.mux.expanded.contains(&key) || descendant_match;
        let show_chevron = !node.children.is_empty();
        // The cwd stays visible as a hint unless it already is the label.
        let cwd_hint = node
            .cwd
            .clone()
            .filter(|cwd| cwd.as_str() != node.label.as_str());
        let agent_attach = node.orphan_agent.then(|| {
            (
                provider.agent_attach_command(windows, session, node),
                i18n.t("sidebar.multiplexer.actions.agent_attach"),
                mux_row_title(provider, session),
                i18n.t("sidebar.multiplexer.toast.attach_opened"),
                i18n.t("sidebar.multiplexer.toast.connection_missing"),
                host.to_owned(),
            )
        });
        let row_host = host.to_owned();
        let row_session = session.to_owned();
        let mono = mono_font_family.clone();
        let mut line = div()
            .w_full()
            .min_w_0()
            .when(node.kind != MuxKind::Pane, |inner| {
                inner.h(px(if node.kind == MuxKind::Workspace {
                    36.0
                } else {
                    32.0
                }))
            })
            .when(node.kind == MuxKind::Pane, |inner| inner.py(px(6.0)))
            .pl(px(10.0 + (depth as f32 + 1.0) * MUX_ROW_INDENT))
            .pr_3()
            .flex()
            .items_center()
            .gap_2()
            .when(show_chevron, |inner| {
                inner.child(
                    div().flex_none().child(
                        svg()
                            .path(if expanded {
                                "lucide/chevron-down.svg"
                            } else {
                                "lucide/chevron-right.svg"
                            })
                            .size(px(11.0))
                            .text_color(rgb(theme.text_muted)),
                    ),
                )
            })
            .child(render_lucide_icon(
                match node.kind {
                    MuxKind::Workspace => "lucide/folder.svg",
                    MuxKind::Tab => "lucide/monitor.svg",
                    MuxKind::Pane => "lucide/terminal.svg",
                },
                13.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .when(node.kind == MuxKind::Pane, |label| {
                        label.font_family(mono.clone())
                    })
                    .child(node.label.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t(match node.kind {
                        MuxKind::Workspace => "sidebar.multiplexer.row_space",
                        MuxKind::Tab => "sidebar.multiplexer.row_tab",
                        MuxKind::Pane if node.orphan_agent => "sidebar.multiplexer.orphan_agent",
                        MuxKind::Pane => "sidebar.multiplexer.row_pane",
                    })),
            )
            .when(node.tab_count > 0, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(mux_count_label(
                            node.tab_count,
                            &i18n.t("sidebar.multiplexer.tabs_suffix_one"),
                            &i18n.t("sidebar.multiplexer.tabs_suffix"),
                        )),
                )
            })
            .when(node.pane_count > 0, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(mux_count_label(
                            node.pane_count,
                            &i18n.t("sidebar.multiplexer.panes_suffix_one"),
                            &i18n.t("sidebar.multiplexer.panes_suffix"),
                        )),
                )
            })
            .when_some(cwd_hint, |inner, cwd| {
                inner.child(
                    div()
                        .flex_none()
                        .min_w_0()
                        .max_w(px(220.0))
                        .truncate()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(cwd),
                )
            })
            .when(node.restore_failed, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(MONITOR_RED))
                        .child(i18n.t("sidebar.multiplexer.restore_error")),
                )
            })
            .when_some(
                self.render_mux_status_chip(node.status.as_deref(), i18n),
                |inner, chip| inner.child(chip),
            )
            .when(node.focused, |inner| {
                inner.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.accent))
                        .child(i18n.t("sidebar.multiplexer.focused")),
                )
            });
        line = match agent_attach {
            // A detached agent has no live pane to act on, so its only verb is
            // pointing a terminal at the agent itself.
            Some((command, label, title, opened, missing, host)) => {
                line.child(host_tools_tooltip_icon_button(
                    tokens,
                    "lucide/cable.svg",
                    12.0,
                    rgb(theme.text),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 22.0,
                        disabled: command.is_none() || !terminal_available,
                        has_background: true,
                        background: Some(rgb(theme.bg_hover)),
                        hover_background: Some(rgb(theme.bg_panel)),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                    },
                    label,
                    "host-mux-agent-attach",
                    false,
                    cx.listener(move |tools, _event, window, cx| {
                        let Some(command) = command.clone() else {
                            return;
                        };
                        tools.dispatch_mux_terminal(
                            &host,
                            command,
                            title.clone(),
                            opened.clone(),
                            missing.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ))
            }
            None => line.children(self.mux_verb_buttons(
                &row_host,
                provider,
                &row_session,
                Some(node),
                true,
                windows,
                if node.kind == MuxKind::Pane {
                    12.0
                } else {
                    13.0
                },
                tokens,
                i18n,
                cx,
            )),
        };

        // The row is the identity, hover and expansion target; the line inside
        // it is the single flex row that carries the label and the verbs, so
        // buttons stay on the same baseline instead of stacking below.
        let row = div()
            .id(host_tools_row_id(
                "host-mux-node",
                host,
                Self::mux_row_identity(provider, session, Some(node)),
            ))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .when(show_chevron, |outer| {
                outer
                    .cursor_pointer()
                    .hover(|hovered| hovered.bg(rgb(theme.bg_hover)))
            })
            .when(show_chevron, |outer| {
                let key = key.clone();
                outer.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |tools, _event, _window, cx| {
                        // Expansion is click-owned; a search-revealed branch
                        // stays outside this set.
                        if !tools.host_tmux.mux.expanded.remove(&key) {
                            tools.host_tmux.mux.expanded.insert(key.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )
            })
            .child(line);

        let mut container = div().w_full().min_w_0().flex().flex_col().child(row);
        if expanded {
            container = container.children(
                node.children
                    .iter()
                    .filter(|child| search.matches_subtree(child))
                    .map(|child| {
                        self.render_mux_node(
                            host,
                            provider,
                            session,
                            child,
                            depth + 1,
                            search,
                            terminal_available,
                            windows,
                            tokens,
                            i18n,
                            mono_font_family.clone(),
                            cx,
                        )
                    }),
            );
        }
        container.into_any_element()
    }

    /// A chip exists only for a reported agent state; "unknown" is the absence
    /// of one and would otherwise repeat on every row.
    fn render_mux_status_chip(&self, status: Option<&str>, i18n: &I18n) -> Option<Div> {
        let status = status?;
        let color = match status {
            "working" => MONITOR_EMERALD,
            "blocked" => MONITOR_RED,
            "idle" | "done" => MONITOR_AMBER,
            _ => return None,
        };
        Some(
            div()
                .flex_none()
                .text_size(px(10.0))
                .text_color(rgb(color))
                .child(i18n.t(status_label_key(status))),
        )
    }
}
