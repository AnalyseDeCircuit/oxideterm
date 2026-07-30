use thiserror::Error;

/// A reason a previously issued runtime handle is no longer usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeRevocationReason {
    ToolSessionFinished,
    ToolSessionCancelled,
    OwnerClosed,
    OwnerReplaced,
    ApplicationShutdown,
}

/// Internal validation detail. Public callers receive a deliberately coarser code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeValidationFailure {
    MissingHandle,
    UnknownHandle,
    ToolSessionInactive,
    WrongToolSession,
    OwnerClosed,
    OwnerReplaced,
    CapabilityUnavailable,
}

impl RuntimeValidationFailure {
    /// Avoid using handle failures as an oracle for another active conversation.
    pub const fn public_code(self) -> &'static str {
        match self {
            Self::MissingHandle => "runtime_handle_missing",
            Self::UnknownHandle | Self::ToolSessionInactive | Self::WrongToolSession => {
                "runtime_handle_expired"
            }
            Self::OwnerClosed => "runtime_owner_closed",
            Self::OwnerReplaced => "runtime_owner_replaced",
            Self::CapabilityUnavailable => "runtime_capability_unavailable",
        }
    }
}

/// A safe validation error that intentionally contains no handle or owner identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeValidationError {
    failure: RuntimeValidationFailure,
}

impl RuntimeValidationError {
    pub const fn new(failure: RuntimeValidationFailure) -> Self {
        Self { failure }
    }

    pub const fn failure(self) -> RuntimeValidationFailure {
        self.failure
    }

    pub const fn public_code(self) -> &'static str {
        self.failure.public_code()
    }
}

/// Errors raised while registering owners or issuing tool-session handles.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RuntimeContextError {
    #[error("invalid runtime context identifier")]
    InvalidIdentifier,
    #[error("invalid stable resource reference")]
    InvalidStableResourceReference,
    #[error("invalid runtime owner registration")]
    InvalidOwnerRegistration,
    #[error("runtime owner is not registered")]
    OwnerNotFound,
    #[error("runtime owner generation moved backwards")]
    OwnerGenerationRegression,
    #[error("runtime owner identity changed without a new generation")]
    OwnerIdentityChangedWithoutGeneration,
    #[error("AI tool session is not active")]
    ToolSessionInactive,
    #[error("runtime handle allocation limit reached")]
    HandleAllocationLimitReached,
}
