use super::*;
use gpui::EventEmitter;

/// Typed requests that cross from HostToolsEntity into workspace runtime services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsEvent {
    RefreshServices { connection_id: String },
    RefreshSchedules { connection_id: String },
    RefreshTmux { connection_id: String },
    ShowNotice(HostToolsNotice),
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
    PackageSnapshotAlreadyRunning,
    PackageConnectionMissing,
    PackageSnapshotLoaded {
        count: usize,
    },
    PackageUnavailable,
    PackageSnapshotFailed,
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
