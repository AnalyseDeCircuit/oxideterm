use super::*;
use gpui::EventEmitter;

/// Typed requests that cross from HostToolsEntity into workspace runtime services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsEvent {
    RefreshGpu { connection_id: String },
    RefreshProfiler { connection_id: String },
    RefreshSchedules { connection_id: String },
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
