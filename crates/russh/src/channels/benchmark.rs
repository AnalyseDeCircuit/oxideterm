#![allow(clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use tokio::{io::AsyncWriteExt, runtime::Runtime, sync::mpsc};

use super::{io::ChannelTx, ChannelId, ChannelMsg, WindowSizeRef};

const SSH_MAX_PACKET_BYTES: u32 = 256 * 1024;
const BENCHMARK_CHUNK_BYTES: [usize; 3] = [32 * 1024, 128 * 1024, 256 * 1024];

struct TransportHarness {
    tx: ChannelTx<(ChannelId, ChannelMsg)>,
    receiver: mpsc::Receiver<(ChannelId, ChannelMsg)>,
    window_size: WindowSizeRef,
    frame: Bytes,
}

impl TransportHarness {
    fn new(frame_len: usize) -> Self {
        let (sender, receiver) = mpsc::channel(2);
        let window_size = WindowSizeRef::new(SSH_MAX_PACKET_BYTES);
        let tx = ChannelTx::new(
            sender,
            ChannelId(1),
            window_size.value.clone(),
            window_size.subscribe(),
            SSH_MAX_PACKET_BYTES,
            None,
        );
        Self {
            tx,
            receiver,
            window_size,
            frame: Bytes::from(vec![0x5a; frame_len]),
        }
    }

    async fn consume_data(&mut self) {
        let (_, message) = self.receiver.recv().await.expect("sender remains live");
        match message {
            ChannelMsg::Data { data } => {
                black_box(data);
            }
            message => panic!("unexpected benchmark message: {message:?}"),
        }
        self.window_size.update(SSH_MAX_PACKET_BYTES).await;
    }

    async fn send_current(&mut self) {
        self.tx
            .write_all(self.frame.as_ref())
            .await
            .expect("current channel path sends");
        self.consume_data().await;
    }

    async fn send_owned(&mut self) {
        self.tx
            .write_bytes(self.frame.clone())
            .await
            .expect("owned channel path sends");
        self.consume_data().await;
    }
}

/// Compares the production borrowed `AsyncWrite` path with the owned `Bytes`
/// path used by SFTP. Both variants use the same channel queue and window.
pub fn bench(c: &mut Criterion) {
    let runtime = Runtime::new().expect("benchmark runtime starts");
    let mut group = c.benchmark_group("sftp_owned_transport");

    for frame_len in BENCHMARK_CHUNK_BYTES {
        group.throughput(Throughput::Bytes(frame_len as u64));
        let size_label = format!("{}_KiB", frame_len / 1024);

        let mut current = TransportHarness::new(frame_len);
        group.bench_with_input(
            BenchmarkId::new("current", &size_label),
            &frame_len,
            |b, _| b.iter(|| runtime.block_on(current.send_current())),
        );

        let mut owned = TransportHarness::new(frame_len);
        group.bench_with_input(
            BenchmarkId::new("owned", &size_label),
            &frame_len,
            |b, _| b.iter(|| runtime.block_on(owned.send_owned())),
        );
    }

    group.finish();
}
