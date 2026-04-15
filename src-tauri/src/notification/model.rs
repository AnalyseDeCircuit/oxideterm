// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Notification data model.

use serde::{Deserialize, Serialize};

/// A notification record emitted to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRecord {
    pub id: String,
    pub created_at_ms: u64,
    pub kind: NotificationKind,
    pub severity: NotificationSeverity,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub source: NotificationSource,
    pub scope: NotificationScope,
    #[serde(default)]
    pub actions: Vec<NotificationAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationKind {
    Connection,
    Security,
    Transfer,
    Update,
    Health,
    Plugin,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum NotificationSource {
    System,
    Plugin { plugin_id: String },
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum NotificationScope {
    Global,
    Node { node_id: String },
    Connection { connection_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
    pub kind: NotificationActionKind,
    pub variant: NotificationActionVariant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationActionKind {
    RetryConnection,
    OpenSessionManager,
    OpenEventLog,
    OpenSettings,
    OpenSavedConnection,
    AcceptHostKey,
    Dismiss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationActionVariant {
    Primary,
    Secondary,
    Danger,
}

impl NotificationRecord {
    /// Create a new notification with a generated UUID.
    pub fn new(
        kind: NotificationKind,
        severity: NotificationSeverity,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            kind,
            severity,
            title: title.into(),
            body: None,
            source: NotificationSource::System,
            scope: NotificationScope::Global,
            actions: Vec::new(),
            dedupe_key: None,
        }
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_scope(mut self, scope: NotificationScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_actions(mut self, actions: Vec<NotificationAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn with_dedupe_key(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
}
