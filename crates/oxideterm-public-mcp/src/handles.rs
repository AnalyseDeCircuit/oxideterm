use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandleParseError {
    #[error("handle must start with {0}")]
    InvalidPrefix(&'static str),
    #[error("handle contains an invalid identifier")]
    InvalidIdentifier,
}

macro_rules! opaque_handle {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(format!("{}{}", $prefix, Uuid::new_v4().simple()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = HandleParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let Some(identifier) = value.strip_prefix($prefix) else {
                    return Err(HandleParseError::InvalidPrefix($prefix));
                };
                Uuid::parse_str(identifier).map_err(|_| HandleParseError::InvalidIdentifier)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

opaque_handle!(ClientRef, "client_");
opaque_handle!(ConnectionRef, "connection_");
opaque_handle!(NodeRef, "node_");
opaque_handle!(TerminalRef, "terminal_");
opaque_handle!(DesktopRef, "desktop_");
opaque_handle!(CommandRef, "command_");
opaque_handle!(OperationRef, "operation_");
opaque_handle!(ApprovalRef, "approval_");
opaque_handle!(AuditRef, "audit_");
opaque_handle!(ArtifactRef, "artifact_");
opaque_handle!(QuickCommandRef, "quickcommand_");
opaque_handle!(AddonRef, "addon_");
opaque_handle!(ForwardRef, "forward_");
opaque_handle!(FileSessionRef, "files_");
