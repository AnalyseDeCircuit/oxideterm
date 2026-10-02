// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use oxideterm_ssh::BoxedSshForwardStream;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{Notify, mpsc, watch},
};

use crate::{ForwardStats, ForwardingError};

pub const FORWARD_BRIDGE_READ_BUFFER_SIZE: usize = 32 * 1024;
pub const FORWARD_BRIDGE_CHANNEL_CAPACITY: usize = 32;
pub const DEFAULT_FORWARD_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default)]
pub struct ActiveConnectionCounter {
    count: Arc<AtomicU64>,
    notify: Arc<Notify>,
}

impl ActiveConnectionCounter {
    pub fn increment(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrement(&self) {
        self.count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                Some(count.saturating_sub(1))
            })
            .ok();
        self.notify.notify_waiters();
    }

    pub fn get(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    pub async fn wait_zero(&self, timeout: Duration) -> bool {
        if self.get() == 0 {
            return true;
        }

        tokio::time::timeout(timeout, async {
            while self.get() != 0 {
                self.notify.notified().await;
            }
        })
        .await
        .is_ok()
    }
}

#[derive(Clone, Debug, Default)]
pub struct BridgeStatsRecorder {
    connection_count: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    active_connections: ActiveConnectionCounter,
}

impl BridgeStatsRecorder {
    pub fn start_connection(&self) -> ConnectionGuard {
        self.connection_count.fetch_add(1, Ordering::SeqCst);
        self.active_connections.increment();
        ConnectionGuard {
            counter: self.active_connections.clone(),
        }
    }

    fn record_sent(&self, count: usize) {
        self.bytes_sent.fetch_add(count as u64, Ordering::SeqCst);
    }

    fn record_received(&self, count: usize) {
        self.bytes_received
            .fetch_add(count as u64, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> ForwardStats {
        ForwardStats {
            connection_count: self.connection_count.load(Ordering::SeqCst),
            active_connections: self.active_connections.get(),
            bytes_sent: self.bytes_sent.load(Ordering::SeqCst),
            bytes_received: self.bytes_received.load(Ordering::SeqCst),
        }
    }

    pub fn active_connections(&self) -> ActiveConnectionCounter {
        self.active_connections.clone()
    }
}

pub struct ConnectionGuard {
    counter: ActiveConnectionCounter,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.counter.decrement();
    }
}

pub async fn bridge_tcp_to_ssh_stream(
    tcp_stream: TcpStream,
    ssh_stream: BoxedSshForwardStream,
    stats: BridgeStatsRecorder,
    idle_timeout: Duration,
    shutdown_rx: watch::Receiver<bool>,
    log_prefix: String,
) -> Result<(), ForwardingError> {
    bridge_tcp_to_ssh_stream_inner(
        tcp_stream,
        ssh_stream,
        stats,
        idle_timeout,
        shutdown_rx,
        log_prefix,
        true,
    )
    .await
}

pub(crate) async fn bridge_tcp_to_ssh_stream_with_existing_connection(
    tcp_stream: TcpStream,
    ssh_stream: BoxedSshForwardStream,
    stats: BridgeStatsRecorder,
    idle_timeout: Duration,
    shutdown_rx: watch::Receiver<bool>,
    log_prefix: String,
) -> Result<(), ForwardingError> {
    bridge_tcp_to_ssh_stream_inner(
        tcp_stream,
        ssh_stream,
        stats,
        idle_timeout,
        shutdown_rx,
        log_prefix,
        false,
    )
    .await
}

async fn bridge_tcp_to_ssh_stream_inner(
    tcp_stream: TcpStream,
    ssh_stream: BoxedSshForwardStream,
    stats: BridgeStatsRecorder,
    idle_timeout: Duration,
    shutdown_rx: watch::Receiver<bool>,
    log_prefix: String,
    track_connection: bool,
) -> Result<(), ForwardingError> {
    let _connection_guard = track_connection.then(|| stats.start_connection());
    let (tcp_read, tcp_write) = tcp_stream.into_split();
    let (ssh_read, ssh_write) = tokio::io::split(ssh_stream);
    bridge_split_streams_inner(
        tcp_read,
        tcp_write,
        ssh_read,
        ssh_write,
        stats,
        idle_timeout,
        shutdown_rx,
        log_prefix,
    )
    .await
}

async fn bridge_split_streams_inner<LR, LW, SR, SW>(
    tcp_read: LR,
    tcp_write: LW,
    ssh_read: SR,
    ssh_write: SW,
    stats: BridgeStatsRecorder,
    idle_timeout: Duration,
    shutdown_rx: watch::Receiver<bool>,
    log_prefix: String,
) -> Result<(), ForwardingError>
where
    LR: AsyncRead + Send + Unpin,
    LW: AsyncWrite + Send + Unpin,
    SR: AsyncRead + Send + Unpin,
    SW: AsyncWrite + Send + Unpin,
{
    let (activity_tx, activity_rx) = watch::channel(0_u64);

    // Keep both pumps as child futures so ending the bridge cancels every pending read.
    let local_to_ssh = pipe_direction(
        tcp_read,
        ssh_write,
        stats.clone(),
        Direction::LocalToSsh,
        activity_tx.clone(),
    );
    let ssh_to_local = pipe_direction(
        ssh_read,
        tcp_write,
        stats,
        Direction::SshToLocal,
        activity_tx,
    );
    let idle = wait_for_idle(activity_rx, idle_timeout);
    let shutdown = wait_for_shutdown(shutdown_rx);

    tokio::select! {
        result = local_to_ssh => result?,
        result = ssh_to_local => result?,
        idle_elapsed = idle => {
            if idle_elapsed {
                tracing::debug!("{log_prefix}: closing idle forwarding bridge");
            }
        }
        _ = shutdown => {}
    }

    Ok(())
}

pub(crate) async fn wait_for_shutdown(mut shutdown_rx: watch::Receiver<bool>) {
    if *shutdown_rx.borrow() {
        return;
    }

    loop {
        match shutdown_rx.changed().await {
            Ok(()) if *shutdown_rx.borrow() => return,
            Ok(()) => continue,
            Err(_) => return,
        }
    }
}

async fn wait_for_idle(mut activity_rx: watch::Receiver<u64>, idle_timeout: Duration) -> bool {
    loop {
        match tokio::time::timeout(idle_timeout, activity_rx.changed()).await {
            Ok(Ok(())) => continue,
            Ok(Err(_)) => return false,
            Err(_) => return true,
        }
    }
}

#[derive(Clone, Copy)]
enum Direction {
    LocalToSsh,
    SshToLocal,
}

async fn pipe_direction<R, W>(
    mut reader: R,
    mut writer: W,
    stats: BridgeStatsRecorder,
    direction: Direction,
    activity_tx: watch::Sender<u64>,
) -> io::Result<()>
where
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
{
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<Vec<u8>>(FORWARD_BRIDGE_CHANNEL_CAPACITY);
    let read_activity_tx = activity_tx.clone();
    let reader_pump = async move {
        let mut buffer = vec![0_u8; FORWARD_BRIDGE_READ_BUFFER_SIZE];
        loop {
            let count = reader.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            record_activity(&read_activity_tx);
            if chunk_tx.send(buffer[..count].to_vec()).await.is_err() {
                break;
            }
        }
        Ok::<_, io::Error>(())
    };
    let writer_pump = async move {
        while let Some(chunk) = chunk_rx.recv().await {
            writer.write_all(&chunk).await?;
            record_activity(&activity_tx);
            match direction {
                Direction::LocalToSsh => stats.record_sent(chunk.len()),
                Direction::SshToLocal => stats.record_received(chunk.len()),
            }
        }
        writer.shutdown().await
    };

    // Structured concurrency drops either pump immediately if its sibling fails or is cancelled.
    tokio::try_join!(reader_pump, writer_pump)?;
    Ok(())
}

fn record_activity(activity_tx: &watch::Sender<u64>) {
    // Updating the version lets the idle waiter observe coalesced activity safely.
    activity_tx.send_modify(|version| *version = version.wrapping_add(1));
}

#[cfg(test)]
mod tests {
    use std::{
        pin::Pin,
        sync::atomic::{AtomicBool, Ordering},
        task::{Context, Poll},
    };

    use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

    use super::*;

    async fn bridge_test_streams<L, S>(
        local_stream: L,
        ssh_stream: S,
        stats: BridgeStatsRecorder,
        idle_timeout: Duration,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Result<(), ForwardingError>
    where
        L: AsyncRead + AsyncWrite + Send + Unpin,
        S: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let (tcp_read, tcp_write) = tokio::io::split(local_stream);
        let (ssh_read, ssh_write) = tokio::io::split(ssh_stream);
        bridge_split_streams_inner(
            tcp_read,
            tcp_write,
            ssh_read,
            ssh_write,
            stats,
            idle_timeout,
            shutdown_rx,
            "test bridge".to_string(),
        )
        .await
    }

    struct DropTrackedStream<T> {
        inner: T,
        dropped: Arc<AtomicBool>,
    }

    impl<T> DropTrackedStream<T> {
        fn new(inner: T, dropped: Arc<AtomicBool>) -> Self {
            Self { inner, dropped }
        }
    }

    impl<T> Drop for DropTrackedStream<T> {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl<T> AsyncRead for DropTrackedStream<T>
    where
        T: AsyncRead + Unpin,
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_read(context, buffer)
        }
    }

    impl<T> AsyncWrite for DropTrackedStream<T>
    where
        T: AsyncWrite + Unpin,
    {
        fn poll_write(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
            buffer: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.inner).poll_write(context, buffer)
        }

        fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_flush(context)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_shutdown(context)
        }
    }

    #[tokio::test]
    async fn active_connection_counter_times_out_until_connection_finishes() {
        let counter = ActiveConnectionCounter::default();
        counter.increment();
        assert!(!counter.wait_zero(Duration::from_millis(10)).await);
        assert_eq!(counter.get(), 1);
        let cloned = counter.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cloned.decrement();
        });

        assert!(counter.wait_zero(Duration::from_secs(1)).await);
        assert_eq!(counter.get(), 0);
    }

    #[tokio::test]
    async fn sustained_activity_past_idle_timeout_keeps_bridge_open() {
        let idle_timeout = Duration::from_millis(100);
        let (local_stream, mut local_peer) = tokio::io::duplex(1024);
        let (ssh_stream, mut ssh_peer) = tokio::io::duplex(1024);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let stats = BridgeStatsRecorder::default();
        let task_stats = stats.clone();
        let bridge_task = tokio::spawn(async move {
            bridge_test_streams(
                local_stream,
                ssh_stream,
                task_stats,
                idle_timeout,
                shutdown_rx,
            )
            .await
        });

        for byte in 0_u8..8 {
            local_peer.write_all(&[byte]).await.unwrap();
            let mut received = [0_u8; 1];
            tokio::time::timeout(idle_timeout, ssh_peer.read_exact(&mut received))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(received[0], byte);
            tokio::time::sleep(Duration::from_millis(30)).await;
        }

        assert!(!bridge_task.is_finished());
        assert_eq!(stats.snapshot().bytes_sent, 8);
        shutdown_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), bridge_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn truly_idle_bridge_closes_after_timeout() {
        let idle_timeout = Duration::from_millis(80);
        let (local_stream, _local_peer) = tokio::io::duplex(1024);
        let (ssh_stream, _ssh_peer) = tokio::io::duplex(1024);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut bridge_task = tokio::spawn(async move {
            bridge_test_streams(
                local_stream,
                ssh_stream,
                BridgeStatsRecorder::default(),
                idle_timeout,
                shutdown_rx,
            )
            .await
        });

        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut bridge_task)
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(1), bridge_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn one_sided_end_drops_all_bridge_io_tasks() {
        let local_dropped = Arc::new(AtomicBool::new(false));
        let ssh_dropped = Arc::new(AtomicBool::new(false));
        let (local_stream, local_peer) = tokio::io::duplex(1024);
        let (ssh_stream, _ssh_peer) = tokio::io::duplex(1024);
        let local_stream = DropTrackedStream::new(local_stream, local_dropped.clone());
        let ssh_stream = DropTrackedStream::new(ssh_stream, ssh_dropped.clone());
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let bridge_task = tokio::spawn(async move {
            bridge_test_streams(
                local_stream,
                ssh_stream,
                BridgeStatsRecorder::default(),
                Duration::from_secs(5),
                shutdown_rx,
            )
            .await
        });

        drop(local_peer);
        tokio::time::timeout(Duration::from_secs(1), bridge_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert!(local_dropped.load(Ordering::SeqCst));
        assert!(ssh_dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn dropped_watch_sender_ends_bridge_without_busy_loop() {
        let (local_stream, _local_peer) = tokio::io::duplex(1024);
        let (ssh_stream, _ssh_peer) = tokio::io::duplex(1024);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        drop(shutdown_tx);

        tokio::time::timeout(
            Duration::from_millis(200),
            bridge_test_streams(
                local_stream,
                ssh_stream,
                BridgeStatsRecorder::default(),
                Duration::from_secs(5),
                shutdown_rx,
            ),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
