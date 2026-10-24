use super::*;

pub(super) struct TransferSummary {
    pub(super) id: String,
    pub(super) remote_key: String,
    pub(super) name: String,
    pub(super) state: BackgroundTransferState,
}

#[derive(Clone)]
pub(super) enum BackgroundTaskTarget {
    Transfer(String),
    Reconnect(NodeId),
}

impl WorkspaceApp {
    pub(super) fn observe_background_transfers(
        manager: Arc<SftpTransferManager>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut changes = manager.subscribe_background_state_changes();
        // The workspace owns only this observer. Closing a window does not cancel transfers.
        cx.spawn(async move |workspace, cx| {
            while changes.changed().await.is_ok() {
                let transfers = manager
                    .list_background_transfers(None)
                    .into_iter()
                    .filter(|transfer| {
                        matches!(
                            transfer.state,
                            BackgroundTransferState::Pending
                                | BackgroundTransferState::Active
                                | BackgroundTransferState::Paused
                        )
                    })
                    .map(|transfer| TransferSummary {
                        id: transfer.id,
                        remote_key: transfer.node_id,
                        name: transfer.name,
                        state: transfer.state,
                    })
                    .collect();
                if workspace
                    .update(cx, |this, cx| {
                        this.background_transfers = transfers;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(super) fn background_task_count(&self, cx: &App) -> usize {
        self.background_transfers.len()
            + self
                .workspace_runtime
                .read(cx)
                .active_reconnect_node_ids()
                .len()
    }

    pub(super) fn background_task_choices(
        &self,
        cx: &App,
    ) -> Vec<(BackgroundTaskTarget, String, String, LucideIcon)> {
        let mut choices = self
            .background_transfers
            .iter()
            .map(|transfer| {
                let state = self.i18n.t(match transfer.state {
                    BackgroundTransferState::Pending => "sftp.queue.status_waiting",
                    BackgroundTransferState::Paused => "sftp.queue.status_paused",
                    _ => "sidebar.background_tasks.transferring",
                });
                let host = self.background_transfer_host_label(&transfer.remote_key);
                (
                    BackgroundTaskTarget::Transfer(transfer.id.clone()),
                    transfer.name.clone(),
                    format!("{host} · {state}"),
                    LucideIcon::ArrowUpDown,
                )
            })
            .collect::<Vec<_>>();
        let mut nodes = self.workspace_runtime.read(cx).active_reconnect_node_ids();
        nodes.sort_by(|left, right| left.0.cmp(&right.0));
        choices.extend(nodes.into_iter().map(|node_id| {
            let title = self
                .ssh_nodes
                .get(&node_id)
                .map(|node| node.title.clone())
                .unwrap_or_else(|| self.i18n.t("sidebar.tab_scope.connection"));
            (
                BackgroundTaskTarget::Reconnect(node_id),
                title,
                self.i18n.t("sidebar.background_tasks.reconnecting"),
                LucideIcon::RefreshCw,
            )
        }));
        choices
    }

    pub(super) fn reveal_background_task(
        &mut self,
        target: BackgroundTaskTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            BackgroundTaskTarget::Transfer(id) => self.reveal_background_transfer(&id, window, cx),
            BackgroundTaskTarget::Reconnect(node_id) => {
                if self.ssh_nodes.contains_key(&node_id) {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.active_session_sidebar_focused_node_id = Some(node_id);
                    self.set_sidebar_section(SidebarSection::Sessions, cx);
                    cx.notify();
                }
            }
        }
    }
}
