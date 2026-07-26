use super::super::*;
use crate::workspace::connection_monitor::{HostToolsNotice, ScheduleActionNoticeKind};

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
            HostToolsWindowIntent::OpenExistingNodeTerminal {
                connection_id,
                command,
                title,
                opened_notice,
                missing_notice,
            } => {
                let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                    return;
                };
                if !self.ssh_nodes.contains_key(&node_id) {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                    return;
                }
                // NodeRouter retains the physical connection; this creates only a tab consumer.
                match self.queue_ssh_terminal_tab_for_existing_node(
                    node_id,
                    Some(command),
                    title,
                    window,
                    cx,
                ) {
                    Ok(()) => self.push_host_tools_window_notice(
                        opened_notice,
                        TerminalNoticeVariant::Success,
                    ),
                    Err(_) => self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    ),
                }
                cx.notify();
            }
            HostToolsWindowIntent::BeginPlainTextImeSelection { input, event } => {
                let Some(target) = workspace_ime_target_for_plain_host_tools_input(input) else {
                    // Secret-bearing tmux dialog input never crosses this
                    // plain-text frame boundary.
                    return;
                };
                self.ime_marked_text = None;
                self.new_connection_caret_visible = true;
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(target, &event, window, cx);
            }
            HostToolsWindowIntent::PrepareTmuxInputDialog => {
                // Only focus state crosses this boundary; the zeroizing input stays in the Entity.
                self.ime_marked_text = None;
                self.clear_ime_selection();
                self.new_connection_caret_visible = true;
                window.focus(&self.focus_handle, cx);
                cx.notify();
            }
            HostToolsWindowIntent::PrepareTmuxConfirm => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            HostToolsWindowIntent::SetMonitoringEnabled { tool, enabled } => {
                self.set_host_tool_monitoring_enabled(tool, enabled, cx);
            }
        }
    }

    fn push_host_tools_window_notice(&mut self, title: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(in crate::workspace) fn push_host_tools_notice(&mut self, notice: HostToolsNotice) {
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
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }
}
