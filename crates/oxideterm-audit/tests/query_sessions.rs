use oxideterm_audit::*;
use zeroize::Zeroizing;

struct Keys;
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![17; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

#[allow(clippy::too_many_arguments)]
fn event(
    title: &str,
    category: AuditCategory,
    session: &str,
    operation_id: &str,
    phase: AuditPhase,
    time: i64,
    transport: &str,
    parent: Option<&str>,
) -> AuditRecord {
    AuditRecord {
        id: uuid::Uuid::new_v4().to_string(),
        occurred_at_ms: time,
        category,
        severity: AuditSeverity::Info,
        details: AuditDetails {
            title: Zeroizing::new(title.into()),
            detail: Some(Zeroizing::new("secret-command-argument".into())),
            source: Zeroizing::new("user".into()),
            actor: Zeroizing::new("local-operator".into()),
            device: Zeroizing::new("workstation".into()),
            target: Some(Zeroizing::new("remote-host".into())),
            node_id: None,
            connection_id: Some(Zeroizing::new("connection-A".into())),
            remote_account: Some(Zeroizing::new("remote-login".into())),
            operation: Some(OperationDetails {
                id: operation_id.into(),
                parent_id: parent.map(str::to_owned),
                instance_id: "instance-A".into(),
                session_id: Some(session.into()),
                consumer_id: None,
                transport_id: Some(transport.into()),
                protocol: Some("ssh".into()),
                agent_id: None,
                source: AuditSource::User,
                action: "execute".into(),
                outcome: if phase == AuditPhase::Result {
                    AuditOutcome::Succeeded
                } else {
                    AuditOutcome::Started
                },
                evidence: AuditEvidence::Protocol,
                authorization: AuditAuthorization::NotRequired,
                authorization_ref: None,
                duration_ms: None,
                exit_code: None,
                bytes: None,
                phase: Some(phase),
                capture: Some(AuditCapture::Complete),
                recovered_by: None,
            }),
        },
    }
}

#[test]
fn identity_filters_are_exact_and_pagination_finds_the_matching_event() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    store
        .append(&event(
            "wanted-one",
            AuditCategory::Command,
            "session-one",
            "op-one",
            AuditPhase::Result,
            100,
            "transport-one",
            Some("batch-one"),
        ))
        .unwrap();
    store
        .append(&event(
            "other session",
            AuditCategory::Command,
            "session-two",
            "op-two",
            AuditPhase::Result,
            101,
            "transport-two",
            Some("batch-two"),
        ))
        .unwrap();
    store
        .append(&event(
            "wanted-two",
            AuditCategory::Command,
            "session-one",
            "op-three",
            AuditPhase::Result,
            102,
            "transport-one",
            Some("batch-one"),
        ))
        .unwrap();
    let query = AuditQuery {
        protocol: Some("ssh".into()),
        connection_id: Some(Zeroizing::new("connection-A".into())),
        remote_account: Some(Zeroizing::new("remote-login".into())),
        local_account: Some(Zeroizing::new("local-operator".into())),
        parent_id: Some("batch-one".into()),
        session_id: Some("session-one".into()),
        limit: 1,
        ..Default::default()
    };
    let page = store.query(&query).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["wanted-two"]
    );
    let next = store
        .query(&AuditQuery {
            before_sequence: page.next_cursor,
            ..query.clone()
        })
        .unwrap();
    assert_eq!(
        next.records
            .iter()
            .map(|record| record.details.title.as_str())
            .collect::<Vec<_>>(),
        ["wanted-one"]
    );
    assert!(next.next_cursor.is_none());
    for mismatch in [
        AuditQuery {
            protocol: Some("telnet".into()),
            ..query.clone()
        },
        AuditQuery {
            connection_id: Some(Zeroizing::new("connection-B".into())),
            ..query.clone()
        },
        AuditQuery {
            remote_account: Some(Zeroizing::new("other-login".into())),
            ..query.clone()
        },
        AuditQuery {
            local_account: Some(Zeroizing::new("other-operator".into())),
            ..query.clone()
        },
        AuditQuery {
            parent_id: Some("batch-two".into()),
            ..query
        },
    ] {
        assert!(store.query(&mismatch).unwrap().records.is_empty());
    }
    for file in std::fs::read_dir(directory.path()).unwrap() {
        let path = file.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        for secret in [
            "remote-login",
            "local-operator",
            "connection-A",
            "secret-command-argument",
            "session-one",
        ] {
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes())
            );
        }
    }
}

#[test]
fn sessions_count_logical_operations_across_results_and_transports() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    for record in [
        event(
            "same command",
            AuditCategory::Command,
            "session-one",
            "command-one",
            AuditPhase::Start,
            10,
            "transport-one",
            None,
        ),
        event(
            "same command",
            AuditCategory::Command,
            "session-two",
            "command-two",
            AuditPhase::Start,
            11,
            "transport-two",
            None,
        ),
        event(
            "same command",
            AuditCategory::Command,
            "session-one",
            "command-one",
            AuditPhase::Result,
            12,
            "transport-one",
            None,
        ),
        event(
            "file",
            AuditCategory::File,
            "session-one",
            "file-one",
            AuditPhase::Result,
            13,
            "transport-two",
            None,
        ),
        event(
            "same command",
            AuditCategory::Command,
            "session-two",
            "command-two",
            AuditPhase::Result,
            14,
            "transport-two",
            None,
        ),
    ] {
        store.append(&record).unwrap();
    }
    let first = store
        .list_sessions(&AuditSessionQuery {
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.sessions.len(), 1);
    assert_eq!(first.sessions[0].session_id, "session-two");
    assert_eq!(
        (
            first.sessions[0].command_count,
            first.sessions[0].operation_count,
            first.sessions[0].transport_count
        ),
        (1, 1, 1)
    );
    let second = store
        .list_sessions(&AuditSessionQuery {
            before: first.next_cursor,
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second.sessions[0].session_id, "session-one");
    assert_eq!(
        (
            second.sessions[0].started_at_ms,
            second.sessions[0].last_event_at_ms
        ),
        (10, 13)
    );
    assert_eq!(
        (
            second.sessions[0].command_count,
            second.sessions[0].file_count,
            second.sessions[0].operation_count,
            second.sessions[0].transport_count
        ),
        (1, 1, 2, 2)
    );
    assert!(second.next_cursor.is_none());
    let window = store
        .list_sessions(&AuditSessionQuery {
            after_ms: Some(14),
            until_ms: Some(14),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        window
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        ["session-two"]
    );
}

#[test]
fn service_reader_returns_sessions_after_queued_write() {
    let directory = tempfile::tempdir().unwrap();
    AuditStore::open(&directory.path().join("audit.db"), &Keys)
        .unwrap()
        .set_policy(AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let service = AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
    let client = service.client();
    client
        .record(event(
            "queued command",
            AuditCategory::Command,
            "queued-session",
            "queued-operation",
            AuditPhase::Result,
            20,
            "transport",
            None,
        ))
        .unwrap();
    let page = futures::executor::block_on(client.list_sessions(AuditSessionQuery {
        limit: 10,
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        page.sessions
            .iter()
            .map(|session| (session.session_id.as_str(), session.command_count))
            .collect::<Vec<_>>(),
        [("queued-session", 1)]
    );
}

#[test]
fn latest_operation_list_does_not_repeat_start_on_next_page_and_keeps_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = AuditStore::open(&directory.path().join("audit.db"), &Keys).unwrap();
    store
        .set_policy(AuditPolicy {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    for (name, operation, phase, time) in [
        ("one started", "one", AuditPhase::Start, 100),
        ("two started", "two", AuditPhase::Start, 101),
        ("one done", "one", AuditPhase::Result, 102),
        ("two done", "two", AuditPhase::Result, 103),
    ] {
        store
            .append(&event(
                name,
                AuditCategory::Command,
                "session",
                operation,
                phase,
                time,
                "transport",
                None,
            ))
            .unwrap();
    }
    let mut query = AuditQuery {
        latest_only: true,
        limit: 1,
        ..Default::default()
    };
    let first = store.query(&query).unwrap();
    assert_eq!(&*first.records[0].details.title, "two done");
    query.before_sequence = first.next_cursor;
    let second = store.query(&query).unwrap();
    assert_eq!(
        second
            .records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["one done"]
    );
    assert_eq!(second.next_cursor, None);
    query.before_sequence = None;
    query.outcome = Some(AuditOutcome::Started);
    assert!(store.query(&query).unwrap().records.is_empty());
    let mut snapshot = Vec::new();
    store
        .export(
            &AuditQuery {
                latest_only: true,
                before_sequence: Some(4),
                limit: 1,
                ..Default::default()
            },
            AuditExportFormat::Json,
            false,
            &mut snapshot,
        )
        .unwrap();
    let exported: Vec<AuditRecord> = serde_json::from_slice(&snapshot).unwrap();
    assert_eq!(
        exported
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["one done", "two started"]
    );
    let history = store
        .query(&AuditQuery {
            operation_id: Some("one".into()),
            limit: 20,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        history
            .records
            .iter()
            .map(|r| r.details.title.as_str())
            .collect::<Vec<_>>(),
        ["one done", "one started"]
    );
}
