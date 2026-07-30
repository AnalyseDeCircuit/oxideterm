use serde::{Deserialize, Serialize};

use super::{
    RuntimeCapability, RuntimeHandleId, RuntimeOwnerKind, RuntimeRegistryEpoch, StableResourceRef,
};

/// The model-facing, non-authoritative description of one current capability lease.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeHandleProjection {
    pub handle_id: RuntimeHandleId,
    pub kind: RuntimeOwnerKind,
    pub label: String,
    pub capabilities: Vec<RuntimeCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_ref: Option<StableResourceRef>,
}

/// A fresh projection for one provider round. It carries no backend owners or services.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeContextSnapshot {
    pub protocol_version: u8,
    pub snapshot_id: String,
    pub observed_at_ms: i64,
    pub registry_epoch: RuntimeRegistryEpoch,
    pub stable_resources: Vec<StableResourceRef>,
    pub live_handles: Vec<RuntimeHandleProjection>,
}
