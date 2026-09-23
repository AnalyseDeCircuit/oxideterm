use super::*;

pub(in crate::workspace) struct SftpPage {
    view: Entity<SftpWorkspaceEntity>,
    _subscriptions: Vec<Subscription>,
}

/// Pins a synchronous dispatch to its source view without moving or copying page state.
pub(in crate::workspace) struct SftpSurfaceScope {
    slot: Rc<Cell<Option<SftpSurfaceId>>>,
    previous: Option<SftpSurfaceId>,
}

impl Drop for SftpSurfaceScope {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn sftp_surface_id(&self) -> SftpSurfaceId {
        self.sftp_dispatch_surface
            .get()
            .unwrap_or(self.sftp_focused_surface)
    }

    pub(in crate::workspace) fn sftp_view(&self) -> &Entity<SftpWorkspaceEntity> {
        match self.sftp_surface_id() {
            SftpSurfaceId::Sidebar => &self.sftp_view,
            SftpSurfaceId::Tab(id) => {
                &self
                    .sftp_pages
                    .get(&id)
                    .expect("SFTP dispatch requires a live page")
                    .view
            }
        }
    }

    pub(in crate::workspace) fn enter_sftp_surface(&self, id: SftpSurfaceId) -> SftpSurfaceScope {
        SftpSurfaceScope {
            slot: self.sftp_dispatch_surface.clone(),
            previous: self.sftp_dispatch_surface.replace(Some(id)),
        }
    }

    pub(in crate::workspace) fn has_sftp_surface(&self, id: SftpSurfaceId) -> bool {
        match id {
            SftpSurfaceId::Sidebar => true,
            SftpSurfaceId::Tab(id) => self.sftp_pages.contains_key(&id),
        }
    }

    pub(in crate::workspace) fn ensure_sftp_page(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.sftp_pages.contains_key(&id) {
            return;
        }
        let view = cx.new(SftpWorkspaceEntity::new);
        let observation = cx.observe(&view, |_, _, cx| cx.notify());
        let events = cx.subscribe(
            &view,
            move |workspace, _, event: &SftpWorkspaceEvent, cx| {
                workspace.handle_sftp_surface_event(SftpSurfaceId::Tab(id), event, cx);
            },
        );
        self.sftp_pages.insert(
            id,
            SftpPage {
                view,
                _subscriptions: vec![observation, events],
            },
        );
    }

    pub(in crate::workspace) fn close_sftp_page(&mut self, id: TabId) {
        if self.sftp_focused_surface == SftpSurfaceId::Tab(id) {
            self.sftp_focused_surface = SftpSurfaceId::Sidebar;
        }
        if self.sftp_dispatch_surface.get() == Some(SftpSurfaceId::Tab(id)) {
            self.sftp_dispatch_surface.set(None);
        }
        self.sftp_pages.remove(&id);
    }

    pub(in crate::workspace) fn focus_sftp_surface(
        &mut self,
        id: SftpSurfaceId,
        cx: &mut Context<Self>,
    ) {
        if self.sftp_focused_surface != id {
            let _scope = self.enter_sftp_surface(self.sftp_focused_surface);
            self.sftp_view().update(cx, |view, cx| {
                view.clear_input_focus(cx);
            });
            self.ime_marked_text = None;
            self.clear_ime_selection();
            self.sftp_focused_surface = id;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn sftp_surface_window(
        &self,
        surface: SftpSurfaceId,
        cx: &App,
    ) -> Option<gpui::WindowId> {
        if !self.has_sftp_surface(surface) {
            return None;
        }
        let detached = match surface {
            SftpSurfaceId::Tab(id) => self.tab_host.read(cx).detached_window_handle(id),
            SftpSurfaceId::Sidebar => None,
        };
        detached
            .or_else(|| {
                self.window_registry
                    .handle_for_role(crate::workspace::window_registry::WindowRole::Main)
            })
            .map(|handle| handle.window_id())
    }

    pub(in crate::workspace) fn sftp_listener<E: ?Sized + 'static>(
        &self,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let surface = self.sftp_surface_id();
        cx.listener(move |workspace, event, window, cx| {
            // An event queued by a closed page must not fall through to another view.
            if !workspace.has_sftp_surface(surface) {
                return;
            }
            let _scope = workspace.enter_sftp_surface(surface);
            f(workspace, event, window, cx);
        })
    }

    pub(in crate::workspace) fn handle_sftp_surface_event(
        &mut self,
        surface: SftpSurfaceId,
        event: &SftpWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.has_sftp_surface(surface) {
            return;
        }
        let _scope = self.enter_sftp_surface(surface);
        match event {
            sftp::SftpWorkspaceEvent::WorkerEffectsReady(effects) => {
                self.handle_sftp_worker_effects(effects, cx);
            }
            sftp::SftpWorkspaceEvent::OpenFileRequested { pane, file } => {
                self.open_or_preview_sftp_file(*pane, file, cx);
            }
            sftp::SftpWorkspaceEvent::TransferStateRequested { id, state } => {
                self.set_sftp_transfer_state(*id, *state, cx);
            }
            sftp::SftpWorkspaceEvent::CancelOrRemoveTransferRequested { id } => {
                self.cancel_or_remove_sftp_transfer(*id, cx);
            }
            sftp::SftpWorkspaceEvent::ResumeIncompleteTransferRequested { transfer_id } => {
                self.resume_sftp_incomplete_transfer(transfer_id.clone(), cx);
            }
            sftp::SftpWorkspaceEvent::DiscardIncompleteTransferRequested { transfer_id } => {
                self.discard_sftp_incomplete_transfer(transfer_id.clone(), cx);
            }
            sftp::SftpWorkspaceEvent::TooltipRequested { id, label, x, y } => {
                self.queue_workspace_tooltip(id, label, *x, *y, cx);
            }
            sftp::SftpWorkspaceEvent::TooltipCleared { id } => {
                self.clear_workspace_tooltip(id, cx);
            }
            sftp::SftpWorkspaceEvent::PreviewSaveRequested {
                path,
                content,
                encoding,
                line_ending,
                generation,
                delivery,
            } => {
                if !self.spawn_remote_sftp_preview_save(
                    path.clone(),
                    content.clone(),
                    encoding.clone(),
                    *line_ending,
                    *generation,
                    delivery.clone(),
                    cx,
                ) {
                    let _ = delivery.send(sftp::SftpWorkerResult::PreviewSaved {
                        generation: *generation,
                        path: path.clone(),
                        content: content.clone(),
                        network_error_message: self.i18n.t("sftp.errors.connection_lost"),
                        result: Err("SFTP connection unavailable".to_string()),
                    });
                }
            }
            sftp::SftpWorkspaceEvent::RemoteLoadReady {
                surface_id,
                remote_id,
                delivery,
            } => {
                self.request_visible_sftp_remote_load(
                    *surface_id,
                    remote_id.clone(),
                    delivery.clone(),
                    cx,
                );
            }
        }

        cx.notify();
    }
}
