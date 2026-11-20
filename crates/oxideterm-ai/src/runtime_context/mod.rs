mod authority;
mod capability;
mod error;
mod identity;
mod projection;
mod registry;

pub const RUNTIME_CONTEXT_MESSAGE_ID: &str = "runtime-context-v2";

/// Identifies ephemeral per-turn/per-round context messages stored with System role
/// internally so they never persist as user turns, while staying out of the static
/// provider system prefix so KV/prefix caching remains intact.
pub(crate) fn is_user_context_system_message(message: &crate::AiChatMessage) -> bool {
    message.role == crate::AiChatRole::System
        && matches!(
            message.id.as_str(),
            RUNTIME_CONTEXT_MESSAGE_ID | "current-terminal-context" | "task-mode"
        )
}

pub use authority::{AiRunCommandOwner, AiRuntimeAuthority, AiSftpRuntimeOwner};
pub use capability::RuntimeCapability;
pub use error::{
    RuntimeContextError, RuntimeRevocationReason, RuntimeValidationError, RuntimeValidationFailure,
};
pub use identity::{
    RuntimeHandleId, RuntimeOwnerGeneration, RuntimeOwnerKey, RuntimeOwnerKind,
    RuntimeRegistryEpoch, StableResourceKind, StableResourceRef, ToolSessionId,
};
pub use projection::{RuntimeContextSnapshot, RuntimeHandleProjection};
pub use registry::{RuntimeCapabilityRegistry, RuntimeOwnerRegistration, ValidatedRuntimeHandle};
