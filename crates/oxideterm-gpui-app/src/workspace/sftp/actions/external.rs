use super::*;

#[cfg(windows)]
const SFTP_EXTERNAL_BRIDGE_CREATE_NO_WINDOW: u32 = 0x08000000;

pub(in crate::workspace::sftp) fn open_path_in_external_app(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        configure_sftp_external_bridge(&mut command);
        command.args(["/C", "start", "", path]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    let status = command
        .status()
        .map_err(|error| format!("failed to launch external app: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("external app exited with status {status}"))
    }
}

#[cfg(target_os = "windows")]
fn configure_sftp_external_bridge(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    // The selected file/folder may open visibly, but the cmd.exe handoff should
    // not flash a separate console window in the GUI app.
    command.creation_flags(SFTP_EXTERNAL_BRIDGE_CREATE_NO_WINDOW);
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn browse_sftp_upload_files(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.visible_sftp_node_id(cx) else {
            return;
        };
        let generation = self.sftp_view().read(cx).view_generation;
        let remote_path = self.sftp_view().read(cx).remote_path.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(self.i18n.t("sftp.context.upload"))),
        });
        let surface = self.sftp_surface_id();
        cx.spawn(async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let _ = workspace.update(cx, |workspace, cx| {
                if !workspace.has_sftp_surface(surface) {
                    return;
                }
                let _scope = workspace.enter_sftp_surface(surface);
                if workspace.visible_sftp_node_id(cx).as_ref() != Some(&node_id)
                    || workspace.sftp_view().read(cx).view_generation != generation
                    || workspace.sftp_view().read(cx).remote_path != remote_path
                {
                    workspace.push_sftp_toast(
                        workspace.i18n.t("sftp.sidebar.upload_target_changed"),
                        None,
                        TerminalNoticeVariant::Warning,
                        cx,
                    );
                    return;
                }
                // The selected paths enter the same node-owned transfer path as
                // drag-and-drop uploads; the picker does not create a transport.
                workspace.queue_sftp_external_upload_paths_for_node(node_id, &paths, cx);
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn handle_sftp_terminal_drop(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        paths: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let Some(node) = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
        else {
            return;
        };
        let Some(target_dir) = self
            .tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .and_then(|pane| {
                crate::workspace::terminal_cwd::terminal_cwd_snapshot_from_pane(
                    oxideterm_environment::CurrentDirectoryScope::ssh_node(node.0.clone()),
                    pane.read(cx),
                )
                .map(|snapshot| snapshot.path().to_string())
            })
        else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toolbar.terminal_directory_unavailable"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        let pending_transfers = build_sftp_pending_uploads(&paths);
        if pending_transfers.is_empty() {
            return;
        }
        let remote_id = SftpRemoteId::Node(node);
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.upload_failed"),
                Some("SFTP endpoint is no longer available".to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        // SFTP session acquisition and channel teardown use Tokio primitives, so the
        // request must run on the node runtime instead of the GPUI foreground executor.
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let runtime = self.forwarding_runtime.clone();
        let listing_dir = target_dir.clone();
        runtime.spawn(async move {
            // Observational listing: it must not move the node's SFTP ready cwd.
            let result = load_remote_sftp_completion_listing(backend, &listing_dir).await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |weak, cx| {
            let Ok(result) = receiver.await else {
                return;
            };
            let _ = weak.update(cx, |this, cx| {
                this.finish_sftp_terminal_drop_upload(
                    remote_id,
                    target_dir,
                    pending_transfers,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    fn finish_sftp_terminal_drop_upload(
        &mut self,
        remote_id: SftpRemoteId,
        target_dir: String,
        pending_transfers: Vec<SftpPendingTransfer>,
        result: Result<RemoteSftpListing, String>,
        cx: &mut Context<Self>,
    ) {
        let listing = match result {
            Ok(listing) => listing,
            Err(error) => {
                self.push_sftp_toast(
                    self.i18n.t("sftp.toast.upload_failed"),
                    Some(error),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        let node_id = remote_id.node_id().cloned();
        // Prefer the node's open SFTP tab; otherwise make the sidebar queue
        // visible instead of silently accumulating transfers in a hidden view.
        let target_surface = node_id
            .as_ref()
            .and_then(|node| {
                self.sftp_tab_nodes.iter().find_map(|(tab_id, tab_node)| {
                    (tab_node == node && self.has_sftp_surface(SftpSurfaceId::Tab(*tab_id)))
                        .then_some(SftpSurfaceId::Tab(*tab_id))
                })
            })
            .unwrap_or(SftpSurfaceId::Sidebar);
        if target_surface == SftpSurfaceId::Sidebar
            && let Some(node) = node_id
        {
            self.open_sftp_sidebar_surface(node, Some(target_dir.clone()), cx);
        }
        let _scope = self.enter_sftp_surface(target_surface);
        let conflicts = sftp_transfer_conflicts(&pending_transfers, &listing.files);
        let action = self.settings_store.settings().sftp.conflict_action;
        let resolved_actions = conflicts
            .into_iter()
            .map(|conflict| {
                (
                    conflict.file_name,
                    sftp_terminal_drop_conflict_resolution(action),
                )
            })
            .collect::<HashMap<_, _>>();
        self.execute_sftp_pending_transfers(
            remote_id,
            pending_transfers,
            resolved_actions,
            Some((&target_dir, &listing.files)),
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn browse_sftp_local_folder(&mut self, cx: &mut Context<Self>) {
        // Tauri SFTP uses @tauri-apps/plugin-dialog `open({ directory: true,
        // multiple: false, defaultPath: localPath })` for this toolbar button.
        // GPUI's platform prompt does not expose defaultPath, but it does open
        // the same system directory chooser and returns the selected folder.
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("sftp.toolbar.browse_folder"),
            )),
        });
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.start_folder_picker(
                async move {
                    let Ok(Ok(Some(paths))) = receiver.await else {
                        return None;
                    };
                    paths
                        .into_iter()
                        .next()
                        .map(|path| path.to_string_lossy().to_string())
                },
                cx,
            );
        });
    }
}
