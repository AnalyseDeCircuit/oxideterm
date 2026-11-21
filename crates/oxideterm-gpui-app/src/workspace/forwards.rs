use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{AnyElement, App, Context, KeyDownEvent, Window};
use oxideterm_forwarding::{
    ForwardEvent, ForwardRule, ForwardStats, ForwardStatus, ForwardUpdate, ForwardingManager,
    ForwardingRegistry, PortDetectionSnapshot,
};
use oxideterm_gpui_forwards::{
    FORWARDS_PORT_SCAN_INTERVAL, ForwardingQuickAction, ForwardingRuntimeSnapshot,
    ForwardsPageState, ForwardsSampler, ForwardsSamplingDue, ForwardsViewBindings, ForwardsViews,
    PortDetectionViewState,
};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_ssh::{ConnectionState, NodeId, NodeReadiness, NodeRouter, PhaseResult};
use oxideterm_workspace::{Tab, TabId, TabKind, TabTitleSource};

use super::{ActiveSurface, WorkspaceApp};

mod actions;
mod ai;
mod entity;
mod mount;
mod pages;
mod reconnect;
mod runtime_service;

pub(in crate::workspace) use entity::{
    ForwardingDeliveryIntent, ForwardingPageScope, ForwardingWorkspaceEntity,
    ForwardingWorkspaceEvent,
};
pub(in crate::workspace) use oxideterm_gpui_forwards::{ForwardInput, ForwardingRuntimeOperation};
pub(in crate::workspace) use reconnect::{
    cleanup_reconnect_created_forwards, forward_restore_failure_label,
    forward_restore_key_for_rule, forward_restore_key_for_snapshot_rule,
    forward_restore_phase_result, forward_restore_result_detail,
    forward_rule_from_reconnect_snapshot, reconnect_forward_rule_from_rule,
};
pub(in crate::workspace) use runtime_service::{
    ForwardRestoreReader, ForwardStopRequest, ForwardingRuntimeService,
    ReconnectForwardRestoreRequest,
};

pub(super) const FORWARDS_NODE_SESSION_PREFIX: &str = "node:";

pub(super) enum ForwardingWorkerResult {
    Operation {
        tab_id: TabId,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        result: Result<(), String>,
    },
    PortScan {
        node_id: NodeId,
        connection_id: Option<String>,
        result: Result<PortDetectionSnapshot, String>,
    },
    ReconnectRestore {
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
        job_id: String,
        created_forwards: Vec<(Arc<ForwardingManager>, String)>,
    },
}
