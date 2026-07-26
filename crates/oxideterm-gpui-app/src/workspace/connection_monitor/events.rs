use super::*;
use gpui::EventEmitter;

/// Typed requests that cross from HostToolsEntity into workspace runtime services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsEvent {
    RefreshGpu { connection_id: String },
    RefreshProfiler { connection_id: String },
    ShowNotice(HostToolsNotice),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsNotice {
    LogSnapshotAlreadyRunning,
    LogConnectionMissing,
    LogPartialSupport { os_type: String },
    LogSnapshotLoaded { count: usize },
    LogUnavailable,
    LogSnapshotFailed,
    PortSnapshotAlreadyRunning,
    PortConnectionMissing,
    PortPartialSupport { os_type: String },
    PortSnapshotLoaded { count: usize },
    PortUnavailable,
    PortSnapshotFailed,
    FilesystemSnapshotAlreadyRunning,
    FilesystemConnectionMissing,
    FilesystemPartialSupport { os_type: String },
    FilesystemSnapshotLoaded { count: usize },
    FilesystemUnavailable,
    FilesystemSnapshotFailed,
}

impl EventEmitter<HostToolsEvent> for HostToolsEntity {}
