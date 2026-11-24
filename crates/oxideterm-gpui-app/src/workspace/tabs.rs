use super::*;

// Keep tab responsibilities in real modules while preserving WorkspaceApp's API.
mod affinity;
mod create;
mod detach;
mod entity;
mod helpers;
mod layout;
mod navigation;
mod nodes;
mod nodes_reconnect_helpers;
mod render;
mod scope;
pub(super) mod split_drop;
mod state;

pub(in crate::workspace) use affinity::{TabAffinity, TabScope};
pub(in crate::workspace) use helpers::effective_shortcut_label;

pub(in crate::workspace) use entity::{
    TabCloseConfirmKeyAction, TabCloseProcessProbe, TabDetachTransition, TabMountCleanupPlan,
    TabMountCloseReason, TabMountId, TabRemovalTransition, TerminalLocation,
    WorkspaceTabHostEntity, WorkspaceTabHostEvent,
};
