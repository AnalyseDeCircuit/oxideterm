use super::*;
use async_channel::{Receiver, Sender};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    thread::JoinHandle,
};

pub(in crate::service) enum RecordingRequest {
    Frame(RecordingMessage),
    Tick,
    Attach(AuditStore, Sender<Result<(), AuditError>>),
    Flush(Sender<Result<(), AuditError>>),
    RefreshPolicy(Sender<Result<(), AuditError>>),
}

// Each stream retains FIFO order, but a hot stream gets only one batch per
// turn. A management request fences all preceding frames before it is applied.
#[derive(Default)]
struct ReadyRecordings {
    streams: HashMap<u64, VecDeque<RecordingMessage>>,
    order: VecDeque<u64>,
    messages: usize,
}
impl ReadyRecordings {
    fn push(&mut self, message: RecordingMessage) {
        let id = message.sink_id();
        let queue = self.streams.entry(id).or_default();
        if queue.is_empty() {
            self.order.push_back(id);
        }
        queue.push_back(message);
        self.messages += 1;
    }
    fn pop(&mut self) -> Option<RecordingMessage> {
        let id = self.order.pop_front()?;
        let queue = self.streams.get_mut(&id).expect("ready recording stream");
        let message = queue.pop_front();
        if queue.is_empty() {
            self.streams.remove(&id);
        } else {
            self.order.push_back(id);
        }
        self.messages -= 1;
        message
    }
}

pub(in crate::service) struct RecordingWorker {
    runtime: Arc<RecordingRuntime>,
    done: std::sync::mpsc::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl RecordingWorker {
    pub fn start(
        runtime: Arc<RecordingRuntime>,
        receiver: Receiver<RecordingRequest>,
        health: Arc<Mutex<HealthState>>,
        stop_at: Arc<Mutex<Option<Instant>>>,
        tick_pending: Arc<AtomicBool>,
        path: PathBuf,
    ) -> Result<Self, AuditError> {
        let (done_tx, done) = std::sync::mpsc::channel();
        let worker_runtime = runtime.clone();
        let thread = std::thread::Builder::new()
            .name("audit-recording-writer".into())
            .spawn(move || {
                let mut recorder = Recorder::new(worker_runtime.clone(), health.clone());
                let mut store = Err(AuditError::Storage);
                let mut ready = ReadyRecordings::default();
                let mut fence = None;
                loop {
                    if ready.messages == 0 && fence.is_none() {
                        match receiver.recv_blocking() {
                            Ok(RecordingRequest::Frame(message)) => ready.push(message),
                            Ok(request) => fence = Some(request),
                            Err(_) => break,
                        }
                    }
                    // Bound staging by the existing slot budget. New streams can
                    // join between turns; nothing after a policy fence overtakes it.
                    while fence.is_none() && ready.messages < MAX_RECORDING_QUEUE_SLOTS {
                        match receiver.try_recv() {
                            Ok(RecordingRequest::Frame(message)) => ready.push(message),
                            Ok(request) => fence = Some(request),
                            Err(_) => break,
                        }
                    }
                    let request = ready
                        .pop()
                        .map(RecordingRequest::Frame)
                        .or_else(|| fence.take())
                        .expect("received recording work");
                    let stopping = stop_at
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .is_some_and(|deadline| Instant::now() >= deadline);
                    if stopping {
                        match request {
                            RecordingRequest::Frame(message) => {
                                recorder.discard_queued(message);
                                health
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .missed(AuditError::Closed, 1);
                            }
                            RecordingRequest::Attach(_, reply)
                            | RecordingRequest::Flush(reply)
                            | RecordingRequest::RefreshPolicy(reply) => {
                                let _ = reply.try_send(Err(AuditError::Closed));
                            }
                            RecordingRequest::Tick => {}
                        }
                        continue;
                    }
                    match request {
                        RecordingRequest::Attach(writer, reply) => {
                            store = Ok(writer);
                            let writer = store.as_mut().unwrap();
                            let result = writer
                                .recover_abandoned_recordings(&path)
                                .and_then(|_| writer.recover_orphan_recording_files(&path))
                                .and_then(|_| writer.prune_recordings(crate::model::now_ms()));
                            {
                                let mut status = health.lock().unwrap_or_else(|e| e.into_inner());
                                match result {
                                    Ok(count) => status.status.expired += count as u64,
                                    Err(error) => status.status.error = Some(error),
                                }
                            }
                            recorder.policy(&mut store);
                            let _ = reply.try_send(Ok(()));
                        }
                        RecordingRequest::Frame(message) => {
                            recorder.handle_queued(message, &mut store);
                        }
                        RecordingRequest::Tick => {
                            tick_pending.store(false, Ordering::Release);
                            recorder.tick(&mut store);
                            recorder.wake_waiters();
                        }
                        RecordingRequest::Flush(reply) => {
                            recorder.flush_before_policy_change(&mut store);
                            let _ =
                                reply.try_send(store.as_ref().map(|_| ()).map_err(|error| *error));
                        }
                        RecordingRequest::RefreshPolicy(reply) => {
                            recorder.policy(&mut store);
                            let _ =
                                reply.try_send(store.as_ref().map(|_| ()).map_err(|error| *error));
                        }
                    }
                }
                worker_runtime.enabled.store(false, Ordering::Release);
                recorder.wake_waiters();
                recorder.shutdown(&mut store);
                // The recording connection retains the shared instance lease through its final commit.
                drop(store);
                let _ = done_tx.send(());
            })
            .map_err(|_| AuditError::Storage)?;
        Ok(Self {
            runtime,
            done,
            thread: Some(thread),
        })
    }

    pub fn attach(&self, store: &AuditStore, path: &Path) -> Result<(), AuditError> {
        let writer = store.recording_writer(path)?;
        self.barrier(|reply| RecordingRequest::Attach(writer, reply))
    }

    pub fn flush(&self) -> Result<(), AuditError> {
        // Stop admission before the FIFO fence; otherwise a producer can leave
        // a pre-policy tail behind messages arriving after the flush request.
        self.runtime.accepting.store(false, Ordering::Release);
        self.runtime.wake_waiters();
        let sinks = self
            .runtime
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for sink in sinks {
            drop(sink.gate.lock().unwrap_or_else(|e| e.into_inner()));
            sink.budget.cancel_waiter(sink.id);
        }
        let result = self.barrier(RecordingRequest::Flush);
        if result.is_err() {
            self.runtime.accepting.store(true, Ordering::Release);
            self.runtime.wake_waiters();
        }
        result
    }

    pub fn refresh_policy(&self) -> Result<(), AuditError> {
        let result = self.barrier(RecordingRequest::RefreshPolicy);
        self.runtime.accepting.store(true, Ordering::Release);
        self.runtime.wake_waiters();
        result
    }

    fn barrier(
        &self,
        request: impl FnOnce(Sender<Result<(), AuditError>>) -> RecordingRequest,
    ) -> Result<(), AuditError> {
        // Policy transitions retain their FIFO boundary. Neither this caller nor
        // the recording worker holds a database transaction while waiting here.
        let (reply, result) = async_channel::bounded(1);
        self.runtime
            .sender
            .send_blocking(request(reply))
            .map_err(|_| AuditError::Closed)?;
        result.recv_blocking().map_err(|_| AuditError::Closed)?
    }
}

impl Drop for RecordingWorker {
    fn drop(&mut self) {
        self.runtime.sender.close();
        if self
            .done
            .recv_timeout(super::super::SHUTDOWN_GRACE + Duration::from_millis(600))
            .is_ok()
        {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
