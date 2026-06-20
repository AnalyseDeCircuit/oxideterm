// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use super::model::{ProjectProbeError, ProjectProbeKey, ProjectProbeOutcome, ProjectSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectProbeState {
    Unknown,
    Loading,
    Ready,
    NoProject,
    CwdUnavailable,
    Error(ProjectProbeError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectProbeEntry {
    state: ProjectProbeState,
    snapshot: Option<ProjectSnapshot>,
    generation: u64,
    updated_at_ms: u64,
}

impl ProjectProbeEntry {
    pub fn state(&self) -> &ProjectProbeState {
        &self.state
    }

    pub fn snapshot(&self) -> Option<&ProjectSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn updated_at_ms(&self) -> u64 {
        self.updated_at_ms
    }
}

#[derive(Default)]
pub struct ProjectStatusStore {
    entries: HashMap<ProjectProbeKey, ProjectProbeEntry>,
    next_generation: u64,
}

impl ProjectStatusStore {
    pub fn get(&self, key: &ProjectProbeKey) -> Option<&ProjectProbeEntry> {
        self.entries.get(key)
    }

    pub fn snapshot(&self, key: &ProjectProbeKey) -> Option<&ProjectSnapshot> {
        self.get(key).and_then(ProjectProbeEntry::snapshot)
    }

    pub fn should_probe(&self, key: &ProjectProbeKey, now_ms: u64, ttl_ms: u64) -> bool {
        match self.entries.get(key) {
            None => true,
            Some(entry) if matches!(entry.state, ProjectProbeState::Loading) => false,
            Some(entry) => now_ms.saturating_sub(entry.updated_at_ms) >= ttl_ms,
        }
    }

    pub fn mark_loading(&mut self, key: ProjectProbeKey, now_ms: u64) -> u64 {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        self.entries
            .entry(key)
            .and_modify(|entry| {
                entry.state = ProjectProbeState::Loading;
                entry.generation = generation;
                entry.updated_at_ms = now_ms;
            })
            .or_insert(ProjectProbeEntry {
                state: ProjectProbeState::Loading,
                snapshot: None,
                generation,
                updated_at_ms: now_ms,
            });
        generation
    }

    pub fn finish_probe(
        &mut self,
        key: &ProjectProbeKey,
        generation: u64,
        outcome: ProjectProbeOutcome,
        now_ms: u64,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if entry.generation != generation {
            return false;
        }

        match outcome {
            ProjectProbeOutcome::Ready(snapshot) => {
                entry.state = ProjectProbeState::Ready;
                entry.snapshot = Some(snapshot);
            }
            ProjectProbeOutcome::NoProject => {
                entry.state = ProjectProbeState::NoProject;
                entry.snapshot = None;
            }
            ProjectProbeOutcome::CwdMissing => {
                entry.state = ProjectProbeState::CwdUnavailable;
                entry.snapshot = None;
            }
            ProjectProbeOutcome::Error(error) => {
                entry.state = ProjectProbeState::Error(error);
            }
        }
        entry.updated_at_ms = now_ms;
        true
    }

    pub fn retain_keys(&mut self, keep: impl Fn(&ProjectProbeKey) -> bool) {
        self.entries.retain(|key, _| keep(key));
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{ProjectFacet, ProjectFacetKind, ProjectProbeScope};
    use super::*;

    fn key() -> ProjectProbeKey {
        ProjectProbeKey::new(ProjectProbeScope::Local, "/repo").unwrap()
    }

    fn snapshot() -> ProjectSnapshot {
        ProjectSnapshot::new(
            "/repo",
            vec![
                ProjectFacet::new(ProjectFacetKind::Cargo, "/repo", "/repo/Cargo.toml", vec![])
                    .unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn loading_keeps_previous_snapshot() {
        let key = key();
        let mut store = ProjectStatusStore::default();
        let first = store.mark_loading(key.clone(), 0);
        store.finish_probe(&key, first, ProjectProbeOutcome::Ready(snapshot()), 5);

        let second = store.mark_loading(key.clone(), 10);
        let entry = store.get(&key).unwrap();
        assert_eq!(entry.generation(), second);
        assert!(matches!(entry.state(), ProjectProbeState::Loading));
        assert_eq!(entry.snapshot().unwrap().root_path(), "/repo");
    }

    #[test]
    fn stale_probe_result_is_ignored() {
        let key = key();
        let mut store = ProjectStatusStore::default();
        let first = store.mark_loading(key.clone(), 0);
        let _second = store.mark_loading(key.clone(), 1);

        let applied = store.finish_probe(&key, first, ProjectProbeOutcome::NoProject, 2);

        assert!(!applied);
        assert!(matches!(
            store.get(&key).unwrap().state(),
            ProjectProbeState::Loading
        ));
    }
}
