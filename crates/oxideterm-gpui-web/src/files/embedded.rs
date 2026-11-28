use super::*;
use oxideterm_gpui_sftp::{SftpSurfaceId, render_presentation_choice};
use oxideterm_settings::{SftpPresentationPreference as Presentation, SftpSidebarViewMode};
use oxideterm_workspace::TerminalSessionId;
use sftp::{Browser, SidebarFrame};

#[derive(Default)]
pub(super) struct EmbeddedSftp {
    requested: Option<(String, Option<String>)>,
    opened: Option<String>,
    pinned: bool,
    pub(super) split_bounds: Rc<std::cell::Cell<Option<(f32, f32)>>>,
    drag: Option<f32>,
    // Hidden views retain their own navigation and unsaved editor, never the SSH transport.
    pub(super) views: HashMap<String, Entity<Browser>>,
}

impl Workspace {
    pub(crate) fn remember_file_terminal(&mut self, cx: &mut App) {
        let Some(id) = self.active_session() else {
            return;
        };
        let Some(session) = self.sessions.get(&id) else {
            return;
        };
        if let Some(files) = &mut self.files
            && !session.is_ble()
            && session.node != "local"
            && files.recent_terminals.get(&session.node) != Some(&id)
        {
            files.recent_terminals.insert(session.node.clone(), id);
            files.refresh_terminal_directory(&session.node, cx);
        }
    }

    pub(super) fn file_terminal(&self, node: &str) -> Option<(TerminalSessionId, &str)> {
        let id = self
            .active_session()
            .filter(|id| self.sessions.get(id).is_some_and(|s| s.node == node))
            .or_else(|| self.files.as_ref()?.recent_terminals.get(node).copied())?;
        let s = self.sessions.get(&id)?;
        if s.is_ble() || s.node != node {
            return None;
        }
        Some((id, self.transport.as_ref()?.directory(id.0)?))
    }

    pub(crate) fn change_file_terminal_directory(
        &mut self,
        id: TerminalSessionId,
        node: &str,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        let unavailable = "sftp.toolbar.terminal_directory_unavailable";
        if !self.demo
            || !self
                .sessions
                .get(&id)
                .is_some_and(|s| !s.is_ble() && s.node == node)
        {
            return Err(unavailable);
        }
        let tab = self.session_tab(id).ok_or(unavailable)?;
        let pane = self
            .layout
            .tab_by_id(tab)
            .and_then(|t| t.root_pane.as_ref()?.pane_id_for_session(id))
            .ok_or(unavailable)?;
        self.transport
            .as_mut()
            .ok_or(unavailable)?
            .change_directory(id.0, path)?;
        self.activate_page(tab, cx);
        self.layout.set_active_pane(Some(tab), pane);
        window.focus(&self.sessions[&id].view.read(cx).focus_handle(cx), cx);
        self.changed(cx);
        Ok(())
    }

    pub(super) fn present_node_files(&mut self, node: &str, cx: &mut Context<Self>) {
        let path = self
            .active_session()
            .and_then(|id| {
                self.sessions
                    .get(&id)
                    .filter(|s| s.node == node)
                    .and_then(|_| self.transport.as_ref()?.directory(id.0))
                    .map(str::to_owned)
            })
            .filter(|_| {
                self.settings
                    .read(cx)
                    .settings(cx)
                    .terminal
                    .command_bar
                    .current_directory_awareness
            });
        self.present_files_at(node, path, cx);
    }

    pub(crate) fn present_files_at(&mut self, node: &str, path: Option<String>, cx: &mut Context<Self>) {
        let Some(files) = &self.files else { return };
        let existing = files
            .sftp_pages
            .values()
            .any(|v| v.read(cx).target.id == node);
        let tab_active = self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp);
        let preference = oxideterm_gpui_sftp::sftp_open_presentation(
            self.settings.read(cx).settings(cx).sftp.presentation,
            existing,
            tab_active,
        );
        match preference {
            Presentation::Tab => self.open_node_files_tab(node, path, cx),
            Presentation::Sidebar => {
                self.open_files_sidebar(node, cx);
                if let Some(path) = path
                    && let Some(view) = self.embedded_files(cx)
                {
                    view.update(cx, |v, cx| {
                        v.navigate(oxideterm_gpui_sftp::SftpPane::Remote, path, cx)
                    });
                }
            }
            Presentation::Ask => {
                self.files.as_mut().unwrap().embedded.requested = Some((node.into(), path));
                cx.notify();
            }
        }
    }

    pub(crate) fn open_node_files_tab(
        &mut self,
        node: &str,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.open_sftp_target(
            SftpTarget {
                id: node.into(),
                profile: None,
            },
            cx,
        );
        if let Some(path) = path
            && let Some(page) = self.focused_tab()
            && let Some(view) = self.files.as_ref().and_then(|f| f.sftp_pages.get(&page.id))
        {
            view.update(cx, |v, cx| {
                v.navigate(oxideterm_gpui_sftp::SftpPane::Remote, path, cx)
            });
        }
    }

    pub(crate) fn sftp_choice_open(&self) -> bool {
        self.files
            .as_ref()
            .is_some_and(|f| f.embedded.requested.is_some())
    }

    pub(crate) fn choose_files_surface(
        &mut self,
        choice: Option<Presentation>,
        cx: &mut Context<Self>,
    ) {
        let Some((node, path)) = self
            .files
            .as_mut()
            .and_then(|f| f.embedded.requested.take())
        else {
            return;
        };
        if let Some(choice) = choice {
            self.settings.update(cx, |s, cx| {
                s.edit_preferences(cx, |p| p.sftp.presentation = choice)
            });
            self.present_files_at(&node, path, cx);
        }
        cx.notify();
    }

    pub(crate) fn sftp_choice(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.sftp_choice_open() {
            return None;
        }
        let owner = cx.weak_entity();
        Some(render_presentation_choice(
            &self.tokens,
            &self.i18n,
            Rc::new(move |choice, cx| {
                let _ = owner.update(cx, |h, cx| h.choose_files_surface(choice, cx));
            }),
        ))
    }

    fn focused_ssh_node(&self) -> Option<&str> {
        let s = self.sessions.get(&self.active_session()?)?;
        (!s.is_ble() && s.node != "local").then_some(s.node.as_str())
    }

    fn open_files_sidebar(&mut self, node: &str, cx: &mut Context<Self>) {
        let pinned = self.focused_ssh_node() != Some(node);
        let Some(files) = &mut self.files else { return };
        files.embedded.opened = Some(node.into());
        files.embedded.pinned = pinned;
        self.show_files_navigator(cx);
        self.record_connection_use(node, cx);
        self.changed(cx);
    }

    pub(crate) fn close_files_sidebar(&mut self, cx: &mut Context<Self>) {
        if let Some(files) = &mut self.files {
            files.embedded.opened = None;
            files.embedded.pinned = false;
            files.embedded.drag = None;
        }
        cx.notify();
    }

    pub(super) fn pin_files_sidebar(&mut self, node: &str, cx: &mut Context<Self>) {
        if let Some(files) = &mut self.files {
            files.embedded.pinned = !files.embedded.pinned;
            files.embedded.opened = Some(node.into());
        }
        cx.notify();
    }

    pub(crate) fn show_sidebar_notice(&mut self, entity: EntityId, cx: &mut Context<Self>) {
        let node = self
            .files
            .as_ref()
            .and_then(|f| {
                f.embedded
                    .views
                    .iter()
                    .find(|(_, v)| v.entity_id() == entity)
            })
            .map(|(n, _)| n.clone());
        if let Some(node) = node {
            if self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp) {
                if let Some(tab) = self
                    .file_terminal(&node)
                    .and_then(|(id, _)| self.session_tab(id))
                {
                    self.activate_page(tab, cx);
                } else {
                    self.open_page(
                        TabKind::SessionManager,
                        "sidebar.panels.connections_header",
                        cx,
                    );
                }
            }
            self.open_files_sidebar(&node, cx);
        }
    }

    pub(crate) fn embedded_files(&mut self, cx: &mut Context<Self>) -> Option<Entity<Browser>> {
        let opened = self.files.as_ref()?.embedded.opened.clone()?;
        let pinned = self.files.as_ref()?.embedded.pinned;
        let active_tab = self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp);
        let target = oxideterm_gpui_sftp::sidebar_sftp_target(
            Some(&opened.as_str()),
            pinned,
            self.focused_ssh_node(),
        );
        let frame = SidebarFrame {
            has_target: target.is_some(),
            pinned,
            active_tab,
            tree: self.settings.read(cx).settings(cx).sftp.sidebar_view_mode
                == SftpSidebarViewMode::Tree,
        };
        let node = target.unwrap_or(&opened).to_owned();
        let f = self.files.as_mut()?;
        let view = f.embedded.views.entry(node.clone()).or_insert_with(|| {
            let title = self
                .catalog_data
                .items
                .iter()
                .find(|row| row.id() == node)
                .map(|row| row.name.clone())
                .unwrap_or_else(|| node.clone());
            let host = cx.weak_entity();
            cx.new(|cx| {
                Browser::new(
                    &f.drives,
                    SftpTarget {
                        id: node,
                        profile: None,
                    },
                    SftpSurfaceId::Sidebar,
                    title,
                    self.tokens,
                    self.i18n.clone(),
                    self.ui_font.clone(),
                    host,
                    f.bookmarks.clone(),
                    cx,
                )
            })
        });
        view.update(cx, |v, cx| v.set_sidebar(frame, cx));
        Some(view.clone())
    }

    pub(crate) fn files_own_keys(&self, window: &Window, cx: &App) -> bool {
        let Some(f) = &self.files else { return false };
        if let Some(page) = self.focused_tab().and_then(|tab| f.sftp_pages.get(&tab.id))
            && page.read(cx).field_focused(window, cx)
        {
            return true;
        }
        if !self.files_navigator_visible(cx) {
            return false;
        }
        oxideterm_gpui_sftp::sidebar_sftp_target(
            f.embedded.opened.as_deref().as_ref(),
            f.embedded.pinned,
            self.focused_ssh_node(),
        )
        .and_then(|id| f.embedded.views.get(id))
        .is_some_and(|v| v.read(cx).sidebar_dialog_open(cx) || v.read(cx).field_focused(window, cx))
    }

    pub(crate) fn shows_files_for_node(&self, node: &str, cx: &App) -> bool {
        if !self.files_navigator_visible(cx)
            || self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp)
        {
            return false;
        }
        let Some(f) = &self.files else {
            return false;
        };
        oxideterm_gpui_sftp::sidebar_sftp_target(
            f.embedded.opened.as_deref().as_ref(),
            f.embedded.pinned,
            self.focused_ssh_node(),
        ) == Some(node)
    }

    pub(crate) fn with_embedded_files(
        &self,
        navigator: AnyElement,
        view: &Entity<Browser>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let f = self.files.as_ref().unwrap();
        let bounds = f.embedded.split_bounds.clone();
        let hover = rgb(self.tokens.ui.shell_active_bg());
        let fraction = f
            .embedded
            .drag
            .unwrap_or(
                self.settings
                    .read(cx)
                    .settings(cx)
                    .sftp
                    .sidebar_session_fraction,
            )
            .clamp(0.2, 0.75);
        let body = |fraction| {
            div()
                .flex_basis(relative(fraction))
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
        };
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(body(fraction).child(navigator))
            .child(
                div()
                    .id("web-sftp-sidebar-splitter")
                    .h(px(6.))
                    .w_full()
                    .flex_none()
                    .flex()
                    .justify_center()
                    .flex_col()
                    .cursor(CursorStyle::ResizeRow)
                    .hover(move |d| d.bg(hover))
                    .child(div().h(px(1.)).w_full().bg(rgb(self.tokens.ui.border)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, w, cx| {
                            if let Some(f) = &mut h.files {
                                f.embedded.drag = Some(fraction);
                            }
                            w.prevent_default();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(body(1. - fraction).child(view.clone()))
            .on_children_prepainted(move |children, _, _| {
                if let (Some(top), Some(bottom)) = (children.first(), children.last()) {
                    bounds.set(Some((
                        top.top().into(),
                        (bottom.bottom() - top.top()).into(),
                    )));
                }
            })
            .into_any_element()
    }

    pub(crate) fn resize_embedded_files(&mut self, y: Option<Pixels>, cx: &mut Context<Self>) {
        let Some(f) = &mut self.files else { return };
        let Some(current) = f.embedded.drag else {
            return;
        };
        if let Some(y) = y {
            if let Some((top, height)) = f.embedded.split_bounds.get().filter(|(_, h)| *h > 0.) {
                f.embedded.drag = Some(((f32::from(y) - top) / height).clamp(0.2, 0.75));
                cx.notify();
            }
        } else {
            f.embedded.drag = None;
            self.settings.update(cx, |s, cx| {
                s.edit_preferences(cx, |p| p.sftp.sidebar_session_fraction = current)
            });
        }
    }
}
