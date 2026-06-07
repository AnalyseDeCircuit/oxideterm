use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use log::debug;
use russh::{client, ChannelId};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::Write as WritePacket;
use russh_sftp::{de::from_bytes, ser::to_bytes};
use std::{sync::Arc, time::Duration};
use tokio::{io::AsyncWriteExt, task};

const FILE_COUNT: usize = 8;
const FILE_SIZE: usize = 10 * 1024 * 1024;

struct Client;

impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        debug!("check_server_key: {:?}", server_public_key);
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        debug!("data on channel {:?}: {}", channel, data.len());
        Ok(())
    }
}

async fn connect() -> SftpSession {
    let config = russh::client::Config::default();
    let mut session = russh::client::connect(Arc::new(config), ("localhost", 22), Client {})
        .await
        .unwrap();
    assert!(session
        .authenticate_password("root", "password")
        .await
        .unwrap()
        .success());
    let channel = session.channel_open_session().await.unwrap();
    channel.request_subsystem(true, "sftp").await.unwrap();
    let sftp = SftpSession::new(channel.into_stream()).await.unwrap();
    sftp
}

async fn upload_many(sftp: &SftpSession, data: Arc<Vec<u8>>) {
    let mut tasks = Vec::with_capacity(FILE_COUNT);
    for i in 0..FILE_COUNT {
        let mut file = sftp.create(format!("bench_{i}")).await.unwrap();
        let chunk = Arc::clone(&data);
        tasks.push(task::spawn(async move {
            file.write_all(&chunk).await.unwrap();
        }));
    }
    futures::future::join_all(tasks).await;
    for i in 0..FILE_COUNT {
        sftp.remove_file(format!("bench_{i}")).await.unwrap();
    }
}

async fn upload_single(sftp: &SftpSession, data: Arc<Vec<u8>>) {
    let mut file = sftp.create("bench_single").await.unwrap();
    file.write_all(&data).await.unwrap();
    sftp.remove_file("bench_single").await.unwrap();
}

fn benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("sftp_upload");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    let data_10mb = Arc::new(vec![0u8; FILE_SIZE]);
    group.throughput(Throughput::Bytes((FILE_COUNT * FILE_SIZE) as u64));
    group.bench_function("8_files_10mb", |b| {
        b.iter_batched(
            || {
                let data = Arc::clone(&data_10mb);
                let sftp = rt.block_on(connect());
                (sftp, data)
            },
            |(sftp, data)| rt.block_on(upload_many(&sftp, data)),
            BatchSize::SmallInput,
        )
    });

    group.finish();

    let mut group = c.benchmark_group("sftp_upload_large");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    let data_256mb = Arc::new(vec![0u8; 256 * 1024 * 1024]);
    group.throughput(Throughput::Bytes(256 * 1024 * 1024));
    group.bench_function("1_file_256mb", |b| {
        b.iter_batched(
            || {
                let data = Arc::clone(&data_256mb);
                let sftp = rt.block_on(connect());
                (sftp, data)
            },
            |(sftp, data)| rt.block_on(upload_single(&sftp, data)),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_serde(c: &mut Criterion) {
    const SIZE: usize = 10 * 1024 * 1024;

    let packet = WritePacket {
        id: 1,
        handle: "handle".into(),
        offset: 0,
        data: vec![0u8; SIZE],
    };
    let serialized = to_bytes(&packet).unwrap();

    let mut group = c.benchmark_group("serde");
    group.throughput(Throughput::Bytes(SIZE as u64));

    group.bench_function("serialize_write_10mb", |b| {
        b.iter(|| to_bytes(&packet).unwrap())
    });

    group.bench_function("deserialize_write_10mb", |b| {
        b.iter_batched(
            || serialized.clone(),
            |mut bytes| from_bytes::<WritePacket>(&mut bytes).unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, benchmark, bench_serde);
criterion_main!(benches);
