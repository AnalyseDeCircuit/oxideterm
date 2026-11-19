use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{Entity, PathPromptOptions, SharedString, Subscription, prelude::*};
use oxideterm_connections::SshChannelStrategy;
use oxideterm_gpui_sftp::*;
pub(super) use oxideterm_gpui_sftp::{
    SftpDialog, SftpInput, SftpRemoteId, SftpSurfaceId, SftpTransferState, SftpWorkspaceEntity,
    SftpWorkspaceEvent,
};
use oxideterm_preview::{PreviewAssetOwner, TextLineEnding};
pub(super) use oxideterm_sftp::SftpFileEntry;
use oxideterm_sftp::{
    PreviewContent, RemoteSftpListing, SftpError, SftpFileType, SftpPreviewSaveResult, SftpSession,
    SftpTransferGuard, StoredTransferProgress, TransferDirection as SftpTransferDirection,
    TransferProtocol as RemoteTransferProtocol, browser_files::parent_path,
};
#[cfg(test)]
use std::{collections::VecDeque, time::Duration};

pub(in crate::workspace) mod bookmarks;
pub(super) mod ftp;
pub(super) use oxideterm_gpui_sftp::native_video;

fn configured_transfer_protocol(
    preference: oxideterm_settings::FileTransferProtocolPreference,
) -> RemoteTransferProtocol {
    match preference {
        oxideterm_settings::FileTransferProtocolPreference::Scp => RemoteTransferProtocol::Scp,
        oxideterm_settings::FileTransferProtocolPreference::Auto
        | oxideterm_settings::FileTransferProtocolPreference::Sftp => RemoteTransferProtocol::Sftp,
    }
}

pub(super) use oxideterm_gpui_sftp::SftpPane;

pub(super) struct StandaloneSftpRuntime {
    pub(super) connection_id: String,
    pub(super) consumer: ConnectionConsumer,
    pub(super) handle: SshConnectionHandle,
    pub(super) title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StandaloneSftpTabBinding {
    pub(super) primary_endpoint_id: String,
    pub(super) secondary_endpoint_id: Option<String>,
    pub(super) secondary_initial_remote_path: Option<String>,
}

impl StandaloneSftpTabBinding {
    pub(super) fn contains_endpoint(&self, endpoint_id: &str) -> bool {
        self.primary_endpoint_id == endpoint_id
            || self.secondary_endpoint_id.as_deref() == Some(endpoint_id)
    }
}

pub(super) struct StandaloneSftpConsumerLease {
    registry: SshConnectionRegistry,
    connection_id: String,
    consumer: ConnectionConsumer,
}

pub(super) type DedicatedSftpConnectionSlot =
    Arc<tokio::sync::Mutex<Option<Arc<DedicatedConnectionLease>>>>;

impl Drop for StandaloneSftpConsumerLease {
    fn drop(&mut self) {
        // A background transfer owns this consumer independently from its tab.
        self.registry.release(&self.connection_id, &self.consumer);
    }
}

#[derive(Clone)]
pub(super) enum SftpRemoteBackend {
    Ftp {
        runtime: Arc<ftp::FtpRuntime>,
    },
    Node {
        router: NodeRouter,
        node_id: NodeId,
        channel_strategy: SshChannelStrategy,
        prompt_handler: Arc<dyn SshPromptHandler>,
        managed_key_resolver: ManagedKeyResolver,
        dedicated_slot: DedicatedSftpConnectionSlot,
    },
    Standalone {
        handle: SshConnectionHandle,
    },
}

impl SftpRemoteBackend {
    async fn node_connection(&self) -> Result<SshConnectionHandle, String> {
        let Self::Node {
            router,
            node_id,
            channel_strategy,
            prompt_handler,
            managed_key_resolver,
            dedicated_slot,
        } = self
        else {
            return Err("SFTP backend is not node-backed".to_string());
        };
        if !channel_strategy.requires_dedicated_consumers() {
            return router
                .resolve_connection(node_id)
                .await
                .map(|resolved| resolved.handle)
                .map_err(|error| error.to_string());
        }

        let mut slot = dedicated_slot.lock().await;
        if let Some(lease) = slot.as_ref()
            && matches!(
                lease.handle().state(),
                ConnectionState::Active | ConnectionState::Idle
            )
            && lease.handle().has_physical()
        {
            return Ok(lease.handle().clone());
        }
        *slot = None;
        let consumer = ConnectionConsumer::Sftp(format!("{}:browse", node_id.0));
        let lease = router
            .acquire_dedicated_connection(
                node_id,
                consumer,
                prompt_handler.clone(),
                managed_key_resolver.clone(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let lease = Arc::new(lease);
        let handle = lease.handle().clone();
        // The node-scoped slot owns browsing until explicit disconnect or a
        // failed transport replaces it; individual SFTP views only borrow it.
        *slot = Some(lease);
        Ok(handle)
    }

    async fn resolve_connection(&self) -> Result<SshConnectionHandle, String> {
        match self {
            Self::Node { .. } => self.node_connection().await,
            Self::Standalone { handle } => Ok(handle.clone()),
            Self::Ftp { .. } => Err("FTP does not provide an SSH connection".into()),
        }
    }

    async fn acquire_sftp(&self) -> Result<Arc<tokio::sync::Mutex<SftpSession>>, String> {
        match self {
            Self::Ftp { .. } => Err("FTP does not provide an SFTP subsystem".into()),
            Self::Node { .. } => self
                .node_connection()
                .await?
                .acquire_sftp()
                .await
                .map_err(|error| error.to_string()),
            Self::Standalone { handle } => handle
                .acquire_sftp()
                .await
                .map_err(|error| error.to_string()),
        }
    }

    async fn acquire_transfer_sftp(&self) -> Result<SftpSession, String> {
        match self {
            Self::Ftp { .. } => Err("FTP does not provide an SFTP subsystem".into()),
            Self::Node {
                router,
                node_id,
                channel_strategy,
                prompt_handler,
                managed_key_resolver,
                ..
            } if channel_strategy.requires_dedicated_consumers() => {
                let consumer = ConnectionConsumer::Sftp(format!(
                    "{}:transfer:{}",
                    node_id.0,
                    uuid::Uuid::new_v4()
                ));
                let lease = Arc::new(
                    router
                        .acquire_dedicated_connection(
                            node_id,
                            consumer,
                            prompt_handler.clone(),
                            managed_key_resolver.clone(),
                        )
                        .await
                        .map_err(|error| error.to_string())?,
                );
                let session = lease
                    .handle()
                    .acquire_transfer_sftp()
                    .await
                    .map_err(|error| error.to_string())?;
                let owner: Arc<dyn Send + Sync> = lease;
                Ok(session
                    .with_connection_owner(owner)
                    .with_single_channel_transport())
            }
            Self::Node {
                router, node_id, ..
            } => router
                .acquire_transfer_sftp(node_id)
                .await
                .map_err(|error| error.to_string()),
            Self::Standalone { handle } => handle
                .acquire_transfer_sftp()
                .await
                .map_err(|error| error.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SftpPresentationRequest {
    node_id: NodeId,
    remote_path: Option<String>,
}

#[cfg(test)]
mod entity_delivery_tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    fn completion(index: usize) -> SftpWorkerResult {
        SftpWorkerResult::RemoteMutationComplete {
            result: Ok(()),
            refresh_remote: false,
            refresh_local: false,
            toast: Some(SftpMutationToast {
                success_title: format!("completed-{index}"),
                success_description: None,
                error_title: "unused".to_string(),
            }),
        }
    }

    fn transfer_item() -> SftpTransferItem {
        SftpTransferItem {
            id: 1,
            transfer_id: "transfer-1".to_string(),
            batch_id: None,
            remote_id: SftpRemoteId::Node(NodeId::new("delivery-test")),
            name: "file.txt".to_string(),
            local_path: "/tmp/file.txt".to_string(),
            remote_path: "/file.txt".to_string(),
            direction: SftpTransferDirection::Upload,
            protocol: RemoteTransferProtocol::Sftp,
            size: 10,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        }
    }

    fn file_entry(name: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: name.to_string(),
            path: format!("/{name}"),
            file_type: SftpFileType::File,
            size_known: true,
            size: 1,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    #[gpui::test]
    fn local_selection_survives_opening_a_remote_file_with_its_identity(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let observed = Arc::new(AtomicBool::new(false));
        let observed_event = observed.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if matches!(
                    event,
                    SftpWorkspaceEvent::OpenFileRequested {
                        pane: SftpPane::Remote,
                        file
                    } if file.name == "remote.txt"
                ) {
                    observed_event.store(true, Ordering::Release);
                }
            })
        });

        entity.update(cx, |sftp, cx| {
            sftp.local_files = vec![file_entry("alpha"), file_entry("beta")];
            sftp.select_file(
                SftpPane::Local,
                "alpha".to_string(),
                gpui::Modifiers::default(),
            );
            assert_eq!(sftp.active_pane, SftpPane::Local);
            sftp.activate_file(SftpPane::Remote, file_entry("remote.txt"), cx);
            assert_eq!(sftp.local_selected, HashSet::from(["alpha".to_string()]));
            assert_eq!(sftp.local_last_selected.as_deref(), Some("alpha"));
        });

        assert!(observed.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn worker_state_is_applied_before_effect_event(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| sftp.transfers.push(transfer_item()));
        let observed_completed_state = Arc::new(AtomicBool::new(false));
        let observed_state = observed_completed_state.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |sftp, _, event: &SftpWorkspaceEvent, _cx| {
                if let SftpWorkspaceEvent::WorkerEffectsReady(effects) = event {
                    observed_state.store(
                        sftp.transfers[0].state == SftpTransferState::Completed,
                        Ordering::Release,
                    );
                    let _ = effects.take();
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::TransferComplete {
                remote_id: SftpRemoteId::Node(NodeId::new("delivery-test")),
                transfer_id: "transfer-1".to_string(),
                id: 1,
                result: Ok(()),
                refresh_remote: false,
                refresh_local: false,
            })
            .expect("SFTP completion delivery");
        cx.run_until_parked();

        assert!(observed_completed_state.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn sidebar_switch_preserves_paths_and_transfer_target_but_retires_selection(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, cx| {
            let a = SftpRemoteId::Node(NodeId::new("delivery-test"));
            let b = SftpRemoteId::Node(NodeId::new("other-server"));
            sftp.activate_view(SftpSurfaceId::Sidebar, a.clone());
            sftp.remote_path = "/server-a".into();
            sftp.remote_selected.insert("old-selection".into());
            sftp.transfers.push(transfer_item());
            let generation = sftp.view_generation;
            sftp.deactivate_view(SftpSurfaceId::Sidebar, &a, cx);
            sftp.activate_view(SftpSurfaceId::Sidebar, b.clone());
            assert!(sftp.remote_selected.is_empty());
            assert_ne!(sftp.view_generation, generation);
            assert_eq!(sftp.transfers[0].remote_id, a);
            assert_eq!(sftp.transfers[0].remote_path, "/file.txt");
            sftp.remote_path = "/server-b".into();
            sftp.deactivate_view(SftpSurfaceId::Sidebar, &b, cx);
            sftp.activate_view(SftpSurfaceId::Sidebar, a);
            assert_eq!(sftp.remote_path, "/server-a");
            sftp.activate_view(SftpSurfaceId::Sidebar, b);
            assert_eq!(sftp.remote_path, "/server-b");
        });
    }

    #[gpui::test]
    fn independent_pages_keep_late_directory_results_and_selection_with_their_owner(
        cx: &mut TestAppContext,
    ) {
        let pages = [
            cx.new(SftpWorkspaceEntity::new),
            cx.new(SftpWorkspaceEntity::new),
        ];
        let remote = SftpRemoteId::Node(NodeId::new("shared-node"));
        for (index, page) in pages.iter().enumerate() {
            page.update(cx, |state, _| {
                state.activate_view(SftpSurfaceId::Tab(TabId(index as u64 + 1)), remote.clone());
                state.remote_load_pending = false;
                state.remote_load_inflight = true;
            });
        }
        // Page A completes after B has already loaded and received a selection.
        for index in [1, 0] {
            let name = ["alpha.txt", "beta.txt"][index];
            let path = ["/alpha", "/beta"][index];
            let (sender, generation) = pages[index].read_with(cx, |state, _| {
                (state.worker_sender(), state.view_generation)
            });
            sender
                .send(SftpWorkerResult::RemoteList {
                    surface_id: SftpSurfaceId::Tab(TabId(index as u64 + 1)),
                    remote_id: remote.clone(),
                    view_generation: generation,
                    session_id: "shared-session".into(),
                    path: path.into(),
                    result: Ok(RemoteSftpListing {
                        cwd: path.into(),
                        files: vec![file_entry(name)],
                    }),
                })
                .unwrap();
            cx.run_until_parked();
            pages[index].update(cx, |state, _| {
                state.remote_selected.insert(name.into());
                state.remote_path_input = format!("{path}/draft");
            });
        }
        for (index, page) in pages.iter().enumerate() {
            page.update(cx, |state, _| {
                let path = ["/alpha", "/beta"][index];
                let name = ["alpha.txt", "beta.txt"][index];
                assert_eq!(state.remote_path, path);
                assert_eq!(state.remote_path_input, format!("{path}/draft"));
                assert_eq!(
                    state
                        .remote_files
                        .iter()
                        .map(|file| file.name.as_str())
                        .collect::<Vec<_>>(),
                    vec![name]
                );
                assert_eq!(state.remote_selected, HashSet::from([name.to_string()]));
                assert!(!state.remote_load_pending);
                assert!(!state.remote_load_inflight);
                assert!(!state.remote_loading);
            });
        }
    }

    #[gpui::test]
    fn stale_remote_list_result_does_not_emit_effect(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            sftp.current_surface_id = Some(SftpSurfaceId::Tab(TabId(1)));
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("current-node")));
            sftp.view_generation = 2;
            sftp.remote_load_inflight = true;
        });
        let effect_events = Arc::new(AtomicUsize::new(0));
        let observed_events = effect_events.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if matches!(event, SftpWorkspaceEvent::WorkerEffectsReady(_)) {
                    observed_events.fetch_add(1, Ordering::AcqRel);
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::RemoteList {
                surface_id: SftpSurfaceId::Tab(TabId(1)),
                remote_id: SftpRemoteId::Node(NodeId::new("current-node")),
                view_generation: 1,
                session_id: "stale-session".to_string(),
                path: "/stale".to_string(),
                result: Ok(RemoteSftpListing {
                    cwd: "/stale".to_string(),
                    files: Vec::new(),
                }),
            })
            .expect("stale SFTP delivery");
        cx.run_until_parked();

        assert_eq!(effect_events.load(Ordering::Acquire), 0);
        cx.read(|cx| assert!(!entity.read(cx).remote_load_inflight));
    }

    #[gpui::test]
    fn pending_terminal_cwd_is_not_overwritten_by_inflight_listing(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            // Opening SFTP queues the terminal cwd after the remembered directory starts loading.
            sftp.current_surface_id = Some(SftpSurfaceId::Tab(TabId(1)));
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("current-node")));
            sftp.view_generation = 1;
            sftp.remote_path = "/root/.oxideterm".to_string();
            sftp.remote_path_input = sftp.remote_path.clone();
            sftp.remote_loading = true;
            sftp.remote_load_inflight = true;
            sftp.remote_load_pending = true;
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::RemoteList {
                surface_id: SftpSurfaceId::Tab(TabId(1)),
                remote_id: SftpRemoteId::Node(NodeId::new("current-node")),
                view_generation: 1,
                session_id: "current-session".to_string(),
                path: "/root".to_string(),
                result: Ok(RemoteSftpListing {
                    cwd: "/root".to_string(),
                    files: Vec::new(),
                }),
            })
            .expect("inflight SFTP directory delivery");
        cx.run_until_parked();

        cx.read(|cx| {
            let sftp = entity.read(cx);
            assert_eq!(sftp.remote_path, "/root/.oxideterm");
            assert_eq!(sftp.remote_path_input, "/root/.oxideterm");
            assert!(sftp.remote_load_pending);
            assert!(!sftp.remote_load_inflight);
        });
    }

    #[gpui::test]
    fn hidden_entity_drains_backlog_by_budget_and_stops_on_release(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let ready_events = Arc::new(AtomicUsize::new(0));
        let delivered_effects = Arc::new(AtomicUsize::new(0));
        let observed_events = ready_events.clone();
        let observed_effects = delivered_effects.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if let SftpWorkspaceEvent::WorkerEffectsReady(effects) = event {
                    observed_events.fetch_add(1, Ordering::AcqRel);
                    observed_effects.fetch_add(effects.take().len(), Ordering::AcqRel);
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());
        let wake = sender.wake();

        // More than two lifecycle budgets proves backlog continuation without rendering.
        for index in 0..130 {
            sender
                .send(completion(index))
                .expect("SFTP worker delivery");
        }
        cx.run_until_parked();

        assert_eq!(delivered_effects.load(Ordering::Acquire), 130);
        assert!(ready_events.load(Ordering::Acquire) >= 3);

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }
}

// Keep each SFTP responsibility in a real module while preserving this file as the facade.
mod actions;
mod dialogs;
#[cfg(test)]
mod file_list;
mod helpers;
mod layout;
mod runtime;
mod surface;
mod tree;
pub(super) mod views;

use helpers::{
    join_sftp_path, load_remote_sftp_completion_listing, load_remote_sftp_listing,
    load_remote_sftp_preview, load_remote_sftp_preview_hex, save_remote_sftp_preview,
};
