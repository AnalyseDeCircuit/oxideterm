use gpui::{Context, Window};
use oxideterm_connections::SecretString;
use oxideterm_ssh::{AuthMethod, NodeId, SshPasswordPrompt, SshPasswordResponse, SshPromptError};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use super::{SshConnectionWorkerResult, kbi_dialog::NativeSshPromptSender};
use crate::workspace::{TerminalNoticeVariant, WorkspaceApp};

use oxideterm_gpui_connections::saved_auth::saved_password_target;
pub(in crate::workspace) use oxideterm_gpui_connections::saved_auth::{
    SavedAuthSaveTarget, SavedPasswordTarget,
};

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
        let Some(snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            return;
        };
        let Some(saved) = SavedAuthSaveTarget::persist_confirmed_auth(
            &mut node.pending_auth_save_target,
            &mut self.connection_store,
            snapshot.config,
            snapshot.connection_id.as_deref(),
            &authenticated_connection_id,
            configured_credentials_confirmed,
        ) else {
            return;
        };
        if saved {
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
        let opened = self.connections.flow.update(cx, |flow, cx| {
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
        let saved = target.persist(&mut self.connection_store, &secret);
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
