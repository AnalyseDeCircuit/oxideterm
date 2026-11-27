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

const CLIPBOARD_IMAGE_UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClipboardImagePasteOrigin {
    pane_id: PaneId,
    session_id: TerminalSessionId,
    node_id: NodeId,
    pane_entity_id: gpui::EntityId,
    reconnect_generation: u64,
    connection_id: Option<String>,
}

fn clipboard_image_paste_target_matches(
    origin: &ClipboardImagePasteOrigin,
    active_pane_id: Option<PaneId>,
    current_session_id: Option<TerminalSessionId>,
    current_node_id: Option<&NodeId>,
    current_pane_entity_id: Option<gpui::EntityId>,
    current_reconnect_generation: Option<u64>,
    current_connection_id: Option<&str>,
    accepts_input: bool,
) -> bool {
    accepts_input
        && active_pane_id == Some(origin.pane_id)
        && current_session_id == Some(origin.session_id)
        && current_node_id == Some(&origin.node_id)
        && current_pane_entity_id == Some(origin.pane_entity_id)
        && current_reconnect_generation == Some(origin.reconnect_generation)
        && current_connection_id == origin.connection_id.as_deref()
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

    pub(in crate::workspace) fn handle_sftp_terminal_clipboard_image_paste(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        image: oxideterm_gpui_terminal::TerminalClipboardImage,
        cx: &mut Context<Self>,
    ) {
        if image.bytes.is_empty() {
            return;
        }
        if image.bytes.len() > oxideterm_sftp::CLIPBOARD_IMAGE_MAX_BYTES {
            let max_mb = oxideterm_sftp::CLIPBOARD_IMAGE_MAX_BYTES / (1024 * 1024);
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.clipboard_image_upload_failed"),
                Some(
                    self.i18n
                        .t("sftp.toast.clipboard_image_too_large")
                        .replace("{{size}}", &format!("{max_mb} MiB")),
                ),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let Some(node_id) = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
        else {
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        let origin = {
            let pane_state = pane.read(cx);
            if !pane_state.terminal_accepts_input() {
                return;
            }
            ClipboardImagePasteOrigin {
                pane_id,
                session_id,
                node_id: node_id.clone(),
                pane_entity_id: pane.entity_id(),
                reconnect_generation: pane_state.ssh_reconnect_generation(),
                connection_id: pane_state
                    .ssh_connection_handle()
                    .map(|connection| connection.connection_id().to_string()),
            }
        };
        let remote_id = SftpRemoteId::Node(node_id);
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.clipboard_image_upload_failed"),
                Some(self.i18n.t("sftp.toast.clipboard_image_sftp_unavailable")),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };

        // Run SFTP acquisition and upload on the node runtime with a bounded timeout
        // and early cancellation if the GPUI waiter is dropped during shutdown.
        let (mut sender, receiver) = tokio::sync::oneshot::channel();
        let runtime = self.forwarding_runtime.clone();
        let extension = image.extension;
        let bytes = image.bytes;
        let timeout_message = self.i18n.t("sftp.toast.clipboard_image_upload_timeout");
        runtime.spawn(async move {
            let upload = async {
                let sftp = backend.acquire_transfer_sftp().await?;
                sftp.write_private_clipboard_image(extension, &bytes)
                    .await
                    .map_err(|error| error.to_string())
            };
            let result = tokio::select! {
                _ = sender.closed() => return,
                timed = tokio::time::timeout(CLIPBOARD_IMAGE_UPLOAD_TIMEOUT, upload) => {
                    match timed {
                        Ok(result) => result,
                        Err(_) => Err(timeout_message),
                    }
                }
            };
            let _ = sender.send(result);
        });
        cx.spawn(async move |weak, cx| {
            let Ok(result) = receiver.await else {
                return;
            };
            let _ = weak.update(cx, |this, cx| {
                this.finish_sftp_terminal_clipboard_image_paste(origin, result, cx);
            });
        })
        .detach();
    }

    fn finish_sftp_terminal_clipboard_image_paste(
        &mut self,
        origin: ClipboardImagePasteOrigin,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        let remote_path = match result {
            Ok(path) => path,
            Err(error) => {
                self.push_sftp_toast(
                    self.i18n.t("sftp.toast.clipboard_image_upload_failed"),
                    Some(error),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };

        let active_pane_id = self.active_pane_id(cx);
        let current_session_id = self.session_id_for_pane(origin.pane_id, cx);
        let current_node_id = current_session_id.and_then(|session_id| {
            self.workspace_runtime
                .read(cx)
                .ssh_terminal_node_id(session_id)
        });
        let pane = self.tab_host.read(cx).panes().get(&origin.pane_id).cloned();
        let (
            current_pane_entity_id,
            current_reconnect_generation,
            current_connection_id,
            accepts_input,
        ) = match pane.as_ref() {
            Some(pane) => {
                let state = pane.read(cx);
                (
                    Some(pane.entity_id()),
                    Some(state.ssh_reconnect_generation()),
                    state
                        .ssh_connection_handle()
                        .map(|connection| connection.connection_id().to_string()),
                    state.terminal_accepts_input(),
                )
            }
            None => (None, None, None, false),
        };

        if !clipboard_image_paste_target_matches(
            &origin,
            active_pane_id,
            current_session_id,
            current_node_id.as_ref(),
            current_pane_entity_id,
            current_reconnect_generation,
            current_connection_id.as_deref(),
            accepts_input,
        ) {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.clipboard_image_target_changed"),
                Some(remote_path),
                TerminalNoticeVariant::Warning,
                cx,
            );
            return;
        }

        let Some(pane) = pane else {
            return;
        };
        let formatted_path = oxideterm_sftp::format_posix_shell_path(&remote_path);
        // Insert into the originating pane only, without sending Enter or broadcasting
        // a host-local remote path to other synced terminals.
        let pasted = pane.update(cx, |pane, cx| pane.paste_text_to_self(&formatted_path, cx));
        if !pasted {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.clipboard_image_target_changed"),
                Some(remote_path),
                TerminalNoticeVariant::Warning,
                cx,
            );
        }
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
