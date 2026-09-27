use super::*;
use crate::{AuditCategory, AuditContext, AuditEvidence, AuditOutcome, AuditSource};
use std::{path::Path, sync::mpsc};
use zeroize::Zeroizing;

struct Keys;
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![37; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

struct PausedFiles {
    entered: mpsc::Sender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

struct PauseFirstFile {
    first: AtomicBool,
    gate: PausedFiles,
}
impl crate::store::RecordingFiles for PauseFirstFile {
    fn write_chunk(&self, path: &Path, header: &[u8], encrypted: &[u8]) -> std::io::Result<()> {
        if !self.first.swap(true, Ordering::AcqRel) {
            crate::store::RecordingFiles::write_chunk(&self.gate, path, header, encrypted)
        } else {
            crate::store::RecordingFiles::write_chunk(
                &crate::store::DurableRecordingFiles,
                path,
                header,
                encrypted,
            )
        }
    }
}

#[test]
fn closed_recording_finishes_while_another_session_keeps_producing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let (entered, waiting) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let service = AuditService::with_recording_files(
        path,
        Keys,
        Arc::new(PauseFirstFile {
            first: AtomicBool::new(false),
            gate: PausedFiles {
                entered,
                resume: Mutex::new(resumed),
            },
        }),
    )
    .unwrap();
    let client = service.client();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(client.set_policy(crate::AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        }))
        .unwrap();
    let busy = AuditContext::new(client.clone(), AuditSource::User)
        .session("local", "busy")
        .recording_sink();
    let payload = vec![b'b'; 32 * 1024];
    busy.record_output(&payload);
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    let mut accepted = payload.len();
    loop {
        let count = busy.try_record_output(&payload).unwrap();
        accepted += count;
        if count != payload.len() {
            break;
        }
    }
    assert!(
        accepted >= 3 * 1024 * 1024 && accepted < 4 * 1024 * 1024,
        "a paused writer must retain the stream quota across pending, queued and writing data"
    );
    assert_eq!(busy.try_record_output(&payload).unwrap(), 0);
    let other_streams = (0..3)
        .map(|index| {
            let sink = AuditContext::new(client.clone(), AuditSource::User)
                .session("local", &format!("other-{index}"))
                .recording_sink();
            while sink.try_record_output(&payload).unwrap() == payload.len() {}
            sink
        })
        .collect::<Vec<_>>();
    let lossy = AuditContext::new(client.clone(), AuditSource::User)
        .session("serial", "idle-after-gap")
        .recording_sink();
    lossy.record_output(&payload);
    let quiet_context =
        AuditContext::new(client.clone(), AuditSource::User).session("local", "finished");
    let quiet = quiet_context.recording_sink();
    assert_eq!(quiet.try_record_output(b"independent-tail").unwrap(), 16);
    quiet.close();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let producer = std::thread::spawn(move || {
        while !stopping.load(Ordering::Acquire) {
            let _ = busy.try_record_output(&payload);
            std::thread::yield_now();
        }
        busy.close();
    });
    resume.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut completed = None;
    while Instant::now() < deadline {
        let list = runtime
            .block_on(client.list_recordings_for_session(
                quiet_context.session_id.clone().unwrap(),
                None,
                10,
            ))
            .unwrap();
        if let Some(recording) = list
            .recordings
            .into_iter()
            .find(|recording| recording.state == crate::RecordingState::Finished)
        {
            completed = Some(recording.id);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    stop.store(true, Ordering::Release);
    producer.join().unwrap();
    drop((other_streams, lossy));
    let id = completed.expect("closed session depended on the busy session becoming idle");
    let page = runtime
        .block_on(client.read_recording_page(id, None, None, 16))
        .unwrap();
    let output = page
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.frames)
        .filter_map(|frame| match &frame.kind {
            crate::StoredRecordingFrameKind::Output(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(output, b"independent-tail");
}
impl crate::store::RecordingFiles for PausedFiles {
    fn write_chunk(
        &self,
        destination: &Path,
        header: &[u8],
        encrypted: &[u8],
    ) -> std::io::Result<()> {
        self.entered.send(()).unwrap();
        self.resume
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        crate::store::RecordingFiles::write_chunk(
            &crate::store::DurableRecordingFiles,
            destination,
            header,
            encrypted,
        )
    }
}

#[test]
fn recording_file_stall_does_not_block_operation_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let (entered, waiting) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let service = AuditService::with_recording_files(
        path.clone(),
        Keys,
        Arc::new(PausedFiles {
            entered,
            resume: Mutex::new(resumed),
        }),
    )
    .unwrap();
    let client = service.client();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(client.set_policy(crate::AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        }))
        .unwrap();
    let context = AuditContext::new(client.clone(), AuditSource::User).session("local", "fixture");
    let sink = context.recording_sink();
    sink.record_output(b"first\r\nsecond\r\n");
    waiting
        .recv_timeout(Duration::from_secs(3))
        .expect("recording did not reach file I/O");
    let operation = context.operation(AuditCategory::File, "file_save", Some("independent-save"));
    let id = operation.id().unwrap().to_owned();
    operation.finish(
        AuditOutcome::Succeeded,
        AuditEvidence::Lifecycle,
        None,
        None,
    );
    let result = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(1),
            client.query(AuditQuery {
                operation_id: Some(id),
                limit: 20,
                ..Default::default()
            }),
        )
        .await
    });
    // Release the real file writer before assertions so a failed regression cleans up.
    resume.send(()).unwrap();
    sink.close();
    let page = result
        .expect("operation query waited for recording file I/O")
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.details.operation.as_ref().unwrap().outcome)
            .collect::<Vec<_>>(),
        [AuditOutcome::Succeeded, AuditOutcome::Started]
    );
    drop(service);
    let store = AuditStore::open(&path, &Keys).unwrap();
    let recordings = store.list_recordings(None, 10).unwrap();
    assert_eq!(
        recordings.recordings[0].state,
        crate::RecordingState::Finished
    );
    let page = store
        .read_recording_page(&recordings.recordings[0].id, None, None, 16)
        .unwrap();
    let output = page
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.frames)
        .filter_map(|frame| match &frame.kind {
            crate::StoredRecordingFrameKind::Output(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(output, b"first\r\nsecond\r\n");
}

#[test]
fn disabling_full_recording_preserves_the_accepted_prefix_and_rejects_new_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let (entered, waiting) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let service = AuditService::with_recording_files(
        path,
        Keys,
        Arc::new(PauseFirstFile {
            first: AtomicBool::new(false),
            gate: PausedFiles {
                entered,
                resume: Mutex::new(resumed),
            },
        }),
    )
    .unwrap();
    let client = service.client();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(client.set_policy(crate::AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        }))
        .unwrap();
    let sink = AuditContext::new(client.clone(), AuditSource::User)
        .session("local", "policy-pressure")
        .recording_sink();
    sink.record_output(b"start\n");
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    let bytes = (0..16 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let accepted = sink.try_record_output(&bytes).unwrap();
    assert!(
        accepted > 0 && accepted < bytes.len(),
        "fixture did not exhaust recording capacity"
    );
    let policy_client = client.clone();
    let disabled = runtime.spawn(async move {
        policy_client
            .set_policy(crate::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .await
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while sink.is_enabled() {
        assert!(
            Instant::now() < deadline,
            "policy flush did not stop new admission"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        sink.try_record_output(b"disabled-private-content"),
        Err(AuditError::Closed)
    );
    resume.send(()).unwrap();
    runtime.block_on(disabled).unwrap().unwrap();
    let list = runtime.block_on(client.list_recordings(None, 10)).unwrap();
    assert_eq!(list.recordings[0].state, crate::RecordingState::Interrupted);
    let mut cursor = None;
    let mut actual = Vec::new();
    loop {
        let page = runtime
            .block_on(client.read_recording_page(list.recordings[0].id.clone(), cursor, None, 16))
            .unwrap();
        for frame in page.chunks.iter().flat_map(|chunk| &chunk.frames) {
            match &frame.kind {
                crate::StoredRecordingFrameKind::Output(bytes) => actual.extend_from_slice(bytes),
                crate::StoredRecordingFrameKind::Gap { .. } => {
                    panic!("accepted content was lost at the policy fence")
                }
                crate::StoredRecordingFrameKind::Resize { .. } => {}
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(&actual[..6], b"start\n");
    assert_eq!(&actual[6..], &bytes[..accepted]);
    assert_eq!(client.health().unrecorded, 0);
}

#[test]
fn chunk_publication_rechecks_policy_and_recording_end_after_file_io() {
    for end_recording in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audit.db");
        let mut store = AuditStore::open(&path, &Keys).unwrap();
        store
            .set_policy(crate::AuditPolicy {
                enabled: true,
                record_output: true,
                ..Default::default()
            })
            .unwrap();
        let id = store
            .create_recording(
                &uuid::Uuid::new_v4().to_string(),
                1,
                &crate::RecordingDetails {
                    session_id: "publication-race".into(),
                    transport_id: None,
                    consumer_id: None,
                    operation_id: None,
                    endpoint: None,
                },
            )
            .unwrap()
            .unwrap();
        let (entered, waiting) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        store.recording_files = Arc::new(PausedFiles {
            entered,
            resume: Mutex::new(resumed),
        });
        let writing_id = id.clone();
        let writer = std::thread::spawn(move || {
            store.append_recording_chunk(
                &writing_id,
                &[crate::RecordingFrame {
                    occurred_at_ms: 2,
                    kind: crate::RecordingFrameKind::Output(b"must-not-be-published"),
                }],
            )
        });
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
        let mut manager = AuditStore::open(&path, &Keys).unwrap();
        if end_recording {
            manager.finish_recording(&id, 3, true).unwrap();
        } else {
            manager
                .set_policy(crate::AuditPolicy {
                    enabled: true,
                    ..Default::default()
                })
                .unwrap();
        }
        resume.send(()).unwrap();
        assert_eq!(writer.join().unwrap(), Err(AuditError::Closed));
        let page = manager.read_recording_page(&id, None, None, 16).unwrap();
        assert!(page.chunks.is_empty(), "uncommitted output became visible");
        assert!(
            !path
                .with_extension("recordings")
                .join(format!("{id}-0000000000000001.chunk"))
                .exists()
        );
        assert_eq!(
            page.state,
            if end_recording {
                crate::RecordingState::Interrupted
            } else {
                crate::RecordingState::InProgress
            }
        );
    }
}

#[test]
fn concurrent_chunk_publication_cannot_overwrite_committed_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys).unwrap();
    store
        .set_policy(crate::AuditPolicy {
            enabled: true,
            record_output: true,
            ..Default::default()
        })
        .unwrap();
    let id = store
        .create_recording(
            &uuid::Uuid::new_v4().to_string(),
            1,
            &crate::RecordingDetails {
                session_id: "concurrent-append".into(),
                transport_id: None,
                consumer_id: None,
                operation_id: None,
                endpoint: None,
            },
        )
        .unwrap()
        .unwrap();
    let (entered, waiting) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    store.recording_files = Arc::new(PausedFiles {
        entered,
        resume: Mutex::new(resumed),
    });
    let writing_id = id.clone();
    let writer = std::thread::spawn(move || {
        store.append_recording_chunk(
            &writing_id,
            &[crate::RecordingFrame {
                occurred_at_ms: 2,
                kind: crate::RecordingFrameKind::Output(b"uncommitted-loser"),
            }],
        )
    });
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    let mut manager = AuditStore::open(&path, &Keys).unwrap();
    manager
        .append_recording_chunk(
            &id,
            &[crate::RecordingFrame {
                occurred_at_ms: 2,
                kind: crate::RecordingFrameKind::Output(b"committed-winner"),
            }],
        )
        .unwrap();
    resume.send(()).unwrap();
    assert_eq!(writer.join().unwrap(), Err(AuditError::Storage));
    let page = manager.read_recording_page(&id, None, None, 16).unwrap();
    assert_eq!(
        page.chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .collect::<Vec<_>>(),
        [1]
    );
    assert!(
        matches!(&page.chunks[0].frames[0].kind, crate::StoredRecordingFrameKind::Output(bytes) if bytes.as_slice() == b"committed-winner")
    );
}
