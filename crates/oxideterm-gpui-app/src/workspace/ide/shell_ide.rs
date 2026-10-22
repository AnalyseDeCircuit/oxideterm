use super::*;
use gpui::Window;
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_workspace::PaneId;

enum ShellOpenError {
    Failed,
    Unsupported,
}

struct AbortShellOpen(tokio::task::AbortHandle);
impl Drop for AbortShellOpen {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn request_shell_ide(
        &mut self,
        pane_id: PaneId,
        pane: Entity<TerminalPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self
            .ide_workspace
            .read(cx)
            .shell_sources
            .iter()
            .find(|(_, source)| **source == pane_id)
            .map(|(tab, _)| *tab);
        if let Some(tab) = existing {
            self.activate_shell_ide(tab, cx);
            return;
        }
        if self
            .ide_workspace
            .read(cx)
            .shell_open_tasks
            .contains_key(&pane_id)
        {
            return;
        }
        let Some(_) = self
            .tabs(cx)
            .iter()
            .find(|tab| {
                let mut ids = Vec::new();
                if let Some(root) = &tab.root_pane {
                    root.collect_pane_ids(&mut ids);
                }
                ids.contains(&pane_id)
            })
            .map(|tab| tab.id)
        else {
            return;
        };
        let source_session = pane.read(cx).shared_session();
        let prompt = choice_dialog(
            self.tokens,
            ChoiceDialog {
                title: self.i18n.t("terminal.shell_ide.convert"),
                description: Some(self.i18n.t("terminal.shell_ide.confirm")),
                choices: [
                    self.i18n.t("terminal.shell_ide.commands"),
                    self.i18n.t("terminal.shell_ide.agent"),
                ],
                cancel: self.i18n.t("common.actions.cancel"),
                confirm: self.i18n.t("common.actions.confirm"),
            },
            window,
            cx,
        );
        let weak_pane = pane.downgrade();
        let fs = self.ide_workspace.read(cx).fs.clone();
        let runtime = self.ide_workspace.read(cx).backend_runtime.clone();
        let (progress_tx, mut progress_rx) =
            tokio::sync::watch::channel(oxideterm_ide_fs::AgentInstallProgress::Checking);
        let labels = self.ide_labels();
        let progress_pane = weak_pane.clone();
        let progress_task = cx.spawn(async move |_weak, cx| {
            while progress_rx.changed().await.is_ok() {
                let status = labels.agent_install_status(*progress_rx.borrow_and_update());
                let Some(pane) = progress_pane.upgrade() else {
                    break;
                };
                pane.update(cx, |pane, cx| pane.set_shell_ide_progress(status, cx));
            }
        });
        let task = cx.spawn(async move |weak, cx| {
            // The startup task owns the progress observer, including prompt cancellation.
            let _progress_task = progress_task;
            let result = async {
                let access = match prompt.await {
                    Some(0) => oxideterm_ide_fs::RemoteAccess::Commands,
                    Some(1) => oxideterm_ide_fs::RemoteAccess::Agent,
                    _ => return Ok(None),
                };
                let acquisition = weak
                    .update(cx, |this, cx| {
                        let pane = weak_pane.upgrade().ok_or(ShellOpenError::Failed)?;
                        let current = this
                            .tab_host
                            .read(cx)
                            .panes()
                            .get(&pane_id)
                            .ok_or(ShellOpenError::Failed)?;
                        if current != &pane
                            || !Arc::ptr_eq(&source_session, &pane.read(cx).shared_session())
                        {
                            return Err(ShellOpenError::Failed);
                        }
                        this.activate_shell_terminal(pane_id, cx);
                        pane.update(cx, |pane, cx| pane.set_shell_ide_preparing(true, cx));
                        let result = source_session
                            .lock()
                            .acquire_shell_channel()
                            .map_err(|_| ShellOpenError::Failed)?;
                        Ok(result)
                    })
                    .map_err(|_| ShellOpenError::Failed)??;
                let owner = source_session.clone();
                let job = runtime.spawn(async move {
                    let mut channel =
                        tokio::time::timeout(std::time::Duration::from_secs(10), acquisition)
                            .await
                            .map_err(|_| ShellOpenError::Failed)?
                            .map_err(|_| ShellOpenError::Failed)?
                            .map_err(|_| ShellOpenError::Failed)?;
                    // The IDE keeps this exact terminal consumer alive through agent shutdown.
                    channel.retain_terminal(owner);
                    let mut released = channel.release_receiver();
                    let opened = fs
                        .from_shell_channel(channel, access, Some(progress_tx))
                        .await;
                    if opened.is_err() {
                        // A transport error can precede its asynchronous PTY cleanup.
                        // Keep startup pending until this terminal can accept input or has closed.
                        let _ = released.wait_for(|released| *released).await;
                    }
                    opened.map_err(|error| {
                        if error.kind == oxideterm_ide_core::IdeFileErrorKind::Unsupported {
                            ShellOpenError::Unsupported
                        } else {
                            ShellOpenError::Failed
                        }
                    })
                });
                let _cancel = AbortShellOpen(job.abort_handle());
                let opened = job.await.map_err(|_| ShellOpenError::Failed)??;
                Ok::<_, ShellOpenError>(Some(opened))
            }
            .await;
            let _ = weak.update(cx, |this, cx| {
                if let Some(pane) = weak_pane.upgrade() {
                    pane.update(cx, |pane, cx| pane.set_shell_ide_preparing(false, cx));
                }
                this.ide_workspace.update(cx, |registry, _| {
                    registry.shell_open_tasks.remove(&pane_id);
                });
                match result {
                    Ok(Some((fs, cwd))) => this.open_shell_ide_surface(fs, cwd, pane_id, cx),
                    Ok(None) => {}
                    Err(error) => this.push_command_palette_toast(
                        this.i18n.t("terminal.shell_ide.convert"),
                        Some(this.i18n.t(match error {
                            ShellOpenError::Unsupported => "terminal.shell_ide.requirements",
                            ShellOpenError::Failed => "terminal.shell_ide.failed",
                        })),
                        super::super::TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            });
        });
        self.ide_workspace.update(cx, |registry, _| {
            registry.shell_open_tasks.insert(pane_id, task);
        });
    }

    fn open_shell_ide_surface(
        &mut self,
        fs: NodeAgentIdeFileSystem,
        cwd: String,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        if !self.tab_host.read(cx).panes().contains_key(&pane_id) {
            fs.close_all_ide_sessions();
            return;
        }
        let Some(id) = fs.terminal_session_id().map(str::to_owned) else {
            return;
        };
        let node_id = NodeId::new(id.clone());
        let tab_id = self.alloc_tab_id(cx);
        let tokens = self.tokens;
        let labels = self.ide_labels();
        let mut settings = self.ide_runtime_settings();
        settings.agent_mode = NodeAgentMode::Enabled;
        // The registry key identifies this surface without creating another tab or pane.
        // The original terminal entity and layout remain owned by the tab host.
        self.ide_workspace.update(cx, |registry, cx| {
            let runtime = registry.backend_runtime.clone();
            let surface = cx.new(|cx| IdeSurface::new(fs, tokens, labels, settings, runtime, cx));
            surface.update(cx, |surface, cx| surface.open_remote_project(id, cwd, cx));
            registry.register_surface(tab_id, node_id, surface, cx);
            registry.shell_sources.insert(tab_id, pane_id);
        });
        self.activate_shell_ide(tab_id, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn guard_shell_ide_close(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut panes = Vec::new();
        if let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        {
            root.collect_pane_ids(&mut panes);
        }
        let surface_id = panes.into_iter().find_map(|pane| {
            self.ide_workspace
                .read(cx)
                .shell_surface_for_pane(pane)
                .map(|(id, _)| id)
        });
        let Some(surface_id) = surface_id else {
            return false;
        };
        self.activate_shell_ide(surface_id, cx);
        if let Some(surface) = self.ide_workspace.read(cx).surface(surface_id) {
            surface.update(cx, |surface, cx| surface.request_shell_return(cx));
        }
        true
    }

    fn activate_shell_ide(&mut self, surface_id: TabId, cx: &mut Context<Self>) {
        let pane = self
            .ide_workspace
            .read(cx)
            .shell_sources
            .get(&surface_id)
            .copied();
        if let Some(pane) = pane {
            self.activate_shell_terminal(pane, cx);
        }
    }

    fn activate_shell_terminal(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let tab = self
            .tabs(cx)
            .iter()
            .find(|tab| {
                let mut ids = Vec::new();
                if let Some(root) = &tab.root_pane {
                    root.collect_pane_ids(&mut ids);
                }
                ids.contains(&pane)
            })
            .map(|tab| tab.id);
        if let Some(tab) = tab {
            self.tab_host.update(cx, |host, _| {
                host.set_active_pane(Some(tab), pane);
            });
            if !self.tab_host.read(cx).is_outside_main_window(tab) {
                self.set_main_window_active_tab(Some(tab), cx);
                self.needs_active_pane_focus = true;
            } else if let Some(handle) = self.tab_host.read(cx).detached_window_handle(tab) {
                let entity = cx.entity();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        entity.update(cx, |this, cx| this.focus_tab_terminal(tab, window, cx));
                    });
                });
            }
            self.sync_ide_surface_mount(tab, cx);
        }
        cx.notify();
    }

    pub(super) fn finish_shell_ide_return(&mut self, surface_id: TabId, cx: &mut Context<Self>) {
        let source = self.ide_workspace.update(cx, |registry, cx| {
            let source = registry.shell_sources.get(&surface_id).copied();
            registry.close_surface(surface_id, IdeSurfaceCloseReason::UserProjectClose, cx);
            source
        });
        if let Some(pane) = source {
            self.activate_shell_terminal(pane, cx);
        }
    }

    pub(in crate::workspace) fn release_shell_ide_for_pane(
        &mut self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) {
        self.ide_workspace.update(cx, |registry, cx| {
            registry.shell_open_tasks.remove(&pane);
            if let Some((id, _)) = registry.shell_surface_for_pane(pane) {
                registry.close_surface(id, IdeSurfaceCloseReason::TransientFolderPickerCancel, cx);
            }
        });
    }
}
