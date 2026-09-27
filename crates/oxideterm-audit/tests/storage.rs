use oxideterm_audit::*;
use std::{path::Path, process::Command};
use zeroize::Zeroizing;

struct Keys(u8);
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![self.0; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

fn enabled_policy() -> AuditPolicy {
    AuditPolicy {
        enabled: true,
        ..Default::default()
    }
}

fn enabled_store(path: &Path, keys: &Keys) -> Result<AuditStore, AuditError> {
    let mut store = AuditStore::open(path, keys)?;
    store.set_policy(enabled_policy())?;
    Ok(store)
}

fn enabled_service(path: std::path::PathBuf, keys: Keys) -> Result<AuditService, AuditError> {
    drop(enabled_store(&path, &keys)?);
    AuditService::with_key_provider(path, keys)
}

struct GatedKeys {
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}
impl AuditKeyProvider for GatedKeys {
    fn load(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.entered
            .send(())
            .map_err(|_| AuditError::KeyUnavailable)?;
        self.resume.recv().map_err(|_| AuditError::KeyUnavailable)?;
        Keys(5).load(id)
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

fn event(title: &str, category: AuditCategory, severity: AuditSeverity) -> AuditRecord {
    AuditRecord::new(
        category,
        severity,
        AuditDetails {
            operation: None,
            title: Zeroizing::new(title.into()),
            detail: Some(Zeroizing::new("token-fixture-secret".into())),
            source: Zeroizing::new("system".into()),
            actor: Zeroizing::new("operator-fixture".into()),
            device: Zeroizing::new("device-fixture".into()),
            target: Some(Zeroizing::new("target-fixture".into())),
            node_id: None,
            connection_id: None,
            remote_account: None,
        },
    )
}

#[test]
fn reopened_history_filters_and_pages_exact_records_without_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = enabled_store(&path, &Keys(7)).unwrap();
    for (title, category, severity) in [
        (
            "connected-one",
            AuditCategory::Connection,
            AuditSeverity::Info,
        ),
        ("retry", AuditCategory::Reconnect, AuditSeverity::Warning),
        (
            "connected-two",
            AuditCategory::Connection,
            AuditSeverity::Info,
        ),
        ("denied", AuditCategory::Connection, AuditSeverity::Error),
    ] {
        store.append(&event(title, category, severity)).unwrap();
    }
    // Inspect database, WAL and any auxiliary files while the connection is live.
    let mut paths = vec![dir.path().to_path_buf()];
    while let Some(path) = paths.pop() {
        if path.is_dir() {
            paths.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        for secret in [
            "token-fixture-secret",
            "operator-fixture",
            "device-fixture",
            "target-fixture",
            "connected-one",
        ] {
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|part| part == secret.as_bytes()),
                "plaintext field persisted"
            );
        }
    }
    drop(store);
    let store = AuditStore::open(&path, &Keys(7)).unwrap();
    let query = AuditQuery {
        category: Some(AuditCategory::Connection),
        severity: Some(AuditSeverity::Info),
        limit: 1,
        ..Default::default()
    };
    let page = store.query(&query).unwrap();
    assert_eq!(&*page.records[0].details.title, "connected-two");
    let next = store
        .query(&AuditQuery {
            before_sequence: page.next_cursor,
            ..query
        })
        .unwrap();
    assert_eq!(
        next.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        vec!["connected-one"]
    );
    assert_eq!(next.next_cursor, None);
    let failed = store
        .query(&AuditQuery {
            severity: Some(AuditSeverity::Error),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        failed
            .records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        vec!["denied"]
    );
}

#[test]
fn tampered_index_and_wrong_key_fail_without_returning_partial_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = enabled_store(&path, &Keys(7)).unwrap();
    store
        .append(&event(
            "original",
            AuditCategory::Connection,
            AuditSeverity::Info,
        ))
        .unwrap();
    drop(store);
    assert!(matches!(
        AuditStore::open(&path, &Keys(8)),
        Err(AuditError::Integrity)
    ));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE audit_events SET severity='error'", [])
        .unwrap();
    drop(db);
    let store = AuditStore::open(&path, &Keys(7)).unwrap();
    assert!(matches!(
        store.query(&AuditQuery {
            limit: 10,
            ..Default::default()
        }),
        Err(AuditError::Integrity)
    ));
}

#[test]
fn retention_removes_only_expired_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = enabled_store(&path, &Keys(7)).unwrap();
    let mut expired = event("expired", AuditCategory::Node, AuditSeverity::Info);
    expired.occurred_at_ms = 1;
    let mut retained = event("retained", AuditCategory::Node, AuditSeverity::Error);
    retained.occurred_at_ms = 100 * 24 * 60 * 60 * 1000;
    store.append(&expired).unwrap();
    store.append(&retained).unwrap();
    assert_eq!(store.prune(retained.occurred_at_ms).unwrap(), 1);
    let page = store
        .query(&AuditQuery {
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        vec!["retained"]
    );
}

fn write_batch(path: &Path, prefix: &str) {
    let mut store = enabled_store(path, &Keys(7)).unwrap();
    for i in 0..20 {
        store
            .append(&event(
                &format!("{prefix}-{i}"),
                AuditCategory::Connection,
                AuditSeverity::Info,
            ))
            .unwrap();
    }
}

#[test]
#[ignore = "subprocess fixture invoked by independent_processes_preserve_every_written_event"]
fn child_writer() {
    // This fixture runs in separate OS processes to exercise SQLite locking.
    let path = std::env::var_os("OXIDETERM_AUDIT_TEST_DB").expect("subprocess database path");
    let prefix = std::env::var("OXIDETERM_AUDIT_TEST_PREFIX").unwrap();
    write_batch(Path::new(&path), &prefix);
}

#[test]
fn independent_processes_preserve_every_written_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    drop(enabled_store(&path, &Keys(7)).unwrap());
    let mut children = (0..2)
        .map(|i| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "child_writer", "--ignored", "--nocapture"])
                .env("OXIDETERM_AUDIT_TEST_DB", &path)
                .env("OXIDETERM_AUDIT_TEST_PREFIX", format!("child{i}"))
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    write_batch(&path, "parent");
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let store = AuditStore::open(&path, &Keys(7)).unwrap();
    let mut actual = store
        .query(&AuditQuery {
            limit: 200,
            ..Default::default()
        })
        .unwrap()
        .records
        .into_iter()
        .map(|r| r.details.title.to_string())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = ["parent", "child0", "child1"]
        .into_iter()
        .flat_map(|p| (0..20).map(move |i| format!("{p}-{i}")))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
fn writer_queries_committed_records_and_owner_shutdown_flushes_pending_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let service = enabled_service(path.clone(), Keys(7)).unwrap();
    let client = service.client();
    client
        .record(event(
            "before-query",
            AuditCategory::Connection,
            AuditSeverity::Info,
        ))
        .unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 10,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        vec!["before-query"]
    );
    client
        .record(event(
            "before-close",
            AuditCategory::Node,
            AuditSeverity::Warning,
        ))
        .unwrap();
    drop(service);
    assert!(matches!(
        client.record(event(
            "after-close",
            AuditCategory::Node,
            AuditSeverity::Info
        )),
        Err(AuditError::Closed)
    ));
    let store = AuditStore::open(&path, &Keys(7)).unwrap();
    let page = store
        .query(&AuditQuery {
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        vec!["before-close", "before-query"]
    );
}

#[test]
fn unavailable_storage_reports_failed_capture_instead_of_empty_success() {
    let dir = tempfile::tempdir().unwrap();
    // Opening a directory as the database exercises a real filesystem failure.
    let service = AuditService::with_key_provider(dir.path().to_path_buf(), Keys(7)).unwrap();
    let client = service.client();
    client
        .record(event(
            "not-saved",
            AuditCategory::Node,
            AuditSeverity::Error,
        ))
        .unwrap();
    assert!(matches!(
        futures::executor::block_on(client.query(AuditQuery {
            limit: 10,
            ..Default::default()
        })),
        Err(AuditError::Storage)
    ));
    assert_eq!(client.health().unrecorded, 1);
    assert_eq!(client.health().error, Some(AuditError::Storage));
}

#[test]
fn operations_keep_session_identity_results_and_redacted_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let service = enabled_service(path.clone(), Keys(9)).unwrap();
    let mut context = AuditContext::new(service.client(), AuditSource::User);
    context.session_id = Some("terminal-one".into());
    let first = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("curl --token 'private-short-token' /"),
    );
    let first_id = first.id().unwrap().to_string();
    first.finish(
        AuditOutcome::Failed,
        AuditEvidence::ShellIntegration,
        Some(7),
        None,
    );
    context.session_id = Some("terminal-two".into());
    drop(context.operation(AuditCategory::Command, "command_execute", Some("pwd")));
    let page = futures::executor::block_on(service.client().query(AuditQuery {
        category: Some(AuditCategory::Command),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    let observed = page
        .records
        .iter()
        .map(|r| {
            let op = r.details.operation.as_ref().unwrap();
            (
                op.session_id.as_deref().unwrap(),
                op.outcome,
                op.exit_code,
                r.details.detail.as_ref().unwrap().as_str(),
                op.capture,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        vec![
            (
                "terminal-two",
                AuditOutcome::Unknown,
                None,
                "pwd",
                Some(AuditCapture::Interrupted)
            ),
            (
                "terminal-two",
                AuditOutcome::Started,
                None,
                "pwd",
                Some(AuditCapture::Complete)
            ),
            (
                "terminal-one",
                AuditOutcome::Failed,
                Some(7),
                "curl --token [REDACTED]",
                Some(AuditCapture::Complete)
            ),
            (
                "terminal-one",
                AuditOutcome::Started,
                None,
                "curl --token [REDACTED]",
                Some(AuditCapture::Complete)
            ),
        ]
    );
    let related = futures::executor::block_on(service.client().query(AuditQuery {
        operation_id: Some(first_id.clone()),
        outcome: Some(AuditOutcome::Failed),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        related
            .records
            .iter()
            .map(|r| r.details.operation.as_ref().unwrap().id.as_str())
            .collect::<Vec<_>>(),
        vec![first_id]
    );
    drop(context);
    drop(service);
    let store = AuditStore::open(&path, &Keys(9)).unwrap();
    let mut exported = Vec::new();
    store
        .export(
            &AuditQuery {
                limit: 20,
                ..Default::default()
            },
            AuditExportFormat::Json,
            true,
            &mut exported,
        )
        .unwrap();
    assert!(
        !String::from_utf8(exported.clone())
            .unwrap()
            .contains("private-short-token")
    );
    let records: Vec<AuditRecord> = serde_json::from_slice(&exported).unwrap();
    assert_eq!(
        records[2].details.operation.as_ref().unwrap().exit_code,
        Some(7)
    );
}

#[test]
fn protected_search_paginates_matches_and_export_neutralizes_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = enabled_store(&dir.path().join("audit.db"), &Keys(3)).unwrap();
    for title in ["match-old", "excluded", "  =match-new", "excluded-new"] {
        store
            .append(&event(title, AuditCategory::File, AuditSeverity::Info))
            .unwrap();
    }
    let mut query = AuditQuery {
        search: Some(Zeroizing::new("match".into())),
        limit: 1,
        ..Default::default()
    };
    let page = store.query(&query).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["  =match-new"]
    );
    query.before_sequence = page.next_cursor;
    let page = store.query(&query).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["match-old"]
    );
    assert_eq!(page.next_cursor, None);
    query.before_sequence = None;
    let mut csv = Vec::new();
    store
        .export(&query, AuditExportFormat::Csv, false, &mut csv)
        .unwrap();
    let csv = String::from_utf8(csv).unwrap();
    assert!(csv.contains("\"'  =match-new\""));
    assert!(!csv.contains("excluded"));
    assert!(!csv.contains("token-fixture-secret"));
}

#[test]
fn first_use_is_opt_in_and_reopening_preserves_choice_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(4)).unwrap();
    let mut policy = store.policy().unwrap();
    assert!(!policy.enabled);
    assert!(!policy.record_output);
    assert!(
        !store
            .append(&event(
                "before-opt-in",
                AuditCategory::Command,
                AuditSeverity::Info
            ))
            .unwrap()
    );
    policy.enabled = true;
    store.set_policy(policy).unwrap();
    assert!(
        store
            .append(&event(
                "after-opt-in",
                AuditCategory::Command,
                AuditSeverity::Info
            ))
            .unwrap()
    );
    drop(store);

    let mut store = AuditStore::open(&path, &Keys(4)).unwrap();
    assert_eq!(store.policy().unwrap(), policy);
    policy.enabled = false;
    store.set_policy(policy).unwrap();
    assert!(
        !store
            .append(&event(
                "after-opt-out",
                AuditCategory::Command,
                AuditSeverity::Info
            ))
            .unwrap()
    );
    drop(store);

    let store = AuditStore::open(&path, &Keys(4)).unwrap();
    assert_eq!(store.policy().unwrap(), policy);
    let page = store.query(&AuditQuery::default()).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["after-opt-in"]
    );
}

#[test]
fn policy_disable_keeps_management_events_and_applies_retention() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = AuditStore::open(&dir.path().join("audit.db"), &Keys(4)).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: false,
            retention_days: 1,
            ..Default::default()
        })
        .unwrap();
    store
        .append(&event(
            "suppressed",
            AuditCategory::Command,
            AuditSeverity::Info,
        ))
        .unwrap();
    store
        .append(&event(
            "disabled",
            AuditCategory::Audit,
            AuditSeverity::Info,
        ))
        .unwrap();
    let records = store
        .query(&AuditQuery {
            limit: 10,
            ..Default::default()
        })
        .unwrap()
        .records;
    assert_eq!(
        records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["disabled"]
    );
    assert_eq!(
        store.prune(records[0].occurred_at_ms + 86_400_001).unwrap(),
        1
    );
    assert_eq!(store.policy().unwrap().retention_days, 1);
}

#[test]
fn authorization_progress_obeys_start_capture_across_policy_changes() {
    futures::executor::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let service =
            AuditService::with_key_provider(dir.path().join("audit.db"), Keys(4)).unwrap();
        let client = service.client();
        let context = AuditContext::new(client.clone(), AuditSource::Ai);

        client
            .set_policy(AuditPolicy {
                enabled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        let mut suppressed = context.operation(AuditCategory::Automation, "suppressed_tool", None);
        suppressed.authorization(
            AuditAuthorization::Pending,
            Some("policy_requires_approval"),
        );
        client.set_policy(enabled_policy()).await.unwrap();
        suppressed.authorization(
            AuditAuthorization::Approved,
            Some("policy_requires_approval"),
        );
        suppressed.finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, None);

        let mut captured = context.operation(AuditCategory::Automation, "captured_tool", None);
        captured.authorization(
            AuditAuthorization::Pending,
            Some("policy_requires_approval"),
        );
        client
            .set_policy(AuditPolicy {
                enabled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        captured.authorization(
            AuditAuthorization::Approved,
            Some("policy_requires_approval"),
        );
        client.set_policy(enabled_policy()).await.unwrap();
        captured.finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, None);

        let page = client
            .query(AuditQuery {
                category: Some(AuditCategory::Automation),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let events = page
            .records
            .iter()
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                (
                    op.action.as_str(),
                    op.phase.unwrap(),
                    op.authorization,
                    op.authorization_ref.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            [
                (
                    "captured_tool",
                    AuditPhase::Result,
                    AuditAuthorization::Approved,
                    Some("policy_requires_approval")
                ),
                (
                    "captured_tool",
                    AuditPhase::Authorization,
                    AuditAuthorization::Pending,
                    Some("policy_requires_approval")
                ),
                (
                    "captured_tool",
                    AuditPhase::Start,
                    AuditAuthorization::Unknown,
                    None
                ),
            ]
        );
    });
}

#[test]
fn synchronous_plugin_dispatch_overrides_and_restores_async_ai_request() {
    futures::executor::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let service = enabled_service(dir.path().join("audit.db"), Keys(6)).unwrap();
        let mut ai = AuditContext::new(service.client(), AuditSource::Ai);
        ai.agent_id = Some(Zeroizing::new("conversation-a".into()));
        ai.parent_id = Some("ai-parent".into());
        let mut plugin = ai.clone();
        plugin.source = AuditSource::Plugin;
        plugin.agent_id = Some(Zeroizing::new("plugin-b".into()));
        plugin.parent_id = Some("plugin-parent".into());
        ai.scope(async {
            AuditContext::with_sync_request(Some(&plugin), || {
                AuditOperation::begin(AuditCategory::Automation, "plugin_nested", None, None)
                    .finish(AuditOutcome::Sent, AuditEvidence::Dispatch, None, None);
                assert_eq!(
                    AuditContext::current_request().unwrap().source,
                    AuditSource::Plugin
                );
            });
            assert_eq!(
                AuditContext::current_request().unwrap().source,
                AuditSource::Ai
            );
            AuditOperation::begin(AuditCategory::Automation, "ai_resumed", None, None).finish(
                AuditOutcome::Succeeded,
                AuditEvidence::Protocol,
                None,
                None,
            );
        })
        .await;
        let page = service
            .client()
            .query(AuditQuery {
                category: Some(AuditCategory::Automation),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        let results = page
            .records
            .iter()
            .filter_map(|record| {
                let op = record.details.operation.as_ref()?;
                (op.phase == Some(AuditPhase::Result)).then_some((
                    op.action.as_str(),
                    op.source,
                    op.agent_id.as_ref().map(|id| id.as_str()),
                    op.parent_id.as_deref(),
                ))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            results,
            [
                (
                    "ai_resumed",
                    AuditSource::Ai,
                    Some("conversation-a"),
                    Some("ai-parent")
                ),
                (
                    "plugin_nested",
                    AuditSource::Plugin,
                    Some("plugin-b"),
                    Some("plugin-parent")
                ),
            ]
        );
    });
}

#[test]
fn file_export_replaces_only_after_success_and_preserves_destination_on_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let destination = dir.path().join("audit.json");
    let service = enabled_service(path.clone(), Keys(8)).unwrap();
    let client = service.client();
    client
        .record(event("saved", AuditCategory::File, AuditSeverity::Info))
        .unwrap();
    std::fs::write(&destination, b"previous export").unwrap();
    let count = futures::executor::block_on(client.export(
        AuditQuery {
            limit: 20,
            ..Default::default()
        },
        destination.clone(),
        AuditExportFormat::Json,
        false,
    ))
    .unwrap();
    assert_eq!(count, 1);
    let records: Vec<AuditRecord> =
        serde_json::from_slice(&std::fs::read(&destination).unwrap()).unwrap();
    assert_eq!(records[0].details.title.as_str(), "saved");
    assert!(records[0].details.detail.is_none());
    let exported = std::fs::read(&destination).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute("UPDATE audit_events SET severity='error'", [])
        .unwrap();
    let result = futures::executor::block_on(client.export(
        AuditQuery {
            limit: 20,
            ..Default::default()
        },
        destination.clone(),
        AuditExportFormat::Json,
        true,
    ));
    assert_eq!(result, Err(AuditError::Integrity));
    assert_eq!(std::fs::read(destination).unwrap(), exported);
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
fn missing_configuration_object_is_not_reported_as_a_completed_change() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(5)).unwrap();
    let context = AuditContext::new(service.client(), AuditSource::Cli);
    let operation = context.operation(
        AuditCategory::Configuration,
        "configuration_delete",
        Some("missing-profile"),
    );
    operation.changed(&Ok::<bool, ()>(false));
    let page = futures::executor::block_on(service.client().query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| (
                record.details.detail.as_ref().unwrap().as_str(),
                record.details.operation.as_ref().unwrap().outcome,
            ))
            .collect::<Vec<_>>(),
        [
            ("missing-profile", AuditOutcome::Unchanged),
            ("missing-profile", AuditOutcome::Started)
        ]
    );
}

#[test]
fn contexts_from_one_writer_share_an_instance_without_merging_operations() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(5)).unwrap();
    let user = AuditContext::new(service.client(), AuditSource::User);
    let agent = AuditContext::new(service.client(), AuditSource::Ai);
    let first = user.operation(AuditCategory::Command, "command_execute", Some("pwd"));
    let second = agent.operation(AuditCategory::Command, "command_execute", Some("pwd"));
    let first_id = first.id().unwrap().to_string();
    let second_id = second.id().unwrap().to_string();
    assert_ne!(first_id, second_id);
    first.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ExitCode,
        Some(0),
        None,
    );
    second.finish(AuditOutcome::Failed, AuditEvidence::ExitCode, Some(1), None);
    let page = futures::executor::block_on(service.client().query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    let operations = page
        .records
        .iter()
        .map(|record| record.details.operation.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        operations
            .iter()
            .map(|op| op.instance_id.as_str())
            .collect::<Vec<_>>(),
        vec![user.instance_id(); 4]
    );
    assert_eq!(
        operations
            .iter()
            .map(|op| (op.id.as_str(), op.source, op.outcome))
            .collect::<Vec<_>>(),
        [
            (second_id.as_str(), AuditSource::Ai, AuditOutcome::Failed),
            (
                first_id.as_str(),
                AuditSource::User,
                AuditOutcome::Succeeded
            ),
            (second_id.as_str(), AuditSource::Ai, AuditOutcome::Started),
            (first_id.as_str(), AuditSource::User, AuditOutcome::Started),
        ]
    );
}

#[test]
#[ignore = "subprocess fixture invoked by recovery_distinguishes_crashed_and_live_writers"]
fn child_live_audit_writer() {
    let database = std::env::var_os("OXIDETERM_AUDIT_TEST_DB").unwrap();
    let ready = std::env::var_os("OXIDETERM_AUDIT_TEST_READY").unwrap();
    let service = enabled_service(database.into(), Keys(7)).unwrap();
    let mut context = AuditContext::new(service.client(), AuditSource::User);
    context.session_id = Some("child-session".into());
    context.target = Some(Zeroizing::new("test-user@crashed-host:22".into()));
    let completed = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("finished-before-crash"),
    );
    let completed_id = completed.id().unwrap().to_string();
    completed.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ShellIntegration,
        Some(0),
        None,
    );
    let pending = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("still-running-at-crash"),
    );
    let pending_id = pending.id().unwrap().to_string();
    // The query is a queue barrier: the parent kills only after the start and
    // completed result have reached SQLite, rather than racing startup delivery.
    futures::executor::block_on(service.client().query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    oxideterm_atomic_file::durable_write(
        Path::new(&ready),
        &serde_json::to_vec(&(context.instance_id(), completed_id, pending_id)).unwrap(),
    )
    .unwrap();
    let mut byte = [0u8; 1];
    std::io::Read::read_exact(&mut std::io::stdin(), &mut byte).unwrap();
    drop(pending);
}

#[test]
fn recovery_distinguishes_crashed_and_live_writers() {
    use std::time::{Duration, Instant};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let ready = directory.path().join("ready.json");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_live_audit_writer",
            "--ignored",
            "--nocapture",
        ])
        .env("OXIDETERM_AUDIT_TEST_DB", &path)
        .env("OXIDETERM_AUDIT_TEST_READY", &ready)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        if child.try_wait().unwrap().is_some() {
            panic!("child exited before committing its audit records");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child did not commit its audit records in time");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let (child_instance, completed_id, pending_id): (String, String, String) =
        serde_json::from_slice(&std::fs::read(&ready).unwrap()).unwrap();
    let live_service = enabled_service(path.clone(), Keys(7)).unwrap();
    let live_context = AuditContext::new(live_service.client(), AuditSource::User);
    let live_operation = live_context.operation(
        AuditCategory::File,
        "file_transfer",
        Some("live-parent-transfer"),
    );
    let live_id = live_operation.id().unwrap().to_string();
    let query = |id: &str| AuditQuery {
        operation_id: Some(id.into()),
        limit: 20,
        ..Default::default()
    };
    let before =
        futures::executor::block_on(live_service.client().query(query(&pending_id))).unwrap();
    let before_outcomes = before
        .records
        .iter()
        .map(|record| record.details.operation.as_ref().unwrap().outcome)
        .collect::<Vec<_>>();
    // Reap before asserting so even a failed liveness check leaves no child process.
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(before_outcomes, [AuditOutcome::Started]);
    let recovering_service = AuditService::with_key_provider(path.clone(), Keys(7)).unwrap();
    let concurrent_service = AuditService::with_key_provider(path.clone(), Keys(7)).unwrap();
    let client = recovering_service.client();
    let concurrent = concurrent_service.client();
    // Both workers start recovery before either barrier is awaited.
    let (first_ready, second_ready) =
        futures::executor::block_on(async { futures::join!(client.policy(), concurrent.policy()) });
    first_ready.unwrap();
    second_ready.unwrap();
    let recovered = futures::executor::block_on(client.query(query(&pending_id))).unwrap();
    assert_eq!(
        recovered
            .records
            .iter()
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (operation.outcome, operation.capture, operation.exit_code)
            })
            .collect::<Vec<_>>(),
        [
            (AuditOutcome::Unknown, Some(AuditCapture::Interrupted), None),
            (AuditOutcome::Started, Some(AuditCapture::Complete), None),
        ]
    );
    let result = recovered.records[0].details.operation.as_ref().unwrap();
    assert_eq!(result.instance_id, child_instance);
    assert!(
        [Some(client.instance_id()), Some(concurrent.instance_id())]
            .contains(&result.recovered_by.as_deref())
    );
    assert_eq!(result.session_id.as_deref(), Some("child-session"));
    assert_eq!(result.duration_ms, None);
    assert_eq!(
        recovered.records[0]
            .details
            .target
            .as_ref()
            .unwrap()
            .as_str(),
        "test-user@crashed-host:22"
    );
    let completed = futures::executor::block_on(client.query(query(&completed_id))).unwrap();
    assert_eq!(
        completed
            .records
            .iter()
            .map(|record| record.details.operation.as_ref().unwrap().outcome)
            .collect::<Vec<_>>(),
        [AuditOutcome::Succeeded, AuditOutcome::Started]
    );
    let live = futures::executor::block_on(client.query(query(&live_id))).unwrap();
    assert_eq!(
        live.records
            .iter()
            .map(|record| record.details.operation.as_ref().unwrap().outcome)
            .collect::<Vec<_>>(),
        [AuditOutcome::Started]
    );
    let repeated = futures::executor::block_on(concurrent.query(query(&pending_id))).unwrap();
    assert_eq!(
        repeated
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        recovered
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>()
    );
    live_operation.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::Protocol,
        None,
        Some(100),
    );
}

#[test]
fn explicit_clear_during_execution_keeps_the_result_but_marks_incomplete_history() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(5)).unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let operation = context.operation(AuditCategory::Command, "command_execute", Some("pwd"));
    futures::executor::block_on(client.clear_before(i64::MAX)).unwrap();
    operation.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ShellIntegration,
        Some(0),
        None,
    );
    let result = futures::executor::block_on(client.query(AuditQuery {
        category: Some(AuditCategory::Command),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        result
            .records
            .iter()
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (operation.outcome, operation.capture, operation.exit_code)
            })
            .collect::<Vec<_>>(),
        [(
            AuditOutcome::Succeeded,
            Some(AuditCapture::Partial),
            Some(0)
        )]
    );
}

#[test]
fn replacing_default_context_does_not_let_the_old_registration_remove_the_new_one() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(5)).unwrap();
    let old = AuditContext::new(service.client(), AuditSource::Application).install();
    let current = AuditContext::new(service.client(), AuditSource::Ai).install();
    drop(old);
    let operation = AuditOperation::begin(
        AuditCategory::File,
        "file_save",
        Some("replacement-context"),
        None,
    );
    operation.result(&Ok::<(), ()>(()));
    let page = futures::executor::block_on(service.client().query(AuditQuery {
        category: Some(AuditCategory::File),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (
                    operation.source,
                    operation.outcome,
                    record.details.target.as_ref().unwrap().as_str(),
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                AuditSource::Ai,
                AuditOutcome::Succeeded,
                "replacement-context"
            ),
            (
                AuditSource::Ai,
                AuditOutcome::Started,
                "replacement-context"
            ),
        ]
    );
    drop(current);
}

#[test]
fn retention_keeps_active_operations_until_their_executor_reports_a_result() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let service = enabled_service(path.clone(), Keys(6)).unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let active = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("long-running"),
    );
    let done = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("already-finished"),
    );
    done.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ExitCode,
        Some(0),
        None,
    );
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    let future = page
        .records
        .iter()
        .map(|record| record.occurred_at_ms)
        .max()
        .unwrap()
        + 100 * 86_400_000;
    let mut store = AuditStore::open(&path, &Keys(6)).unwrap();
    assert_eq!(store.prune(future).unwrap(), 2);
    active.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ExitCode,
        Some(0),
        None,
    );
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (
                    record.details.detail.as_ref().unwrap().as_str(),
                    operation.outcome,
                    operation.capture,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "long-running",
                AuditOutcome::Succeeded,
                Some(AuditCapture::Complete)
            ),
            (
                "long-running",
                AuditOutcome::Started,
                Some(AuditCapture::Complete)
            ),
        ]
    );
}

#[test]
fn missing_pending_payload_reports_corruption_without_looping_during_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let service = enabled_service(path.clone(), Keys(5)).unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let _operation = context.operation(AuditCategory::Command, "command_execute", Some("pending"));
    futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM audit_events;")
        .unwrap();
    drop(db);
    drop(service);
    assert_eq!(client.health().error, Some(AuditError::Integrity));
    let reopened = AuditService::with_key_provider(path, Keys(5)).unwrap();
    assert!(matches!(
        futures::executor::block_on(reopened.client().query(AuditQuery {
            limit: 20,
            ..Default::default()
        })),
        Err(AuditError::Integrity)
    ));
}

#[test]
#[ignore = "manual retention performance measurement; run with --ignored --nocapture"]
fn retained_operation_cleanup_benchmark() {
    let directory = tempfile::tempdir().unwrap();
    let seed = enabled_service(directory.path().join("seed.db"), Keys(5)).unwrap();
    let context = AuditContext::new(seed.client(), AuditSource::User);
    context
        .operation(
            AuditCategory::Command,
            "command_execute",
            Some("benchmark-command"),
        )
        .finish(
            AuditOutcome::Succeeded,
            AuditEvidence::ExitCode,
            Some(0),
            None,
        );
    let mut template = futures::executor::block_on(seed.client().query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap()
    .records
    .remove(0);
    template.occurred_at_ms = 1;
    template.details.operation.as_mut().unwrap().phase = Some(AuditPhase::Observation);
    for run in 0..4 {
        let mut store =
            enabled_store(&directory.path().join(format!("run-{run}.db")), &Keys(5)).unwrap();
        for _ in 0..3000 {
            template.id = uuid::Uuid::new_v4().to_string();
            template.details.operation.as_mut().unwrap().id = uuid::Uuid::new_v4().to_string();
            store.append(&template).unwrap();
        }
        let started = std::time::Instant::now();
        let removed = store.prune(100 * 86_400_000).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(removed, 3000);
        println!(
            "AUDIT_RETENTION_BENCH run={run} rows={removed} elapsed_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        );
    }
}

#[test]
fn queued_enable_does_not_reveal_an_operation_started_while_capture_was_disabled() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(5)).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: false,
            ..Default::default()
        })
        .unwrap();
    drop(store);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let service = AuditService::with_key_provider(
        path,
        GatedKeys {
            entered: entered_tx,
            resume: resume_rx,
        },
    )
    .unwrap();
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let hidden = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("disabled-command"),
    );
    let mut enable = Box::pin(client.set_policy(enabled_policy()));
    let mut poll_context = std::task::Context::from_waker(futures::task::noop_waker_ref());
    assert!(matches!(
        std::future::Future::poll(enable.as_mut(), &mut poll_context),
        std::task::Poll::Pending
    ));
    hidden.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ExitCode,
        Some(0),
        None,
    );
    resume_tx.send(()).unwrap();
    futures::executor::block_on(enable).unwrap();
    context
        .operation(
            AuditCategory::Command,
            "command_execute",
            Some("enabled-command"),
        )
        .finish(
            AuditOutcome::Succeeded,
            AuditEvidence::ExitCode,
            Some(0),
            None,
        );
    let page = futures::executor::block_on(client.query(AuditQuery {
        category: Some(AuditCategory::Command),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| (
                record.details.detail.as_ref().unwrap().as_str(),
                record.details.operation.as_ref().unwrap().outcome
            ))
            .collect::<Vec<_>>(),
        [
            ("enabled-command", AuditOutcome::Succeeded),
            ("enabled-command", AuditOutcome::Started),
        ]
    );
    assert_eq!(client.health().unrecorded, 0);
}

#[test]
fn disabling_capture_during_execution_keeps_only_the_previously_recorded_intent() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(5)).unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let mut operation = context.operation(
        AuditCategory::Command,
        "command_execute",
        Some("visible-before-disable"),
    );
    futures::executor::block_on(client.set_policy(AuditPolicy {
        enabled: false,
        ..Default::default()
    }))
    .unwrap();
    operation.summary("post-disable-result");
    operation.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ExitCode,
        Some(0),
        Some(123),
    );
    futures::executor::block_on(client.set_policy(enabled_policy())).unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert!(
        !serde_json::to_string(&page.records)
            .unwrap()
            .contains("post-disable-result")
    );
    assert_eq!(
        page.records
            .iter()
            .filter(|record| record.category == AuditCategory::Command)
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (
                    record.details.detail.as_ref().unwrap().as_str(),
                    operation.outcome,
                    operation.capture,
                    operation.exit_code,
                    operation.bytes,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "visible-before-disable",
                AuditOutcome::Unknown,
                Some(AuditCapture::Disabled),
                None,
                None
            ),
            (
                "visible-before-disable",
                AuditOutcome::Started,
                Some(AuditCapture::Complete),
                None,
                None
            ),
        ]
    );
}

#[test]
fn recovery_does_not_remove_a_lease_file_before_its_writer_has_registered() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let leases = directory.path().join("audit.db.instances");
    std::fs::create_dir(&leases).unwrap();
    let starting_file = leases.join(format!("{}.lock", uuid::Uuid::new_v4()));
    std::fs::write(&starting_file, []).unwrap();
    let service = AuditService::with_key_provider(path, Keys(5)).unwrap();
    let client = service.client();
    futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    // Verify that the fixture used the live lease directory, not an unrelated folder.
    assert!(
        leases
            .join(format!("{}.lock", client.instance_id()))
            .is_file()
    );
    assert!(starting_file.is_file());
}

#[test]
#[ignore = "manual same-machine search/write latency benchmark"]
fn search_write_latency_benchmark() {
    use std::future::Future;
    use std::task::{Context, Poll};
    use std::time::Instant;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = enabled_store(&path, &Keys(4)).unwrap();
    for index in 0..2000 {
        let mut record = event(
            &format!("fixture-{index}"),
            AuditCategory::File,
            AuditSeverity::Info,
        );
        record.details.detail = Some(Zeroizing::new("fixture-data ".repeat(500)));
        store.append(&record).unwrap();
    }
    drop(store);
    let service = AuditService::with_key_provider(path, Keys(4)).unwrap();
    let client = service.client();
    futures::executor::block_on(client.policy()).unwrap();
    for round in 0..4 {
        let mut query = Box::pin(client.query(AuditQuery {
            search: Some(Zeroizing::new("absent-search-marker".into())),
            limit: 20,
            ..Default::default()
        }));
        let mut task = Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(query.as_mut().poll(&mut task), Poll::Pending));
        let started = Instant::now();
        let record = event(
            &format!("during-search-{round}"),
            AuditCategory::System,
            AuditSeverity::Info,
        );
        client.record(record).unwrap();
        // The policy response fences the preceding write without waiting for another search.
        futures::executor::block_on(client.policy()).unwrap();
        let elapsed = started.elapsed();
        assert!(
            futures::executor::block_on(query)
                .unwrap()
                .records
                .is_empty()
        );
        let page = futures::executor::block_on(client.query(AuditQuery {
            category: Some(AuditCategory::System),
            limit: 1,
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(
            page.records[0].details.title.as_str(),
            format!("during-search-{round}")
        );
        assert_eq!(client.health().unrecorded, 0);
        eprintln!(
            "search/write round {round}: {:.3} ms{}",
            elapsed.as_secs_f64() * 1000.0,
            if round == 0 { " (warm-up)" } else { "" }
        );
    }
}

#[test]
fn export_uses_one_snapshot_while_another_writer_clears_and_appends() {
    use std::io::{self, Write};
    use std::sync::mpsc;
    use std::time::Duration;
    struct PausedOutput {
        bytes: Vec<u8>,
        reached_record: Option<mpsc::Sender<()>>,
        resume: mpsc::Receiver<()>,
    }
    impl Write for PausedOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            // The initial '[' precedes SELECT; pause only once a record is being serialized.
            if !self.bytes.is_empty() {
                if let Some(sender) = self.reached_record.take() {
                    sender.send(()).unwrap();
                    self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut writer = enabled_store(&path, &Keys(6)).unwrap();
    for index in 0..150 {
        writer
            .append(&event(
                &format!("original-{index}"),
                AuditCategory::File,
                AuditSeverity::Info,
            ))
            .unwrap();
    }
    let reader = AuditStore::open(&path, &Keys(6)).unwrap();
    let (reached, ready) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut output = PausedOutput {
            bytes: Vec::new(),
            reached_record: Some(reached),
            resume: resumed,
        };
        let count = reader
            .export(
                &AuditQuery::default(),
                AuditExportFormat::Json,
                false,
                &mut output,
            )
            .unwrap();
        (count, output.bytes)
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    // These must commit while the exporter is held inside its read transaction.
    assert_eq!(writer.clear_before(i64::MAX).unwrap(), 150);
    writer
        .append(&event(
            "after-clear",
            AuditCategory::System,
            AuditSeverity::Info,
        ))
        .unwrap();
    resume.send(()).unwrap();
    let (count, bytes) = worker.join().unwrap();
    let exported: Vec<AuditRecord> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(count, 150);
    assert_eq!(
        exported
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        (0..150)
            .rev()
            .map(|index| format!("original-{index}"))
            .collect::<Vec<_>>()
    );
    let page = writer
        .query(&AuditQuery {
            limit: 200,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["after-clear"]
    );
}

#[test]
fn service_audits_policy_clear_and_export_without_ui_wrappers() {
    futures::executor::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let service = enabled_service(directory.path().join("audit.db"), Keys(2)).unwrap();
        let client = service.client().with_source(AuditSource::User);
        client
            .record(event("to-clear", AuditCategory::File, AuditSeverity::Info))
            .unwrap();
        let disabled = AuditPolicy {
            enabled: false,
            ..Default::default()
        };
        client.set_policy(disabled).await.unwrap();
        client.set_policy(disabled).await.unwrap();
        assert_eq!(
            client
                .set_policy(AuditPolicy {
                    retention_days: 0,
                    ..disabled
                })
                .await,
            Err(AuditError::InvalidPolicy)
        );
        assert_eq!(client.policy().await.unwrap(), disabled);
        let changes = client
            .query(AuditQuery {
                category: Some(AuditCategory::Audit),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            changes
                .records
                .iter()
                .map(|record| {
                    let op = record.details.operation.as_ref().unwrap();
                    (op.action.as_str(), op.source, op.phase, op.outcome)
                })
                .collect::<Vec<_>>(),
            [
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Result),
                    AuditOutcome::Failed
                ),
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Start),
                    AuditOutcome::Started
                ),
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Result),
                    AuditOutcome::Unchanged
                ),
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Start),
                    AuditOutcome::Started
                ),
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Result),
                    AuditOutcome::Succeeded
                ),
                (
                    "audit_policy",
                    AuditSource::User,
                    Some(AuditPhase::Start),
                    AuditOutcome::Started
                ),
            ]
        );
        let policy_detail: serde_json::Value =
            serde_json::from_str(changes.records[4].details.detail.as_ref().unwrap()).unwrap();
        assert_eq!(policy_detail["before"]["enabled"], true);
        assert_eq!(policy_detail["requested"]["enabled"], false);
        assert_eq!(client.clear_before(i64::MAX).await.unwrap(), 7);
        let cleared = client
            .query(AuditQuery {
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            cleared
                .records
                .iter()
                .map(|record| {
                    let op = record.details.operation.as_ref().unwrap();
                    (op.action.as_str(), op.outcome, op.capture)
                })
                .collect::<Vec<_>>(),
            [
                (
                    "audit_clear",
                    AuditOutcome::Succeeded,
                    Some(AuditCapture::Complete)
                ),
                (
                    "audit_clear",
                    AuditOutcome::Started,
                    Some(AuditCapture::Complete)
                ),
            ]
        );
        assert_eq!(
            cleared.records[0].details.detail.as_ref().unwrap().as_str(),
            "before_ms=9223372036854775807, removed=7"
        );
        assert_eq!(
            cleared.records[0].details.operation.as_ref().unwrap().id,
            cleared.records[1].details.operation.as_ref().unwrap().id
        );
        let cli = client.with_source(AuditSource::Cli);
        let destination = directory.path().join("export.json");
        assert_eq!(
            cli.export(
                AuditQuery::default(),
                destination.clone(),
                AuditExportFormat::Json,
                true
            )
            .await
            .unwrap(),
            2
        );
        let exported: Vec<AuditRecord> =
            serde_json::from_slice(&std::fs::read(destination).unwrap()).unwrap();
        assert_eq!(
            exported.iter().map(|record| &record.id).collect::<Vec<_>>(),
            cleared
                .records
                .iter()
                .map(|record| &record.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            exported[0].details.detail,
            cleared.records[0].details.detail
        );
        assert_eq!(
            cli.export(
                AuditQuery::default(),
                directory.path().join("missing/export.json"),
                AuditExportFormat::Json,
                false
            )
            .await,
            Err(AuditError::Storage)
        );
        let final_page = client
            .query(AuditQuery {
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            final_page
                .records
                .iter()
                .map(|record| {
                    let op = record.details.operation.as_ref().unwrap();
                    (op.action.as_str(), op.source, op.outcome)
                })
                .collect::<Vec<_>>(),
            [
                ("audit_export", AuditSource::Cli, AuditOutcome::Failed),
                ("audit_export", AuditSource::Cli, AuditOutcome::Started),
                ("audit_export", AuditSource::Cli, AuditOutcome::Succeeded),
                ("audit_export", AuditSource::Cli, AuditOutcome::Started),
                ("audit_clear", AuditSource::User, AuditOutcome::Succeeded),
                ("audit_clear", AuditSource::User, AuditOutcome::Started),
            ]
        );
        assert!(
            final_page.records[2]
                .details
                .detail
                .as_ref()
                .unwrap()
                .ends_with("exported_records=2")
        );
        assert_eq!(client.health().unrecorded, 0);
    });
}

#[test]
fn accepted_management_request_records_its_result_after_caller_is_dropped() {
    use std::{
        future::Future,
        task::{Context, Poll},
        time::Duration,
    };
    let directory = tempfile::tempdir().unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let service = AuditService::with_key_provider(
        directory.path().join("audit.db"),
        GatedKeys {
            entered: entered_tx,
            resume: resume_rx,
        },
    )
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let client = service.client();
    let policy = AuditPolicy {
        enabled: false,
        retention_days: 30,
        ..Default::default()
    };
    let mut request = Box::pin(client.set_policy(policy));
    let mut task = Context::from_waker(futures::task::noop_waker_ref());
    assert!(matches!(request.as_mut().poll(&mut task), Poll::Pending));
    drop(request);
    resume_tx.send(()).unwrap();
    assert_eq!(
        futures::executor::block_on(client.policy()).unwrap(),
        policy
    );
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                (op.action.as_str(), op.source, op.outcome, op.capture)
            })
            .collect::<Vec<_>>(),
        [
            (
                "audit_policy",
                AuditSource::Application,
                AuditOutcome::Succeeded,
                Some(AuditCapture::Complete)
            ),
            (
                "audit_policy",
                AuditSource::Application,
                AuditOutcome::Started,
                Some(AuditCapture::Complete)
            ),
        ]
    );
    assert_eq!(
        page.records[0].details.operation.as_ref().unwrap().id,
        page.records[1].details.operation.as_ref().unwrap().id
    );
}

#[test]
fn queue_overflow_is_persisted_as_a_gap_without_retaining_rejected_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    drop(enabled_store(&path, &Keys(5)).unwrap());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let service = AuditService::with_key_provider(
        path.clone(),
        GatedKeys {
            entered: entered_tx,
            resume: resume_rx,
        },
    )
    .unwrap();
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let client = service.client();
    let mut accepted = Vec::new();
    loop {
        let title = format!("queued-{}", accepted.len());
        match client.record(event(&title, AuditCategory::File, AuditSeverity::Info)) {
            Ok(()) => accepted.push(title),
            Err(AuditError::QueueFull) => break,
            Err(error) => panic!("unexpected queue failure: {error}"),
        }
        assert!(accepted.len() <= 1024, "event queue must be bounded");
    }
    for _ in 0..2 {
        assert_eq!(
            client.record(event(
                "rejected-sensitive-content",
                AuditCategory::File,
                AuditSeverity::Info
            )),
            Err(AuditError::QueueFull)
        );
    }
    resume_tx.send(()).unwrap();
    let mut records = Vec::new();
    let mut before_sequence = None;
    loop {
        let page = futures::executor::block_on(client.query(AuditQuery {
            before_sequence,
            limit: 200,
            ..Default::default()
        }))
        .unwrap();
        records.extend(page.records);
        match page.next_cursor {
            Some(cursor) => before_sequence = Some(cursor),
            None => break,
        }
    }
    assert_eq!(
        records
            .iter()
            .filter(|record| record.category == AuditCategory::File)
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        accepted
            .iter()
            .rev()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    let gaps = records
        .iter()
        .filter(|record| record.category == AuditCategory::Audit)
        .collect::<Vec<_>>();
    assert_eq!(
        gaps.iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["event_log.actions.audit_capture_gap"]
    );
    let gap = gaps[0];
    let detail: serde_json::Value =
        serde_json::from_str(gap.details.detail.as_ref().unwrap()).unwrap();
    assert_eq!(detail["unrecorded_records"], 3);
    assert_eq!(detail["first_error"], "audit queue is full");
    let operation = gap.details.operation.as_ref().unwrap();
    assert_eq!(
        (operation.source, operation.outcome, operation.capture),
        (
            AuditSource::System,
            AuditOutcome::Partial,
            Some(AuditCapture::Partial)
        )
    );
    assert_eq!(client.health().unrecorded, 3);
    assert_eq!(client.health().error, None);
    assert!(
        !serde_json::to_string(&records)
            .unwrap()
            .contains("rejected-sensitive-content")
    );
    let gap_id = gap.id.clone();
    drop(service);
    let reopened = AuditService::with_key_provider(path, Keys(5)).unwrap();
    let page = futures::executor::block_on(reopened.client().query(AuditQuery {
        category: Some(AuditCategory::Audit),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        [gap_id.as_str()]
    );
}

#[test]
fn failed_inserts_recover_with_exact_gap_count_and_no_recursive_loss() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let service = enabled_service(path.clone(), Keys(6)).unwrap();
    let client = service.client();
    futures::executor::block_on(client.policy()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_audit_insert BEFORE INSERT ON audit_events BEGIN SELECT RAISE(FAIL, 'fixture insert failure'); END;").unwrap();
    for title in ["lost-one", "lost-two"] {
        client
            .record(event(title, AuditCategory::File, AuditSeverity::Info))
            .unwrap();
        futures::executor::block_on(client.policy()).unwrap();
    }
    assert_eq!(client.health().unrecorded, 2);
    assert_eq!(client.health().error, Some(AuditError::Storage));
    db.execute_batch("DROP TRIGGER reject_audit_insert;")
        .unwrap();
    client
        .record(event(
            "after-recovery",
            AuditCategory::File,
            AuditSeverity::Info,
        ))
        .unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["after-recovery", "event_log.actions.audit_capture_gap"]
    );
    let detail: serde_json::Value =
        serde_json::from_str(page.records[1].details.detail.as_ref().unwrap()).unwrap();
    assert_eq!(detail["unrecorded_records"], 2);
    assert_eq!(detail["last_error"], "audit storage is unavailable");
    assert_eq!(client.health().unrecorded, 2);
    assert_eq!(client.health().error, None);
}

#[test]
fn unavailable_database_reports_the_gap_after_storage_becomes_available() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    std::fs::create_dir(&path).unwrap();
    let service = AuditService::with_key_provider(path.clone(), Keys(7)).unwrap();
    let client = service.client();
    client
        .record(event(
            "could-not-save",
            AuditCategory::File,
            AuditSeverity::Info,
        ))
        .unwrap();
    assert_eq!(
        futures::executor::block_on(client.policy()),
        Err(AuditError::Storage)
    );
    std::fs::remove_dir(&path).unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["event_log.actions.audit_capture_gap"]
    );
    let detail: serde_json::Value =
        serde_json::from_str(page.records[0].details.detail.as_ref().unwrap()).unwrap();
    assert_eq!(detail["unrecorded_records"], 1);
    assert!(client.health().ready);
    assert_eq!(client.health().error, None);
}

#[test]
fn recording_recovery_error_does_not_block_queries_after_database_reopens() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    std::fs::create_dir(&path).unwrap();
    let service = AuditService::with_key_provider(path.clone(), Keys(7)).unwrap();
    let client = service.client();
    assert_eq!(
        futures::executor::block_on(client.policy()),
        Err(AuditError::Storage)
    );
    std::fs::remove_dir(&path).unwrap();
    let mut store = AuditStore::open(&path, &Keys(7)).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        })
        .unwrap();
    store
        .append(&event(
            "readable-event",
            AuditCategory::File,
            AuditSeverity::Info,
        ))
        .unwrap();
    let recording_id = store
        .create_recording(
            &uuid::Uuid::new_v4().to_string(),
            1,
            &RecordingDetails {
                session_id: "abandoned-session".into(),
                transport_id: None,
                consumer_id: None,
                operation_id: None,
                endpoint: None,
            },
        )
        .unwrap()
        .unwrap();
    drop(store);
    let database = rusqlite::Connection::open(&path).unwrap();
    database
        .execute(
            "UPDATE audit_recordings SET state_tag=x'00' WHERE id=?",
            [&recording_id],
        )
        .unwrap();
    drop(database);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let page = runtime
        .block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                client.query(AuditQuery::default()),
            )
            .await
        })
        .expect("recording recovery blocked the audit writer")
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["readable-event"]
    );
    assert_eq!(client.health().error, Some(AuditError::Integrity));
    assert!(matches!(
        runtime.block_on(client.list_recordings(None, 10)),
        Err(AuditError::Integrity)
    ));
}

#[test]
fn interleaved_requests_keep_provenance_and_the_executing_transport_separate() {
    futures::executor::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let service = enabled_service(directory.path().join("audit.db"), Keys(3)).unwrap();
        let mut runtime = AuditContext::new(service.client(), AuditSource::User)
            .session("ssh", "alice@actual.example:22");
        runtime.session_id = Some("physical-session".into());
        runtime.node_id = Some(Zeroizing::new("physical-node".into()));
        runtime.parent_id = Some("physical-open".into());
        runtime.connection_id = Some(Zeroizing::new("connection-actual".into()));
        runtime.transport_id = Some("transport-actual".into());
        runtime.consumer_id = Some("files-consumer".into());
        let mut ai = AuditContext::new(service.client(), AuditSource::Ai)
            .session("ssh", "incorrect-request-target");
        ai.agent_id = Some(Zeroizing::new("conversation-a".into()));
        ai.parent_id = Some("tool-a".into());
        ai.node_id = Some(Zeroizing::new("node-a".into()));
        ai.session_id = Some("logical-a".into());
        let mut mcp = ai.clone();
        mcp.source = AuditSource::Mcp;
        mcp.agent_id = Some(Zeroizing::new("client-b".into()));
        mcp.parent_id = Some("tool-b".into());
        mcp.node_id = Some(Zeroizing::new("node-b".into()));
        mcp.session_id = Some("logical-b".into());
        let (started_a, waiting_a) = futures::channel::oneshot::channel();
        let (resume_a, continued_a) = futures::channel::oneshot::channel();
        let (started_b, waiting_b) = futures::channel::oneshot::channel();
        let (resume_b, continued_b) = futures::channel::oneshot::channel();
        let work_a = ai.scope(async {
            let operation = AuditOperation::in_request(
                Some(&runtime),
                AuditCategory::File,
                "request_a",
                Some("/a"),
            );
            started_a.send(()).unwrap();
            continued_a.await.unwrap();
            operation.finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, None);
        });
        let work_b = mcp.scope(async {
            let operation = AuditOperation::in_request(
                Some(&runtime),
                AuditCategory::File,
                "request_b",
                Some("/b"),
            );
            started_b.send(()).unwrap();
            continued_b.await.unwrap();
            operation.finish(AuditOutcome::Failed, AuditEvidence::Protocol, None, None);
        });
        futures::join!(work_a, work_b, async {
            waiting_a.await.unwrap();
            waiting_b.await.unwrap();
            resume_b.send(()).unwrap();
            resume_a.send(()).unwrap();
        });
        assert!(AuditContext::current_request().is_none());
        AuditOperation::in_request(
            Some(&runtime),
            AuditCategory::File,
            "outside",
            Some("/outside"),
        )
        .finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, None);
        let page = service
            .client()
            .query(AuditQuery {
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let mut results = page
            .records
            .iter()
            .filter_map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                if op.phase != Some(AuditPhase::Result) {
                    return None;
                }
                assert_eq!(
                    record.details.target.as_ref().unwrap().as_str(),
                    "alice@actual.example:22"
                );
                assert_eq!(
                    record.details.connection_id.as_ref().unwrap().as_str(),
                    "connection-actual"
                );
                assert_eq!(op.transport_id.as_deref(), Some("transport-actual"));
                assert_eq!(op.consumer_id.as_deref(), Some("files-consumer"));
                Some((
                    op.action.as_str(),
                    op.source,
                    op.parent_id.as_deref(),
                    op.agent_id.as_ref().map(|s| s.as_str()),
                    record.details.node_id.as_ref().map(|s| s.as_str()),
                    op.session_id.as_deref(),
                    op.outcome,
                ))
            })
            .collect::<Vec<_>>();
        results.sort_by_key(|row| row.0);
        assert_eq!(
            results,
            [
                (
                    "outside",
                    AuditSource::User,
                    Some("physical-open"),
                    None,
                    Some("physical-node"),
                    Some("physical-session"),
                    AuditOutcome::Succeeded
                ),
                (
                    "request_a",
                    AuditSource::Ai,
                    Some("tool-a"),
                    Some("conversation-a"),
                    Some("physical-node"),
                    Some("physical-session"),
                    AuditOutcome::Succeeded
                ),
                (
                    "request_b",
                    AuditSource::Mcp,
                    Some("tool-b"),
                    Some("client-b"),
                    Some("physical-node"),
                    Some("physical-session"),
                    AuditOutcome::Failed
                ),
            ]
        );
    });
}

#[test]
#[ignore = "manual same-machine audit overhead and persistence benchmark with actual small-file writes"]
fn service_small_file_operation_pressure_benchmark() {
    use std::time::Instant;
    for enabled in [false, true] {
        for round in 0..4 {
            let directory = tempfile::tempdir().unwrap();
            let files = directory.path().join("files");
            std::fs::create_dir(&files).unwrap();
            let service =
                AuditService::with_key_provider(directory.path().join("audit.db"), Keys(23))
                    .unwrap();
            let client = service.client();
            let mut policy = futures::executor::block_on(client.policy()).unwrap();
            policy.enabled = enabled;
            futures::executor::block_on(client.set_policy(policy)).unwrap();
            let context =
                AuditContext::new(client.clone(), AuditSource::User).session("local", "fixture");
            let payload = vec![b'x'; 4096];
            let started = Instant::now();
            for index in 0..1000 {
                let path = files.join(format!("small-{index:04}"));
                let operation = context.operation(
                    AuditCategory::File,
                    "file_save",
                    Some(&path.to_string_lossy()),
                );
                let result = std::fs::write(&path, &payload);
                operation.result(&result);
                result.unwrap();
            }
            let work_ms = started.elapsed().as_secs_f64() * 1000.0;
            let mut query = AuditQuery {
                category: Some(AuditCategory::File),
                limit: 200,
                ..Default::default()
            };
            let mut starts = 0;
            let mut results = 0;
            loop {
                let page = futures::executor::block_on(client.query(query.clone())).unwrap();
                for record in &page.records {
                    if let Some(operation) = &record.details.operation {
                        if operation.action == "file_save" {
                            match operation.phase {
                                Some(AuditPhase::Start) => starts += 1,
                                Some(AuditPhase::Result) => results += 1,
                                _ => {}
                            }
                        }
                    }
                }
                query.before_sequence = page.next_cursor;
                if query.before_sequence.is_none() {
                    break;
                }
            }
            let total_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(std::fs::read_dir(&files).unwrap().count(), 1000);
            for index in 0..1000 {
                assert_eq!(
                    std::fs::read(files.join(format!("small-{index:04}"))).unwrap(),
                    payload
                );
            }
            eprintln!(
                "SMALL_FILE_AUDIT enabled={enabled} round={round} files=1000 bytes_per_file=4096 attempted=2000 persisted={} starts={starts} results={results} unrecorded={} work_ms={work_ms:.3} total_ms={total_ms:.3}",
                starts + results,
                client.health().unrecorded
            );
        }
    }
}

#[test]
fn oversized_queue_entry_is_rejected_without_losing_the_following_record() {
    let directory = tempfile::tempdir().unwrap();
    let service = enabled_service(directory.path().join("audit.db"), Keys(41)).unwrap();
    let client = service.client();
    futures::executor::block_on(client.policy()).unwrap();
    let mut oversized = event("oversized", AuditCategory::File, AuditSeverity::Info);
    oversized.details.detail = Some(Zeroizing::new("private-value-not-for-storage".repeat(4096)));
    assert_eq!(client.record(oversized), Err(AuditError::TooLarge));
    client
        .record(event("following", AuditCategory::File, AuditSeverity::Info))
        .unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .filter(|record| record.category == AuditCategory::File)
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["following"]
    );
    let gap = page
        .records
        .iter()
        .find(|record| {
            record
                .details
                .operation
                .as_ref()
                .is_some_and(|operation| operation.action == "audit_capture_gap")
        })
        .unwrap();
    let details: serde_json::Value =
        serde_json::from_str(gap.details.detail.as_deref().unwrap()).unwrap();
    assert_eq!(details["unrecorded_records"], 1);
    assert_eq!(
        details["first_error"],
        "audit record exceeds the size limit"
    );
    assert!(
        !serde_json::to_string(&page.records)
            .unwrap()
            .contains("private-value-not-for-storage")
    );
    assert_eq!(client.health().unrecorded, 1);
}

#[test]
fn queued_batch_keeps_valid_neighbors_when_one_event_has_a_duplicate_id() {
    let directory = tempfile::tempdir().unwrap();
    drop(enabled_store(&directory.path().join("audit.db"), &Keys(5)).unwrap());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let service = AuditService::with_key_provider(
        directory.path().join("audit.db"),
        GatedKeys {
            entered: entered_tx,
            resume: resume_rx,
        },
    )
    .unwrap();
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let client = service.client();
    let first = event("first-valid", AuditCategory::File, AuditSeverity::Info);
    let mut duplicate = first.clone();
    duplicate.details.title = Zeroizing::new("duplicate-rejected".into());
    client.record(first).unwrap();
    client.record(duplicate).unwrap();
    client
        .record(event(
            "last-valid",
            AuditCategory::File,
            AuditSeverity::Info,
        ))
        .unwrap();
    resume_tx.send(()).unwrap();
    let page = futures::executor::block_on(client.query(AuditQuery {
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .filter(|record| record.category == AuditCategory::File)
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["last-valid", "first-valid"]
    );
    assert_eq!(client.health().unrecorded, 1);
    let gap = page
        .records
        .iter()
        .find(|record| {
            record
                .details
                .operation
                .as_ref()
                .is_some_and(|operation| operation.action == "audit_capture_gap")
        })
        .unwrap();
    let detail: serde_json::Value =
        serde_json::from_str(gap.details.detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail["unrecorded_records"], 1);
}

#[test]
fn rejected_start_cannot_reveal_a_disabled_operation_after_enable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(5)).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: false,
            ..Default::default()
        })
        .unwrap();
    drop(store);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let service = AuditService::with_key_provider(
        path,
        GatedKeys {
            entered: entered_tx,
            resume: resume_rx,
        },
    )
    .unwrap();
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let client = service.client();
    for index in 0..=1024 {
        match client.record(event(
            "disabled-fill",
            AuditCategory::File,
            AuditSeverity::Info,
        )) {
            Ok(()) => assert!(index < 1024),
            Err(AuditError::QueueFull) => break,
            Err(error) => panic!("unexpected error: {error}"),
        }
    }
    let context = AuditContext::new(client.clone(), AuditSource::User);
    let operation = context.operation(
        AuditCategory::Command,
        "disabled-command",
        Some("private-command-during-disabled-window"),
    );
    resume_tx.send(()).unwrap();
    futures::executor::block_on(client.set_policy(enabled_policy())).unwrap();
    operation.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::ShellIntegration,
        Some(0),
        None,
    );
    context
        .operation(AuditCategory::Command, "enabled-command", Some("pwd"))
        .finish(
            AuditOutcome::Succeeded,
            AuditEvidence::ShellIntegration,
            Some(0),
            None,
        );
    let page = futures::executor::block_on(client.query(AuditQuery {
        category: Some(AuditCategory::Command),
        limit: 20,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.records
            .iter()
            .filter_map(|record| record.details.operation.as_ref())
            .map(|operation| (
                operation.action.as_str(),
                operation.phase,
                operation.outcome
            ))
            .collect::<Vec<_>>(),
        [
            (
                "enabled-command",
                Some(AuditPhase::Result),
                AuditOutcome::Succeeded
            ),
            (
                "enabled-command",
                Some(AuditPhase::Start),
                AuditOutcome::Started
            ),
        ]
    );
    assert!(
        !serde_json::to_string(&page.records)
            .unwrap()
            .contains("private-command-during-disabled-window")
    );
}
