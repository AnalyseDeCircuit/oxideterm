mod capability;
mod error;
mod identity;
mod projection;
mod registry;

pub const RUNTIME_CONTEXT_MESSAGE_ID: &str = "runtime-context-v2";

pub(crate) fn is_runtime_context_message(message: &crate::AiChatMessage) -> bool {
    message.role == crate::AiChatRole::System && message.id == RUNTIME_CONTEXT_MESSAGE_ID
}

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
