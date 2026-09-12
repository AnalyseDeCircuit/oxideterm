use super::*;
use parking_lot::Mutex;
use tokio::sync::{Semaphore, mpsc, oneshot, watch};

mod encoding;

pub const HISTORY_PENDING_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryWriteState {
    Ready,
    Failed,
    Closed,
}

struct Batch {
    payload: Payload,
    // Admission belongs to the queued write even when its caller is cancelled.
    _admission: tokio::sync::OwnedMutexGuard<()>,
    reply: oneshot::Sender<Result<(), String>>,
}

enum Payload {
    Encoded {
        bytes: zeroize::Zeroizing<Vec<u8>>,
        _permit: tokio::sync::OwnedSemaphorePermit,
    },
    Stream {
        mutations: Arc<Vec<HistoryMutation>>,
        runtime: tokio::runtime::Handle,
    },
}

enum Command {
    Write(Batch),
    Flush(oneshot::Sender<Result<(), String>>),
    Retry(oneshot::Sender<Result<(), String>>),
    Shutdown(oneshot::Sender<Result<(), String>>),
}

pub(super) struct Owner {
    sender: Mutex<Option<mpsc::Sender<Command>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    budget: Arc<Semaphore>,
    admission: Arc<tokio::sync::Mutex<()>>,
    status: watch::Receiver<HistoryWriteState>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.sender.get_mut().take();
        // Explicit shutdown joins off the UI thread. Abandonment closes the mailbox;
        // the writer retains the database until its current transaction finishes.
        self.thread.get_mut().take();
    }
}

#[derive(Clone)]
pub struct HistoryWriter(Arc<Owner>);

impl HistoryWriter {
    pub fn new(store: ConversationStore) -> Result<Self> {
        let writer_slot = store.writer.clone();
        let mut slot = writer_slot.lock();
        if let Some(owner) = slot.upgrade() {
            return Ok(Self(owner));
        }
        let (tx, mut rx) = mpsc::channel(128);
        let (status, changes) = watch::channel(HistoryWriteState::Ready);
        let budget = Arc::new(Semaphore::new(HISTORY_PENDING_BYTES));
        let writer_budget = budget.clone();
        let thread = std::thread::Builder::new()
            .name("ai-history-writer".into())
            .spawn(move || {
                let mut pending = std::collections::VecDeque::<Batch>::new();
                let mut failed = false;
                while let Some(command) = rx.blocking_recv() {
                    let (reply, stop, retry) = match command {
                        Command::Write(batch) => {
                            pending.push_back(batch);
                            (None, false, false)
                        }
                        Command::Flush(reply) => (Some(reply), false, false),
                        Command::Retry(reply) => (Some(reply), false, true),
                        Command::Shutdown(reply) => (Some(reply), true, true),
                    };
                    if retry && failed {
                        failed = store.reopen().is_err();
                    }
                    while !failed {
                        let Some(batch) = pending.front() else {
                            break;
                        };
                        let applied = match &batch.payload {
                            Payload::Encoded { bytes, .. } => store.apply_encoded(bytes.as_slice()),
                            Payload::Stream { mutations, runtime } => {
                                let _runtime = runtime.enter();
                                encoding::Encoder::start(mutations.clone(), writer_budget.clone())
                                    .and_then(|(encoder, reader)| {
                                        let result = store.apply_encoded(reader);
                                        drop(encoder);
                                        result
                                    })
                            }
                        };
                        match applied {
                            Ok(()) => {
                                let batch = pending.pop_front().unwrap();
                                let _ = batch.reply.send(Ok(()));
                            }
                            Err(_) => {
                                // Keep the exact failed batch and its byte permit for a storage-only retry.
                                failed = true;
                                status.send_replace(HistoryWriteState::Failed);
                            }
                        }
                    }
                    if !failed {
                        status.send_replace(HistoryWriteState::Ready);
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(if failed {
                            Err("History could not be saved".into())
                        } else {
                            Ok(())
                        });
                    }
                    if stop && !failed {
                        break;
                    }
                }
                status.send_replace(HistoryWriteState::Closed);
            })?;
        let owner = Arc::new(Owner {
            sender: Mutex::new(Some(tx)),
            thread: Mutex::new(Some(thread)),
            budget,
            admission: Arc::new(tokio::sync::Mutex::new(())),
            status: changes,
        });
        *slot = Arc::downgrade(&owner);
        Ok(Self(owner))
    }

    pub fn subscribe(&self) -> watch::Receiver<HistoryWriteState> {
        self.0.status.clone()
    }

    pub fn pending_bytes(&self) -> usize {
        HISTORY_PENDING_BYTES - self.0.budget.available_permits()
    }

    pub async fn submit(&self, mut mutations: Vec<HistoryMutation>) -> Result<()> {
        for mutation in &mut mutations {
            mutation.sanitize();
        }
        let admission = self.0.admission.clone().lock_owned().await;
        let mut size = encoding::EncodedSize::default();
        mutations.serialize(&mut rmp_serde::Serializer::new(&mut size))?;
        let payload = if size.0 > HISTORY_PENDING_BYTES {
            Payload::Stream {
                mutations: Arc::new(mutations),
                runtime: tokio::runtime::Handle::current(),
            }
        } else {
            let permit = self
                .0
                .budget
                .clone()
                .acquire_many_owned(size.0 as u32)
                .await?;
            let bytes = zeroize::Zeroizing::new(rmp_serde::to_vec(&mutations)?);
            Payload::Encoded {
                bytes,
                _permit: permit,
            }
        };
        let (reply, result) = oneshot::channel();
        self.sender()?
            .send(Command::Write(Batch {
                payload,
                _admission: admission,
                reply,
            }))
            .await
            .map_err(|_| anyhow!("History writer is closed"))?;
        result
            .await
            .map_err(|_| anyhow!("History write was interrupted"))?
            .map_err(anyhow::Error::msg)
    }

    fn sender(&self) -> Result<mpsc::Sender<Command>> {
        self.0
            .sender
            .lock()
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow!("History writer is closed"))
    }

    pub async fn flush(&self) -> Result<()> {
        self.barrier(false, false).await
    }
    pub async fn retry(&self) -> Result<()> {
        self.barrier(true, false).await
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.retry().await?;
        self.barrier(false, true).await?;
        self.0.sender.lock().take();
        let thread = self.0.thread.lock().take();
        if let Some(thread) = thread {
            tokio::task::spawn_blocking(move || thread.join())
                .await
                .map_err(|_| anyhow!("History shutdown was interrupted"))?
                .map_err(|_| anyhow!("History writer failed"))?;
        }
        Ok(())
    }

    async fn barrier(&self, retry: bool, shutdown: bool) -> Result<()> {
        // Retry must reach the owner while the failed write still owns admission.
        // Other barriers wait for admitted writes, but report a failure promptly.
        let _admission = if retry {
            None
        } else {
            let mut status = self.subscribe();
            let lock = self.0.admission.lock();
            tokio::pin!(lock);
            loop {
                if *status.borrow_and_update() != HistoryWriteState::Ready {
                    return Err(anyhow!("History could not be saved"));
                }
                tokio::select! {
                    guard = &mut lock => break Some(guard),
                    changed = status.changed() => {
                        changed.map_err(|_| anyhow!("History writer is closed"))?;
                    }
                }
            }
        };
        let (reply, result) = oneshot::channel();
        let command = if shutdown {
            Command::Shutdown(reply)
        } else if retry {
            Command::Retry(reply)
        } else {
            Command::Flush(reply)
        };
        self.sender()?
            .send(command)
            .await
            .map_err(|_| anyhow!("History writer is closed"))?;
        result
            .await
            .map_err(|_| anyhow!("History writer stopped before confirming persistence"))?
            .map_err(anyhow::Error::msg)
    }
}
