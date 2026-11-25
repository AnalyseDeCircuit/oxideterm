use crate::{
    ble::{BleEvent, BleProfile, WebBleLink, is_ble_node},
    host::Workspace,
    transport::{GatewayEvent, GatewayRequest},
};
use futures::{StreamExt, channel::mpsc};
use gpui::{prelude::*, *};
use oxideterm_gpui_terminal_view::*;
use oxideterm_workspace::*;
use zeroize::Zeroizing;

pub struct Session {
    pub view: Entity<TerminalView>,
    pub node: String,
    _events: Subscription,
    _ble_task: Option<Task<()>>,
}

impl Workspace {
    pub fn active_session(&self) -> Option<TerminalSessionId> {
        let tab = self.layout.active_tab()?;
        tab.root_pane
            .as_ref()?
            .session_id_for_pane(tab.active_pane_id?)
    }
    fn metrics(&self, window: &Window, cx: &App) -> TerminalMetrics {
        let settings = &self.settings.read(cx).settings(cx).terminal;
        let configured = settings
            .font_family
            .terminal_family_name(&settings.custom_font_family);
        let font = if cx.text_system().font_id(&font(configured.clone())).is_ok() {
            font(configured)
        } else {
            font("Lilex")
        };
        TerminalMetrics::measure(
            window,
            font,
            px(settings.font_size as f32),
            settings.line_height as f32,
            (settings.character_spacing
                * oxideterm_settings::TERMINAL_CHARACTER_SPACING_BASELINE_SCALE) as f32,
        )
    }
    pub fn refresh_terminals(&self, w: &Window, cx: &mut App) {
        let metrics = self.metrics(w, cx);
        for s in self.sessions.values() {
            s.view.update(cx, |v, cx| {
                v.set_theme(TerminalUiTheme::from_tokens(self.tokens), cx);
                v.set_metrics(metrics.clone(), cx)
            });
        }
    }
    pub fn new_terminal(
        &mut self,
        node: String,
        split: Option<SplitDirection>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_ble_node(&node)
            && split.is_none()
            && let Some(profile) = self.ble_profiles.get(&node).cloned()
        {
            self.open_ble_terminal(node, profile, w, cx);
            return;
        }
        if !self.ready {
            return;
        }
        let pane = self.layout.alloc_pane_id();
        let session = TerminalSessionId(pane.0);
        if let Some(dir) = split {
            let Some((tab, active)) = self
                .layout
                .active_tab()
                .and_then(|t| Some((t.id, t.active_pane_id?)))
            else {
                return;
            };
            let group = self.layout.alloc_pane_id();
            if !self
                .layout
                .split_pane(tab, active, group, dir, pane, session)
            {
                return;
            }
        } else {
            let id = self.layout.alloc_tab_id();
            let is_local = node == "local";
            let profile_name = (!is_local).then(|| {
                self.catalog_data.items.iter().find_map(|row| match &row.target {
                    oxideterm_gpui_connections::session_manager::SessionManagerOpenTarget::Connection(pid)
                        if pid == &node =>
                    {
                        Some(row.name.clone())
                    }
                    _ => None,
                })
            }).flatten();
            let (title, title_source) = if let Some(name) = profile_name {
                (name, TabTitleSource::Static)
            } else {
                (
                    self.i18n.t("menu.terminal"),
                    TabTitleSource::I18nKey("menu.terminal"),
                )
            };
            self.expanded_nodes.insert(node.clone());
            self.layout.insert_and_select_main_tab(Tab {
                id,
                kind: if is_local {
                    TabKind::LocalTerminal
                } else {
                    TabKind::SshTerminal
                },
                title,
                title_source,
                root_pane: Some(PaneNode::leaf(pane, session)),
                active_pane_id: Some(pane),
            });
        }
        let metrics = self.metrics(w, cx);
        let view =
            cx.new(|cx| TerminalView::new(TerminalUiTheme::from_tokens(self.tokens), metrics, cx));
        let events = cx.subscribe_in(&view, w, move |h, _, event, _, cx| match event {
            TerminalViewEvent::SendBytes(bytes) => h.input(session, bytes, cx),
            TerminalViewEvent::Resized(size) => h.send(
                GatewayRequest::Resize {
                    session: session.0,
                    cols: size.cols.min(65535) as u16,
                    rows: size.rows.min(65535) as u16,
                },
                cx,
            ),
            TerminalViewEvent::TitleChanged(Some(title)) => {
                if let Some(id) = h.session_tab(session) {
                    h.layout.rename_terminal_tab(id, title);
                    h.changed(cx);
                }
            }
            TerminalViewEvent::Bell => {
                if let Some(id) = h.session_tab(session) {
                    h.layout.mark_bell(id);
                    h.changed(cx);
                }
            }
            TerminalViewEvent::OpenUrl(url)
                if url.starts_with("https://") || url.starts_with("http://") =>
            {
                let _ = web_sys::window().and_then(|b| {
                    b.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer")
                        .ok()
                });
            }
            _ => {}
        });
        w.focus(&view.read(cx).focus_handle(cx), cx);
        self.sessions.insert(
            session,
            Session {
                view,
                node: node.clone(),
                _events: events,
                _ble_task: None,
            },
        );
        self.send(
            GatewayRequest::Open {
                node,
                session: session.0,
                request: session.0,
            },
            cx,
        );
        self.changed(cx);
    }
    pub fn open_ble_terminal(
        &mut self,
        node: String,
        profile: BleProfile,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.layout.alloc_pane_id();
        let session = TerminalSessionId(pane.0);
        let id = self.layout.alloc_tab_id();
        let title = if profile.name.trim().is_empty() {
            profile.device_name.clone()
        } else {
            profile.name.clone()
        };
        self.expanded_nodes.insert(node.clone());
        self.layout.insert_and_select_main_tab(Tab {
            id,
            kind: TabKind::LocalTerminal,
            title: title.clone(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane, session)),
            active_pane_id: Some(pane),
        });
        let metrics = self.metrics(w, cx);
        let view =
            cx.new(|cx| TerminalView::new(TerminalUiTheme::from_tokens(self.tokens), metrics, cx));
        let banner = format!("\x1b[36m[BLE]\x1b[0m Connecting to {title}...\r\n");
        view.update(cx, |v, cx| v.feed_bytes(banner.as_bytes(), cx));
        let events = cx.subscribe_in(&view, w, move |h, _, event, _, cx| match event {
            TerminalViewEvent::SendBytes(bytes) => h.input(session, bytes, cx),
            TerminalViewEvent::TitleChanged(Some(title)) => {
                if let Some(tab_id) = h.session_tab(session) {
                    h.layout.rename_terminal_tab(tab_id, title);
                    h.changed(cx);
                }
            }
            TerminalViewEvent::Bell => {
                if let Some(tab_id) = h.session_tab(session) {
                    h.layout.mark_bell(tab_id);
                    h.changed(cx);
                }
            }
            TerminalViewEvent::OpenUrl(url)
                if url.starts_with("https://") || url.starts_with("http://") =>
            {
                let _ = web_sys::window().and_then(|b| {
                    b.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer")
                        .ok()
                });
            }
            _ => {}
        });
        w.focus(&view.read(cx).focus_handle(cx), cx);

        let existing_paired = self.paired_ble.as_ref().and_then(|p| {
            let matches = profile
                .device_id
                .as_deref()
                .is_none_or(|wanted| wanted == p.id);
            matches.then(|| p.clone())
        });
        let config = profile.config.clone();
        let node_for_task = node.clone();
        let ble_task = cx.spawn(async move |owner, cx| {
            let paired = match existing_paired {
                Some(p) => Ok(p),
                None => crate::ble::pair_and_inspect(config.service_uuid).await,
            };
            let paired = match paired {
                Ok(p) => {
                    let _ = owner.update(cx, |h, _| {
                        h.paired_ble = Some(p.clone());
                    });
                    p
                }
                Err(err) => {
                    let _ = owner.update(cx, |h, cx| {
                        if let Some(s) = h.sessions.get(&session) {
                            let line = format!("\x1b[31m[BLE]\x1b[0m {err}\r\n");
                            s.view.update(cx, |v, cx| v.feed_bytes(line.as_bytes(), cx));
                        }
                        h.notifications_notice(err, cx);
                    });
                    return;
                }
            };

            let (event_tx, mut event_rx) = mpsc::unbounded::<BleEvent>();
            let opened = WebBleLink::open(paired, config, event_tx, cx).await;
            let (link, info) = match opened {
                Ok(pair) => pair,
                Err(err) => {
                    let _ = owner.update(cx, |h, cx| {
                        if let Some(s) = h.sessions.get(&session) {
                            let line = format!("\x1b[31m[BLE]\x1b[0m {err}\r\n");
                            s.view.update(cx, |v, cx| v.feed_bytes(line.as_bytes(), cx));
                        }
                        h.notifications_notice(err, cx);
                    });
                    return;
                }
            };

            let registered = owner
                .update(cx, |h, cx| {
                    let Some(s) = h.sessions.get(&session) else {
                        return false;
                    };
                    let caps = match (info.has_tx, info.has_rx) {
                        (true, true) => "TX+RX",
                        (true, false) => "TX only",
                        (false, true) => "RX only",
                        (false, false) => "GATT",
                    };
                    let line = format!(
                        "\x1b[32m[BLE]\x1b[0m Connected: {} ({} · {}) [{}]\r\n",
                        info.device_name, info.service_label, info.service_uuid, caps
                    );
                    s.view.update(cx, |v, cx| v.feed_bytes(line.as_bytes(), cx));
                    if let Some(saved) = h.ble_profiles.get_mut(&node_for_task) {
                        saved.device_name = info.device_name.clone();
                        if saved.config.service_uuid.is_none() {
                            saved.config.service_uuid = Some(info.service_uuid);
                        }
                    }
                    h.ble_links.insert(session, link);
                    h.changed(cx);
                    true
                })
                .unwrap_or(false);

            if !registered {
                return;
            }

            while let Some(event) = event_rx.next().await {
                let keep_going = owner
                    .update(cx, |h, cx| match event {
                        BleEvent::Rx(bytes) => {
                            let Some(s) = h.sessions.get(&session) else {
                                return false;
                            };
                            s.view.update(cx, |v, cx| v.feed_bytes(&bytes, cx));
                            if let Some(tab) = h.session_tab(session)
                                && h.layout.mark_unread(tab).is_some()
                            {
                                h.changed(cx);
                            }
                            true
                        }
                        BleEvent::Disconnected => {
                            h.ble_links.remove(&session);
                            if let Some(s) = h.sessions.get(&session) {
                                s.view.update(cx, |v, cx| {
                                    v.feed_bytes(
                                        b"\r\n\x1b[33m[BLE]\x1b[0m Device disconnected.\r\n",
                                        cx,
                                    );
                                });
                            }
                            h.changed(cx);
                            false
                        }
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        });

        self.sessions.insert(
            session,
            Session {
                view,
                node,
                _events: events,
                _ble_task: Some(ble_task),
            },
        );
        self.changed(cx);
    }
    fn session_tab(&self, id: TerminalSessionId) -> Option<TabId> {
        self.layout
            .tabs()
            .iter()
            .find(|t| {
                t.root_pane
                    .as_ref()
                    .is_some_and(|p| p.pane_id_for_session(id).is_some())
            })
            .map(|t| t.id)
    }
    pub fn input(&mut self, session: TerminalSessionId, bytes: &[u8], cx: &mut Context<Self>) {
        if let Some(link) = self.ble_links.get(&session) {
            if let Some(echo) = link.write(bytes)
                && let Some(s) = self.sessions.get(&session)
            {
                s.view.update(cx, |v, cx| v.feed_bytes(&echo, cx));
            }
            return;
        }
        if self.ready && self.sessions.contains_key(&session) {
            self.send(
                GatewayRequest::Input {
                    session: session.0,
                    bytes: Zeroizing::new(bytes.to_vec()),
                },
                cx,
            );
        }
    }
    pub fn send(&mut self, request: GatewayRequest, cx: &mut Context<Self>) {
        if !self
            .transport
            .as_mut()
            .is_some_and(|t| t.send(request).is_ok())
        {
            self.ready = false;
            self.changed(cx);
        }
    }
    pub fn release_session(&mut self, session: TerminalSessionId, cx: &mut Context<Self>) {
        let was_ble = self.ble_links.remove(&session).is_some()
            || self
                .sessions
                .get(&session)
                .is_some_and(|s| is_ble_node(&s.node));
        self.sessions.remove(&session);
        if !was_ble && self.ready {
            self.send(GatewayRequest::Unsubscribe { session: session.0 }, cx);
        }
    }
    pub fn delivery(&mut self, event: GatewayEvent, w: &mut Window, cx: &mut Context<Self>) {
        match event {
            GatewayEvent::Connected => {
                self.connecting = false;
                self.ready = true;
                if self.layout.tabs().is_empty() {
                    self.new_terminal("local".into(), None, w, cx)
                }
            }
            GatewayEvent::Disconnected => {
                self.connecting = false;
                self.ready = false;
                self.notice("connections.state.disconnected", cx);
            }
            GatewayEvent::Output { session, bytes } => {
                let id = TerminalSessionId(session);
                if let Some(s) = self.sessions.get(&id) {
                    s.view.update(cx, |v, cx| v.feed_bytes(&bytes, cx));
                }
                if let Some(tab) = self.session_tab(id)
                    && self.layout.mark_unread(tab).is_some()
                {
                    self.changed(cx);
                }
                return;
            }
            GatewayEvent::Rejected { request } => {
                if let Some(f) = &self.files {
                    f.copied(request, false, cx);
                }
                self.notice("common.status.error", cx);
            }
            GatewayEvent::Copied { transfer } => {
                if let Some(f) = &self.files {
                    f.copied(transfer, true, cx);
                }
            }
        }
        self.changed(cx);
    }
}
