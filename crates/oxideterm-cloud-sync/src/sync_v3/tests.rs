use super::*;
use crate::secrets::{
    CloudSyncSecretError, CloudSyncSecretProvider, CloudSyncSecretValue, SecretReadMode,
};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct Secrets(std::collections::BTreeMap<String, CloudSyncSecretValue>);

impl CloudSyncSecretProvider for Secrets {
    fn has_hint(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    fn get_secret(
        &mut self,
        key: &str,
        _: SecretReadMode,
    ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError> {
        Ok(self.0.get(key).cloned())
    }
    fn store_secret(&mut self, key: &str, value: Option<&str>) -> Result<(), CloudSyncSecretError> {
        if let Some(value) = value {
            self.0
                .insert(key.into(), zeroize::Zeroizing::new(value.into()));
        } else {
            self.0.remove(key);
        }
        Ok(())
    }
}

fn resource() -> SyncResource {
    SyncResource {
        kind: ResourceKind::Connection,
        id: "connection-1".into(),
    }
}

fn field(group: &str) -> SyncField {
    SyncField {
        resource: resource(),
        group: group.into(),
    }
}

fn values(name: &str, note: &str) -> SyncValues {
    [
        (
            SyncField::presence(resource()),
            FieldValue::encode(&true).unwrap(),
        ),
        (field("name"), FieldValue::encode(&name).unwrap()),
        (field("note"), FieldValue::encode(&note).unwrap()),
    ]
    .into()
}

fn replicas() -> (SyncReplica, SyncReplica, SyncValues, BTreeSet<SyncResource>) {
    let baseline = values("Original", "original note");
    let selected = BTreeSet::from([resource()]);
    let mut left = SyncReplica::new(Uuid::from_u128(1));
    left.capture_local(&SyncValues::new(), &baseline, &selected)
        .unwrap();
    let right = SyncReplica::load(left.snapshot(), Uuid::from_u128(2)).unwrap();
    (left, right, baseline, selected)
}

#[test]
fn independent_changes_and_repeated_delivery_preserve_both_edits() {
    let (mut left, mut right, baseline, selected) = replicas();
    let local = values("Renamed locally", "original note");
    let remote = values("Original", "remote note");
    left.capture_local(&baseline, &local, &selected).unwrap();
    right.capture_local(&baseline, &remote, &selected).unwrap();
    let remote_snapshot = right.snapshot();
    right.merge(left.snapshot()).unwrap();
    left.merge(remote_snapshot.clone()).unwrap();
    left.merge(remote_snapshot).unwrap();
    assert_eq!(
        left.materialize(&local, &selected).unwrap(),
        values("Renamed locally", "remote note")
    );
    assert_eq!(
        right.materialize(&remote, &selected).unwrap(),
        values("Renamed locally", "remote note")
    );
    assert_eq!(left.conflicts().unwrap(), Vec::new());
}

#[test]
fn concurrent_candidates_keep_local_value_until_explicit_resolution() {
    let (mut left, mut right, baseline, selected) = replicas();
    let local = values("Local name", "original note");
    let remote = values("Remote name", "original note");
    left.capture_local(&baseline, &local, &selected).unwrap();
    right.capture_local(&baseline, &remote, &selected).unwrap();
    left.merge(right.snapshot()).unwrap();
    right.merge(left.snapshot()).unwrap();
    assert_eq!(left.materialize(&local, &selected).unwrap(), local);
    assert_eq!(right.materialize(&remote, &selected).unwrap(), remote);
    let conflicts = left.conflicts().unwrap();
    assert_eq!(
        conflicts
            .iter()
            .map(|conflict| conflict.field.clone())
            .collect::<Vec<_>>(),
        vec![field("name")]
    );
    let candidates = conflicts[0]
        .candidates
        .iter()
        .map(|candidate| {
            left.candidate_value(candidate)
                .unwrap()
                .decode::<String>()
                .unwrap()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        candidates,
        BTreeSet::from(["Local name".to_string(), "Remote name".to_string()])
    );
    left.resolve(
        &conflicts[0],
        Some(&FieldValue::encode(&"Chosen name").unwrap()),
    )
    .unwrap();
    assert!(
        left.resolve(
            &conflicts[0],
            Some(&FieldValue::encode(&"Stale choice").unwrap())
        )
        .is_err()
    );
    right.merge(left.snapshot()).unwrap();
    assert_eq!(
        right.materialize(&remote, &selected).unwrap(),
        values("Chosen name", "original note")
    );
    assert_eq!(right.conflicts().unwrap(), Vec::new());
}

#[test]
fn deselection_does_not_delete_and_concurrent_delete_edit_remains_pending() {
    let (mut left, mut right, baseline, selected) = replicas();
    left.capture_local(&baseline, &SyncValues::new(), &BTreeSet::new())
        .unwrap();
    assert_eq!(left.materialize(&baseline, &selected).unwrap(), baseline);
    left.capture_local(&baseline, &SyncValues::new(), &selected)
        .unwrap();
    let remote = values("Original", "edited while other device deletes");
    right.capture_local(&baseline, &remote, &selected).unwrap();
    left.merge(right.snapshot()).unwrap();
    right.merge(left.snapshot()).unwrap();
    assert_eq!(
        left.materialize(&SyncValues::new(), &selected).unwrap(),
        SyncValues::new()
    );
    assert_eq!(right.materialize(&remote, &selected).unwrap(), remote);
    let conflicts = right.conflicts().unwrap();
    let presence = conflicts
        .iter()
        .find(|conflict| conflict.field.is_presence())
        .unwrap();
    assert_eq!(
        presence
            .candidates
            .iter()
            .map(|candidate| {
                right
                    .candidate_value(candidate)
                    .unwrap()
                    .decode::<bool>()
                    .unwrap()
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([false, true])
    );
    right
        .resolve(presence, Some(&FieldValue::encode(&false).unwrap()))
        .unwrap();
    assert_eq!(
        right.materialize(&remote, &selected).unwrap(),
        SyncValues::new()
    );
    assert_eq!(right.conflicts().unwrap(), Vec::new());
}

#[test]
fn encrypted_replica_reopens_and_retries_the_exact_pending_publication() {
    use oxideterm_connections::oxide_file::{
        OxideBatchDecryptionContext, OxideBatchEncryptionContext, OxideDocumentKind,
        open_oxide_document,
    };

    let path = std::env::temp_dir().join(format!("oxideterm-replica-{}", Uuid::new_v4()));
    let mut secrets = Secrets::default();
    let store = ReplicaStore::open(&path, &mut secrets).unwrap();
    assert!(ReplicaStore::open(&path, &mut secrets).is_err());
    let local = values("Private host", "synthetic-private-value");
    let selected = BTreeSet::from([resource()]);
    let mut record = LocalReplica::new();
    let mut replica = SyncReplica::load(record.snapshot, record.writer).unwrap();
    replica
        .capture_local(&SyncValues::new(), &local, &selected)
        .unwrap();
    record.snapshot = replica.snapshot();
    record.effective = local;
    store
        .bind_sync_password(&mut record, "synthetic-sync-password")
        .unwrap();
    assert!(
        store
            .bind_sync_password(&mut record, "changed-password")
            .is_err()
    );
    assert_eq!(store.load().unwrap().unwrap().next_sequence, 1);
    let context = OxideBatchEncryptionContext::new("synthetic-sync-password").unwrap();
    store.stage_publication(&mut record, &context).unwrap();
    let pending = record.pending.clone().unwrap();
    let id = PublicationId::parse(&pending.path).unwrap();
    assert_eq!(id.sequence, 1);
    assert_eq!(record.next_sequence, 2);
    let disk_bytes = std::fs::read(path.join("replica.oxide")).unwrap();
    assert!(
        !disk_bytes
            .windows(b"synthetic-private-value".len())
            .any(|window| window == b"synthetic-private-value")
    );
    drop(store);
    let moved = path.with_extension("moved");
    std::fs::rename(&path, &moved).unwrap();
    let path = moved;
    let store = ReplicaStore::open(&path, &mut secrets).unwrap();
    let mut reloaded = store.load().unwrap().unwrap();
    assert_eq!(
        reloaded.effective,
        values("Private host", "synthetic-private-value")
    );
    store.stage_publication(&mut reloaded, &context).unwrap();
    assert_eq!(reloaded.pending.as_ref().unwrap().path, pending.path);
    assert_eq!(reloaded.pending.as_ref().unwrap().bytes, pending.bytes);
    assert_eq!(reloaded.next_sequence, 2);
    let mut decryption = OxideBatchDecryptionContext::new("synthetic-sync-password").unwrap();
    let publication = PublishedReplica::decode(&id, &pending.bytes, &mut decryption).unwrap();
    let received = SyncReplica::load(publication.snapshot, Uuid::from_u128(8)).unwrap();
    assert_eq!(
        received.materialize(&SyncValues::new(), &selected).unwrap(),
        values("Private host", "synthetic-private-value")
    );
    assert!(
        open_oxide_document(&pending.bytes, OxideDocumentKind::Archive, &mut decryption).is_err()
    );
    let mut wrong_identity = id.clone();
    wrong_identity.sequence += 1;
    assert!(PublishedReplica::decode(&wrong_identity, &pending.bytes, &mut decryption).is_err());
    assert!(
        store
            .confirm_publication(&mut reloaded, &wrong_identity.path())
            .is_err()
    );
    store
        .confirm_publication(&mut reloaded, &pending.path)
        .unwrap();
    assert!(store.load().unwrap().unwrap().pending.is_none());
    assert!(!reloaded.needs_publication());
    drop(store);
    secrets.0.clear();
    assert!(ReplicaStore::open(&path, &mut secrets).is_err());
    assert!(path.join("replica.oxide").exists());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn domain_mapping_and_recovery_restore_interrupted_writes_then_keep_committed_values() {
    use oxideterm_connections::{
        ConnectionStore, ConnectionStoreData, LocalTerminalProfile, SerialProfile, TelnetProfile,
    };
    use oxideterm_forwarding::{ForwardingRegistry, SavedForwardStore};
    use oxideterm_settings::SettingsStore;
    let directory = std::env::temp_dir().join(format!("oxide-sync-recovery-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let connection_path = directory.join("connections.json");
    let settings_path = directory.join("settings.json");
    let now = chrono::Utc::now();
    let mut data = ConnectionStoreData::default();
    data.local_terminal_profiles.push(LocalTerminalProfile {
        id: "local-1".into(),
        name: "Original terminal".into(),
        group: None,
        icon: None,
        color: None,
        icon_background_color: None,
        shell_id: Some("zsh".into()),
        cwd: Some("/original".into()),
        created_at: now,
        updated_at: now,
        last_used_at: None,
    });
    data.serial_profiles
        .push(SerialProfile::new("Console", "/dev/test"));
    data.telnet_profiles
        .push(TelnetProfile::new("Router", "router.test", 23));
    std::fs::write(&connection_path, serde_json::to_vec(&data).unwrap()).unwrap();
    let mut connections = ConnectionStore::load(&connection_path).unwrap();
    let forwards = ForwardingRegistry::new_with_store(
        SavedForwardStore::load(directory.join("forwards.json")).unwrap(),
    );
    let mut settings = SettingsStore::load_from_path(&settings_path).unwrap();
    let mut secrets = Secrets::default();
    let scope = crate::SyncScope::default();
    let view = ConfigurationView::collect(&connections, &forwards, &settings, &scope).unwrap();
    let mut desired = view.values.clone();
    desired.insert(
        SyncField {
            resource: SyncResource {
                kind: ResourceKind::LocalTerminal,
                id: "local-1".into(),
            },
            group: "name".into(),
        },
        FieldValue::encode(&"Merged terminal").unwrap(),
    );
    let resolved = view.resolved_connections(&desired).unwrap();
    assert_eq!(
        resolved.connections.local_terminal_profiles[0].name,
        "Merged terminal"
    );
    assert_eq!(
        resolved.connections.local_terminal_profiles[0]
            .cwd
            .as_deref(),
        Some("/original")
    );
    assert_eq!(resolved.serial[0].port_path, "/dev/test");
    assert_eq!(resolved.telnet[0].host, "router.test");
    let original = std::fs::read(&connection_path).unwrap();
    let replica_directory = directory.join("cloud-sync-v3/target");
    let store = ReplicaStore::open(&replica_directory, &mut secrets).unwrap();
    let local = LocalReplica::new();
    store.save(&local).unwrap();
    let journal =
        RecoveryJournal::begin(&store, &local, &connections, &forwards, &settings).unwrap();
    let prepared = connections
        .prepare_resolved_configuration(resolved)
        .unwrap();
    let _cleanup = connections
        .commit_prepared_saved_connections_snapshot(prepared)
        .unwrap();
    // Simulate process loss after a domain write, without an in-memory rollback.
    drop(journal);
    drop(connections);
    drop(store);
    let mut connections = ConnectionStore::load(&connection_path).unwrap();
    assert_eq!(
        connections.local_terminal_profiles()[0].name,
        "Merged terminal"
    );
    assert_eq!(
        RecoveryJournal::recover_pending(
            &mut connections,
            &mut settings,
            &directory.join("forwards.json"),
            &mut secrets
        )
        .unwrap(),
        1
    );
    let store = ReplicaStore::open(&replica_directory, &mut secrets).unwrap();
    assert_eq!(std::fs::read(&connection_path).unwrap(), original);
    assert_eq!(
        connections.local_terminal_profiles()[0].name,
        "Original terminal"
    );

    let mut journal =
        RecoveryJournal::begin(&store, &local, &connections, &forwards, &settings).unwrap();
    let prepared = connections
        .prepare_resolved_configuration(view.resolved_connections(&desired).unwrap())
        .unwrap();
    let cleanup = connections
        .commit_prepared_saved_connections_snapshot(prepared)
        .unwrap();
    let mut next = local;
    next.effective = desired;
    journal.commit(&store, &next, Some(cleanup), None).unwrap();
    drop(journal);
    drop(store);
    assert_eq!(
        RecoveryJournal::recover_pending(
            &mut connections,
            &mut settings,
            &directory.join("forwards.json"),
            &mut secrets
        )
        .unwrap(),
        1
    );
    let store = ReplicaStore::open(&replica_directory, &mut secrets).unwrap();
    assert_eq!(
        ConnectionStore::load(&connection_path)
            .unwrap()
            .local_terminal_profiles()[0]
            .name,
        "Merged terminal"
    );
    assert_eq!(store.load().unwrap().unwrap().effective, next.effective);
    assert!(!RecoveryJournal::recover(&store, &mut connections, &forwards, &mut settings).unwrap());
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
