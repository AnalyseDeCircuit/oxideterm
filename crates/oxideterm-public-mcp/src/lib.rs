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
pub use artifact::{ArtifactError, ArtifactPage, ArtifactProjection, ArtifactStore};
pub use audit::{
    AuditAuthorization, AuditPage, AuditProjection, AuditQuery, AuditRecord, AuditStore,
};
pub use auth::{
    ClientApprovalMode, ClientCredential, ClientProjection, ClientRegistry, ClientRegistryError,
    RegisteredClient, ToolGroup,
};
pub use broker::{BrokerError, DomainBroker, DomainMessage, DomainRequest, DomainRequestReceiver};
pub use calls::{
    HostToolLogPreset, HostToolOperation, HostToolResource, PublicToolCall, ToolEnvelope,
    ToolOutcome,
};
pub use handles::{
    ApprovalRef, ArtifactRef, AuditRef, ClientRef, CommandRef, ConnectionRef, HandleParseError,
    NodeRef, OperationRef, TerminalRef,
};
pub use runtime::{PublicMcpHttpServer, start_http_server};
pub use service::{PublicMcpService, PublicMcpState};
