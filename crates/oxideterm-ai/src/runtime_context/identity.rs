use std::{
    fmt,
    fmt::Write as _,
    hash::{Hash, Hasher},
};

use rand::{RngCore as _, rngs::OsRng};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use uuid::Uuid;
use zeroize::Zeroize;

use super::RuntimeContextError;

const RUNTIME_HANDLE_PREFIX: &str = "rt_";
const TOOL_SESSION_PREFIX: &str = "tool_";
const OWNER_KEY_PREFIX: &str = "owner_";
const REGISTRY_EPOCH_PREFIX: &str = "epoch_";
const MAX_IDENTIFIER_LENGTH: usize = 160;
const MAX_LABEL_LENGTH: usize = 256;

/// Identifies the in-memory registry instance. It is not accepted as tool authority.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct RuntimeRegistryEpoch(String);

impl RuntimeRegistryEpoch {
    pub fn new() -> Self {
        Self(random_control_identifier(REGISTRY_EPOCH_PREFIX))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn parse(value: String) -> Result<Self, RuntimeContextError> {
        valid_uuid_identifier(&value, REGISTRY_EPOCH_PREFIX)
            .then_some(Self(value))
            .ok_or(RuntimeContextError::InvalidIdentifier)
    }
}

impl Default for RuntimeRegistryEpoch {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RuntimeRegistryEpoch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeRegistryEpoch(..)")
    }
}

impl Serialize for RuntimeRegistryEpoch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuntimeRegistryEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(|_| D::Error::custom("invalid runtime registry epoch"))
    }
}

/// An opaque token that authorizes one operation family during one active AI tool session.
#[derive(Clone, Eq, PartialEq)]
pub struct RuntimeHandleId(String);

impl RuntimeHandleId {
    pub fn new() -> Self {
        Self(random_control_identifier(RUNTIME_HANDLE_PREFIX))
    }

    pub fn parse(mut value: String) -> Result<Self, RuntimeContextError> {
        if valid_uuid_identifier(&value, RUNTIME_HANDLE_PREFIX) {
            Ok(Self(value))
        } else {
            value.zeroize();
            Err(RuntimeContextError::InvalidIdentifier)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RuntimeHandleId {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RuntimeHandleId {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Hash for RuntimeHandleId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for RuntimeHandleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeHandleId([redacted])")
    }
}

impl Serialize for RuntimeHandleId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuntimeHandleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(|_| D::Error::custom("invalid runtime handle id"))
    }
}

/// An internal capability owner key. It is never exposed to the model or persisted history.
#[derive(Clone, Eq, PartialEq)]
pub struct RuntimeOwnerKey(String);

impl RuntimeOwnerKey {
    pub fn new() -> Self {
        Self(random_control_identifier(OWNER_KEY_PREFIX))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RuntimeOwnerKey {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RuntimeOwnerKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Hash for RuntimeOwnerKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for RuntimeOwnerKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeOwnerKey([redacted])")
    }
}

/// An internal stream-scoped identity that prevents cross-turn handle replay.
#[derive(Clone, Eq, PartialEq)]
pub struct ToolSessionId(String);

impl ToolSessionId {
    pub fn new() -> Self {
        Self(random_control_identifier(TOOL_SESSION_PREFIX))
    }
}

impl Default for ToolSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ToolSessionId {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Hash for ToolSessionId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for ToolSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ToolSessionId([redacted])")
    }
}

/// The monotonically increasing generation of one runtime owner's physical identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RuntimeOwnerGeneration(u64);

impl RuntimeOwnerGeneration {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// The type of live object that owns a runtime capability.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnerKind {
    LocalShell,
    Terminal,
    SshNode,
    SftpSession,
    IdeSurface,
    AppSurface,
}

/// A durable resource type that may safely appear in conversation history.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StableResourceKind {
    SavedConnection,
    LocalShellProfile,
    SettingsScope,
    RagIndex,
    AppSurface,
}

/// A durable resource reference. Equality intentionally ignores its presentation label.
#[derive(Clone)]
pub struct StableResourceRef {
    kind: StableResourceKind,
    id: String,
    label: Option<String>,
}

impl StableResourceRef {
    pub fn new(
        kind: StableResourceKind,
        id: String,
        label: Option<String>,
    ) -> Result<Self, RuntimeContextError> {
        if !stable_identifier_is_valid(kind, &id) || !label_is_valid(label.as_deref()) {
            return Err(RuntimeContextError::InvalidStableResourceReference);
        }
        Ok(Self { kind, id, label })
    }

    pub const fn kind(&self) -> StableResourceKind {
        self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

impl fmt::Debug for StableResourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StableResourceRef")
            .field("kind", &self.kind)
            .field("id", &self.id)
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for StableResourceRef {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.id == other.id
    }
}

impl Eq for StableResourceRef {}

impl Hash for StableResourceRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.id.hash(state);
    }
}

impl Serialize for StableResourceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct(
            "StableResourceRef",
            if self.label.is_some() { 3 } else { 2 },
        )?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("id", &self.id)?;
        if let Some(label) = self.label.as_deref() {
            state.serialize_field("label", label)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for StableResourceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireResourceRef {
            kind: StableResourceKind,
            id: String,
            #[serde(default)]
            label: Option<String>,
        }

        let wire = WireResourceRef::deserialize(deserializer)?;
        Self::new(wire.kind, wire.id, wire.label)
            .map_err(|_| D::Error::custom("invalid stable resource reference"))
    }
}

fn valid_uuid_identifier(value: &str, prefix: &str) -> bool {
    value.len() == prefix.len() + 32
        && value.starts_with(prefix)
        && Uuid::parse_str(&value[prefix.len()..]).is_ok()
}

fn random_control_identifier(prefix: &str) -> String {
    // Use all 128 random bits. UUID v4 text would reserve version and variant
    // bits, which is below the capability-token entropy required by this boundary.
    let mut random = [0_u8; 16];
    OsRng.fill_bytes(&mut random);
    let mut identifier = String::with_capacity(prefix.len() + 32);
    identifier.push_str(prefix);
    for byte in random {
        write!(&mut identifier, "{byte:02x}").expect("writing to String cannot fail");
    }
    identifier
}

fn stable_identifier_is_valid(kind: StableResourceKind, id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_IDENTIFIER_LENGTH || id.chars().any(char::is_control) {
        return false;
    }

    match kind {
        StableResourceKind::SavedConnection => Uuid::parse_str(id).is_ok(),
        StableResourceKind::SettingsScope => id == "app",
        StableResourceKind::LocalShellProfile | StableResourceKind::RagIndex => {
            id == "default" || safe_slug(id)
        }
        StableResourceKind::AppSurface => matches!(
            id,
            "settings"
                | "connection_manager"
                | "connection_pool"
                | "connection_monitor"
                | "sftp"
                | "ide"
                | "file_manager"
                | "local_terminal"
                | "terminal"
        ),
    }
}

fn safe_slug(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn label_is_valid(label: Option<&str>) -> bool {
    label.is_none_or(|value| {
        value.chars().count() <= MAX_LABEL_LENGTH && !value.chars().any(char::is_control)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        RuntimeHandleId, RuntimeOwnerKey, StableResourceKind, StableResourceRef, ToolSessionId,
    };

    #[test]
    fn runtime_handle_debug_redacts_control_token() {
        let handle = RuntimeHandleId::new();
        let raw = handle.as_str().to_string();

        assert!(!format!("{handle:?}").contains(&raw));
        assert_eq!(
            serde_json::to_string(&handle).expect("handle serializes"),
            format!("\"{raw}\"")
        );
    }

    #[test]
    fn malformed_runtime_handle_is_rejected() {
        assert!(RuntimeHandleId::parse("rt_not-a-uuid".to_string()).is_err());
        assert!(RuntimeHandleId::parse("rt_".to_string()).is_err());
    }

    #[test]
    fn oversized_runtime_handle_is_rejected_before_lookup() {
        assert!(RuntimeHandleId::parse(format!("rt_{}", "a".repeat(512))).is_err());
    }

    #[test]
    fn internal_owner_and_tool_session_debug_are_redacted() {
        let owner = RuntimeOwnerKey::new();
        let raw_owner = owner.as_str().to_string();
        let tool_session = ToolSessionId::new();

        assert!(!format!("{owner:?}").contains(&raw_owner));
        assert!(!format!("{tool_session:?}").contains("tool_"));
    }

    #[test]
    fn stable_resource_identity_ignores_label() {
        let id = "4e22e673-067e-46e2-8b9f-902d7b21af4c".to_string();
        let first = StableResourceRef::new(
            StableResourceKind::SavedConnection,
            id.clone(),
            Some("Production".to_string()),
        )
        .expect("valid stable reference");
        let second = StableResourceRef::new(
            StableResourceKind::SavedConnection,
            id,
            Some("Renamed production".to_string()),
        )
        .expect("valid stable reference");

        let mut references = HashSet::new();
        references.insert(first.clone());

        assert_eq!(first, second);
        assert!(references.contains(&second));
    }

    #[test]
    fn stable_resource_wire_format_rejects_unknown_fields() {
        let decoded = serde_json::from_str::<StableResourceRef>(
            r#"{\"kind\":\"settings_scope\",\"id\":\"app\",\"unexpected\":true}"#,
        );

        assert!(decoded.is_err());
    }
}
