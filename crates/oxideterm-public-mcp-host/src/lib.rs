//! Platform-independent Public MCP host. The protocol crate remains transport and policy only.

mod addons;
mod desktops;
mod files;
pub use desktops::{PublicMcpClipboardSnapshot, PublicMcpDesktopFrame, PublicMcpDesktopOpen};
mod history;
mod host_tools;
mod model;
mod recordings;
pub mod sync_protocol;
mod terminals;
mod transfers;
pub use terminals::{
    PublicMcpSavedTerminal, PublicMcpTerminalSerialState, PublicMcpTerminalSession,
    finish_terminal_read, telnet_control, terminal_projection,
};
mod workspaces;
pub use addons::{PublicAddonProjection, PublicMcpAddonPackage, public_addon_projection};
pub use recordings::{RecordingPaneAction, RecordingPaneSnapshot};
mod connections;
mod quick_commands;
pub use connections::{connection_revision, public_store_error};
mod background;
mod directory;
mod endpoint;
mod forwards;
mod policy;
mod ports;
pub use policy::{PublicMcpClientChange, PublicMcpClientEffect};
mod sync_handles;

pub use background::{PublicMcpBackground, PublicMcpWindowDelivery, PublicMcpWindowRequest};
pub use model::{
    CONNECTION_KEY_DESKTOP_PREFIX, CONNECTION_KEY_MOSH_PREFIX, CONNECTION_KEY_SERIAL_PREFIX,
    CONNECTION_KEY_SSH_PREFIX, CONNECTION_KEY_TELNET_PREFIX, CONNECTION_TYPE_MOSH,
    CONNECTION_TYPE_SERIAL, CONNECTION_TYPE_TELNET, PublicMcpDesktopRecord, PublicMcpHost,
    PublicMcpNodeLease, PublicMcpPendingTerminalOpen, PublicMcpTerminalRecord,
};
pub use ports::{PublicMcpForwardMutation, PublicMcpForwardPort, PublicMcpHostEffect};
use sync_handles::{
    PublicMcpLocalSyncCheckpoint, PublicMcpSyncPlan, PublicMcpSyncPlanKind, PublicMcpSyncUndo,
};

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use base64::Engine;
use oxideterm_connections::{ConnectionInfo, ConnectionStore};
use oxideterm_plugin_registry as plugin_host;
use oxideterm_public_mcp::{
    AddonRef, ApprovalRef, ApprovalStatus, ArtifactRef, AuditQuery, ClientApprovalMode,
    ClientCredential, ClientProjection, ClientRef, ClientRegistry, CommandRef, ConnectionRef,
    DesktopRef, DomainBroker, DomainRequest, DomainRequestReceiver, FileSessionRef, ForwardRef,
    NodeRef, OperationRef, PublicMcpHttpServer, PublicMcpState, PublicToolCall, QuickCommandRef,
    RecordingRef, TerminalRef, ToolEnvelope, ToolGroup, ToolOutcome, TransferRef, WorkspaceRef,
    start_http_server,
};
use oxideterm_ssh::{ConnectionConsumer, NodeId, NodeRouter, SshTransportError};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use background::*;
use model::*;
use oxideterm_workspace::{TabId, TerminalSessionId};

pub(crate) fn all_tool_groups() -> BTreeSet<ToolGroup> {
    let mut tool_groups = ToolGroup::selectable()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    tool_groups.insert(ToolGroup::Basic);
    tool_groups
}

pub fn finish_serialized(request: DomainRequest, value: serde_json::Value) {
    request.finish(ToolEnvelope {
        outcome: ToolOutcome::Completed,
        data: Some(value),
        error: None,
    });
}

#[cfg(test)]
mod tests;
