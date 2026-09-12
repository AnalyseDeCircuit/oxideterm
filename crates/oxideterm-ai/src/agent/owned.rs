use super::{AgentRunRef, AgentRuntime, AgentText};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnedResourceKind {
    LocalProcess,
    TerminalCommand,
    Observation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnedResourceState {
    Running,
    Completed,
    Stopped,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OwnedResource {
    pub id: String,
    pub kind: OwnedResourceKind,
    pub label: AgentText,
    pub outcome: Option<AgentText>,
    #[serde(deserialize_with = "restore_resource_state")]
    pub state: OwnedResourceState,
}

fn restore_resource_state<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<OwnedResourceState, D::Error> {
    Ok(match OwnedResourceState::deserialize(deserializer)? {
        OwnedResourceState::Running => OwnedResourceState::OutcomeUnknown,
        state => state,
    })
}

struct ResourceRecord {
    runtime: AgentRuntime,
    run: AgentRunRef,
    id: String,
    kind: OwnedResourceKind,
}

impl Drop for ResourceRecord {
    fn drop(&mut self) {
        let state = if self.kind == OwnedResourceKind::Observation {
            OwnedResourceState::Stopped
        } else {
            OwnedResourceState::OutcomeUnknown
        };
        self.runtime
            .finish_resource(&self.run, &self.id, state, true);
    }
}

#[derive(Clone)]
pub struct AgentResourceRecord(Arc<ResourceRecord>);

impl AgentResourceRecord {
    pub(super) fn new(
        runtime: AgentRuntime,
        run: AgentRunRef,
        id: String,
        kind: OwnedResourceKind,
    ) -> Self {
        Self(Arc::new(ResourceRecord {
            runtime,
            run,
            id,
            kind,
        }))
    }
    pub fn outcome(&self, text: &str) {
        self.0
            .runtime
            .record_resource_outcome(&self.0.run, &self.0.id, AgentText::new(text));
    }
    pub fn finish(&self, state: OwnedResourceState) {
        self.0
            .runtime
            .finish_resource(&self.0.run, &self.0.id, state, false);
    }
}
