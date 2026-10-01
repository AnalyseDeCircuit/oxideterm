// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use automerge::{ActorId, AutoCommit, ROOT, ReadDoc, ScalarValue, transaction::Transactable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{FieldValue, SyncCandidate, SyncConflict, SyncField, SyncResource, SyncValues};

#[derive(Clone, Serialize, Deserialize)]
pub struct ReplicaSnapshot {
    schema: u32,
    document: Vec<u8>,
    values: BTreeMap<String, FieldValue>,
}

pub struct SyncReplica {
    document: AutoCommit,
    values: BTreeMap<String, FieldValue>,
}

impl ReplicaSnapshot {
    pub(super) fn document_digest(&self) -> String {
        super::model::content_digest(&self.document)
    }
}

impl SyncReplica {
    pub fn new(writer: Uuid) -> Self {
        let mut document = AutoCommit::new();
        document.set_actor(ActorId::from(writer.as_bytes().as_slice()));
        Self {
            document,
            values: BTreeMap::new(),
        }
    }

    pub fn load(snapshot: ReplicaSnapshot, writer: Uuid) -> Result<Self> {
        if snapshot.schema != 3 {
            bail!("Unsupported cloud sync replica schema");
        }
        let mut document = AutoCommit::load(&snapshot.document)
            .map_err(|_| anyhow::anyhow!("Invalid cloud sync causal history"))?;
        document.set_actor(ActorId::from(writer.as_bytes().as_slice()));
        let replica = Self {
            document,
            values: snapshot.values,
        };
        replica.validate()?;
        Ok(replica)
    }

    pub fn snapshot(&mut self) -> ReplicaSnapshot {
        ReplicaSnapshot {
            schema: 3,
            document: self.document.save(),
            values: self.values.clone(),
        }
    }

    pub fn merge(&mut self, remote: ReplicaSnapshot) -> Result<()> {
        let writer = self.document.get_actor().clone();
        let mut incoming = Self::load(remote, Uuid::nil())?;
        self.document
            .merge(&mut incoming.document)
            .map_err(|_| anyhow::anyhow!("Cannot merge cloud sync causal history"))?;
        self.document.set_actor(writer);
        self.values.extend(incoming.values);
        Ok(())
    }

    pub fn conflicts(&self) -> Result<Vec<SyncConflict>> {
        let mut conflicts = Vec::new();
        for key in self.document.keys(ROOT) {
            let field = SyncField::from_key(&key)?;
            if !field.is_presence() {
                let presence =
                    self.candidates(&SyncField::presence(field.resource.clone()).key()?)?;
                if distinct_values(&presence) > 1
                    || presence.first().is_some_and(|candidate| {
                        self.candidate_value(candidate)
                            .is_some_and(|value| value.decode::<bool>().ok() == Some(false))
                    })
                {
                    continue;
                }
            }
            let candidates = self.candidates(&key)?;
            if distinct_values(&candidates) > 1 {
                conflicts.push(SyncConflict { field, candidates });
            }
        }
        Ok(conflicts)
    }

    pub fn candidate_value(&self, candidate: &SyncCandidate) -> Option<&FieldValue> {
        candidate
            .value_digest
            .as_ref()
            .and_then(|digest| self.values.get(digest))
    }

    /// Compare against what actually reached the domain stores, not the CRDT's
    /// arbitrary displayed winner. Otherwise retained local conflicts become writes.
    pub fn capture_local(
        &mut self,
        previous: &SyncValues,
        current: &SyncValues,
        selected: &BTreeSet<SyncResource>,
    ) -> Result<bool> {
        let conflicts = self.conflicts()?;
        let blocked_fields = conflicts
            .iter()
            .map(|conflict| &conflict.field)
            .collect::<BTreeSet<_>>();
        let blocked_records = conflicts
            .iter()
            .filter(|conflict| conflict.field.is_presence())
            .map(|conflict| &conflict.field.resource)
            .collect::<BTreeSet<_>>();
        let keys = previous
            .keys()
            .chain(current.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut changed_records = BTreeSet::new();
        let mut deleted_records = BTreeSet::new();
        for resource in selected {
            let presence = SyncField::presence(resource.clone());
            let was_present = previous
                .get(&presence)
                .map(|value| value.decode::<bool>())
                .transpose()?
                .unwrap_or(false);
            let is_present = current
                .get(&presence)
                .map(|value| value.decode::<bool>())
                .transpose()?
                .unwrap_or(false);
            if !is_present && was_present && !blocked_records.contains(resource) {
                // Deleting the entity must not also delete all of its fields:
                // retaining those values lets a concurrent edit be recovered.
                self.write(&presence, Some(&FieldValue::encode(&false)?))?;
                deleted_records.insert(resource.clone());
                changed_records.insert(resource.clone());
            }
        }
        for field in keys {
            if !selected.contains(&field.resource)
                || blocked_records.contains(&field.resource)
                || deleted_records.contains(&field.resource)
                || blocked_fields.contains(&field)
                || previous.get(&field) == current.get(&field)
            {
                continue;
            }
            if field.is_presence() {
                // Absence in a selected, previously materialized record is an
                // explicit deletion. Unselected records never enter this branch.
                let present = current
                    .get(&field)
                    .map(|value| value.decode::<bool>())
                    .transpose()?
                    .unwrap_or(false);
                self.write(&field, Some(&FieldValue::encode(&present)?))?;
            } else {
                self.write(&field, current.get(&field))?;
            }
            changed_records.insert(field.resource);
        }
        for resource in &changed_records {
            let presence = SyncField::presence(resource.clone());
            if current
                .get(&presence)
                .map(|value| value.decode::<bool>())
                .transpose()?
                .unwrap_or(false)
            {
                self.write(&presence, Some(&FieldValue::encode(&true)?))?;
            }
        }
        self.document.commit();
        Ok(!changed_records.is_empty())
    }

    pub fn materialize(
        &self,
        local: &SyncValues,
        selected: &BTreeSet<SyncResource>,
    ) -> Result<SyncValues> {
        let mut result = local.clone();
        for resource in selected {
            let presence = SyncField::presence(resource.clone());
            let candidates = self.candidates(&presence.key()?)?;
            if candidates.is_empty() || distinct_values(&candidates) > 1 {
                continue;
            }
            let present = self
                .candidate_value(&candidates[0])
                .ok_or_else(|| anyhow::anyhow!("Missing sync record presence"))?
                .decode::<bool>()?;
            if !present {
                result.retain(|field, _| field.resource != *resource);
                continue;
            }
            // A new record with unresolved fields has no complete local value
            // to retain. Wait for resolution rather than creating a partial record.
            if !local.contains_key(&presence)
                && self
                    .conflicts()?
                    .iter()
                    .any(|conflict| conflict.field.resource == *resource)
            {
                continue;
            }
            for key in self.document.keys(ROOT) {
                let field = SyncField::from_key(&key)?;
                if field.resource != *resource {
                    continue;
                }
                let candidates = self.candidates(&key)?;
                if candidates.is_empty() || distinct_values(&candidates) > 1 {
                    continue;
                }
                if let Some(value) = self.candidate_value(&candidates[0]) {
                    result.insert(field, value.clone());
                } else {
                    result.remove(&field);
                }
            }
        }
        Ok(result)
    }

    pub fn resolve(&mut self, conflict: &SyncConflict, choice: Option<&FieldValue>) -> Result<()> {
        let key = conflict.field.key()?;
        if self.candidates(&key)? != conflict.candidates {
            bail!("Sync conflict changed; refresh before resolving it");
        }
        if conflict.field.is_presence() {
            choice
                .ok_or_else(|| anyhow::anyhow!("A record resolution requires a presence value"))?
                .decode::<bool>()?;
        }
        self.write(&conflict.field, choice)?;
        self.document.commit();
        Ok(())
    }

    pub fn resources(&self) -> Result<BTreeSet<SyncResource>> {
        self.document
            .keys(ROOT)
            .map(|key| SyncField::from_key(&key).map(|field| field.resource))
            .collect()
    }

    fn write(&mut self, field: &SyncField, value: Option<&FieldValue>) -> Result<()> {
        let scalar = if let Some(value) = value {
            let digest = value.digest();
            self.values.insert(digest.clone(), value.clone());
            // Automerge elides a put equal to the visible value. Each accepted
            // write needs an identity so an edit can reaffirm record presence
            // concurrently with a deletion, even when presence was already true.
            ScalarValue::Str(format!("{digest}:{}", Uuid::new_v4()).into())
        } else {
            ScalarValue::Null
        };
        self.document
            .put(ROOT, field.key()?, scalar)
            .map_err(|_| anyhow::anyhow!("Cannot record cloud sync change"))?;
        Ok(())
    }

    fn candidates(&self, key: &str) -> Result<Vec<SyncCandidate>> {
        let mut candidates = self
            .document
            .get_all(ROOT, key)
            .map_err(|_| anyhow::anyhow!("Invalid sync register"))?
            .into_iter()
            .map(|(value, operation)| {
                let value_digest = if let Some(reference) = value.as_str() {
                    let (digest, write_id) = reference
                        .split_once(':')
                        .ok_or_else(|| anyhow::anyhow!("Invalid sync value reference"))?;
                    Uuid::parse_str(write_id)
                        .map_err(|_| anyhow::anyhow!("Invalid sync write identity"))?;
                    Some(digest.to_string())
                } else if value
                    == automerge::Value::Scalar(std::borrow::Cow::Owned(ScalarValue::Null))
                {
                    None
                } else {
                    bail!("Invalid sync register value");
                };
                Ok(SyncCandidate {
                    operation: operation.to_string(),
                    value_digest,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        candidates.sort_by(|left, right| left.operation.cmp(&right.operation));
        Ok(candidates)
    }

    fn validate(&self) -> Result<()> {
        for (digest, value) in &self.values {
            if value.digest() != *digest {
                bail!("Cloud sync value digest mismatch");
            }
        }
        for key in self.document.keys(ROOT) {
            let field = SyncField::from_key(&key)?;
            for candidate in self.candidates(&key)? {
                if let Some(digest) = &candidate.value_digest {
                    let value = self
                        .values
                        .get(digest)
                        .ok_or_else(|| anyhow::anyhow!("Cloud sync value is missing"))?;
                    if field.is_presence() {
                        value.decode::<bool>()?;
                    }
                } else if field.is_presence() {
                    bail!("Invalid sync record presence");
                }
            }
            if self
                .candidates(&SyncField::presence(field.resource).key()?)?
                .is_empty()
            {
                bail!("Cloud sync record has no presence state");
            }
        }
        Ok(())
    }
}

fn distinct_values(candidates: &[SyncCandidate]) -> usize {
    candidates
        .iter()
        .map(|candidate| &candidate.value_digest)
        .collect::<BTreeSet<_>>()
        .len()
}
