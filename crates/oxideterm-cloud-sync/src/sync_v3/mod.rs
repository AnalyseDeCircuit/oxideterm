// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Encrypted, causal configuration replicas. Domain stores retain ownership of
//! effective local values; receiving a conflict never silently selects a winner.

pub(crate) mod auxiliary_secrets;
mod mapping;
mod model;
mod publication;
mod recovery;
mod replica;
mod storage;

pub use mapping::ConfigurationView;
pub(crate) use model::PrivateJson;
pub use model::{
    FieldValue, ResourceKind, SyncCandidate, SyncConflict, SyncField, SyncResource, SyncValues,
};
pub use publication::{PublicationId, PublishedReplica};
pub use recovery::RecoveryJournal;
pub use replica::{ReplicaSnapshot, SyncReplica};
pub use storage::{LocalReplica, PendingPublication, ReplicaStore};

#[cfg(test)]
pub(crate) mod tests;
