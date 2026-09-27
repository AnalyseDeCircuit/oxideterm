mod health;
mod management;
use health::{HealthState, flush_gap};
mod reader;
mod recording;
use management::ManagementOperation;
use reader::{AuditReader, ReadRequest};
pub use recording::RecordingSink;
use recording::{RecordingRequest, RecordingRuntime, RecordingWorker};

use crate::AuditKeyProvider;
use crate::{
    AuditError, AuditPage, AuditQuery, AuditRecord, AuditSessionPage, AuditSessionQuery,
    AuditStore, PlatformAuditKeyProvider,
};
use async_channel::{Receiver, Sender, TrySendError};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

// A bounded backlog absorbs short file-operation bursts; recordings use an independent queue.
const QUEUE_CAPACITY: usize = 1024;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct AuditHealth {
    pub ready: bool,
    pub revision: u64,
    pub error: Option<AuditError>,
    pub unrecorded: u64,
    pub expired: u64,
}

use crate::model::PendingRecord;

enum Request {
    Record(PendingRecord),
    Query(AuditQuery, Sender<Result<AuditPage, AuditError>>),
    Sessions(
        AuditSessionQuery,
        Sender<Result<AuditSessionPage, AuditError>>,
    ),
    Policy(Sender<Result<crate::AuditPolicy, AuditError>>),
    SetPolicy(
        crate::AuditPolicy,
        ManagementOperation,
        Sender<Result<(), AuditError>>,
    ),
    Clear(i64, ManagementOperation, Sender<Result<usize, AuditError>>),
    Export(
        AuditQuery,
        PathBuf,
        crate::AuditExportFormat,
        bool,
        ManagementOperation,
        Sender<Result<u64, AuditError>>,
    ),
    ExportCompleted(
        ManagementOperation,
        Result<u64, AuditError>,
        Sender<Result<u64, AuditError>>,
    ),
    RecordingList(
        Option<String>,
        Option<(i64, String)>,
        usize,
        Sender<Result<crate::RecordingListPage, AuditError>>,
    ),
    RecordingPage(
        String,
        Option<i64>,
        Option<i64>,
        usize,
        Sender<Result<crate::RecordingPage, AuditError>>,
    ),
}

#[derive(Clone)]
pub struct AuditClient {
    source: crate::AuditSource,
    instance_id: String,
    sender: Sender<Request>,
    health: Arc<Mutex<HealthState>>,
    recording: Arc<RecordingRuntime>,
}

impl AuditClient {
    pub fn with_source(&self, source: crate::AuditSource) -> Self {
        Self {
            source,
            ..self.clone()
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn record(&self, record: AuditRecord) -> Result<(), AuditError> {
        self.enqueue(PendingRecord {
            record,
            suppressed: None,
            start: false,
        })
    }

    pub(crate) fn record_start(
        &self,
        record: AuditRecord,
    ) -> (Arc<AtomicBool>, Result<(), AuditError>) {
        let suppressed = Arc::new(AtomicBool::new(false));
        let result = self.enqueue(PendingRecord {
            record,
            suppressed: Some(suppressed.clone()),
            start: true,
        });
        if result.is_err() {
            // Without the FIFO start, the writer cannot establish whether capture
            // was enabled. A later policy change must not expose this operation.
            suppressed.store(true, Ordering::Release);
        }
        (suppressed, result)
    }

    pub(crate) fn record_result(
        &self,
        record: AuditRecord,
        suppressed: Arc<AtomicBool>,
    ) -> Result<(), AuditError> {
        if suppressed.load(Ordering::Acquire) {
            return Ok(());
        }
        self.enqueue(PendingRecord {
            record,
            suppressed: Some(suppressed),
            start: false,
        })
    }

    fn enqueue(&self, record: PendingRecord) -> Result<(), AuditError> {
        if record.record.queued_bytes() > crate::store::MAX_RECORD_BYTES {
            self.health
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .missed(AuditError::TooLarge, 1);
            return Err(AuditError::TooLarge);
        }
        self.sender
            .try_send(Request::Record(record))
            .map_err(|error| {
                let error = match error {
                    TrySendError::Full(_) => AuditError::QueueFull,
                    TrySendError::Closed(_) => AuditError::Closed,
                };
                let mut health = self.health.lock().unwrap_or_else(|e| e.into_inner());
                health.missed(error, 1);
                error
            })
    }

    pub fn health(&self) -> AuditHealth {
        self.health.lock().unwrap_or_else(|e| e.into_inner()).status
    }

    pub async fn list_recordings(
        &self,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<crate::RecordingListPage, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::RecordingList(None, before, limit, tx))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn list_recordings_for_session(
        &self,
        session_id: String,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<crate::RecordingListPage, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::RecordingList(Some(session_id), before, limit, tx))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn read_recording_page(
        &self,
        id: String,
        after_sequence: Option<i64>,
        after_ms: Option<i64>,
        limit: usize,
    ) -> Result<crate::RecordingPage, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::RecordingPage(
                id,
                after_sequence,
                after_ms,
                limit,
                tx,
            ))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn query(&self, query: AuditQuery) -> Result<AuditPage, AuditError> {
        let (sender, receiver) = async_channel::bounded(1);
        self.sender
            .send(Request::Query(query, sender))
            .await
            .map_err(|_| AuditError::Closed)?;
        receiver.recv().await.map_err(|_| AuditError::Closed)?
    }
    pub async fn list_sessions(
        &self,
        query: AuditSessionQuery,
    ) -> Result<AuditSessionPage, AuditError> {
        let (sender, receiver) = async_channel::bounded(1);
        self.sender
            .send(Request::Sessions(query, sender))
            .await
            .map_err(|_| AuditError::Closed)?;
        receiver.recv().await.map_err(|_| AuditError::Closed)?
    }
    pub async fn policy(&self) -> Result<crate::AuditPolicy, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::Policy(tx))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn set_policy(&self, policy: crate::AuditPolicy) -> Result<(), AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::SetPolicy(
                policy,
                ManagementOperation::new(self, "audit_policy", None, None),
                tx,
            ))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn clear_before(&self, before: i64) -> Result<usize, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        self.sender
            .send(Request::Clear(
                before,
                ManagementOperation::new(
                    self,
                    "audit_clear",
                    None,
                    Some(&format!("before_ms={before}")),
                ),
                tx,
            ))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }

    pub async fn export(
        &self,
        query: AuditQuery,
        path: PathBuf,
        format: crate::AuditExportFormat,
        details: bool,
    ) -> Result<u64, AuditError> {
        let (tx, rx) = async_channel::bounded(1);
        let operation = ManagementOperation::new(
            self,
            "audit_export",
            Some(&path.to_string_lossy()),
            Some(&management::export_scope(&query, format, details)),
        );
        self.sender
            .send(Request::Export(query, path, format, details, operation, tx))
            .await
            .map_err(|_| AuditError::Closed)?;
        rx.recv().await.map_err(|_| AuditError::Closed)?
    }
}

pub struct AuditService {
    client: AuditClient,
    stop_at: Arc<Mutex<Option<Instant>>>,
    done: std::sync::mpsc::Receiver<()>,
    worker: Option<JoinHandle<()>>,
    ticker: Option<JoinHandle<()>>,
    tick_stop: Arc<AtomicBool>,
}

impl AuditService {
    pub fn start(path: PathBuf) -> Result<Self, AuditError> {
        Self::with_key_provider(path, PlatformAuditKeyProvider)
    }

    pub fn with_key_provider(
        path: PathBuf,
        keys: impl AuditKeyProvider + Send + 'static,
    ) -> Result<Self, AuditError> {
        Self::with_recording_files(path, keys, Arc::new(crate::store::DurableRecordingFiles))
    }

    /// Supplies the encrypted-file writer while retaining SQLite publication,
    /// policy boundaries and worker ownership inside the audit service.
    pub fn with_recording_files(
        path: PathBuf,
        keys: impl AuditKeyProvider + Send + 'static,
        files: Arc<dyn crate::store::RecordingFiles>,
    ) -> Result<Self, AuditError> {
        let (sender, receiver) = async_channel::bounded(QUEUE_CAPACITY);
        let health = Arc::new(Mutex::new(HealthState::default()));
        let (recording, recording_receiver) = RecordingRuntime::new();
        let recording = Arc::new(recording);
        let client = AuditClient {
            source: crate::AuditSource::Application,
            instance_id: uuid::Uuid::new_v4().to_string(),
            sender,
            health: health.clone(),
            recording: recording.clone(),
        };
        let stop_at = Arc::new(Mutex::new(None));
        let stopping = stop_at.clone();
        let (done_tx, done) = std::sync::mpsc::channel();
        let instance_id = client.instance_id.clone();
        let context = crate::AuditContext::new(client.clone(), crate::AuditSource::System);
        let writer_recording = recording.clone();
        let tick_pending = Arc::new(AtomicBool::new(false));
        let writer_tick_pending = tick_pending.clone();
        let worker = std::thread::Builder::new()
            .name("audit-writer".into())
            .spawn(move || {
                run(
                    path,
                    receiver,
                    health,
                    stopping,
                    keys,
                    instance_id,
                    context,
                    writer_recording,
                    writer_tick_pending,
                    recording_receiver,
                    files,
                );
                let _ = done_tx.send(());
            })
            .map_err(|_| AuditError::Storage)?;
        let tick_stop = Arc::new(AtomicBool::new(false));
        let stop_tick = tick_stop.clone();
        let ticker_sender = client.recording.sender.clone();
        let ticker_pending = tick_pending.clone();
        let ticker = std::thread::Builder::new()
            .name("audit-recording-tick".into())
            .spawn(move || {
                while !stop_tick.load(Ordering::Acquire) {
                    std::thread::sleep(recording::FLUSH_INTERVAL);
                    if !ticker_pending.swap(true, Ordering::AcqRel) {
                        if ticker_sender.try_send(RecordingRequest::Tick).is_err() {
                            ticker_pending.store(false, Ordering::Release);
                            if ticker_sender.is_closed() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|_| AuditError::Storage)?;
        Ok(Self {
            client,
            stop_at,
            done,
            worker: Some(worker),
            ticker: Some(ticker),
            tick_stop,
        })
    }

    pub fn client(&self) -> AuditClient {
        self.client.clone()
    }
}

impl Drop for AuditService {
    fn drop(&mut self) {
        // Closing the sender also wakes an idle writer; clones cannot extend its owner lifetime.
        *self.stop_at.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(Instant::now() + SHUTDOWN_GRACE);
        self.tick_stop.store(true, Ordering::Release);
        self.client.sender.close();
        self.client.recording.sender.close();
        self.client.recording.wake_waiters();
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
        if self
            .done
            .recv_timeout(SHUTDOWN_GRACE + Duration::from_millis(600))
            .is_ok()
        {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
        // An OS credential prompt may outlive the grace period. The worker observes
        // the deadline immediately after that call and never retains an app/window owner.
    }
}

fn run(
    path: PathBuf,
    receiver: Receiver<Request>,
    health: Arc<Mutex<HealthState>>,
    stop_at: Arc<Mutex<Option<Instant>>>,
    keys: impl AuditKeyProvider,
    instance_id: String,
    context: crate::AuditContext,
    recording: Arc<RecordingRuntime>,
    tick_pending: Arc<AtomicBool>,
    recording_receiver: Receiver<RecordingRequest>,
    files: Arc<dyn crate::store::RecordingFiles>,
) {
    let mut store = AuditStore::open_owned(&path, &keys, &instance_id);
    if let Ok(store) = &mut store {
        store.recording_files = files.clone();
    }
    {
        let mut health = health.lock().unwrap_or_else(|e| e.into_inner());
        health.status.ready = store.is_ok();
        health.status.error = store.as_ref().err().copied();
    }
    if let Ok(store) = &mut store {
        let result = store.prune(crate::model::now_ms());
        let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(count) => status.status.expired = count as u64,
            Err(error) => status.status.error = Some(error),
        }
    }
    let mut reader = None;
    let recorder = RecordingWorker::start(
        recording,
        recording_receiver,
        health.clone(),
        stop_at.clone(),
        tick_pending,
        path.clone(),
    );
    let attach = recorder
        .as_ref()
        .map_err(|error| *error)
        .and_then(|recorder| {
            store
                .as_ref()
                .map_err(|error| *error)
                .and_then(|store| recorder.attach(store, &path))
        });
    if let Err(error) = attach {
        health
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .error = Some(error);
    }
    let mut writes = 0u32;
    let mut retry_after = Instant::now() + Duration::from_secs(1);
    let mut retry_delay = Duration::from_secs(1);
    let mut next_request = None;
    while let Some(request) = next_request
        .take()
        .or_else(|| receiver.recv_blocking().ok())
    {
        if stop_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            let mut lost = discard_at_shutdown(request);
            while let Ok(pending) = receiver.try_recv() {
                lost += discard_at_shutdown(pending);
            }
            let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
            status.status.error = Some(AuditError::Closed);
            status.missed(AuditError::Closed, lost);
            break;
        }
        if store.is_err()
            && (matches!(&request, Request::Query(..) | Request::Sessions(..))
                || Instant::now() >= retry_after)
        {
            store = AuditStore::open_owned(&path, &keys, &instance_id);
            if let Ok(store) = &mut store {
                store.recording_files = files.clone();
            }
            {
                let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
                status.status.ready = store.is_ok();
                status.status.error = store.as_ref().err().copied();
            }
            retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
            retry_after = Instant::now() + retry_delay;
            if let (Ok(store), Ok(recorder)) = (&store, &recorder) {
                if let Err(error) = recorder.attach(store, &path) {
                    health
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .status
                        .error = Some(error);
                }
            }
        }
        flush_gap(&mut store, &health, &context);
        match request {
            Request::RecordingList(session_id, before, limit, sender) => {
                let recovery = store
                    .as_mut()
                    .map_err(|error| *error)
                    .and_then(|store| store.recover_abandoned_recordings(&path));
                match recovery {
                    Ok(_) => dispatch_read(
                        &mut store,
                        &mut reader,
                        &path,
                        &health,
                        ReadRequest::RecordingList(session_id, before, limit, sender),
                    ),
                    Err(error) => {
                        let _ = sender.try_send(Err(error));
                    }
                }
            }
            Request::RecordingPage(id, after_sequence, after_ms, limit, sender) => dispatch_read(
                &mut store,
                &mut reader,
                &path,
                &health,
                ReadRequest::RecordingPage(id, after_sequence, after_ms, limit, sender),
            ),
            Request::Record(pending) => {
                let mut batch = vec![pending];
                while batch.len() < 256 {
                    match receiver.try_recv() {
                        Ok(Request::Record(pending)) => batch.push(pending),
                        Ok(request) => {
                            next_request = Some(request);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                // A control request is a FIFO barrier: no batch crosses a policy,
                // read fence, clear or export request.
                let results = match &mut store {
                    Ok(store) => store.append_pending_batch(&batch),
                    Err(error) => vec![Err(*error); batch.len()],
                };
                {
                    let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
                    for result in results {
                        match result {
                            Err(error) => status.missed(error, 1),
                            Ok(true) => status.captured(),
                            Ok(false) => {}
                        }
                    }
                }
                let previous = writes / 128;
                writes = writes.wrapping_add(batch.len() as u32);
                if writes / 128 != previous {
                    if let Ok(store) = &mut store {
                        let result = store.prune(crate::model::now_ms());
                        let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
                        match result {
                            Ok(count) => status.status.expired += count as u64,
                            Err(error) => status.status.error = Some(error),
                        }
                    }
                }
            }
            Request::Policy(sender) => {
                let result = store
                    .as_ref()
                    .map_err(|e| *e)
                    .and_then(|store| store.policy());
                let _ = sender.try_send(result);
            }
            Request::SetPolicy(policy, mut operation, sender) => {
                if !policy.record_output || !policy.enabled {
                    if let Ok(recorder) = &recorder {
                        if let Err(error) = recorder.flush() {
                            operation.start(&mut store, &health);
                            operation.finish(
                                &mut store,
                                &health,
                                crate::AuditOutcome::Failed,
                                Some(error),
                                None,
                            );
                            let _ = sender.try_send(Err(error));
                            continue;
                        }
                    }
                }
                let previous = store.as_ref().ok().and_then(|store| store.policy().ok());
                operation.summary(
                    &serde_json::json!({"before": previous, "requested": policy}).to_string(),
                );
                operation.start(&mut store, &health);
                let mut saved = false;
                let mut result = store.as_mut().map_err(|e| *e).and_then(|store| {
                    store.set_policy(policy)?;
                    saved = true;
                    let removed = store.prune(crate::model::now_ms())?;
                    health
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .status
                        .expired += removed as u64;
                    Ok(())
                });
                if saved || !policy.enabled || !policy.record_output {
                    if let Err(error) = recorder
                        .as_ref()
                        .map_err(|error| *error)
                        .and_then(RecordingWorker::refresh_policy)
                    {
                        health
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .status
                            .error = Some(error);
                        result = result.and(Err(error));
                    }
                }
                let outcome = match &result {
                    Ok(()) if previous == Some(policy) => crate::AuditOutcome::Unchanged,
                    Ok(()) => crate::AuditOutcome::Succeeded,
                    Err(_) if saved => crate::AuditOutcome::Partial,
                    Err(_) => crate::AuditOutcome::Failed,
                };
                operation.finish(
                    &mut store,
                    &health,
                    outcome,
                    result.as_ref().err().copied(),
                    None,
                );
                let _ = sender.try_send(result);
            }
            Request::Clear(before, mut operation, sender) => {
                operation.start(&mut store, &health);
                let result = store
                    .as_mut()
                    .map_err(|e| *e)
                    .and_then(|store| store.clear_before_except(before, &operation.record.id));
                if let Ok(count) = result {
                    operation.summary(&format!("before_ms={before}, removed={count}"));
                }
                operation.finish(
                    &mut store,
                    &health,
                    if result.is_ok() {
                        crate::AuditOutcome::Succeeded
                    } else {
                        crate::AuditOutcome::Failed
                    },
                    result.as_ref().err().copied(),
                    None,
                );
                let _ = sender.try_send(result);
            }
            Request::Export(mut query, destination, format, details, mut operation, sender) => {
                let boundary = store
                    .as_ref()
                    .map_err(|e| *e)
                    .and_then(|store| store.next_sequence());
                operation.start(&mut store, &health);
                match boundary {
                    Ok(boundary) => {
                        query.before_sequence = Some(
                            query
                                .before_sequence
                                .map_or(boundary, |cursor| cursor.min(boundary)),
                        );
                        dispatch_read(
                            &mut store,
                            &mut reader,
                            &path,
                            &health,
                            ReadRequest::Export(
                                query,
                                destination,
                                format,
                                details,
                                operation,
                                sender,
                            ),
                        );
                    }
                    Err(error) => finish_export(&mut store, &health, operation, Err(error), sender),
                }
            }
            Request::ExportCompleted(operation, result, sender) => {
                finish_export(&mut store, &health, operation, result, sender);
            }
            Request::Query(query, sender) => {
                let result = match &mut store {
                    Ok(store) => store.recover_abandoned(&path).and_then(|count| {
                        if count > 0 {
                            health
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .status
                                .revision += count as u64;
                        }
                        Ok(())
                    }),
                    Err(error) => Err(*error),
                };
                match result {
                    Ok(()) => dispatch_read(
                        &mut store,
                        &mut reader,
                        &path,
                        &health,
                        ReadRequest::Query(query, sender),
                    ),
                    Err(error) => {
                        let _ = sender.try_send(Err(error));
                    }
                }
            }
            Request::Sessions(query, sender) => {
                let result = match &mut store {
                    Ok(store) => store.recover_abandoned(&path).and_then(|count| {
                        if count > 0 {
                            health
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .status
                                .revision += count as u64;
                        }
                        Ok(())
                    }),
                    Err(error) => Err(*error),
                };
                match result {
                    Ok(()) => dispatch_read(
                        &mut store,
                        &mut reader,
                        &path,
                        &health,
                        ReadRequest::Sessions(query, sender),
                    ),
                    Err(error) => {
                        let _ = sender.try_send(Err(error));
                    }
                }
            }
        }
    }
    drop(reader);
    drop(recorder);
    flush_gap(&mut store, &health, &context);
    if let Ok(store) = &mut store {
        match store.finish_owned_operations() {
            Ok(count) => {
                health
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .status
                    .revision += count as u64
            }
            Err(error) => {
                health
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .status
                    .error = Some(error)
            }
        }
    }
}

fn discard_at_shutdown(request: Request) -> u64 {
    u64::from(matches!(request, Request::Record(_)))
}

fn finish_export(
    store: &mut Result<AuditStore, AuditError>,
    health: &Arc<Mutex<HealthState>>,
    mut operation: ManagementOperation,
    result: Result<u64, AuditError>,
    sender: Sender<Result<u64, AuditError>>,
) {
    if let Ok(count) = result {
        operation.add_summary(&format!("exported_records={count}"));
    }
    let outcome = match result {
        Ok(_) => crate::AuditOutcome::Succeeded,
        Err(AuditError::Closed) => crate::AuditOutcome::Cancelled,
        Err(_) => crate::AuditOutcome::Failed,
    };
    operation.finish(store, health, outcome, result.as_ref().err().copied(), None);
    let _ = sender.try_send(result);
}

fn fail_read(
    store: &mut Result<AuditStore, AuditError>,
    health: &Arc<Mutex<HealthState>>,
    request: ReadRequest,
    error: AuditError,
) {
    match request {
        ReadRequest::RecordingList(_, _, _, sender) => {
            let _ = sender.try_send(Err(error));
        }
        ReadRequest::RecordingPage(_, _, _, _, sender) => {
            let _ = sender.try_send(Err(error));
        }
        ReadRequest::Query(_, sender) => {
            let _ = sender.try_send(Err(error));
        }
        ReadRequest::Sessions(_, sender) => {
            let _ = sender.try_send(Err(error));
        }
        ReadRequest::Export(_, _, _, _, operation, sender) => {
            finish_export(store, health, operation, Err(error), sender)
        }
    }
}

fn dispatch_read(
    store: &mut Result<AuditStore, AuditError>,
    reader: &mut Option<AuditReader>,
    path: &std::path::Path,
    health: &Arc<Mutex<HealthState>>,
    request: ReadRequest,
) {
    if reader.is_none() {
        match store
            .as_ref()
            .map_err(|error| *error)
            .and_then(|store| store.reader(path))
            .and_then(AuditReader::start)
        {
            Ok(worker) => *reader = Some(worker),
            Err(error) => {
                fail_read(store, health, request, error);
                return;
            }
        }
    }
    if let Some(reader) = reader {
        if let Err((request, error)) = reader.send(request) {
            fail_read(store, health, request, error);
        }
    }
}

#[cfg(test)]
mod tests;
