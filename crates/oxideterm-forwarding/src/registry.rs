// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use dashmap::DashMap;
use oxideterm_ssh::SshConnectionHandle;

use crate::ForwardingManager;

#[derive(Clone, Debug, Default)]
pub struct ForwardingRegistry {
    managers: Arc<DashMap<String, Arc<ForwardingManager>>>,
}

impl ForwardingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Arc<ForwardingManager> {
        let session_id = session_id.into();
        self.managers
            .entry(session_id.clone())
            .or_insert_with(|| Arc::new(ForwardingManager::new(session_id, ssh_connection)))
            .clone()
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        self.managers
            .get(session_id)
            .map(|manager| manager.value().clone())
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        let (_, manager) = self.managers.remove(session_id)?;
        manager.stop_all().await;
        Some(manager)
    }

    pub async fn stop_all(&self) {
        let managers: Vec<Arc<ForwardingManager>> = self
            .managers
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for manager in managers {
            manager.stop_all().await;
        }
    }

    pub fn session_ids(&self) -> Vec<String> {
        let mut session_ids: Vec<String> = self
            .managers
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        session_ids.sort();
        session_ids
    }
}
