//! Public MCP protocol boundary for OxideTerm.
//!
//! This crate owns external handles, client grants, approvals, auditing, and the
//! typed bridge into the GPUI domain runtime. It intentionally has no access to
//! internal GPUI entity identifiers or arbitrary plugin functions.

pub mod approval;
pub mod artifact;
pub mod audit;
pub mod auth;
pub mod broker;
pub mod calls;
pub mod handles;
pub mod runtime;
pub mod service;

pub use approval::{
    ApprovalError, ApprovalProjection, ApprovalReview, ApprovalStatus, ApprovalStore,
};
pub use artifact::{
    ArtifactContent, ArtifactError, ArtifactPage, ArtifactProjection, ArtifactStore,
};
pub use audit::{
    AuditAuthorization, AuditPage, AuditProjection, AuditQuery, AuditRecord, AuditStore,
};
pub use auth::{
    ClientApprovalMode, ClientCredential, ClientProjection, ClientRegistry, ClientRegistryError,
    RegisteredClient, ToolGroup,
};
pub use broker::{BrokerError, DomainBroker, DomainMessage, DomainRequest, DomainRequestReceiver};
pub use calls::{
    CredentialStatusArgs, DesktopButtonState, DesktopClipboardImageFormat, DesktopClipboardKind,
    DesktopClipboardPayload, DesktopFrameArgs, DesktopHandleArgs, DesktopInputArgs,
    DesktopInputEvent, ForgetCredentialArgs, ForwardKind, HostToolLogPreset, HostToolOperation,
    HostToolResource, OpenDesktopArgs, PublicConnectionAuth, PublicCredentialSlot,
    PublicDesktopMouseButton, PublicMoshIpFamily, PublicMoshPredictionMode,
    PublicMoshUdpPortSelection, PublicRdpNetworkProfile, PublicRemoteDesktopOptions,
    PublicRemoteDesktopProfile, PublicSavedConnectionProfile, PublicSerialFlowControl,
    PublicSerialParity, PublicSyncConflictStrategy, PublicSyncSection, PublicTelnetControl,
    PublicTerminalBackspaceSequence, PublicTerminalDeleteSequence, PublicTerminalEncoding,
    PublicTerminalOptions, PublicTerminalSessionLogPolicy, PublicToolCall, PublicUpstreamProxy,
    PublicUpstreamProxyProtocol, PublicVncCompression, PublicVncImageQuality,
    PublicVncSecurityPolicy, PublicVncSessionMode, PublicX11ForwardingMode,
    ReadDesktopClipboardArgs, RecordingExportFormat, RecordingStatusTarget, RecordingsControlArgs,
    RecordingsExportArgs, RecordingsSearchArgs, RecordingsStatusArgs, RemovePublicConnectionArgs,
    ResizeDesktopArgs, SavePublicConnectionArgs, StartTransferArgs, StoreCredentialArgs,
    SyncApplyPlanArgs, SyncPublishPreviewArgs, SyncPullPreviewArgs, SyncRestoreArgs, SyncSelection,
    SyncStatusArgs, TerminalControlAction, TerminalOpenSource, ToolEnvelope, ToolOutcome,
    TransferHandleArgs, WriteDesktopClipboardArgs,
};
pub use handles::{
    AddonRef, ApprovalRef, ArtifactRef, AuditRef, ClientRef, CommandRef, ConnectionRef, DesktopRef,
    FileSessionRef, ForwardRef, HandleParseError, NodeRef, OperationRef, QuickCommandRef,
    RecordingRef, SyncPlanRef, TerminalRef, TransferRef, UndoRef, WorkspaceRef,
};
pub use runtime::{PublicMcpHttpServer, start_http_server};
pub use service::{PublicMcpService, PublicMcpState};
