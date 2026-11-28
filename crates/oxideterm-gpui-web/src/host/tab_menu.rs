use super::*;
use oxideterm_gpui_shell::tabs::menu::{Command, Options, TabMenu};

impl Workspace {
    pub(super) fn open_tab_menu(
        &mut self,
        id: TabId,
        position: Point<Pixels>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = cx.weak_entity();
        let target = host.clone();
        let menu = TabMenu::open(
            id,
            position,
            w,
            cx,
            move |cx| {
                host.read_with(cx, |h, cx| {
                    Some((h.tokens, h.i18n.clone(), h.tab_menu_options(id, cx)?))
                })
                .ok()
                .flatten()
            },
            move |cmd, mods, w, cx| {
                let _ = target.update(cx, |h, cx| {
                    h.tab_menu = None;
                    if let Some(cmd) = cmd {
                        h.run_tab_menu(id, cmd, mods, w, cx);
                    }
                    h.changed(cx);
                });
            },
        );
        self.tab_menu = Some((id, menu));
        cx.notify();
    }

    fn menu_session(&self, tab: TabId) -> Option<&Session> {
        let t = self.layout.tab_by_id(tab)?;
        self.sessions.get(
            &t.root_pane
                .as_ref()?
                .session_id_for_pane(t.active_pane_id?)?,
        )
    }

    fn menu_endpoint(&self, id: TabId, cx: &App) -> Option<String> {
        self.menu_session(id).map(|s| s.node.clone()).or_else(|| {
            let page = self.layout.focused_page_id(id);
            self.files
                .as_ref()?
                .sftp_pages
                .get(&page)
                .map(|p| p.read(cx).target.id.clone())
        })
    }

    fn tab_menu_options(&self, id: TabId, cx: &App) -> Option<Options> {
        use Command::*;
        let t = self.layout.tab_by_id(id)?;
        let mut actions = Vec::new();
        if self.menu_endpoint(id, cx).is_some_and(|node| {
            self.catalog_data.items.iter().any(|r| r.id() == node)
                && (self.demo || crate::ble::is_ble_node(&node))
        }) {
            actions.push(Edit);
        }
        let mut terminals = Vec::new();
        if let Some(s) = self.menu_session(id) {
            actions.push(CloseTerminals);
            if !s.is_ble() && s.node != "local" {
                if self.ready {
                    actions.push(NewTerminal);
                }
                if self.demo {
                    actions.extend([Sftp, Ide, Forwards]);
                }
                for (_, tab) in self.layout.unfiltered_main_tabs() {
                    if let Some(root) = &tab.root_pane {
                        let mut ids = Vec::new();
                        root.collect_session_ids(&mut ids);
                        terminals.extend(
                            ids.into_iter()
                                .filter(|id| {
                                    self.sessions
                                        .get(id)
                                        .is_some_and(|other| other.node == s.node)
                                })
                                .map(|id| (id, self.active_session() == Some(id))),
                        );
                    }
                }
            }
        }
        if self.demo && t.kind == TabKind::SessionManager {
            actions.push(Sidebar);
        }
        let active = self.layout.active_tab_id();
        Some(Options {
            kind: t.kind.clone(),
            actions,
            terminals,
            active_merge: active.filter(|to| self.layout.can_combine_pages(id, *to)),
            merge_targets: self
                .layout
                .main_tabs()
                .filter(|(_, t)| active == Some(id) && self.layout.can_combine_pages(id, t.id))
                .map(|(_, t)| (t.id, t.title.clone()))
                .collect(),
            visible: self.layout.main_tabs().map(|(_, t)| t.id).collect(),
        })
    }

    fn run_tab_menu(
        &mut self,
        id: TabId,
        cmd: Command,
        mods: Modifiers,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use Command::*;
        match cmd {
            Rename => self.begin_tab_rename(id, w, cx),
            Edit => {
                if let Some(node) = self.menu_endpoint(id, cx) {
                    self.form(Some(&node), None, w, cx);
                }
            }
            Merge(target, dir) => {
                self.combine_tabs(id, target, None, dir, false, w, cx);
            }
            Terminal(sid) => {
                if let Some(tab) = self.session_tab(sid)
                    && let Some(pane) = self
                        .layout
                        .tab_by_id(tab)
                        .and_then(|t| t.root_pane.as_ref()?.pane_id_for_session(sid))
                {
                    self.activate_page(tab, cx);
                    self.pane_request(
                        ShellPaneRequest::Focus(tab, pane, PaneContent::Terminal(sid)),
                        w,
                        cx,
                    );
                }
            }
            NewTerminal | Sftp | Ide | Forwards | CloseTerminals => {
                let Some(node) = self.menu_session(id).map(|s| s.node.clone()) else {
                    return;
                };
                match cmd {
                    NewTerminal => {
                        self.new_terminal_with_sftp(node, None, mods.control.then_some(true), w, cx)
                    }
                    Sftp => self.open_node_sftp(&node, cx),
                    Ide => self.open_node_ide(&node, w, cx),
                    Forwards => self.open_node_forwards(&node, cx),
                    CloseTerminals => self.request_close_terminals(Some(&node), w, cx),
                    _ => unreachable!(),
                }
            }
            Sidebar => {
                self.close(id, cx);
                self.rails
                    .select_section(SidebarSection::Connections, &self.tokens);
                self.collapse(RailSide::Left, false, cx);
            }
            Close => self.request_close(CloseTarget::Tab(id), w, cx),
            Others => self.request_close(
                CloseTarget::Others(self.layout.other_main_tab_ids(id)),
                w,
                cx,
            ),
            Right => {
                let ids = self
                    .layout
                    .main_tabs()
                    .skip_while(|(_, t)| t.id != id)
                    .skip(1)
                    .map(|(_, t)| t.id)
                    .collect();
                self.request_close(CloseTarget::Right(ids), w, cx);
            }
            All => {
                let ids = self.layout.main_tabs().map(|(_, t)| t.id).collect();
                self.request_close(CloseTarget::All(ids), w, cx);
            }
            Detach | Return | DrillIn | Disconnect => {}
        }
    }
}
