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
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(self.i18n.t("sftp.context.upload"))),
        });
        cx.spawn(async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let _ = workspace.update(cx, |workspace, cx| {
                // The selected paths enter the same node-owned transfer path as
                // drag-and-drop uploads; the picker does not create a transport.
                workspace.queue_sftp_external_upload_paths_for_node(node_id, &paths, cx);
            });
        })
        .detach();
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
        self.sftp_view.update(cx, |sftp, cx| {
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
