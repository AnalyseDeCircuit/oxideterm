// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use dashmap::DashMap;
use oxideterm_ssh::SshConnectionHandle;

use crate::{
    ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate, ForwardingError,
    dynamic::DynamicForward,
    local::LocalForward,
    remote::{RemoteForward, RemoteForwardRouter},
};

pub struct ForwardingManager {
    session_id: String,
    ssh_connection: SshConnectionHandle,
    remote_router: Arc<RemoteForwardRouter>,
    local_forwards: DashMap<String, LocalForward>,
    remote_forwards: DashMap<String, RemoteForward>,
    dynamic_forwards: DashMap<String, DynamicForward>,
    stopped_forwards: DashMap<String, ForwardRule>,
}

impl ForwardingManager {
    pub fn new(session_id: impl Into<String>, ssh_connection: SshConnectionHandle) -> Self {
        Self {
            session_id: session_id.into(),
            ssh_connection,
            remote_router: Arc::new(RemoteForwardRouter::default()),
            local_forwards: DashMap::new(),
            remote_forwards: DashMap::new(),
            dynamic_forwards: DashMap::new(),
            stopped_forwards: DashMap::new(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn create_forward(&self, rule: ForwardRule) -> Result<ForwardRule, ForwardingError> {
        if self.has_rule(&rule.id) {
            return Err(ForwardingError::AlreadyExists(rule.id));
        }

        match rule.forward_type {
            ForwardType::Local => {
                let forward = LocalForward::start(rule, self.ssh_connection.clone()).await?;
                let active_rule = forward.rule();
                self.local_forwards.insert(active_rule.id.clone(), forward);
                Ok(active_rule)
            }
            ForwardType::Dynamic => {
                let forward = DynamicForward::start(rule, self.ssh_connection.clone()).await?;
                let active_rule = forward.rule();
                self.dynamic_forwards
                    .insert(active_rule.id.clone(), forward);
                Ok(active_rule)
            }
            ForwardType::Remote => {
                let forward = RemoteForward::start(
                    rule,
                    self.ssh_connection.clone(),
                    self.remote_router.clone(),
                )
                .await?;
                let active_rule = forward.rule();
                self.remote_forwards.insert(active_rule.id.clone(), forward);
                Ok(active_rule)
            }
        }
    }

    pub async fn stop_forward(&self, rule_id: &str) -> Result<ForwardRule, ForwardingError> {
        if let Some((_, forward)) = self.local_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            return Ok(stopped);
        }
        if let Some((_, forward)) = self.dynamic_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            return Ok(stopped);
        }
        if let Some((_, forward)) = self.remote_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            return Ok(stopped);
        }
        self.stopped_forwards
            .get(rule_id)
            .map(|rule| rule.clone())
            .ok_or_else(|| ForwardingError::NotFound(rule_id.to_string()))
    }

    pub async fn restart_forward(&self, rule_id: &str) -> Result<ForwardRule, ForwardingError> {
        let Some((_, mut rule)) = self.stopped_forwards.remove(rule_id) else {
            return Err(ForwardingError::NotFound(rule_id.to_string()));
        };
        rule.status = ForwardStatus::Starting;

        match self.create_forward(rule.clone()).await {
            Ok(active) => Ok(active),
            Err(error) => {
                let mut restored = rule;
                restored.status = ForwardStatus::Stopped;
                self.stopped_forwards.insert(restored.id.clone(), restored);
                Err(error)
            }
        }
    }

    pub async fn delete_forward(&self, rule_id: &str) -> Result<(), ForwardingError> {
        if let Some((_, forward)) = self.local_forwards.remove(rule_id) {
            let _ = forward.stop().await;
            return Ok(());
        }
        if let Some((_, forward)) = self.dynamic_forwards.remove(rule_id) {
            let _ = forward.stop().await;
            return Ok(());
        }
        if let Some((_, forward)) = self.remote_forwards.remove(rule_id) {
            let _ = forward.stop().await;
            return Ok(());
        }
        self.stopped_forwards
            .remove(rule_id)
            .map(|_| ())
            .ok_or_else(|| ForwardingError::NotFound(rule_id.to_string()))
    }

    pub fn update_stopped_forward(
        &self,
        rule_id: &str,
        update: ForwardUpdate,
    ) -> Result<ForwardRule, ForwardingError> {
        if self.local_forwards.contains_key(rule_id)
            || self.dynamic_forwards.contains_key(rule_id)
            || self.remote_forwards.contains_key(rule_id)
        {
            return Err(ForwardingError::ActiveRuleCannotBeEdited(
                rule_id.to_string(),
            ));
        }

        let Some(mut rule) = self.stopped_forwards.get_mut(rule_id) else {
            return Err(ForwardingError::NotFound(rule_id.to_string()));
        };
        rule.apply_update(update);
        rule.status = ForwardStatus::Stopped;
        Ok(rule.clone())
    }

    pub fn list_forwards(&self) -> Vec<ForwardRule> {
        let mut rules = Vec::new();
        rules.extend(self.local_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.remote_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.dynamic_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.stopped_forwards.iter().map(|entry| entry.clone()));
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        rules
    }

    pub fn get_stats(&self, rule_id: &str) -> Result<ForwardStats, ForwardingError> {
        if let Some(forward) = self.local_forwards.get(rule_id) {
            return Ok(forward.stats());
        }
        if let Some(forward) = self.dynamic_forwards.get(rule_id) {
            return Ok(forward.stats());
        }
        if let Some(forward) = self.remote_forwards.get(rule_id) {
            return Ok(forward.stats());
        }
        if self.stopped_forwards.contains_key(rule_id) {
            return Ok(ForwardStats::default());
        }
        Err(ForwardingError::NotFound(rule_id.to_string()))
    }

    pub async fn stop_all(&self) {
        let local_ids: Vec<String> = self
            .local_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let dynamic_ids: Vec<String> = self
            .dynamic_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let remote_ids: Vec<String> = self
            .remote_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for rule_id in local_ids.into_iter().chain(dynamic_ids).chain(remote_ids) {
            let _ = self.stop_forward(&rule_id).await;
        }
    }

    fn has_rule(&self, rule_id: &str) -> bool {
        self.local_forwards.contains_key(rule_id)
            || self.dynamic_forwards.contains_key(rule_id)
            || self.remote_forwards.contains_key(rule_id)
            || self.stopped_forwards.contains_key(rule_id)
    }
}

impl std::fmt::Debug for ForwardingManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForwardingManager")
            .field("session_id", &self.session_id)
            .field("local_forwards", &self.local_forwards.len())
            .field("remote_forwards", &self.remote_forwards.len())
            .field("dynamic_forwards", &self.dynamic_forwards.len())
            .field("stopped_forwards", &self.stopped_forwards.len())
            .finish()
    }
}
