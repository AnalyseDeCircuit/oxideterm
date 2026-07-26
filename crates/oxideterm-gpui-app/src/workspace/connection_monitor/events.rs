use super::*;
use gpui::EventEmitter;

/// Typed requests that cross from HostToolsEntity into workspace runtime services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostToolsEvent {
    RefreshGpu { connection_id: String },
}

impl EventEmitter<HostToolsEvent> for HostToolsEntity {}
