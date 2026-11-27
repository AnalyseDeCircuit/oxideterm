use super::*;
use ProviderAvailability::{Active, AwaitingActivation, Unavailable};
use oxideterm_connections::ConnectionStore;
use tokio::sync::{mpsc, oneshot};

type PageReply = oneshot::Sender<Result<DiscoveryPage, DiscoveryFailure>>;

struct PageCall {
    refresh: String,
    cursor: Option<String>,
    reply: PageReply,
}

struct ProviderEndpoint {
    actions: mpsc::UnboundedSender<(String, oneshot::Sender<Result<(), DiscoveryFailure>>)>,
    pages: mpsc::UnboundedSender<PageCall>,
    canceled: mpsc::UnboundedSender<String>,
}

#[async_trait::async_trait]
impl DiscoveryProvider for ProviderEndpoint {
    fn generation(&self) -> u64 {
        1
    }
    fn supports_source_action(&self, action_id: &str) -> bool {
        action_id == "configureKey"
    }
    async fn source_action(
        &self,
        _: &DiscoverySource,
        action_id: &str,
        _: CancellationToken,
    ) -> Result<(), DiscoveryFailure> {
        let (reply, response) = oneshot::channel();
        self.actions
            .send((action_id.to_owned(), reply))
            .map_err(|_| DiscoveryFailure::Unavailable)?;
        response.await.map_err(|_| DiscoveryFailure::Unavailable)?
    }
    async fn list(
        &self,
        _: &DiscoverySource,
        refresh: &str,
        cursor: Option<String>,
        _: &CancellationToken,
    ) -> Result<DiscoveryPage, DiscoveryFailure> {
        let (reply, response) = oneshot::channel();
        self.pages
            .send(PageCall {
                refresh: refresh.to_owned(),
                cursor,
                reply,
            })
            .map_err(|_| DiscoveryFailure::Unavailable)?;
        response.await.map_err(|_| DiscoveryFailure::Unavailable)?
    }

    async fn cancel_list(&self, _: &DiscoverySource, refresh: &str) {
        let _ = self.canceled.send(refresh.to_owned());
    }
}

struct Endpoint {
    provider: Provider,
    actions: mpsc::UnboundedReceiver<(String, oneshot::Sender<Result<(), DiscoveryFailure>>)>,
    pages: mpsc::UnboundedReceiver<PageCall>,
    canceled: mpsc::UnboundedReceiver<String>,
}

impl Endpoint {
    fn new() -> Self {
        let (actions, action_calls) = mpsc::unbounded_channel();
        let (pages, page_calls) = mpsc::unbounded_channel();
        let (canceled, cancellations) = mpsc::unbounded_channel();
        Self {
            provider: Arc::new(ProviderEndpoint {
                actions,
                pages,
                canceled,
            }),
            actions: action_calls,
            pages: page_calls,
            canceled: cancellations,
        }
    }

    async fn next_page(&mut self) -> PageCall {
        tokio::time::timeout(Duration::from_secs(1), self.pages.recv())
            .await
            .expect("expected an enumeration before the next scheduled interval")
            .unwrap()
    }
}

fn clock() -> Arc<Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap(),
    )
}

fn page(id: &str, next: Option<&str>) -> DiscoveryPage {
    DiscoveryPage {
        targets: vec![DiscoveryTarget {
            id: id.to_owned(),
            label: id.to_owned(),
            detail: None,
            scope: None,
            group_paths: vec![],
            tags: Vec::new(),
            target_revision: Some("revision-1".into()),
        }],
        next_cursor: next.map(str::to_owned),
    }
}

fn source() -> DiscoverySource {
    DiscoverySource {
        id: "inventory".into(),
        name: "Hosts".into(),
        ..Default::default()
    }
}

#[test]
fn discovery_refresh_retires_directory_reuse_when_a_target_revision_changes() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let service = DiscoveryService::shared(&dir.path().join("library"), &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        service.configure(vec![(source(), Active(endpoint.provider.clone()))]);
        let mut changes = service.subscribe();
        service.request_directory("inventory", true);
        let first = endpoint.next_page().await;
        changes.borrow_and_update();
        first.reply.send(Ok(page("asset", None))).unwrap();
        changes.changed().await.unwrap();
        let origin = crate::DiscoveryOrigin::new(
            "inventory".into(),
            "asset".into(),
            service.epoch("inventory").unwrap(),
        )
        .with_target_revision(&service, Some("revision-1".into()))
        .with_session_identity(Some("operator".into()))
        .with_directory_reuse(true, Default::default());
        origin.connected();
        assert!(origin.allows_directory_reuse());

        for (revision, reusable) in [
            ("revision-1", true),
            ("revision-2", false),
            ("revision-1", false),
        ] {
            let mut result = page("asset", None);
            result.targets[0].group_paths = vec![vec![crate::DiscoveryGroupSegment {
                id: "rotated".into(),
                label: "New placement".into(),
            }]];
            result.targets[0].label = "Renamed asset".into();
            result.targets[0].target_revision = Some(revision.into());
            service.request_directory("inventory", true);
            let call = endpoint.next_page().await;
            changes.borrow_and_update();
            call.reply.send(Ok(result)).unwrap();
            changes.changed().await.unwrap();
            assert_eq!(origin.allows_directory_reuse(), reusable);
            let replacement =
                crate::DiscoveryOrigin::new("inventory".into(), "asset".into(), origin.epoch)
                    .with_target_revision(&service, Some(revision.into()))
                    .with_session_identity(Some("operator".into()));
            assert_eq!(
                origin.same_access_identity(&replacement),
                reusable.then_some(true)
            );
        }
        assert!(origin.allows_target_operations());
    });
}

#[test]
fn discovery_online_reuse_expires_with_route_or_issuer_but_cannot_authorize_a_new_handshake() {
    use crate::DiscoveryOrigin;
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library");
    let service = DiscoveryService::shared(&path, &runtime);
    let mut endpoint = Endpoint::new();
    let issuer = CancellationToken::new();
    let origin = runtime.block_on(async {
        service.configure(vec![(source(), Active(endpoint.provider.clone()))]);
        let mut changes = service.subscribe();
        service.request_directory("inventory", true);
        let call = endpoint.next_page().await;
        changes.borrow_and_update();
        call.reply.send(Ok(page("asset", None))).unwrap();
        changes.changed().await.unwrap();
        DiscoveryOrigin::new(
            "inventory".into(),
            "asset".into(),
            service.epoch("inventory").unwrap(),
        )
        .with_target_revision(&service, Some("revision-1".into()))
        .with_directory_reuse(true, issuer.clone())
    });
    assert!(origin.can_handshake());
    origin.connected();
    assert!(!origin.can_handshake());
    assert!(origin.allows_directory_reuse());
    let inherited_route = origin.clone().with_global_proxy(&None);
    assert!(inherited_route.consumer_authority_current());
    let config = serde_json::from_value(
        serde_json::json!({"protocol":"socks5","host":"relay.example","port":1080}),
    )
    .unwrap();
    service.set_global_proxy(&Some(config));
    assert!(!inherited_route.consumer_authority_current());
    assert!(origin.consumer_authority_current());
    issuer.cancel();
    assert!(!origin.allows_directory_reuse());
    assert!(origin.allows_target_operations());
}

#[test]
fn windows_share_a_refresh_and_leave_the_saved_library_untouched() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let first = DiscoveryService::shared(&path, &runtime);
    let second = DiscoveryService::shared(&path, &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        let src = source();
        std::fs::write(&path, br#"{"connections":[{"id":"manual","name":"temporary","host":"manual.example","username":"ops","auth":{"type":"agent"},"created_at":"2026-01-01T00:00:00Z"}]}"#).unwrap();
        for window in [&first, &second] {
            window.configure(vec![(src.clone(), Active(endpoint.provider.clone()))]);
        }
        assert!(second.visible_entries().is_empty());
        let before = std::fs::read(&path).unwrap();
        let mut changes = first.subscribe();
        second.ensure_visible();
        changes.changed().await.unwrap();
        let start = endpoint.next_page().await;
        first.request_directory("inventory", true);
        let grouped = |id, next| {
            let mut result = page(id, next);
            result.targets[0].group_paths = vec![vec![crate::DiscoveryGroupSegment { id: "shared".into(), label: "On call".into() }]];
            result
        };
        start.reply.send(Ok(grouped("temporary", Some("more")))).unwrap();
        let last = endpoint.next_page().await;
        assert_eq!(last.refresh, start.refresh);
        assert_eq!(last.cursor.as_deref(), Some("more"));
        second.request_directory("inventory", true);
        assert!(second.visible_entries().is_empty());
        last.reply.send(Ok(grouped("second", None))).unwrap();
        changes.changed().await.unwrap();
        tokio::task::yield_now().await;
        let snapshot = first.snapshot("inventory").unwrap();
        assert!(!snapshot.refreshing);
        assert_eq!(
            snapshot.targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["second", "temporary"]
        );
        assert!(endpoint.pages.try_recv().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let entries = second.visible_entries();
        let epoch = first.epoch("inventory").unwrap();
        let grant = first.authority("inventory", epoch).unwrap();
        second.configure(vec![(src.clone(), AwaitingActivation)]);
        assert_eq!(second.visible_entries(), entries);
        assert!(!grant.is_cancelled());
        assert_eq!(
            entries.iter().map(|entry| entry.target_id.as_str()).collect::<Vec<_>>(),
            ["second", "temporary"]
        );
        let disabled = DiscoverySource { enabled: false, ..src };
        first.configure(vec![(disabled, Active(endpoint.provider.clone()))]);
        second.ensure_visible();
        assert!(second.visible_entries().is_empty());
        assert!(!second.snapshot("inventory").unwrap().refreshing);
    });
}

#[test]
fn changing_the_open_binding_preserves_an_inflight_directory_page() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let service = DiscoveryService::shared(&path, &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        let mut src = source();
        service.configure(vec![(src.clone(), Active(endpoint.provider.clone()))]);
        service.request_directory("inventory", true);
        let directory = endpoint.next_page().await;
        src.ssh_preferences.skip_connection_form = true;
        service.configure(vec![(src.clone(), Active(endpoint.provider.clone()))]);
        let mut changes = service.subscribe();
        directory
            .reply
            .send(Ok(page("directory-only", None)))
            .unwrap();
        changes.changed().await.unwrap();
        assert_eq!(service.visible_entries()[0].target_id, "directory-only");
        let grant = service
            .authority("inventory", service.epoch("inventory").unwrap())
            .unwrap();
        service.configure(vec![(src, Unavailable)]);
        assert!(grant.is_cancelled());
        assert!(service.visible_entries().is_empty());
    });
}

#[test]
fn advancing_cursors_expire_at_the_round_deadline_then_retry_from_the_last_complete_snapshot() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let service = DiscoveryService::shared(&dir.path().join("connections.json"), &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        let src = DiscoverySource {
            refresh_minutes: std::num::NonZeroU32::new(1),
            ..source()
        };
        service.configure(vec![(src, Active(endpoint.provider.clone()))]);
        service.request_directory("inventory", false);
        let mut changes = service.subscribe();
        endpoint
            .pages
            .recv()
            .await
            .unwrap()
            .reply
            .send(Ok(page("last-complete", None)))
            .unwrap();
        changes.changed().await.unwrap();
        service.request_directory("inventory", true);
        changes.changed().await.unwrap();
        let mut pending = endpoint.pages.recv().await.unwrap();
        let refresh = pending.refresh.clone();
        for n in 1..=11 {
            tokio::time::advance(Duration::from_secs(10)).await;
            let cursor = format!("next-{n}");
            pending
                .reply
                .send(Ok(page("partial", Some(&cursor))))
                .unwrap();
            pending = endpoint.pages.recv().await.unwrap();
            assert_eq!(pending.cursor.as_deref(), Some(cursor.as_str()));
            assert_eq!(pending.refresh, refresh);
        }
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(endpoint.canceled.recv().await.unwrap(), refresh);
        let _ = pending.reply.send(Ok(page("late", None)));
        changes.changed().await.unwrap();
        let snapshot = service.snapshot("inventory").unwrap();
        assert_eq!(snapshot.failure, Some(DiscoveryFailure::Unavailable));
        assert_eq!(
            snapshot
                .targets
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["last-complete"]
        );
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(endpoint.pages.try_recv().is_err());
        tokio::time::advance(Duration::from_secs(1)).await;
        let retry = endpoint.pages.recv().await.unwrap();
        changes.changed().await.unwrap();
        assert!(service.snapshot("inventory").unwrap().refreshing);
        assert_ne!(retry.refresh, refresh);
        assert_eq!(retry.cursor, None);
        retry.reply.send(Ok(page("recovered", None))).unwrap();
        changes.changed().await.unwrap();
        assert_eq!(service.visible_entries()[0].target_id, "recovered");
    });
}

#[test]
fn only_a_complete_manual_refresh_restores_access_to_the_source() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let service = DiscoveryService::shared(&path, &runtime);
    let second_window = DiscoveryService::shared(&path, &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        let mut changes = service.subscribe();
        let mut source = source();
        service.configure(vec![(source.clone(), Active(endpoint.provider.clone()))]);
        let mut other_process = Endpoint::new();
        second_window.configure(vec![(
            source.clone(),
            Active(other_process.provider.clone()),
        )]);
        service.request_directory("inventory", true);
        let first_refresh = endpoint.next_page().await;
        let failure = DiscoveryFailure::Provider(oxideterm_plugin_protocol::PluginError::runtime(
            "sourceAccessLost",
            "Configure your access key",
        ));
        assert!(service.source_action("inventory", "unknown").is_none());
        let (_, task) = second_window
            .source_action("inventory", "configureKey")
            .unwrap();
        let (id, action_reply) = endpoint.actions.recv().await.unwrap();
        assert_eq!(id, "configureKey");
        action_reply.send(Err(failure.clone())).unwrap();
        assert_eq!(task.await.unwrap(), Err(failure.clone()));
        assert!(service.epoch("inventory").is_some());

        first_refresh.reply.send(Err(failure.clone())).unwrap();
        while service.snapshot("inventory").unwrap().failure.as_ref() != Some(&failure) {
            changes.changed().await.unwrap();
        }
        assert_eq!(endpoint.canceled.try_recv().unwrap(), first_refresh.refresh);
        assert!(service.epoch("inventory").is_none());
        let (_, task) = second_window
            .source_action("inventory", "configureKey")
            .unwrap();
        let (id, action_reply) = endpoint.actions.recv().await.unwrap();
        assert_eq!(id, "configureKey");
        action_reply.send(Ok(())).unwrap();
        task.await.unwrap().unwrap();
        assert!(matches!(
            other_process.actions.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        let (receipt, pending_action) = service.source_action("inventory", "configureKey").unwrap();
        let (_, old_action) = endpoint.actions.recv().await.unwrap();
        source.config = serde_json::json!({"root/site": "updated.example"});
        service.configure(vec![(source, Active(endpoint.provider.clone()))]);
        old_action.send(Ok(())).unwrap();
        pending_action.await.unwrap().unwrap();
        assert!(receipt.is_cancelled());
        service.ensure_visible();
        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            endpoint.pages.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        service.request_directory("inventory", true);
        let first = endpoint.next_page().await;
        first.reply.send(Ok(page("current", Some("next")))).unwrap();
        let last = endpoint.next_page().await;
        assert_eq!(last.refresh, first.refresh);
        assert_eq!(last.cursor.as_deref(), Some("next"));
        assert!(!service.can_open("inventory", "current"));
        last.reply.send(Ok(page("other", None))).unwrap();
        while !service.can_open("inventory", "current") {
            changes.changed().await.unwrap();
        }
        assert!(service.can_open("inventory", "current"));
    });
}

#[test]
fn revocation_removes_snapshot_and_late_pages_cannot_restore_open_authority() {
    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let service = DiscoveryService::shared(&dir.path().join("connections.json"), &runtime);
    runtime.block_on(async {
        let mut endpoint = Endpoint::new();
        service.configure(vec![(source(), Active(endpoint.provider.clone()))]);
        let mut changes = service.subscribe();
        service.request_directory("inventory", true);
        changes.changed().await.unwrap();
        endpoint
            .pages
            .recv()
            .await
            .unwrap()
            .reply
            .send(Ok(page("old", None)))
            .unwrap();
        changes.changed().await.unwrap();
        let old_epoch = service.epoch("inventory").unwrap();
        let old_authority = service.authority("inventory", old_epoch).unwrap();
        service.request_directory("inventory", true);
        let stale = endpoint.pages.recv().await.unwrap();
        let denied = DiscoveryFailure::Provider(oxideterm_plugin_protocol::PluginError::runtime(
            "sourceAccessLost",
            "Restore access to this organization",
        ));
        service.report_failure("inventory", old_epoch, denied.clone());
        let _ = stale.reply.send(Ok(page("late", None)));
        tokio::task::yield_now().await;
        assert!(old_authority.is_cancelled());
        assert!(!service.can_open("inventory", "old"));
        assert!(service.visible_entries().is_empty());
        assert_eq!(service.snapshot("inventory").unwrap().failure, Some(denied));
        service.request_directory("inventory", true);
        let verified = endpoint.pages.recv().await.unwrap();
        assert_ne!(verified.refresh, stale.refresh);
        changes.borrow_and_update();
        verified.reply.send(Ok(page("new", None))).unwrap();
        changes.changed().await.unwrap();
        assert!(service.can_open("inventory", "new"));
        assert!(service.acquire("inventory", old_epoch).is_none());
    });
}

#[test]
fn discovery_removal_drains_both_windows_and_resumes_partial_cleanup_after_restart() {
    use oxideterm_settings::SettingsStore;

    let runtime = clock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let settings_path = dir.path().join("settings.json");
    let saved_id = String::from("ordinary");
    std::fs::write(&path, br#"{"discovery":{"sources":{"inventory":{"targets":{"build":{"connection_id":"ordinary"}}}}},"connections":[{"id":"ordinary","name":"build","host":"build.example","username":"ops","auth":{"type":"agent"},"created_at":"2026-01-01T00:00:00Z"}]}"#).unwrap();
    let mut settings = SettingsStore::load_from_path(&settings_path).unwrap();
    let mut inventory = source();
    let other = DiscoverySource {
        id: "other".into(),
        ..source()
    };
    settings.settings_mut().discovery_sources = vec![inventory.clone(), other.clone()];
    settings
        .replace_discovery_sources(settings.settings().discovery_sources.clone())
        .unwrap();
    let mut other_window_settings = SettingsStore::load_from_path(&settings_path).unwrap();
    let first = DiscoveryService::shared(&path, &runtime);
    let second = DiscoveryService::shared(&path, &runtime);
    let endpoint = Endpoint::new();
    first.configure_saved(&settings, |_| Active(endpoint.provider.clone()));
    let epoch = first.epoch("inventory").unwrap();
    let connect = |window: &DiscoveryService| {
        let mut metadata = crate::DiscoveryOrigin::new("inventory".into(), "build".into(), epoch);
        metadata.host_authorized(
            window.acquire("inventory", epoch).unwrap(),
            window.authority("inventory", epoch).unwrap(),
        );
        let transport = metadata.claim_transport_ownership().unwrap();
        (metadata.clone(), transport)
    };
    let (left_metadata, left) = connect(&first);
    let (right_metadata, right) = connect(&second);
    assert!(
        first
            .begin_remove("inventory", &mut settings)
            .unwrap()
            .is_none()
    );
    drop(left);
    assert!(
        first
            .begin_remove("inventory", &mut settings)
            .unwrap()
            .is_none()
    );
    drop(right);

    let backup = settings_path.with_extension("backup");
    std::fs::rename(&settings_path, &backup).unwrap();
    std::fs::create_dir(&settings_path).unwrap();
    assert!(first.begin_remove("inventory", &mut settings).is_err());
    assert_eq!(second.epoch("inventory"), Some(epoch));
    assert!(settings.settings().discovery_sources[0].enabled);
    std::fs::remove_dir(&settings_path).unwrap();
    std::fs::rename(&backup, &settings_path).unwrap();

    let in_progress = second.track_open("inventory", epoch).unwrap();
    inventory.config = serde_json::json!({"scope":"changed"});
    first.configure(vec![
        (inventory, Active(endpoint.provider.clone())),
        (other.clone(), Unavailable),
    ]);
    let mut drained = first
        .begin_remove("inventory", &mut settings)
        .unwrap()
        .unwrap();
    drop((left_metadata, right_metadata));
    assert!(second.epoch("inventory").is_none());
    assert!(!drained.has_changed().unwrap());
    drop(in_progress);
    runtime.block_on(async { assert!(drained.changed().await.is_err()) });
    std::fs::rename(&settings_path, &backup).unwrap();
    std::fs::create_dir(&settings_path).unwrap();
    assert!(first.finish_remove("inventory", &mut settings).is_err());
    std::fs::remove_dir(&settings_path).unwrap();
    std::fs::rename(backup, &settings_path).unwrap();
    other_window_settings.settings_mut().terminal.font_size = 19;
    other_window_settings.save().unwrap();
    drop((first, second));

    let mut settings = SettingsStore::load_from_path(&settings_path).unwrap();
    let restarted = DiscoveryService::shared(&path, &runtime);
    restarted.configure_saved(&settings, |_| Active(endpoint.provider.clone()));
    let pending = restarted.source("inventory").unwrap();
    assert!(pending.removal_pending && !pending.enabled);
    assert!(restarted.epoch("inventory").is_none());
    let drained = restarted
        .begin_remove("inventory", &mut settings)
        .unwrap()
        .unwrap();
    assert!(drained.has_changed().is_err());
    restarted.finish_remove("inventory", &mut settings).unwrap();
    let mut preferences = other_window_settings.settings().clone();
    preferences.terminal.font_size = 20;
    other_window_settings.replace_and_save(preferences).unwrap();
    let persisted = SettingsStore::load_from_path(settings_path).unwrap();
    assert_eq!(persisted.settings().terminal.font_size, 20);
    assert_eq!(persisted.settings().discovery_sources, [other]);
    assert_eq!(
        ConnectionStore::load_read_only(path)
            .unwrap()
            .get(&saved_id)
            .unwrap()
            .host,
        "build.example"
    );
}
