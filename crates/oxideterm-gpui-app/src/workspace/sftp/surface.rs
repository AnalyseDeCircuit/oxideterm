use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_sftp_presentation_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.sftp_presentation_request.as_ref()?;
        let window = cx.weak_entity();
        Some(oxideterm_gpui_sftp::render_presentation_choice(
            &self.tokens,
            &self.i18n,
            Rc::new(move |preference, cx| {
                let _ = window.update(cx, |app, cx| {
                    if let Some(preference) = preference {
                        app.choose_sftp_presentation(preference, cx);
                    } else {
                        app.sftp_presentation_request = None;
                        cx.notify();
                    }
                });
            }),
        ))
    }

    pub(in crate::workspace) fn render_sftp_sidebar_surface(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Sidebar);
        let target = self.embedded_sftp_target(cx);
        let title = target
            .as_ref()
            .and_then(|node| self.ssh_nodes.get(node))
            .map(|node| node.title.clone())
            .unwrap_or_else(|| self.i18n.t("sftp.sidebar.remote_host"));
        self.mount_sftp_browser(true, target, title, None, false, cx)
    }

    pub(in crate::workspace) fn render_sftp_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(tab_id) = self.active_tab_id(cx) else {
            return self.render_empty_workspace(f32::from(window.viewport_size().width), cx);
        };
        self.render_sftp_surface_for_tab(tab_id, window, cx)
    }

    pub(in crate::workspace) fn render_sftp_surface_for_tab(
        &self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(tab_id));
        let Some(remote_id) = self.sftp_remote_id_for_tab(tab_id) else {
            return self.render_empty_workspace(f32::from(window.viewport_size().width), cx);
        };
        let view = self.sftp_view().read(cx);
        if view.remote_load_pending && !view.remote_load_inflight {
            self.request_visible_sftp_remote_load(
                SftpSurfaceId::Tab(tab_id),
                remote_id.clone(),
                view.worker_sender(),
                cx,
            );
        }
        let remote_title = match &remote_id {
            SftpRemoteId::Ftp(id) => self
                .connection_store
                .get_ftp_profile(id)
                .map(|p| p.name.clone())
                .or_else(|| {
                    self.ftp_sessions.get(id).map(|r| {
                        format!(
                            "{}@{}:{}",
                            r.options.username, r.options.host, r.options.port
                        )
                    })
                })
                .unwrap_or_else(|| id.clone()),
            SftpRemoteId::Node(node_id) => self
                .ssh_nodes
                .get(node_id)
                .map(|node| node.title.clone())
                .unwrap_or_else(|| node_id.0.clone()),
            SftpRemoteId::Standalone(endpoint_id) => self
                .standalone_sftp_sessions
                .get(endpoint_id)
                .map(|runtime| runtime.title.clone())
                .unwrap_or_else(|| endpoint_id.clone()),
        };
        let pair_primary_title =
            self.sftp_pair_primary_remote_id(cx)
                .map(|remote_id| match remote_id {
                    SftpRemoteId::Ftp(id) => id,
                    SftpRemoteId::Node(node_id) => node_id.0,
                    SftpRemoteId::Standalone(endpoint_id) => self
                        .connection_store
                        .get_standalone_sftp_profile(&endpoint_id)
                        .map(|profile| profile.host.clone())
                        .unwrap_or(endpoint_id),
                });
        let remote_title = match &remote_id {
            SftpRemoteId::Standalone(endpoint_id) if endpoint_id.ends_with(":secondary") => {
                let profile_id = endpoint_id.trim_end_matches(":secondary");
                self.connection_store
                    .get_standalone_sftp_profile(profile_id)
                    .and_then(|profile| profile.secondary_endpoint.as_ref())
                    .map(|endpoint| endpoint.host.clone())
                    .unwrap_or(remote_title)
            }
            _ => remote_title,
        };

        self.mount_sftp_browser(
            false,
            None,
            remote_title,
            pair_primary_title,
            self.background_surface_active("sftp"),
            cx,
        )
    }

    fn mount_sftp_browser(
        &self,
        sidebar: bool,
        target: Option<NodeId>,
        remote_title: String,
        pair_primary_title: Option<String>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::super::selectable_text::selectable_document_group_id;
        use oxideterm_gpui_sftp::{SelectionRole, SftpBrowserFrame, SftpBrowserView};
        let source = self.sftp_view().clone();
        let surface = self.sftp_surface_id();
        let window_owner = cx.entity().downgrade();
        let text = self.selectable_text_render_state(cx);
        let anchors = self.text_input_anchors.clone();
        let reveal_terminal_key =
            crate::keybindings::action_definition("sftp.revealTerminalDirectory")
                .and_then(|definition| {
                    crate::keybindings::effective_combo(
                        definition,
                        &self.settings_store.settings().keybindings.overrides,
                        crate::keybindings::KeybindingSide::current(),
                    )
                })
                .map(|combo| crate::keybindings::format_combo(&combo));
        let bookmark_remote_key = source
            .read(cx)
            .current_remote_id
            .as_ref()
            .map(|remote| self.sftp_bookmark_key_for_remote(remote));
        let frame = SftpBrowserFrame {
            sidebar,
            sidebar_target: target,
            sidebar_open: self.embedded_sftp_node_id.clone(),
            active_sftp_tab: self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Sftp),
            embedded_sftp_pinned: self.embedded_sftp_pinned,
            remote_title,
            pair_primary_title,
            terminal_directory: self.sftp_terminal_directory(cx),
            reveal_terminal_key,
            can_sync_terminal: self.sftp_terminal_target_pane(cx).is_some(),
            has_background,
            tree_enabled: self.settings_store.settings().sftp.sidebar_view_mode
                == oxideterm_settings::SftpSidebarViewMode::Tree,
            bookmarks: self.sftp_bookmarks.clone(),
            bookmark_remote_key,
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            focus_handle: self.focus_handle.clone(),
            mono_font: settings_mono_font_family(self.settings_store.settings()),
            ime: [
                SftpInput::LocalPath,
                SftpInput::RemotePath,
                SftpInput::LocalFilter,
                SftpInput::RemoteFilter,
            ]
            .into_iter()
            .map(|input| (input, self.sftp_input_ime(surface, input, cx)))
            .collect(),
            completion_anchor: Rc::new(move |input| {
                anchors.bounds(WorkspaceImeTarget::Sftp(surface, input).anchor_id())
            }),
            selectable: Rc::new(move |role, group, id, order, value, color| {
                let role = match role {
                    SelectionRole::Document => SelectableTextRole::PlainDocument,
                    SelectionRole::RowSafe => SelectableTextRole::RowSafe,
                    SelectionRole::NonSelectable => SelectableTextRole::NonSelectable,
                };
                text.render_display_text_with_id_in_group(
                    role,
                    group.unwrap_or_else(selectable_document_group_id),
                    id,
                    order,
                    value,
                    color,
                )
            }),
            dispatch: Rc::new(move |request, window, cx| {
                let _ = window_owner.update(cx, |app, cx| {
                    if !app.has_sftp_surface(surface) {
                        return;
                    }
                    let _scope = app.enter_sftp_surface(surface);
                    app.handle_sftp_browser_request(surface, request, window, cx);
                });
            }),
        };
        SftpBrowserView::mount(&source, frame, cx)
    }

    fn handle_sftp_browser_request(
        &mut self,
        surface: SftpSurfaceId,
        request: SftpBrowserRequest,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match request {
            SftpBrowserRequest::Focus => self.focus_sftp_surface(surface, cx),
            SftpBrowserRequest::CloseSidebar(node) => {
                self.close_embedded_sftp_for_node(&node, cx);
            }
            SftpBrowserRequest::OpenTab(node, path) => {
                self.open_sftp_tab_surface(node, Some(path), cx)
            }
            SftpBrowserRequest::TogglePin => {
                self.embedded_sftp_pinned = !self.embedded_sftp_pinned;
                self.activate_embedded_sftp_sidebar_if_visible(cx);
                cx.notify();
            }
            SftpBrowserRequest::BrowseUploads => self.browse_sftp_upload_files(cx),
            SftpBrowserRequest::BrowseFolder => self.browse_sftp_local_folder(cx),
            SftpBrowserRequest::ResizeCaptureChanged => cx.notify(),
            SftpBrowserRequest::RevealTerminalDirectory => self.reveal_sftp_terminal_directory(cx),
            SftpBrowserRequest::SyncTerminalDirectory(path) => {
                if let Some(window) = window {
                    self.sync_sftp_directory_to_terminal(path, window, cx);
                }
            }
            SftpBrowserRequest::ToggleTreeMode => self.edit_settings(
                |settings| {
                    use oxideterm_settings::SftpSidebarViewMode::{Flat, Tree};
                    settings.sftp.sidebar_view_mode = match settings.sftp.sidebar_view_mode {
                        Flat => Tree,
                        Tree => Flat,
                    };
                },
                cx,
            ),
            SftpBrowserRequest::EnsureConnection(node) => {
                self.ensure_node_connection_started(&node, cx);
            }
            SftpBrowserRequest::QuickDownload => self.queue_quick_scp_download(cx),
            SftpBrowserRequest::LoadPairPrimary => self.request_sftp_pair_primary_load(cx),
            SftpBrowserRequest::LoadRemote => self.request_sftp_remote_load(cx),
            SftpBrowserRequest::Transfer(pane, direction) => {
                self.queue_sftp_transfers(pane, direction, cx)
            }
            SftpBrowserRequest::TreeDownload(file) => self.queue_sftp_tree_download(file, cx),
            SftpBrowserRequest::Preview(pane, file) => {
                self.open_or_preview_sftp_file(pane, &file, cx)
            }
            SftpBrowserRequest::Extract(file) => self.extract_remote_sftp_archive(file, cx),
            SftpBrowserRequest::SaveBookmark => self.save_current_sftp_bookmark(cx),
            SftpBrowserRequest::RemoveBookmark(id) => {
                let result = self
                    .sftp_bookmarks
                    .update(cx, |bookmarks, cx| bookmarks.remove(&id, cx));
                self.sftp_bookmark_write_result(result, cx);
            }
            SftpBrowserRequest::DismissOutside => {
                if let Some(window) = window {
                    self.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                }
            }
            SftpBrowserRequest::InputPressed(input, event) => {
                if let Some(window) = window {
                    self.ime_marked_text = None;
                    window.focus(&self.focus_handle, cx);
                    self.begin_ime_selection_from_mouse_down(
                        WorkspaceImeTarget::Sftp(surface, input),
                        &event,
                        window,
                        cx,
                    );
                }
            }
            SftpBrowserRequest::InputDragged(event) => {
                if let Some(window) = window {
                    self.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                }
            }
            SftpBrowserRequest::TransferFiles(pane, direction, names) => {
                self.queue_sftp_named_transfers(pane, direction, names, cx);
            }
            SftpBrowserRequest::ExternalDrop(paths) => {
                self.queue_sftp_external_upload_paths(&paths, cx)
            }
        }
    }

    fn save_current_sftp_bookmark(&self, cx: &mut Context<Self>) {
        let view = self.sftp_view().read(cx);
        let Some(remote) = view.current_remote_id.as_ref() else {
            return;
        };
        let (local_path, remote_path) = (
            view.local_path.trim().to_owned(),
            view.remote_path.trim().to_owned(),
        );
        let remote_key = self.sftp_bookmark_key_for_remote(remote);
        let template = self.i18n.t("sftp.bookmarks.default_name");
        match self.sftp_bookmarks.update(cx, |bookmarks, cx| {
            bookmarks.save_paths(remote_key, local_path, remote_path, &template, cx)
        }) {
            Ok(true) => self.push_sftp_toast(
                self.i18n.t("sftp.bookmarks.saved"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            ),
            Ok(false) => {}
            Err(error) => {
                self.sftp_bookmark_write_result(Err(error), cx);
            }
        }
    }
}
