use crate::{
    host::Workspace,
    transport::{GatewayEvent, GatewayRequest},
};
use gpui::{prelude::*, *};
use oxideterm_gpui_terminal_view::*;
use oxideterm_workspace::*;
use zeroize::Zeroizing;

pub struct Session {
    pub view: Entity<TerminalView>,
    pub node: String,
    _events: Subscription,
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
            self.layout.insert_and_select_main_tab(Tab {
                id,
                kind: if node == "local" {
                    TabKind::LocalTerminal
                } else {
                    TabKind::SshTerminal
                },
                title: self.i18n.t("menu.terminal"),
                title_source: TabTitleSource::I18nKey("menu.terminal"),
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
        self.sessions.remove(&session);
        if self.ready {
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
