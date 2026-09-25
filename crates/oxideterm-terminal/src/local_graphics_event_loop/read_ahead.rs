// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::File,
    io::{self, ErrorKind, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, select};
use polling::{Event, Events, PollMode, Poller};
use zeroize::{Zeroize, Zeroizing};

const BUFFER_BYTES: usize = 64 * 1024;
const BUFFER_COUNT: usize = 4;
const SATURATED_READ_BYTES: usize = 1024;
const REFILL_RETRIES: usize = 16;
const GATHER_BUDGET: Duration = Duration::from_millis(3);
type Buffer = Box<Zeroizing<[u8; BUFFER_BYTES]>>;
struct Batch {
    buffer: Buffer,
    len: usize,
}

/// The PTY event loop owns this reader and joins it when parsing stops. Only
/// raw reads move here; protocol state and ordered control messages stay on
/// the event loop. A fixed pool bounds read-ahead and preserves PTY backpressure.
pub(super) struct PtyReadAhead {
    ready: Receiver<io::Result<Batch>>,
    spare: Sender<Buffer>,
    current: Option<Batch>,
    offset: usize,
    finish: Arc<AtomicBool>,
    stop: Sender<()>,
    poll: Arc<Poller>,
    thread: Option<JoinHandle<()>>,
}

impl PtyReadAhead {
    pub(super) fn new(mut file: File, consumer: Arc<Poller>) -> io::Result<Self> {
        let poll = Arc::new(Poller::new()?);
        let (ready_tx, ready) = bounded(BUFFER_COUNT);
        let (spare, spare_rx) = bounded(BUFFER_COUNT);
        for _ in 0..BUFFER_COUNT {
            spare
                .send(Box::new(Zeroizing::new([0; BUFFER_BYTES])))
                .unwrap();
        }
        let (stop, stopped) = bounded(1);
        let finish = Arc::new(AtomicBool::new(false));
        let finish_reader = finish.clone();
        let reader_poll = poll.clone();
        // The registered file outlives the worker's poll registration.
        unsafe {
            poll.add_with_mode(&file, Event::readable(0), PollMode::Level)?;
        }
        let thread = std::thread::Builder::new().name("OxideTerm PTY read-ahead".into()).spawn(move || {
            let result = (|| -> io::Result<()> {
                let mut events = Events::new();
                loop {
                    let mut buffer = select! {
                        recv(stopped) -> _ => return Ok(()),
                        recv(spare_rx) -> buffer => match buffer { Ok(buffer) => buffer, Err(_) => return Ok(()) },
                    };
                    let mut len = 0;
                    let mut eof = false;
                    let mut batch_started = None;
                    let mut refill_retries = 0;
                    loop {
                        if !stopped.is_empty() { return Ok(()); }
                        match file.read(&mut buffer[len..]) {
                            Ok(0) => { eof = true; break; },
                            Ok(n) => {
                                len += n;
                                batch_started.get_or_insert_with(Instant::now);
                                refill_retries = 0;
                                if len == BUFFER_BYTES { break; }
                            },
                            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                                if finish_reader.load(Ordering::Acquire) { break; }
                                // Only bridge refill gaps after a full Darwin PTY queue.
                                // Small interactive output is delivered on the first EAGAIN.
                                if len >= SATURATED_READ_BYTES && refill_retries < REFILL_RETRIES
                                    && batch_started.is_some_and(|started| started.elapsed() < GATHER_BUDGET)
                                {
                                    refill_retries += 1;
                                    continue;
                                }
                                if len > 0 { break; }
                                events.clear();
                                match reader_poll.wait(&mut events, None) {
                                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                                    result => { result?; },
                                }
                            },
                            Err(error) => return Err(error),
                        }
                    }
                    if len > 0 {
                        select! {
                            recv(stopped) -> _ => return Ok(()),
                            send(ready_tx, Ok(Batch { buffer, len })) -> sent => if sent.is_err() { return Ok(()); },
                        }
                        consumer.notify()?;
                    }
                    if eof || finish_reader.load(Ordering::Acquire) { return Ok(()); }
                }
            })();
            if let Err(error) = result {
                select! {
                    recv(stopped) -> _ => {},
                    send(ready_tx, Err(error)) -> _ => {},
                }
            }
            let _ = reader_poll.delete(&file);
            drop(ready_tx);
            let _ = consumer.notify();
        })?;
        Ok(Self {
            ready,
            spare,
            current: None,
            offset: 0,
            finish,
            stop,
            poll,
            thread: Some(thread),
        })
    }

    pub(super) fn has_pending(&self) -> bool {
        self.current.is_some() || !self.ready.is_empty()
    }

    pub(super) fn finish(&self) {
        self.finish.store(true, Ordering::Release);
        let _ = self.poll.notify();
    }

    pub(super) fn wait_pending(&mut self) -> io::Result<bool> {
        if self.current.is_some() {
            return Ok(true);
        }
        match self.ready.recv() {
            Ok(batch) => {
                self.current = Some(batch?);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

impl Read for PtyReadAhead {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if self.current.is_none() {
            match self.ready.try_recv() {
                Ok(batch) => self.current = Some(batch?),
                Err(TryRecvError::Empty) => return Err(ErrorKind::WouldBlock.into()),
                Err(TryRecvError::Disconnected) => {
                    return Ok(0);
                }
            }
        }
        let batch = self.current.as_ref().unwrap();
        let n = destination.len().min(batch.len - self.offset);
        destination[..n].copy_from_slice(&batch.buffer[self.offset..self.offset + n]);
        self.offset += n;
        if self.offset == batch.len {
            let mut batch = self.current.take().unwrap();
            // Buffers containing terminal output are cleared before reuse.
            batch.buffer[..batch.len].zeroize();
            let _ = self.spare.try_send(batch.buffer);
            self.offset = 0;
        }
        Ok(n)
    }
}

impl Drop for PtyReadAhead {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        let _ = self.poll.notify();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("PTY read-ahead thread panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::{fd::OwnedFd, unix::net::UnixStream};

    #[test]
    fn pooled_reads_preserve_bytes_across_backpressure_and_eof() {
        let (read, mut write) = UnixStream::pair().unwrap();
        read.set_nonblocking(true).unwrap();
        let fd: OwnedFd = read.into();
        let mut reader = PtyReadAhead::new(fd.into(), Arc::new(Poller::new().unwrap())).unwrap();
        let expected: Vec<_> = (0..BUFFER_BYTES * (BUFFER_COUNT + 2) + 7)
            .map(|i| (i % 251) as u8)
            .collect();
        let bytes = expected.clone();
        let writer = std::thread::spawn(move || write.write_all(&bytes).unwrap());
        let mut actual = Vec::new();
        let mut buffer = [0; 777];
        while reader.wait_pending().unwrap() {
            let n = reader.read(&mut buffer).unwrap();
            actual.extend_from_slice(&buffer[..n]);
        }
        writer.join().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn dropping_idle_or_backpressured_reader_joins_worker() {
        for saturated in [false, true] {
            let (read, mut write) = UnixStream::pair().unwrap();
            read.set_nonblocking(true).unwrap();
            let fd: OwnedFd = read.into();
            let reader = PtyReadAhead::new(fd.into(), Arc::new(Poller::new().unwrap())).unwrap();
            let writer = if saturated {
                Some(std::thread::spawn(move || {
                    let _ = write.write_all(&vec![42; BUFFER_BYTES * (BUFFER_COUNT + 4)]);
                }))
            } else {
                // Keep the writer open until cancellation completes: EOF must not wake the test.
                None
            };
            if saturated {
                let deadline = Instant::now() + Duration::from_secs(2);
                while reader.ready.len() < BUFFER_COUNT {
                    assert!(
                        Instant::now() < deadline,
                        "reader never became backpressured"
                    );
                    std::thread::yield_now();
                }
            }
            let (done, joined) = std::sync::mpsc::channel();
            let closer = std::thread::spawn(move || {
                drop(reader);
                done.send(()).unwrap();
            });
            joined
                .recv_timeout(Duration::from_secs(2))
                .expect("reader did not join on cancellation");
            closer.join().unwrap();
            if let Some(writer) = writer {
                writer.join().unwrap();
            }
        }
    }
}
