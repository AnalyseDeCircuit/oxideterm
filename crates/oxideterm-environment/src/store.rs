// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use crate::model::{GitProbeError, GitProbeKey, GitProbeOutcome, GitRepositorySnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitProbeState {
    Unknown,
    Loading,
    Ready,
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    Error(GitProbeError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitProbeEntry {
    state: GitProbeState,
    snapshot: Option<GitRepositorySnapshot>,
    generation: u64,
    updated_at_ms: u64,
}

impl GitProbeEntry {
    pub fn state(&self) -> &GitProbeState {
        &self.state
    }

    pub fn snapshot(&self) -> Option<&GitRepositorySnapshot> {
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
pub struct GitStatusStore {
    entries: HashMap<GitProbeKey, GitProbeEntry>,
    next_generation: u64,
}

impl GitStatusStore {
    pub fn get(&self, key: &GitProbeKey) -> Option<&GitProbeEntry> {
        self.entries.get(key)
    }

    pub fn snapshot(&self, key: &GitProbeKey) -> Option<&GitRepositorySnapshot> {
        self.get(key).and_then(GitProbeEntry::snapshot)
    }

    pub fn should_probe(&self, key: &GitProbeKey, now_ms: u64, ttl_ms: u64) -> bool {
        match self.entries.get(key) {
            None => true,
            Some(entry) if matches!(entry.state, GitProbeState::Loading) => false,
            Some(entry) => now_ms.saturating_sub(entry.updated_at_ms) >= ttl_ms,
        }
    }

    pub fn mark_loading(&mut self, key: GitProbeKey, now_ms: u64) -> u64 {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        self.entries
            .entry(key)
            .and_modify(|entry| {
                entry.state = GitProbeState::Loading;
                entry.generation = generation;
                entry.updated_at_ms = now_ms;
            })
            .or_insert(GitProbeEntry {
                state: GitProbeState::Loading,
                snapshot: None,
                generation,
                updated_at_ms: now_ms,
            });
        generation
    }

    pub fn finish_probe(
        &mut self,
        key: &GitProbeKey,
        generation: u64,
        outcome: GitProbeOutcome,
        now_ms: u64,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if entry.generation != generation {
            return false;
        }

        match outcome {
            GitProbeOutcome::Ready(snapshot) => {
                entry.state = GitProbeState::Ready;
                entry.snapshot = Some(snapshot);
            }
            GitProbeOutcome::NotRepository => {
                entry.state = GitProbeState::NotRepository;
                entry.snapshot = None;
            }
            GitProbeOutcome::GitUnavailable => {
                entry.state = GitProbeState::GitUnavailable;
                entry.snapshot = None;
            }
            GitProbeOutcome::CwdUnavailable => {
                entry.state = GitProbeState::CwdUnavailable;
                entry.snapshot = None;
            }
            GitProbeOutcome::Error(error) => {
                entry.state = GitProbeState::Error(error);
            }
        }
        entry.updated_at_ms = now_ms;
        true
    }

    pub fn retain_keys(&mut self, keep: impl Fn(&GitProbeKey) -> bool) {
        self.entries.retain(|key, _| keep(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GitBranchIdentity, GitProbeScope};

    fn key() -> GitProbeKey {
        GitProbeKey::new(GitProbeScope::Local, "/repo").unwrap()
    }

    #[test]
    fn loading_keeps_previous_snapshot() {
        let key = key();
        let mut store = GitStatusStore::default();
        let first = store.mark_loading(key.clone(), 0);
        store.finish_probe(
            &key,
            first,
            GitProbeOutcome::Ready(
                GitRepositorySnapshot::new("/repo", GitBranchIdentity::Branch("main".to_string()))
                    .unwrap(),
            ),
            5,
        );

        let second = store.mark_loading(key.clone(), 10);
        let entry = store.get(&key).unwrap();
        assert_eq!(entry.generation(), second);
        assert!(matches!(entry.state(), GitProbeState::Loading));
        assert_eq!(entry.snapshot().unwrap().branch.display_text(), "main");
    }

    #[test]
    fn stale_probe_result_is_ignored() {
        let key = key();
        let mut store = GitStatusStore::default();
        let first = store.mark_loading(key.clone(), 0);
        let _second = store.mark_loading(key.clone(), 1);

        let applied = store.finish_probe(&key, first, GitProbeOutcome::NotRepository, 2);

        assert!(!applied);
        assert!(matches!(
            store.get(&key).unwrap().state(),
            GitProbeState::Loading
        ));
    }

    #[test]
    fn ttl_suppresses_fresh_probe() {
        let key = key();
        let mut store = GitStatusStore::default();
        let generation = store.mark_loading(key.clone(), 100);
        store.finish_probe(&key, generation, GitProbeOutcome::NotRepository, 100);

        assert!(!store.should_probe(&key, 150, 1000));
        assert!(store.should_probe(&key, 1200, 1000));
    }
}
