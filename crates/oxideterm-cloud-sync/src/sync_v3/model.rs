// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, fmt};

use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Connection,
    LocalTerminal,
    Forward,
    QuickCommand,
    QuickCommandCategory,
    Serial,
    Telnet,
    Mosh,
    StandaloneSftp,
    Ftp,
    RemoteDesktop,
    Totp,
    AppSettings,
    PluginSettings,
    Credential,
    ManagedKey,
    AiCredential,
    PluginCredential,
    PrivilegeCredential,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SyncResource {
    pub kind: ResourceKind,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SyncField {
    pub resource: SyncResource,
    pub group: String,
}

impl SyncField {
    pub fn presence(resource: SyncResource) -> Self {
        Self {
            resource,
            group: "$present".into(),
        }
    }

    pub fn is_presence(&self) -> bool {
        self.group == "$present"
    }

    pub(super) fn key(&self) -> Result<String> {
        if self.resource.id.is_empty() || self.group.is_empty() {
            bail!("Invalid sync field identity");
        }
        Ok(serde_json::to_string(self)?)
    }

    pub(super) fn from_key(key: &str) -> Result<Self> {
        let field: Self = serde_json::from_str(key)
            .map_err(|_| anyhow::anyhow!("Invalid sync field identity"))?;
        if field.key()? != key {
            bail!("Noncanonical sync field identity");
        }
        Ok(field)
    }
}

/// The CRDT stores only content digests. Potentially sensitive field values are
/// owned here, encrypted on disk and zeroized when the replica is released.
#[derive(Clone, Eq, PartialEq)]
pub struct FieldValue(Zeroizing<Vec<u8>>);

impl FieldValue {
    pub fn encode<T: Serialize>(value: &T) -> Result<Self> {
        let mut canonical =
            serde_json::to_value(value).map_err(|_| anyhow::anyhow!("Cannot encode sync field"))?;
        canonical.sort_all_objects();
        let bytes = serde_json::to_vec(&canonical);
        clear_json(&mut canonical);
        Ok(Self(Zeroizing::new(bytes.map_err(|_| {
            anyhow::anyhow!("Cannot encode sync field")
        })?)))
    }

    pub fn decode<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.0).map_err(|_| anyhow::anyhow!("Invalid sync field value"))
    }

    pub(super) fn digest(&self) -> String {
        content_digest(self.0.as_slice())
    }
}

pub(super) fn content_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn clear_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(value) => value.zeroize(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(clear_json),
        serde_json::Value::Object(values) => {
            for (mut key, mut value) in std::mem::take(values) {
                key.zeroize();
                clear_json(&mut value);
            }
        }
        _ => {}
    }
}

pub(crate) struct PrivateJson(pub serde_json::Value);

impl Drop for PrivateJson {
    fn drop(&mut self) {
        clear_json(&mut self.0);
    }
}

impl fmt::Debug for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FieldValue([redacted])")
    }
}

impl Serialize for FieldValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FieldValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<u8>::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

pub type SyncValues = BTreeMap<SyncField, FieldValue>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncCandidate {
    pub operation: String,
    pub value_digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub field: SyncField,
    pub candidates: Vec<SyncCandidate>,
}
