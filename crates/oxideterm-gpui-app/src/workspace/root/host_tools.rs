use super::super::*;
use crate::workspace::connection_monitor::{
    HostToolsNotice, LOCAL_HOST_CONNECTION_ID, ScheduleActionNoticeKind,
};

/// Pane identity herdr exports into every process of a managed pane.
const HERDR_PANE_ENV_VARS: [&str; 5] = [
    "HERDR_ENV",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "HERDR_SOCKET_PATH",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_tool_terminal_runs_native_command_without_loading_preferred_shell() {
        let line = if cfg!(windows) {
            "echo oxide-host-tool&exit 7"
        } else {
            "printf oxide-host-tool; exit 7"
        };
        let config = local_host_tool_terminal_config(line, "Host tool");
        let shell = config.shell.unwrap();
        let args = shell.args.clone();
        let mut process = std::process::Command::new(&shell.path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            assert!(shell.path.ends_with(r"System32\cmd.exe"));
            // ConPTY passes the shell's command tail without CRT escaping.
            for arg in args {
                process.raw_arg(arg);
            }
        }
        #[cfg(unix)]
        {
            assert_eq!(shell.path, std::path::Path::new("/bin/sh"));
            process.args(args);
        }
        let output = process.output().unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "oxide-host-tool"
        );
        assert_eq!(output.status.code(), Some(7));
    }
}

fn local_host_tool_terminal_config(command: &str, title: &str) -> LocalPtyConfig {
    // Match the native host used by captures, not the user's preferred WSL or custom shell.
    #[cfg(windows)]
    let shell = {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        oxideterm_terminal::ShellInfo::new(
            "host-tool",
            title,
            std::path::PathBuf::from(root).join(r"System32\cmd.exe"),
        )
        .with_args(vec!["/D".into(), "/K".into(), command.into()])
    };
    #[cfg(not(windows))]
    let shell = oxideterm_terminal::ShellInfo::new("host-tool", title, "/bin/sh")
        .with_args(vec!["-c".into(), format!("{command}\nexec /bin/sh -i")]);
    LocalPtyConfig {
        shell: Some(shell),
        load_profile: false,
        remove_env: HERDR_PANE_ENV_VARS
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        ..Default::default()
    }
}

impl WorkspaceApp {
    /// Handles the one-shot window work that must remain with workspace-owned nodes and tabs.
    pub(in crate::workspace) fn handle_host_tools_window_request(
        &mut self,
        request: &HostToolsWindowRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(intent) = request.take() else {
            return;
        };
        match intent {
            HostToolsWindowIntent::OpenLocalMultiplexer {
                command,
                title,
                opened_notice,
                missing_notice,
            } => {
                // The inventory belongs to the native host, even if the preferred terminal starts WSL.
                #[cfg(windows)]
                let shell = {
                    let root =
                        std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
                    oxideterm_terminal::ShellInfo::new(
                        "host-multiplexer",
                        &title,
                        std::path::PathBuf::from(root).join(r"System32\cmd.exe"),
                    )
                    .with_args(vec!["/D".into(), "/C".into(), command])
                };
                #[cfg(not(windows))]
                let shell =
                    oxideterm_terminal::ShellInfo::new("host-multiplexer", &title, "/bin/sh")
                        .with_args(vec!["-c".into(), command]);
                let config = LocalPtyConfig {
                    shell: Some(shell),
                    remove_env: HERDR_PANE_ENV_VARS
                        .iter()
                        .copied()
                        .chain([
                            "HERDR_SESSION",
                            "ZELLIJ",
                            "ZELLIJ_SESSION_NAME",
                            "ZELLIJ_PANE_ID",
                        ])
                        .map(str::to_owned)
                        .collect(),
                    ..Default::default()
                };
                let opened = self
                    .create_local_terminal_tab_with_config(config, title, window, cx)
                    .is_ok();
                // The copy is earned by the tab, not by the click: reporting
                // the open before the terminal exists would cover a failed
                // launch with a success notice the user cannot act on.
                self.push_host_tools_window_notice(
                    if opened {
                        opened_notice
                    } else {
                        missing_notice
                    },
                    if opened {
                        TerminalNoticeVariant::Success
                    } else {
                        TerminalNoticeVariant::Error
                    },
                    cx,
                );
            }
            HostToolsWindowIntent::OpenExistingNodeTerminal {
                connection_id,
                command,
                title,
                opened_notice,
                missing_notice,
            } => {
                if connection_id == LOCAL_HOST_CONNECTION_ID {
                    self.open_local_host_tool_terminal(
                        &command,
                        title,
                        opened_notice,
                        missing_notice,
                        window,
                        cx,
                    );
                    cx.notify();
                    return;
                }
                let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                    cx.notify();
                    return;
                };
                if !self.ssh_nodes.contains_key(&node_id) {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                    cx.notify();
                    return;
                }
                // NodeRouter retains the physical connection; this creates only a tab consumer.
                match self.queue_ssh_terminal_tab_for_existing_node(
                    node_id,
                    Some(command),
                    title,
                    false,
                    window,
                    cx,
                ) {
                    Ok(()) => self.push_host_tools_window_notice(
                        opened_notice,
                        TerminalNoticeVariant::Success,
                        cx,
                    ),
                    Err(_) => self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
                cx.notify();
            }
            HostToolsWindowIntent::BeginPlainTextImeSelection { input, event } => {
                let Some(target) = workspace_ime_target_for_plain_host_tools_input(input) else {
                    // Secret-bearing dialog input never crosses this plain-text
                    // frame boundary.
                    return;
                };
                self.ime_marked_text = None;
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(target, &event, window, cx);
            }
            HostToolsWindowIntent::PrepareTmuxInputDialog => {
                // Only focus state crosses this boundary; the zeroizing input stays in the Entity.
                self.ime_marked_text = None;
                self.clear_ime_selection();
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
                cx.notify();
            }
            HostToolsWindowIntent::PrepareTmuxConfirm => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            HostToolsWindowIntent::PrepareMuxInputDialog => {
                // Only focus state crosses this boundary; the zeroizing input stays in the Entity.
                self.ime_marked_text = None;
                self.clear_ime_selection();
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
                cx.notify();
            }
            HostToolsWindowIntent::PrepareMuxConfirm => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            HostToolsWindowIntent::SetMonitoringEnabled { tool, enabled } => {
                self.set_host_tool_monitoring_enabled(tool, enabled, cx);
            }
        }
    }

    /// Diagnostic output stays in a native-host shell after the command finishes.
    fn open_local_host_tool_terminal(
        &mut self,
        command: &str,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = local_host_tool_terminal_config(command, &title);
        let opened = self
            .create_local_terminal_tab_with_config(config, title, window, cx)
            .is_ok();
        if opened {
            self.push_host_tools_window_notice(opened_notice, TerminalNoticeVariant::Success, cx);
        } else {
            self.push_host_tools_window_notice(missing_notice, TerminalNoticeVariant::Error, cx);
        }
    }

    fn push_host_tools_window_notice(
        &self,
        title: String,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn push_host_tools_notice(&self, notice: HostToolsNotice, cx: &App) {
        let (message, variant) = match notice {
            HostToolsNotice::ProcessActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_processes.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ProcessInvalidNice => (
                self.i18n.t("sidebar.host_processes.toast.invalid_nice"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessConnectionMissing => (
                self.i18n
                    .t("sidebar.host_processes.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_processes.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ProcessActionFailed => (
                self.i18n.t("sidebar.host_processes.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessActionFinished { pid, succeeded } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_processes.toast.action_succeeded",
                            &[("pid", pid)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_processes.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::DockerActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_docker.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::DockerLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_docker.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::DockerConnectionMissing => (
                self.i18n.t("sidebar.host_docker.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerActionFailed => (
                self.i18n.t("sidebar.host_docker.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerLogsFailed => (
                self.i18n.t("sidebar.host_docker.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerActionFinished {
                container_name,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_docker.toast.action_succeeded",
                            &[("name", container_name)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_docker.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::ServiceActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_services.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_services.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceConnectionMissing => (
                self.i18n
                    .t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServicePartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceActionFailed => (
                self.i18n.t("sidebar.host_services.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServiceLogsFailed => (
                self.i18n.t("sidebar.host_services.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServiceActionFinished {
                description,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_services.toast.action_succeeded",
                            &[("name", description)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_services.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::TmuxSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_tmux.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxConnectionMissing => (
                self.i18n.t("sidebar.host_tmux.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_tmux.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::TmuxUnavailable => (
                self.i18n.t("sidebar.host_tmux.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_tmux.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_tmux.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxInputRequired => (
                self.i18n.t("sidebar.host_tmux.toast.input_required"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxActionFailed => (
                self.i18n.t("sidebar.host_tmux.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxActionFinished {
                target_label,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_tmux.toast.action_succeeded",
                            &[("target", target_label)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_tmux.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::MuxSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.multiplexer.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::MuxConnectionMissing => (
                self.i18n.t("sidebar.multiplexer.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::MuxActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.multiplexer.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::MuxActionFailed => (
                self.i18n.t("sidebar.multiplexer.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::MuxReadAlreadyRunning => (
                self.i18n
                    .t("sidebar.multiplexer.toast.read_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::MuxReadFailed => (
                self.i18n.t("sidebar.multiplexer.toast.read_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::MuxActionSucceeded { target_label } => (
                self.i18n_replace(
                    "sidebar.multiplexer.toast.action_succeeded",
                    &[("target", target_label)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::LogSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_logs.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogConnectionMissing => (
                self.i18n.t("sidebar.host_logs.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::LogPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::LogUnavailable => (
                self.i18n.t("sidebar.host_logs.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_logs.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PortSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_ports.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortConnectionMissing => (
                self.i18n.t("sidebar.host_ports.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PortPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PortUnavailable => (
                self.i18n.t("sidebar.host_ports.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_ports.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PortKillCommandCopied { command } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.kill_command_copied",
                    &[("command", command)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PortEndpointCopied { endpoint } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.copied_endpoint",
                    &[("endpoint", endpoint)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::FilesystemSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_filesystems.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemConnectionMissing => (
                self.i18n
                    .t("sidebar.host_filesystems.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::FilesystemPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::FilesystemUnavailable => (
                self.i18n.t("sidebar.host_filesystems.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_filesystems.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::FilesystemPathCopied { path } => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.copied_path",
                    &[("path", path)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PackageSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_packages.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PackageConnectionMissing => (
                self.i18n
                    .t("sidebar.host_packages.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PackageSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PackageUnavailable => (
                self.i18n.t("sidebar.host_packages.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PackageSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_packages.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PackageInspectUnsupported { manager } => {
                let manager_label = if manager.trim().is_empty() {
                    "—".to_string()
                } else {
                    manager
                };
                (
                    self.i18n_replace(
                        "sidebar.host_packages.toast.inspect_unsupported",
                        &[("manager", manager_label)],
                    ),
                    TerminalNoticeVariant::Error,
                )
            }
            HostToolsNotice::PackageNameCopied { package_name } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.copied_name",
                    &[("name", package_name)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PackageIdCopied { package_id } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.copied_id",
                    &[("id", package_id)],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PackageUninstallUnsupported { manager } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.uninstall_unsupported",
                    &[("manager", manager)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PackageUpgradeUnsupported { manager } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.upgrade_unsupported",
                    &[("manager", manager)],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleConnectionMissing => (
                self.i18n
                    .t("sidebar.host_schedules.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::SchedulePartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::ScheduleUnavailable => (
                self.i18n.t("sidebar.host_schedules.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_schedules.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleLogsFailed => (
                self.i18n.t("sidebar.host_schedules.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleActionFailed => (
                self.i18n.t("sidebar.host_schedules.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleActionFinished {
                kind,
                task_name,
                succeeded,
            } => {
                if succeeded {
                    let message_key = match kind {
                        ScheduleActionNoticeKind::RunNow => {
                            "sidebar.host_schedules.toast.run_now_started"
                        }
                        ScheduleActionNoticeKind::Enable => {
                            "sidebar.host_schedules.toast.enable_succeeded"
                        }
                        ScheduleActionNoticeKind::Disable => {
                            "sidebar.host_schedules.toast.disable_succeeded"
                        }
                    };
                    (
                        self.i18n_replace(message_key, &[("name", task_name)]),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_schedules.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
        };
        self.push_workspace_notice(
            TerminalNotice {
                title: message,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }
}
