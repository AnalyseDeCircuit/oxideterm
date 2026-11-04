use super::*;
use oxideterm_connection_monitor::multiplexer::{
    Multiplexer, MuxAction, MuxKind, MuxNode, MuxSession,
};
use std::collections::HashSet;
use tokio_util::task::AbortOnDropHandle;
use zeroize::Zeroize;

#[derive(Default)]
pub(super) struct MuxPanel {
    pub(super) provider: Option<Multiplexer>,
    host: String,
    sessions: Vec<MuxSession>,
    session: Option<String>,
    tree: Vec<MuxNode>,
    expanded: HashSet<(MuxKind, String)>,
    error: Option<&'static str>,
    task: Option<gpui::Task<()>>,
}

impl MuxPanel {
    pub(super) fn clear_host(&mut self) {
        *self = Self {
            provider: self.provider,
            ..Default::default()
        };
    }
}

enum Query {
    Sessions,
    Tree(String),
    Action(String, String),
}
enum ResultData {
    Sessions(Vec<MuxSession>),
    Tree(Vec<MuxNode>),
    Action,
}

impl HostToolsEntity {
    pub(super) fn render_mux_tabs(
        &self,
        cx: &mut Context<Self>,
        tokens: &ThemeTokens,
    ) -> AnyElement {
        let mut tabs = oxideterm_gpui_ui::tabs::tabs_list(tokens);
        for provider in [None, Some(Multiplexer::Zellij), Some(Multiplexer::Herdr)] {
            let label = provider.map(Multiplexer::name).unwrap_or("tmux");
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

    pub(super) fn refresh_mux_sessions(&mut self, host: String, cx: &mut Context<Self>) {
        if self.host_tmux.mux.host != host {
            self.host_tmux.mux.clear_host();
            self.host_tmux.mux.host = host;
        }
        self.run_mux_query(Query::Sessions, cx);
    }

    fn run_mux_query(&mut self, query: Query, cx: &mut Context<Self>) {
        let state = &self.host_tmux.mux;
        let host = state.host.clone();
        let Some(provider) = state.provider else {
            return;
        };
        let windows = self
            .connection_os_type(&host)
            .is_some_and(|os| os.eq_ignore_ascii_case("windows"));
        let command = match &query {
            Query::Sessions => Some(provider.list_command(windows)),
            Query::Tree(session) => provider.inspect(windows, session),
            Query::Action(_, command) => Some(command.clone()),
        };
        let (Some(command), Some(target), Some(runtime)) = (
            command,
            self.host_command_target(&host),
            self.lifecycle_runtime.clone(),
        ) else {
            self.host_tmux.mux.error = Some("sidebar.multiplexer.failed");
            cx.notify();
            return;
        };
        let mutation = matches!(&query, Query::Action(..));
        let audit = mutation
            .then(|| self.host_tool_audit(&host, "manage_multiplexer", Some(provider.name())));
        let worker = AbortOnDropHandle::new(runtime.spawn(async move {
            let mut result = target
                .run_command_capture(
                    &command,
                    HOST_TMUX_SNAPSHOT_TIMEOUT,
                    HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await;
            if let Some(audit) = audit {
                super::entity::finish_host_query_audit(audit, &result);
            }
            let parsed = match &mut result {
                Ok(out) => {
                    let parsed = if out.truncated {
                        Err("sidebar.multiplexer.failed")
                    } else if matches!(query, Query::Sessions) && out.exit_code == Some(127) {
                        Err("sidebar.multiplexer.unavailable")
                    } else if matches!(query, Query::Sessions)
                        && provider == Multiplexer::Zellij
                        && out.exit_code == Some(1)
                        && out.stderr.contains("No active zellij sessions found.")
                    {
                        Ok(ResultData::Sessions(vec![]))
                    } else if out.exit_code != Some(0) {
                        Err("sidebar.multiplexer.failed")
                    } else {
                        match &query {
                            Query::Sessions => {
                                provider.sessions(&out.stdout).map(ResultData::Sessions)
                            }
                            Query::Tree(_) => provider.tree(&out.stdout).map(ResultData::Tree),
                            Query::Action(..) => Some(ResultData::Action),
                        }
                        .ok_or("sidebar.multiplexer.failed")
                    };
                    out.stdout.zeroize();
                    out.stderr.zeroize();
                    parsed
                }
                Err(()) => Err("sidebar.multiplexer.failed"),
            };
            (query, parsed)
        }));
        self.host_tmux.mux.error = None;
        self.host_tmux.mux.task = Some(cx.spawn(async move |weak, cx| {
            // Navigation cancels reads. A confirmed mutation finishes on its captured host under the capture timeout.
            let result = if mutation {
                worker.detach().await
            } else {
                worker.await
            };
            let _ = weak.update(cx, |tools, cx| {
                let state = &mut tools.host_tmux.mux;
                state.task = None;
                if state.provider != Some(provider) || state.host != host {
                    return;
                }
                match result {
                    Ok((_, Ok(ResultData::Sessions(sessions)))) => {
                        state.sessions = sessions;
                        if let Some(name) = state.session.clone().filter(|name| {
                            state.sessions.iter().any(|s| s.name == *name && s.running)
                        }) {
                            tools.run_mux_query(Query::Tree(name), cx);
                        } else {
                            state.session = None;
                            state.tree.clear();
                            state.expanded.clear();
                        }
                    }
                    Ok((Query::Tree(name), Ok(ResultData::Tree(tree))))
                        if state.session.as_ref() == Some(&name) =>
                    {
                        state.tree = tree;
                    }
                    Ok((Query::Action(session, _), Ok(ResultData::Action))) => {
                        state.session = Some(session);
                        tools.run_mux_query(Query::Sessions, cx);
                    }
                    Ok((Query::Sessions, Err(key))) => {
                        state.sessions.clear();
                        state.session = None;
                        state.tree.clear();
                        state.expanded.clear();
                        state.error = Some(key);
                    }
                    Ok((_, Err(key))) => {
                        state.error = Some(key);
                        state.tree.clear();
                    }
                    _ => {
                        state.error = Some("sidebar.multiplexer.failed");
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn render_mux_sessions(
        &self,
        host: &str,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.host_tmux.mux;
        let provider = state.provider.unwrap();
        let host = host.to_owned();
        let windows = self
            .connection_os_type(&host)
            .is_some_and(|s| s.eq_ignore_ascii_case("windows"));
        let mut list = div()
            .id("mux-sessions")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3();
        let refresh_host = host.clone();
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(mux_button(
                    tokens,
                    LucideIcon::RefreshCw,
                    i18n.t("sidebar.multiplexer.refresh"),
                    state.task.is_some(),
                    cx.listener(move |tools, _, _, cx| {
                        tools.refresh_mux_sessions(refresh_host.clone(), cx)
                    }),
                ))
                .child(self.mux_terminal_button(&host, provider, None, windows, tokens, i18n, cx)),
        );
        if state.host != host {
            return list.into_any_element();
        }
        if let Some(key) = state.error {
            list = list.child(
                div()
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(i18n.t(key)),
            );
        }
        let query = self.ui.host_tmux_search_query.trim().to_lowercase();
        if state.task.is_some() {
            list = list.child(div().child(i18n.t("sidebar.multiplexer.loading")));
        } else if state.error.is_none()
            && !state
                .sessions
                .iter()
                .any(|s| s.name.to_lowercase().contains(&query))
        {
            list = list.child(div().child(i18n.t("sidebar.multiplexer.empty")));
        }
        for session in state
            .sessions
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query))
        {
            let expanded = state.session.as_ref() == Some(&session.name);
            let name = session.name.clone();
            let running = session.running;
            let busy = state.task.is_some();
            let mut row = div()
                .id(host_tools_row_id(
                    "mux-session",
                    host.as_str(),
                    name.clone(),
                ))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .when(running && !busy, |row| {
                            row.cursor_pointer().on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |tools, _, _, cx| {
                                    let state = &mut tools.host_tmux.mux;
                                    state.tree.clear();
                                    state.expanded.clear();
                                    state.session = (!expanded).then(|| name.clone());
                                    if !expanded {
                                        tools.run_mux_query(Query::Tree(name.clone()), cx);
                                    }
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )
                        })
                        .when(running, |row| {
                            row.child(if expanded { "▾ " } else { "▸ " })
                        })
                        .child(session.name.clone()),
                );
            row = row.child(self.mux_terminal_button(
                &host,
                provider,
                Some(session),
                windows,
                tokens,
                i18n,
                cx,
            ));
            if running {
                row = row.child(self.mux_action_button(
                    &host,
                    provider,
                    session,
                    None,
                    MuxAction::CreateChild,
                    windows,
                    tokens,
                    i18n,
                    cx,
                ));
            }
            list = list.child(row);
            if expanded {
                for node in &state.tree {
                    list = list.child(
                        self.mux_node(&host, provider, session, node, 0, windows, tokens, i18n, cx),
                    );
                }
            }
        }
        list.into_any_element()
    }

    fn mux_terminal_button(
        &self,
        host: &str,
        provider: Multiplexer,
        session: Option<&MuxSession>,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let command = match session {
            None => Some(
                match provider {
                    Multiplexer::Herdr => "herdr",
                    Multiplexer::Zellij => "zellij",
                }
                .to_owned(),
            ),
            Some(s) => provider.attach(windows, &s.name),
        };
        let title = session
            .map(|s| format!("{} · {}", provider.name(), s.name))
            .unwrap_or_else(|| provider.name().into());
        let label = i18n.t(match session {
            Some(s) if !s.running => "window_controls.restore",
            Some(_) => "sidebar.host_tmux.actions.attach",
            None => "sidebar.multiplexer.open",
        });
        let missing = i18n.t("sidebar.multiplexer.failed");
        let host = host.to_owned();
        mux_button(
            tokens,
            LucideIcon::Terminal,
            label,
            command.is_none(),
            cx.listener(move |_, _, window, cx| {
                let Some(command) = command.clone() else {
                    return;
                };
                let intent = if host == LOCAL_HOST_CONNECTION_ID {
                    HostToolsWindowIntent::OpenLocalMultiplexer {
                        command,
                        title: title.clone(),
                        missing_notice: missing.clone(),
                    }
                } else {
                    HostToolsWindowIntent::OpenExistingNodeTerminal {
                        connection_id: host.clone(),
                        command,
                        title: title.clone(),
                        opened_notice: title.clone(),
                        missing_notice: missing.clone(),
                    }
                };
                window.dispatch_action(Box::new(HostToolsWindowRequest::new(intent)), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn mux_node(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &MuxSession,
        node: &MuxNode,
        depth: usize,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = (node.kind, node.id.clone());
        let expanded = self.host_tmux.mux.expanded.contains(&key);
        let mut row = div()
            .id(host_tools_row_id(
                "mux-object",
                host,
                format!("{:?}:{}", node.kind, node.id),
            ))
            .flex()
            .items_center()
            .gap_1()
            .pl(px((depth + 1) as f32 * 12.0))
            .child(WorkspaceApp::render_lucide_icon(
                match node.kind {
                    MuxKind::Workspace => LucideIcon::Folder,
                    MuxKind::Tab => LucideIcon::Monitor,
                    MuxKind::Pane => LucideIcon::Terminal,
                },
                13.,
                rgb(tokens.ui.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(!node.children.is_empty(), |row| {
                        row.child(if expanded { "▾ " } else { "▸ " })
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |tools, _, _, cx| {
                                    let seen = &mut tools.host_tmux.mux.expanded;
                                    if !seen.remove(&key) {
                                        seen.insert(key.clone());
                                    }
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )
                    })
                    .child(if node.label.is_empty() {
                        node.id.clone()
                    } else {
                        node.label.clone()
                    }),
            );
        for action in [MuxAction::Focus, MuxAction::CreateChild, MuxAction::Close] {
            row = row.child(self.mux_action_button(
                host,
                provider,
                session,
                Some(node),
                action,
                windows,
                tokens,
                i18n,
                cx,
            ));
        }
        let mut branch = div().child(row);
        if expanded {
            for child in &node.children {
                branch = branch.child(self.mux_node(
                    host,
                    provider,
                    session,
                    child,
                    depth + 1,
                    windows,
                    tokens,
                    i18n,
                    cx,
                ));
            }
        }
        branch.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn mux_action_button(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &MuxSession,
        node: Option<&MuxNode>,
        action: MuxAction,
        windows: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(command) = provider.action(windows, &session.name, node, action) else {
            return div().into_any_element();
        };
        let (icon, key) = match action {
            MuxAction::Focus => (LucideIcon::Eye, "sidebar.multiplexer.focus"),
            MuxAction::CreateChild => (
                LucideIcon::Plus,
                match (provider, node.map(|n| n.kind)) {
                    (Multiplexer::Herdr, None) => "sidebar.multiplexer.new_workspace",
                    (_, Some(MuxKind::Pane)) | (Multiplexer::Zellij, Some(MuxKind::Tab)) => {
                        "sidebar.multiplexer.split"
                    }
                    _ => "sidebar.multiplexer.new_tab",
                },
            ),
            MuxAction::Close => (LucideIcon::X, "window_controls.close"),
        };
        let host = host.to_owned();
        let session = session.name.clone();
        let close = i18n.t("window_controls.close");
        let cancel = i18n.t("common.actions.cancel");
        let target = self
            .monitor_connections()
            .into_iter()
            .find(|c| c.connection_id == host)
            .map(|c| monitor_connection_label(&c))
            .unwrap_or_else(|| host.clone());
        let desc = format!(
            "{target} · {} · {session} · {}",
            provider.name(),
            node.map(|n| format!("{} ({})", n.label, n.id))
                .unwrap_or_default()
        );
        mux_button(
            tokens,
            icon,
            i18n.t(key),
            self.host_tmux.mux.task.is_some(),
            cx.listener(move |tools, _, window, cx| {
                cx.stop_propagation();
                if tools.selected_connection_id() != Some(host.as_str())
                    || tools.host_tmux.mux.provider != Some(provider)
                    || tools.host_tmux.mux.task.is_some()
                {
                    return;
                }
                if action != MuxAction::Close {
                    tools.run_mux_query(Query::Action(session.clone(), command.clone()), cx);
                    return;
                }
                let prompt = window.prompt(
                    gpui::PromptLevel::Warning,
                    &close,
                    Some(&desc),
                    &[cancel.as_str(), close.as_str()],
                    cx,
                );
                let (host, session, command) = (host.clone(), session.clone(), command.clone());
                cx.spawn(async move |weak, cx| {
                    if matches!(prompt.await, Ok(1)) {
                        let _ = weak.update(cx, |tools, cx| {
                            let state = &tools.host_tmux.mux;
                            if tools.selected_connection_id() == Some(host.as_str())
                                && state.provider == Some(provider)
                                && state.host == host
                                && state.session.as_ref() == Some(&session)
                                && state.task.is_none()
                            {
                                tools.run_mux_query(Query::Action(session, command), cx);
                            }
                        });
                    }
                })
                .detach();
            }),
        )
        .into_any_element()
    }
}

fn mux_button(
    tokens: &ThemeTokens,
    icon: LucideIcon,
    label: String,
    disabled: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    host_tools_tooltip_icon_button(
        tokens,
        icon,
        13.,
        rgb(tokens.ui.text),
        oxideterm_gpui_ui::button::IconButtonOptions {
            disabled,
            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.)
        },
        label,
        "host-mux-control",
        true,
        listener,
    )
}
