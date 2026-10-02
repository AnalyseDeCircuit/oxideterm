use gpui::{Context, Window};
use oxideterm_connections::{ConnectionCredentialSlot, SavedAuth, SavedConnection, SecretString};
use oxideterm_ssh::{
    AuthMethod, NodeId, NodeOrigin, SshConfig, SshPasswordPrompt, SshPasswordResponse,
    SshPromptError,
};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use super::{SshConnectionWorkerResult, kbi_dialog::NativeSshPromptSender};
use crate::workspace::{TerminalNoticeVariant, WorkspaceApp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct SavedAuthSaveTarget {
    connection_id: String,
    host: String,
    port: u16,
    username: String,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SavedAuthSaveTarget {
    pub(super) fn new(connection: &SavedConnection) -> Self {
        Self {
            connection_id: connection.id.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            updated_at: connection.updated_at,
        }
    }

    fn is_current(&self, connection: &SavedConnection) -> bool {
        self.connection_id == connection.id
            && self.host == connection.host
            && self.port == connection.port
            && self.username == connection.username
            && self.updated_at == connection.updated_at
    }

    fn take_confirmed_auth(
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

    fn refresh_after_password_save(
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
pub(in crate::workspace) enum SavedPasswordLoadError {
    NotFound,
    Locked,
    Unavailable,
}

impl SavedPasswordLoadError {
    pub(super) fn from_store_error(error: &anyhow::Error) -> Self {
        use oxideterm_portable_runtime::keystore::PortableKeystoreError;
        match error.downcast_ref::<PortableKeystoreError>() {
            Some(PortableKeystoreError::Locked) => Self::Locked,
            Some(PortableKeystoreError::Missing | PortableKeystoreError::NotFound(_)) => {
                Self::NotFound
            }
            _ => Self::Unavailable,
        }
    }

    pub(super) fn message_key(self) -> &'static str {
        match self {
            Self::NotFound => "sessionManager.edit_properties.password_not_found",
            Self::Locked => "sessionManager.edit_properties.password_store_locked",
            Self::Unavailable => "sessionManager.edit_properties.password_load_failed",
        }
    }
}

// Consent is tied to one saved record and slot, never inferred from an endpoint search.
pub(in crate::workspace) struct SavedPasswordTarget {
    node_id: NodeId,
    connection_id: String,
    slot: ConnectionCredentialSlot,
    prompt: SshPasswordPrompt,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn saved_password_target(
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
    fn is_current(&self, connection: &SavedConnection) -> bool {
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

impl WorkspaceApp {
    pub(in crate::workspace) fn mark_saved_connection_used_for_node(
        &mut self,
        node_id: &NodeId,
        connection_id: &str,
    ) {
        let refresh_consent = self
            .ssh_nodes
            .get(node_id)
            .and_then(|node| node.pending_auth_save_target.as_ref())
            .zip(self.connection_store.get(connection_id))
            .is_some_and(|(target, connection)| target.is_current(connection));
        if matches!(self.connection_store.mark_used(connection_id), Ok(true))
            && refresh_consent
            && let Some(connection) = self.connection_store.get(connection_id)
            && let Some(node) = self.ssh_nodes.get_mut(node_id)
        {
            // Recency touches change updated_at; preserve consent only after checking the previous record.
            node.pending_auth_save_target = Some(SavedAuthSaveTarget::new(connection));
        }
    }

    pub(in crate::workspace) fn arm_saved_auth_save(
        &mut self,
        node_id: &NodeId,
        target: Option<SavedAuthSaveTarget>,
    ) {
        if let Some(target) = target
            && let Some(node) = self.ssh_nodes.get_mut(node_id)
            && node.readiness != oxideterm_ssh::NodeReadiness::Ready
        {
            // The node retains consent metadata; its runtime remains the only credential owner.
            node.pending_auth_save_target = Some(target);
        }
    }

    pub(super) fn save_confirmed_connection_auth(
        &mut self,
        node_id: NodeId,
        authenticated_connection_id: String,
        configured_credentials_confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.ssh_nodes.get_mut(&node_id) else {
            return;
        };
        let Some(target) = node.pending_auth_save_target.as_ref() else {
            return;
        };
        let connection_id = target.connection_id.clone();
        let Some(connection) = self.connection_store.get(&connection_id) else {
            return;
        };
        let Some(snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            return;
        };
        let Some(auth) = SavedAuthSaveTarget::take_confirmed_auth(
            &mut node.pending_auth_save_target,
            connection,
            snapshot.config,
            snapshot.connection_id.as_deref(),
            &authenticated_connection_id,
            configured_credentials_confirmed,
        ) else {
            return;
        };
        let mut form = super::ssh_flow::form_from_runtime_config(
            oxideterm_ssh::SshConfig {
                auth,
                ..Default::default()
            },
            None,
            String::new(),
        );
        form.save_password = true;
        // Move the short-lived runtime projection into protected persistence; the draft zeroizes on drop.
        let auth = oxideterm_connections::saved_auth_from_draft(
            crate::workspace::session_manager::auth_draft_from_form(&mut form, true),
        );
        if matches!(
            self.connection_store
                .set_connection_auth(&connection_id, auth),
            Ok(true)
        ) {
            self.queue_cloud_sync_dirty_refresh(cx);
            self.push_command_palette_toast(
                self.i18n.t("ssh.form.credentials_saved"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            );
        } else {
            tracing::warn!("Unable to save confirmed SSH authentication information");
            self.push_command_palette_toast(
                self.i18n.t("ssh.form.save_credentials_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn open_password_prompt(
        &mut self,
        node_id: Option<NodeId>,
        prompt: SshPasswordPrompt,
        response_tx: oneshot::Sender<Result<SshPasswordResponse, SshPromptError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = node_id.and_then(|node_id| {
            let node = self.node_router.node_metadata(&node_id)?;
            if node.host != prompt.host
                || node.port != prompt.port
                || node.username != prompt.username
            {
                return None;
            }
            let saved_id = node.origin.saved_connection_id().or_else(|| {
                self.ssh_nodes
                    .get(&node_id)
                    .and_then(|node| node.saved_connection_id.as_deref())
            })?;
            let connection = self.connection_store.get(saved_id)?;
            saved_password_target(connection, node_id, &node.origin, prompt.clone())
        });
        let tx = self.ssh_worker_sender(cx);
        let on_authenticated = target.map(|target| {
            Box::new(move |password| {
                // The completed authentication transfers its only pending secret owner to UI persistence.
                let _ =
                    tx.send(SshConnectionWorkerResult::PasswordAuthenticated { target, password });
            }) as Box<dyn FnOnce(Zeroizing<String>) + Send>
        });
        let opened = self.connection_flow.update(cx, |flow, cx| {
            flow.open_keyboard_interactive_challenge(
                prompt.challenge(),
                NativeSshPromptSender::Password {
                    response_tx,
                    on_authenticated,
                },
                cx,
            )
        });
        if opened {
            self.prepare_modal_interaction_boundary(cx);
            self.show_active_input_caret(cx);
            self.needs_active_pane_focus = false;
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    pub(super) fn save_authenticated_prompt_password(
        &mut self,
        target: SavedPasswordTarget,
        password: Zeroizing<String>,
        cx: &mut Context<Self>,
    ) {
        let secret = SecretString::from(password);
        let saved = self
            .connection_store
            .get(&target.connection_id)
            .is_some_and(|connection| target.is_current(connection))
            && matches!(
                self.connection_store.store_connection_credential(
                    &target.connection_id,
                    target.slot,
                    &secret
                ),
                Ok(true)
            );
        if !saved {
            // Storage errors can contain protected-store paths; only a fixed message reaches diagnostics.
            tracing::warn!("Unable to save authenticated SSH prompt password");
            self.push_command_palette_toast(
                self.i18n.t("ssh.kbi.save_password_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        if let Some(connection) = self.connection_store.get(&target.connection_id) {
            for node in self.ssh_nodes.values_mut() {
                if let Some(pending) = node.pending_auth_save_target.as_mut() {
                    pending.refresh_after_password_save(&target, connection);
                }
            }
        }
        if let Some(mut node) = self.node_router.node_runtime_snapshot(&target.node_id)
            && node.config.host == target.prompt.host
            && node.config.port == target.prompt.port
            && node.config.username == target.prompt.username
        {
            // Reconnect and additional terminals read the node-owned credential, not the old prompt policy.
            let auth = match &mut node.config.auth {
                AuthMethod::KerberosPreferred { fallback, .. } => fallback.as_mut(),
                auth => auth,
            };
            if matches!(auth, AuthMethod::Password { prompt: true, .. }) {
                *auth = AuthMethod::password_secret(secret.into_zeroizing());
                self.node_router
                    .upsert_node_with_origin(target.node_id, node.config, node.origin);
            }
        }
        self.queue_cloud_sync_dirty_refresh(cx);
        self.push_command_palette_toast(
            self.i18n.t("ssh.kbi.password_saved"),
            None,
            TerminalNoticeVariant::Success,
            cx,
        );
        cx.notify();
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

    #[test]
    fn password_read_errors_preserve_actionable_categories_through_context() {
        use oxideterm_portable_runtime::keystore::PortableKeystoreError;
        for (error, expected) in [
            (
                PortableKeystoreError::NotFound("private-reference".into()),
                SavedPasswordLoadError::NotFound,
            ),
            (
                PortableKeystoreError::Missing,
                SavedPasswordLoadError::NotFound,
            ),
            (
                PortableKeystoreError::Locked,
                SavedPasswordLoadError::Locked,
            ),
            (
                PortableKeystoreError::CredentialStore,
                SavedPasswordLoadError::Unavailable,
            ),
        ] {
            let error = anyhow::Error::new(error).context("credential read failed");
            assert_eq!(SavedPasswordLoadError::from_store_error(&error), expected);
        }
    }
}
