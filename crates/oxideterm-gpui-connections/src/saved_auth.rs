// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionCredentialSlot, ConnectionStore, SavedAuth, SavedConnection,
};
use oxideterm_ssh::{AuthMethod, NodeId, NodeOrigin, SshConfig, SshPasswordPrompt};

impl crate::entity::ConnectionFlowEntity {
    pub fn reveal_saved_password(
        &mut self, store: &ConnectionStore, cx: &mut gpui::Context<Self>,
    ) {
        let Some(id) = self.form.form.as_ref().and_then(|form| form.saved_password_keychain_id.clone()) else { return; };
        let store = store.clone();
        let executor = cx.background_executor().clone();
        self.start_password_load(async move {
            // Native credential access can block on the protected store.
            executor.spawn(async move {
                store.get_saved_auth_password_optional(&SavedAuth::Password {
                    empty_password: false,
                    keychain_id: Some(id),
                    plaintext_password: None,
                }).map_err(|error| SavedPasswordLoadError::from_store_error(&error))?
                    .ok_or(SavedPasswordLoadError::NotFound)
            }).await
        }, cx);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedAuthSaveTarget {
    connection_id: String,
    host: String,
    port: u16,
    username: String,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SavedAuthSaveTarget {
    pub fn persist_confirmed_auth(
        pending: &mut Option<Self>,
        store: &mut ConnectionStore,
        config: SshConfig,
        current_connection_id: Option<&str>,
        authenticated_connection_id: &str,
        configured_credentials_confirmed: bool,
    ) -> Option<bool> {
        let id = pending.as_ref()?.connection_id.clone();
        let auth = Self::take_confirmed_auth(
            pending,
            store.get(&id)?,
            config,
            current_connection_id,
            authenticated_connection_id,
            configured_credentials_confirmed,
        )?;
        // Only the configured credentials confirmed by this session may reach storage.
        let mut draft = crate::conversion::form_from_runtime_config(
            SshConfig {
                auth,
                ..Default::default()
            },
            None,
            String::new(),
        );
        draft.save_password = true;
        let auth = oxideterm_connections::saved_auth_from_draft(
            crate::configuration::auth_draft_from_form(&mut draft, true),
        );
        Some(matches!(store.set_connection_auth(&id, auth), Ok(true)))
    }

    pub fn new(connection: &SavedConnection) -> Self {
        Self {
            connection_id: connection.id.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            updated_at: connection.updated_at,
        }
    }

    pub fn is_current(&self, connection: &SavedConnection) -> bool {
        self.connection_id == connection.id
            && self.host == connection.host
            && self.port == connection.port
            && self.username == connection.username
            && self.updated_at == connection.updated_at
    }

    pub fn take_confirmed_auth(
        pending: &mut Option<Self>,
        connection: &SavedConnection,
        config: SshConfig,
        current_connection_id: Option<&str>,
        authenticated_connection_id: &str,
        configured_credentials_confirmed: bool,
    ) -> Option<AuthMethod> {
        // Replacement attempts retire the old physical connection. Check its receipt
        // before consuming consent, including after the current transport finishes.
        if current_connection_id != Some(authenticated_connection_id) {
            return None;
        }
        let target = pending.take()?;
        if !configured_credentials_confirmed
            || !target.is_current(connection)
            || config.host != target.host
            || config.port != target.port
            || config.username != target.username
        {
            return None;
        }
        Some(config.auth)
    }

    pub fn refresh_after_password_save(
        &mut self,
        saved: &SavedPasswordTarget,
        connection: &SavedConnection,
    ) {
        // A verified write to a jump slot changes the record revision, not the
        // primary authentication the user consented to save. Never revive stale consent.
        if matches!(saved.slot, ConnectionCredentialSlot::ProxyHop { .. })
            && self.connection_id == saved.connection_id
            && self.connection_id == connection.id
            && self.updated_at == saved.updated_at
            && self.host == connection.host
            && self.port == connection.port
            && self.username == connection.username
        {
            self.updated_at = connection.updated_at;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedPasswordLoadError {
    NotFound,
    Locked,
    Unavailable,
}

impl SavedPasswordLoadError {
    pub fn from_store_error(error: &anyhow::Error) -> Self {
        use oxideterm_portable_runtime::keystore::PortableKeystoreError;
        match error.downcast_ref::<PortableKeystoreError>() {
            Some(PortableKeystoreError::Locked) => Self::Locked,
            Some(PortableKeystoreError::Missing | PortableKeystoreError::NotFound(_)) => {
                Self::NotFound
            }
            _ => Self::Unavailable,
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            Self::NotFound => "sessionManager.edit_properties.password_not_found",
            Self::Locked => "sessionManager.edit_properties.password_store_locked",
            Self::Unavailable => "sessionManager.edit_properties.password_load_failed",
        }
    }
}

// Consent is tied to one saved record and slot, never inferred from an endpoint search.
pub struct SavedPasswordTarget {
    pub node_id: NodeId,
    pub connection_id: String,
    pub slot: ConnectionCredentialSlot,
    pub prompt: SshPasswordPrompt,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn saved_password_target(
    connection: &SavedConnection,
    node_id: NodeId,
    origin: &NodeOrigin,
    prompt: SshPasswordPrompt,
) -> Option<SavedPasswordTarget> {
    let slot = match origin {
        NodeOrigin::ManualPreset { hop_index, .. }
            if (*hop_index as usize) < connection.proxy_chain.len() =>
        {
            ConnectionCredentialSlot::ProxyHop {
                index: *hop_index as usize,
            }
        }
        NodeOrigin::ManualPreset { hop_index, .. }
            if (*hop_index as usize) != connection.proxy_chain.len() =>
        {
            return None;
        }
        _ => ConnectionCredentialSlot::Primary,
    };
    let target = SavedPasswordTarget {
        node_id,
        connection_id: connection.id.clone(),
        slot,
        prompt,
        updated_at: connection.updated_at,
    };
    target.is_current(connection).then_some(target)
}

impl SavedPasswordTarget {
    pub fn persist(
        &self,
        store: &mut ConnectionStore,
        secret: &oxideterm_connections::SecretString,
    ) -> bool {
        store
            .get(&self.connection_id)
            .is_some_and(|connection| self.is_current(connection))
            && matches!(
                store.store_connection_credential(&self.connection_id, self.slot, secret),
                Ok(true)
            )
    }

    pub fn is_current(&self, connection: &SavedConnection) -> bool {
        if connection.id != self.connection_id || connection.updated_at != self.updated_at {
            return false;
        }
        let (host, port, username, auth) = match self.slot {
            ConnectionCredentialSlot::Primary => (
                &connection.host,
                connection.port,
                &connection.username,
                &connection.auth,
            ),
            ConnectionCredentialSlot::ProxyHop { index } => {
                let Some(hop) = connection.proxy_chain.get(index) else {
                    return false;
                };
                (&hop.host, hop.port, &hop.username, &hop.auth)
            }
            ConnectionCredentialSlot::UpstreamProxy => return false,
        };
        host == &self.prompt.host
            && port == self.prompt.port
            && username == &self.prompt.username
            && matches!(auth.conventional_fallback(), SavedAuth::Password { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved_password_connection() -> SavedConnection {
        serde_json::from_value(serde_json::json!({
            "id": "saved-a", "name": "Saved", "host": "target.test", "username": "user",
            "auth": {"type": "password"}, "created_at": "2026-09-30T00:00:00Z",
            "proxy_chain": [{"host": "jump.test", "username": "jump-user", "auth": {"type": "password"}}]
        })).unwrap()
    }

    #[test]
    fn stale_authentication_receipt_cannot_consume_replacement_consent() {
        use oxideterm_ssh::{
            ConnectionConsumer, ConnectionPoolConfig, NodeRouter, SshConfig, SshConnectionRegistry,
        };
        let connection = saved_password_connection();
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let router = NodeRouter::new(registry.clone());
        let node_id = NodeId::new("reused-node");
        let config = SshConfig::password("target.test", 22, "user", "new-fixture-password");
        let handle = registry.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        router.upsert_node(node_id.clone(), config);
        router
            .bind_connection(&node_id, handle.connection_id())
            .unwrap();
        let mut pending = Some(SavedAuthSaveTarget::new(&connection));

        for confirmed in [true, false] {
            let snapshot = router.node_runtime_snapshot(&node_id).unwrap();
            assert_eq!(
                SavedAuthSaveTarget::take_confirmed_auth(
                    &mut pending,
                    &connection,
                    snapshot.config,
                    snapshot.connection_id.as_deref(),
                    "retired-connection",
                    confirmed,
                ),
                None
            );
            assert!(pending.as_ref().unwrap().is_current(&connection));
        }
        let snapshot = router.node_runtime_snapshot(&node_id).unwrap();
        assert_eq!(
            SavedAuthSaveTarget::take_confirmed_auth(
                &mut pending,
                &connection,
                snapshot.config,
                snapshot.connection_id.as_deref(),
                handle.connection_id(),
                true,
            ),
            Some(AuthMethod::password("new-fixture-password"))
        );
        assert!(pending.is_none());
    }

    #[test]
    fn saving_a_jump_password_preserves_only_unchanged_primary_consent() {
        let mut connection = saved_password_connection();
        let mut pending = SavedAuthSaveTarget::new(&connection);
        let mut stale = pending.clone();
        stale.updated_at = Some(
            chrono::DateTime::parse_from_rfc3339("2026-09-29T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let target = saved_password_target(
            &connection,
            NodeId::new("jump"),
            &NodeOrigin::ManualPreset {
                saved_connection_id: connection.id.clone(),
                hop_index: 0,
            },
            SshPasswordPrompt {
                host: "jump.test".into(),
                port: 22,
                username: "jump-user".into(),
            },
        )
        .unwrap();
        // A successful protected-slot write changes the record revision without changing primary auth.
        connection.proxy_chain[0].auth = SavedAuth::Password {
            empty_password: false,
            keychain_id: Some("fixture-protected-reference".into()),
            plaintext_password: None,
        };
        connection.updated_at = Some(chrono::Utc::now());
        pending.refresh_after_password_save(&target, &connection);
        stale.refresh_after_password_save(&target, &connection);
        assert!(pending.is_current(&connection));
        assert!(!stale.is_current(&connection));

        connection.updated_at = Some(chrono::Utc::now() + chrono::Duration::seconds(1));
        assert!(
            !pending.is_current(&connection),
            "later user edits must still invalidate consent"
        );
    }

    #[test]
    fn password_prompt_target_keeps_saved_identity_and_jump_slot_and_rejects_stale_records() {
        let mut connection = saved_password_connection();
        connection.auth = SavedAuth::KerberosPreferred {
            server_identity: None,
            delegate_credentials: false,
            fallback: Box::new(connection.auth),
        };
        let node = NodeId::new("node-a");
        for (hop_index, host, username, expected_slot) in [
            (
                0,
                "jump.test",
                "jump-user",
                ConnectionCredentialSlot::ProxyHop { index: 0 },
            ),
            (1, "target.test", "user", ConnectionCredentialSlot::Primary),
        ] {
            let origin = NodeOrigin::ManualPreset {
                saved_connection_id: "saved-a".into(),
                hop_index,
            };
            let prompt = SshPasswordPrompt {
                host: host.into(),
                port: 22,
                username: username.into(),
            };
            let target =
                saved_password_target(&connection, node.clone(), &origin, prompt.clone()).unwrap();
            assert_eq!(
                (&target.connection_id, target.slot),
                (&"saved-a".to_string(), expected_slot)
            );
            let mut other = connection.clone();
            other.id = "saved-b".into();
            assert!(!target.is_current(&other));
            other = connection.clone();
            other.updated_at = Some(chrono::Utc::now());
            assert!(!target.is_current(&other));
            let mut wrong_endpoint = prompt;
            wrong_endpoint.port = 2222;
            assert!(
                saved_password_target(&connection, node.clone(), &origin, wrong_endpoint).is_none()
            );
        }
        connection.auth = SavedAuth::KeyboardInteractive;
        assert!(
            saved_password_target(
                &connection,
                node,
                &NodeOrigin::Restored {
                    saved_connection_id: "saved-a".into()
                },
                SshPasswordPrompt {
                    host: "target.test".into(),
                    port: 22,
                    username: "user".into()
                },
            )
            .is_none()
        );
    }
}
