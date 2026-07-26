use super::*;
use gpui::EventEmitter;
use std::sync::{Arc, Mutex};

/// Typed requests that cross from HostToolsEntity into workspace runtime services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsEvent {
    ShowNotice(HostToolsNotice),
    ToolSelected(ContextSidebarTool),
}

/// Moves a Host Tools request across GPUI's cloneable action boundary exactly once.
#[derive(gpui::Action)]
#[action(no_json, no_register)]
pub(in crate::workspace) struct HostToolsWindowRequest {
    intent: Arc<Mutex<Option<HostToolsWindowIntent>>>,
}

impl HostToolsWindowRequest {
    pub(in crate::workspace) fn new(intent: HostToolsWindowIntent) -> Self {
        Self {
            intent: Arc::new(Mutex::new(Some(intent))),
        }
    }

    pub(in crate::workspace) fn take(&self) -> Option<HostToolsWindowIntent> {
        self.intent.lock().ok()?.take()
    }
}

impl Clone for HostToolsWindowRequest {
    fn clone(&self) -> Self {
        Self {
            intent: Arc::clone(&self.intent),
        }
    }
}

impl PartialEq for HostToolsWindowRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.intent, &other.intent)
    }
}

/// Carries terminal data without exposing it through cloning or debug formatting.
pub(in crate::workspace) enum HostToolsWindowIntent {
    OpenExistingNodeTerminal {
        connection_id: String,
        command: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
    },
    BeginPlainTextImeSelection {
        input: HostToolsTextInput,
        event: MouseDownEvent,
    },
    /// Gives the workspace focus to a secret-capable tmux input without moving its value.
    PrepareTmuxInputDialog,
    /// Resets the shared confirmation focus before showing an Entity-owned tmux action.
    PrepareTmuxConfirm,
    /// Persists a Host Tools monitoring toggle while the Entity owns its UI.
    SetMonitoringEnabled {
        tool: ContextSidebarTool,
        enabled: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ScheduleActionNoticeKind {
    RunNow,
    Enable,
    Disable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsNotice {
    ProcessActionAlreadyRunning,
    ProcessInvalidNice,
    ProcessConnectionMissing,
    ProcessPartialSupport {
        os_type: String,
    },
    ProcessActionFailed,
    ProcessActionFinished {
        pid: String,
        succeeded: bool,
    },
    DockerActionAlreadyRunning,
    DockerLogsAlreadyRunning,
    DockerConnectionMissing,
    DockerActionFailed,
    DockerLogsFailed,
    DockerActionFinished {
        container_name: String,
        succeeded: bool,
    },
    ServiceActionAlreadyRunning,
    ServiceLogsAlreadyRunning,
    ServiceConnectionMissing,
    ServicePartialSupport {
        os_type: String,
    },
    ServiceActionFailed,
    ServiceLogsFailed,
    ServiceActionFinished {
        description: String,
        succeeded: bool,
    },
    TmuxSnapshotAlreadyRunning,
    TmuxConnectionMissing,
    TmuxSnapshotLoaded {
        count: usize,
    },
    TmuxUnavailable,
    TmuxSnapshotFailed,
    TmuxActionAlreadyRunning,
    TmuxInputRequired,
    TmuxActionFailed,
    TmuxActionFinished {
        target_label: String,
        succeeded: bool,
    },
    LogSnapshotAlreadyRunning,
    LogConnectionMissing,
    LogPartialSupport {
        os_type: String,
    },
    LogSnapshotLoaded {
        count: usize,
    },
    LogUnavailable,
    LogSnapshotFailed,
    PortSnapshotAlreadyRunning,
    PortConnectionMissing,
    PortPartialSupport {
        os_type: String,
    },
    PortSnapshotLoaded {
        count: usize,
    },
    PortUnavailable,
    PortSnapshotFailed,
    /// Reports the endpoint that the user explicitly copied from the port table.
    PortEndpointCopied {
        endpoint: String,
    },
    FilesystemSnapshotAlreadyRunning,
    FilesystemConnectionMissing,
    FilesystemPartialSupport {
        os_type: String,
    },
    FilesystemSnapshotLoaded {
        count: usize,
    },
    FilesystemUnavailable,
    FilesystemSnapshotFailed,
    /// Reports the path that the user explicitly copied from the filesystem table.
    FilesystemPathCopied {
        path: String,
    },
    PackageSnapshotAlreadyRunning,
    PackageConnectionMissing,
    PackageSnapshotLoaded {
        count: usize,
    },
    PackageUnavailable,
    PackageSnapshotFailed,
    /// Reports the package manager whose inspect command is not supported.
    PackageInspectUnsupported {
        manager: String,
    },
    /// Reports the package name that the user explicitly copied from the package table.
    PackageNameCopied {
        package_name: String,
    },
    ScheduleSnapshotAlreadyRunning,
    ScheduleConnectionMissing,
    SchedulePartialSupport {
        os_type: String,
    },
    ScheduleSnapshotLoaded {
        count: usize,
    },
    ScheduleUnavailable,
    ScheduleSnapshotFailed,
    ScheduleLogsAlreadyRunning,
    ScheduleLogsFailed,
    ScheduleActionAlreadyRunning,
    ScheduleActionFailed,
    ScheduleActionFinished {
        kind: ScheduleActionNoticeKind,
        task_name: String,
        succeeded: bool,
    },
}

impl EventEmitter<HostToolsEvent> for HostToolsEntity {}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_request(command: &str) -> HostToolsWindowRequest {
        HostToolsWindowRequest::new(HostToolsWindowIntent::OpenExistingNodeTerminal {
            connection_id: "connection-1".to_string(),
            command: command.to_string(),
            title: "Service logs".to_string(),
            opened_notice: "Opened".to_string(),
            missing_notice: "Missing".to_string(),
        })
    }

    #[test]
    fn window_request_clones_share_one_consumable_intent() {
        let request = terminal_request("journalctl --follow");
        let cloned = request.clone();

        assert!(request == cloned);
        match cloned.take() {
            Some(HostToolsWindowIntent::OpenExistingNodeTerminal { command, .. }) => {
                assert_eq!(command, "journalctl --follow");
            }
            Some(_) => panic!("cloned request retained the wrong intent kind"),
            None => panic!("cloned request should retain the shared intent"),
        }
        assert!(request.take().is_none());
    }

    #[test]
    fn separate_window_requests_are_not_equal() {
        let first = terminal_request("journalctl --follow");
        let second = terminal_request("journalctl --follow");

        assert!(first != second);
    }
}
