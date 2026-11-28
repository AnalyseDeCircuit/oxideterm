use crate::{
    ble::{BleEvent, BleProfile, WebBleLink, is_ble_node},
    host::Workspace,
    transport::{GatewayEvent, GatewayRequest, GatewaySessionState},
};
use futures::{StreamExt, channel::mpsc};
use gpui::{prelude::*, *};
use oxideterm_gpui_terminal_view::*;
use oxideterm_workspace::*;
use zeroize::Zeroizing;

pub struct Session {
    pub view: Entity<TerminalView>,
    pub node: String,
    pub title: String,
    _events: Subscription,
    transport: SessionTransport,
    unused_demo_preview: bool,
}

enum SessionTransport {
    Gateway(GatewaySessionState),
    Ble {
        profile_name: String,
        device_name: String,
        link: Option<WebBleLink>,
        _task: Task<()>,
    },
}

impl Session {
    pub fn kind(&self) -> TabKind {
        if self.is_ble() || self.node == "local" {
            TabKind::LocalTerminal
        } else {
            TabKind::SshTerminal
        }
    }
    pub fn is_ble(&self) -> bool {
        matches!(self.transport, SessionTransport::Ble { .. })
    }

    pub(crate) fn ble_labels(&self) -> Option<(&str, &str)> {
        match &self.transport {
            SessionTransport::Ble {
                profile_name,
                device_name,
                ..
            } => Some((profile_name, device_name)),
            _ => None,
        }
    }

    pub fn writable(&self, gateway_ready: bool) -> bool {
        match &self.transport {
            SessionTransport::Gateway(state) => state.writable(gateway_ready),
            SessionTransport::Ble { link, .. } => link.as_ref().is_some_and(WebBleLink::writable),
        }
    }

    pub fn connected(&self, gateway_ready: bool) -> bool {
        match &self.transport {
            SessionTransport::Gateway(state) => state.running(gateway_ready),
            SessionTransport::Ble { link, .. } => link.is_some(),
        }
    }
}

impl Workspace {
    pub fn active_session(&self) -> Option<TerminalSessionId> {
        let tab = self.layout.active_tab()?;
        tab.root_pane
            .as_ref()?
            .session_id_for_pane(tab.active_pane_id?)
    }
    pub fn split_target(&self) -> Option<&str> {
        let session = self.sessions.get(&self.active_session()?)?;
        (!session.is_ble() && session.writable(self.ready)).then_some(session.node.as_str())
    }
    fn metrics(&self, window: &Window, cx: &App) -> TerminalMetrics {
        let settings = &self.settings.read(cx).settings(cx).terminal;
        let font = oxideterm_gpui_terminal_view::metrics::resolve_portable_terminal_font(
            cx.text_system(),
            settings,
        )
        .font;
        TerminalMetrics::measure(
            window,
            font,
            px(settings.font_size as f32),
            settings.line_height as f32,
            (settings.character_spacing
                * oxideterm_settings::TERMINAL_CHARACTER_SPACING_BASELINE_SCALE) as f32,
        )
    }
    pub(crate) fn terminal_background_alpha(&self, kind: &TabKind) -> Option<u8> {
        self.wallpaper
            .as_ref()
            .filter(|bg| bg.includes(tab_background_key(kind)))
            .map(|_| {
                (self.tokens.metrics.sidebar_vibrancy_alpha.clamp(0., 1.) * 255.).round() as u8
            })
    }

    fn terminal_view(&self, kind: &TabKind, w: &Window, cx: &mut App) -> Entity<TerminalView> {
        let metrics = self.metrics(w, cx);
        let prefs = TerminalViewPreferences::from(&self.settings.read(cx).settings(cx).terminal);
        cx.new(|cx| {
            let mut view =
                TerminalView::new(TerminalUiTheme::from_tokens(self.tokens), metrics, cx);
            view.set_background_alpha(self.terminal_background_alpha(kind), cx);
            view.apply_preferences(prefs, cx);
            view
        })
    }
    pub fn refresh_terminals(&self, w: &Window, cx: &mut App) {
        let metrics = self.metrics(w, cx);
        let prefs = TerminalViewPreferences::from(&self.settings.read(cx).settings(cx).terminal);
        for s in self.sessions.values() {
            s.view.update(cx, |v, cx| {
                v.set_background_alpha(self.terminal_background_alpha(&s.kind()), cx);
                v.apply_preferences(prefs, cx);
                v.set_theme(TerminalUiTheme::from_tokens(self.tokens), cx);
                v.set_metrics(metrics.clone(), cx)
            });
        }
    }
    pub(crate) fn open_demo_preview(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.new_terminal_with_sftp("demo".into(), None, Some(false), w, cx);
        if let Some(session) = self
            .active_session()
            .and_then(|id| self.sessions.get_mut(&id))
        {
            // A page reload is not a user visit to the example connection.
            session.unused_demo_preview = true;
        }
    }

    pub fn new_terminal(
        &mut self,
        node: String,
        split: Option<SplitDirection>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_terminal_with_sftp(node, split, None, w, cx);
    }

    pub(crate) fn new_terminal_with_sftp(
        &mut self,
        node: String,
        split: Option<SplitDirection>,
        open_sftp: Option<bool>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_ble_node(&node) {
            if split.is_some() {
                self.notice("remote_desktop.capability_unsupported", cx);
            } else if let Some(profile) = self.ble_profiles.get(&node).cloned() {
                self.open_ble_terminal(node, profile, w, cx);
            } else {
                self.notice("sessionManager.toast.connection_not_found", cx);
            }
            return;
        }
        if !self.ready {
            return;
        }
        let is_local = node == "local";
        let kind = if is_local {
            TabKind::LocalTerminal
        } else {
            TabKind::SshTerminal
        };
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
            self.expanded_nodes.insert(node.clone());
            self.layout.insert_and_select_main_tab(Tab {
                id,
                kind: kind.clone(),
                title: title.clone(),
                title_source,
                root_pane: Some(PaneNode::leaf(pane, session)),
                active_pane_id: Some(pane),
            });
        }
        let view = self.terminal_view(&kind, w, cx);
        let events = cx.subscribe_in(&view, w, move |h, _, event, window, cx| match event {
            TerminalViewEvent::ReviewPaste(text) => {
                h.review_paste(session, text.clone(), window, cx)
            }
            TerminalViewEvent::ClipboardReadFailed => {
                h.notice("terminal.paste.clipboard_read_failed", cx)
            }
            TerminalViewEvent::SendBytes(bytes) => h.input(session, bytes, cx),
            TerminalViewEvent::Resized(size)
                if h.sessions
                    .get(&session)
                    .is_some_and(|s| s.writable(h.ready)) =>
            {
                h.send(
                    GatewayRequest::Resize {
                        session: session.0,
                        cols: size.cols.min(65535) as u16,
                        rows: size.rows.min(65535) as u16,
                    },
                    cx,
                )
            }
            TerminalViewEvent::TitleChanged(Some(title)) => {
                if let Some(s) = h.sessions.get_mut(&session) {
                    s.title = title.clone();
                }
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
                title,
                _events: events,
                transport: SessionTransport::Gateway(GatewaySessionState::Opening),
                unused_demo_preview: false,
            },
        );
        self.sync_active_scope(cx);
        let sftp_node = (self.demo
            && open_sftp
                .unwrap_or_else(|| self.profile_details.get(&node).is_some_and(|p| p.open_sftp)))
        .then(|| node.clone());
        self.send(
            GatewayRequest::Open {
                node,
                session: session.0,
                request: session.0,
            },
            cx,
        );
        if !self.ready {
            self.reject_open(session, cx);
        } else if let Some(node) = sftp_node
            && let Some(tab) = self.session_tab(session)
        {
            self.open_node_sftp(&node, cx);
            self.activate_page(tab, cx);
            w.focus(&self.sessions[&session].view.read(cx).focus_handle(cx), cx);
        }
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
        let view = self.terminal_view(&TabKind::LocalTerminal, w, cx);
        let banner = format!("\x1b[36m[BLE]\x1b[0m Connecting to {title}...\r\n");
        view.update(cx, |v, cx| v.feed_bytes(banner.as_bytes(), cx));
        let events = cx.subscribe_in(&view, w, move |h, _, event, window, cx| match event {
            TerminalViewEvent::ReviewPaste(text) => {
                h.review_paste(session, text.clone(), window, cx)
            }
            TerminalViewEvent::ClipboardReadFailed => {
                h.notice("terminal.paste.clipboard_read_failed", cx)
            }
            TerminalViewEvent::SendBytes(bytes) => h.input(session, bytes, cx),
            TerminalViewEvent::TitleChanged(Some(title)) => {
                if let Some(s) = h.sessions.get_mut(&session) {
                    s.title = title.clone();
                }
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
                .is_some_and(|wanted| wanted == p.id);
            matches.then(|| p.clone())
        });
        let config = profile.config.clone();
        let node_for_task = node.clone();
        let selection = existing_paired.is_none().then(|| {
            crate::ble::pair_and_inspect(config.service_uuid, profile.device_id.as_deref())
        });
        let ble_task = cx.spawn(async move |owner, cx| {
            let paired = match selection {
                Some(picker) => picker.await,
                None => Ok(existing_paired),
            };
            let paired = match paired {
                Ok(Some(p)) => {
                    let _ = owner.update(cx, |h, _| {
                        h.paired_ble = Some(p.clone());
                    });
                    p
                }
                Ok(None) => {
                    let _ = owner.update(cx, |h, cx| {
                        if let Some(tab) = h.session_tab(session) {
                            h.remove_terminal_pane(tab, pane, session, cx);
                        }
                    });
                    return;
                }
                Err(err) => {
                    let _ = owner.update(cx, |h, cx| {
                        let err = h.i18n.t(err);
                        if let Some(s) = h.sessions.get(&session) {
                            let line = format!("\x1b[31m[BLE]\x1b[0m {err}\r\n");
                            s.view.update(cx, |v, cx| v.feed_bytes(line.as_bytes(), cx));
                        }
                        h.session_error(session, err, cx);
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
                        h.session_error(session, err, cx);
                    });
                    return;
                }
            };

            let registered = owner
                .update(cx, |h, cx| {
                    let Some(s) = h.sessions.get_mut(&session) else {
                        return false;
                    };
                    let SessionTransport::Ble {
                        link: slot,
                        device_name,
                        ..
                    } = &mut s.transport
                    else {
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
                    *device_name = info.device_name;
                    *slot = Some(link);
                    h.record_connection_use(&node_for_task, cx);
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
                        BleEvent::WriteFailed => {
                            h.session_error(session, h.i18n.t("terminal.ble.write_failed"), cx);
                            true
                        }
                        BleEvent::Disconnected => {
                            if let Some(s) = h.sessions.get_mut(&session) {
                                if let SessionTransport::Ble { link, .. } = &mut s.transport {
                                    *link = None;
                                }
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
                title,
                _events: events,
                unused_demo_preview: false,
                transport: SessionTransport::Ble {
                    profile_name: profile.name,
                    device_name: profile.device_name,
                    link: None,
                    _task: ble_task,
                },
            },
        );
        self.sync_active_scope(cx);
        self.changed(cx);
    }
    pub(crate) fn session_tab(&self, id: TerminalSessionId) -> Option<TabId> {
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
        self.try_input(session, bytes, cx);
    }

    pub(crate) fn write_sender_frame(
        &mut self,
        id: TerminalSessionId,
        frame: &oxideterm_gpui_command_sender::TerminalSenderFrame,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        use oxideterm_gpui_command_sender::TerminalSenderFrame::*;
        let session = self.sessions.get(&id)?;
        Some(match frame {
            TextLine(line) => {
                let mut bytes = Zeroizing::new(line.as_bytes().to_vec());
                bytes.push(b'\r');
                self.try_input(id, &bytes, cx)
            }
            TextChunk(text) => self.try_input(id, text.as_bytes(), cx),
            RawBytes(bytes) => match &session.transport {
                SessionTransport::Ble { link, .. } => {
                    link.as_ref().is_some_and(|l| l.write_raw(bytes).is_ok())
                }
                SessionTransport::Gateway(_) => self.try_input(id, bytes, cx),
            },
        })
    }

    pub(crate) fn try_input(
        &mut self,
        session: TerminalSessionId,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(s) = self.sessions.get(&session) else {
            return false;
        };
        match &s.transport {
            SessionTransport::Ble { link, .. } => {
                let Some(Ok(echo)) = link.as_ref().map(|link| link.write(bytes)) else {
                    return false;
                };
                if let Some(echo) = echo {
                    s.view.update(cx, |v, cx| v.feed_bytes(&echo, cx));
                }
                true
            }
            SessionTransport::Gateway(_) if s.writable(self.ready) => {
                let preview = s.unused_demo_preview && !bytes.is_empty();
                self.send(
                    GatewayRequest::Input {
                        session: session.0,
                        bytes: Zeroizing::new(bytes.to_vec()),
                    },
                    cx,
                );
                if preview
                    && self.ready
                    && let Some(s) = self.sessions.get_mut(&session)
                {
                    s.unused_demo_preview = false;
                    let node = s.node.clone();
                    self.record_connection_use(&node, cx);
                }
                self.ready
            }
            SessionTransport::Gateway(_) => false,
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
        self.forget_notice_target(crate::host::NoticeTarget::Session(session), cx);
        if self
            .paste_review
            .as_ref()
            .is_some_and(|p| p.session == session)
        {
            self.paste_review = None;
        }
        if let Some(s) = self.sessions.remove(&session) {
            if let Some(files) = &mut self.files {
                files.forget_file_terminal(&s.node, session, cx);
            }
            if !s.is_ble() && s.writable(self.ready) {
                self.send(GatewayRequest::Unsubscribe { session: session.0 }, cx);
            }
        }
    }
    fn reject_open(&mut self, session: TerminalSessionId, cx: &mut Context<Self>) {
        let Some(s) = self.sessions.get_mut(&session) else {
            return;
        };
        let SessionTransport::Gateway(state @ GatewaySessionState::Opening) = &mut s.transport
        else {
            return;
        };
        *state = GatewaySessionState::Rejected;
        let message = self.i18n.t("connections.status.connection_failed");
        s.view
            .update(cx, |v, cx| v.feed_bytes(message.as_bytes(), cx));
        self.session_error(session, message, cx);
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
            GatewayEvent::DemoDirectoryChanged { session } => {
                if let Some(s) = self.sessions.get(&TerminalSessionId(session))
                    && let Some(f) = &self.files
                {
                    f.refresh_terminal_directory(&s.node, cx);
                }
            }
            GatewayEvent::Disconnected => {
                self.connecting = false;
                self.ready = false;
                self.notice("connections.state.disconnected", cx);
            }
            GatewayEvent::Output { session, bytes } => {
                let id = TerminalSessionId(session);
                let Some(s) = self.sessions.get_mut(&id) else {
                    return;
                };
                let SessionTransport::Gateway(state) = &mut s.transport else {
                    return;
                };
                let Some(first_output) = state.receive_output(self.ready) else {
                    return;
                };
                s.view.update(cx, |v, cx| {
                    if first_output && self.demo {
                        let banner = self.i18n.t("terminal.demo_shell_welcome");
                        v.feed_bytes(format!("{banner}\r\n").as_bytes(), cx);
                    }
                    v.feed_bytes(&bytes, cx);
                });
                if first_output && !s.unused_demo_preview {
                    let node = s.node.clone();
                    self.record_connection_use(&node, cx);
                }
                if let Some(tab) = self.session_tab(id)
                    && (self.layout.mark_unread(tab).is_some() || first_output)
                {
                    self.changed(cx);
                }
                return;
            }
            GatewayEvent::Rejected { request } => {
                self.reject_open(TerminalSessionId(request), cx);
            }
            GatewayEvent::Copied { .. } => {}
        }
        self.changed(cx);
    }
}
