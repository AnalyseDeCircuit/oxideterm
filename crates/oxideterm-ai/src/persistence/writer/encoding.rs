use super::*;
use std::io::{self, Read, Write};

const CHUNK_BYTES: usize = 64 * 1024;

pub(super) struct Chunk {
    bytes: zeroize::Zeroizing<Vec<u8>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

pub(super) struct EncodedReader {
    receiver: mpsc::Receiver<Chunk>,
    current: Option<Chunk>,
    offset: usize,
}

impl Read for EncodedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self
            .current
            .as_ref()
            .is_none_or(|chunk| self.offset == chunk.bytes.len())
        {
            self.current.take();
            self.current = self.receiver.blocking_recv();
            self.offset = 0;
        }
        let Some(chunk) = &self.current else {
            return Ok(0);
        };
        let count = output.len().min(chunk.bytes.len() - self.offset);
        output[..count].copy_from_slice(&chunk.bytes[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

pub(super) struct Encoder {
    cancelled: watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<io::Result<()>>>,
}

impl Encoder {
    pub fn start(
        mutations: Arc<Vec<HistoryMutation>>,
        budget: Arc<Semaphore>,
    ) -> Result<(Self, EncodedReader)> {
        let (sender, receiver) = mpsc::channel(128);
        let (cancelled, cancel) = watch::channel(false);
        let runtime = tokio::runtime::Handle::current();
        let thread = std::thread::Builder::new()
            .name("ai-history-encoder".into())
            .spawn(move || {
                let mut sink = Sink {
                    sender,
                    budget,
                    cancel,
                    runtime,
                    buffer: zeroize::Zeroizing::new(Vec::with_capacity(CHUNK_BYTES)),
                };
                mutations
                    .serialize(&mut rmp_serde::Serializer::new(&mut sink))
                    .map_err(|_| io::Error::other("History encoding failed"))?;
                sink.flush()
            })?;
        Ok((
            Self {
                cancelled,
                thread: Some(thread),
            },
            EncodedReader {
                receiver,
                current: None,
                offset: 0,
            },
        ))
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.cancelled.send_replace(true);
        // Cancellation wakes both the byte-budget and channel waits before joining.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Sink {
    sender: mpsc::Sender<Chunk>,
    budget: Arc<Semaphore>,
    cancel: watch::Receiver<bool>,
    runtime: tokio::runtime::Handle,
    buffer: zeroize::Zeroizing<Vec<u8>>,
}
impl Write for Sink {
    fn write(&mut self, mut input: &[u8]) -> io::Result<usize> {
        let length = input.len();
        while !input.is_empty() {
            if *self.cancel.borrow() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = input.len().min(CHUNK_BYTES - self.buffer.len());
            self.buffer.extend_from_slice(&input[..count]);
            input = &input[count..];
            if self.buffer.len() == CHUNK_BYTES {
                self.flush()?;
            }
        }
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let budget = self.budget.clone();
        let sender = self.sender.clone();
        let cancel = &mut self.cancel;
        let buffer = &mut self.buffer;
        self.runtime.block_on(async {
            if *cancel.borrow() { return Err(io::ErrorKind::Interrupted.into()); }
            let permit = tokio::select! {
                biased;
                _ = cancel.changed() => return Err(io::ErrorKind::Interrupted.into()),
                permit = budget.acquire_many_owned(buffer.len() as u32) => permit.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?,
            };
            let bytes = zeroize::Zeroizing::new(std::mem::take(&mut **buffer));
            tokio::select! {
                biased;
                _ = cancel.changed() => Err(io::ErrorKind::Interrupted.into()),
                sent = sender.send(Chunk { bytes,_permit:permit }) => sent.map_err(|_| io::ErrorKind::BrokenPipe.into()),
            }
        })
    }
}

#[derive(Default)]
pub(super) struct EncodedSize(pub usize);
impl Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("History size overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
