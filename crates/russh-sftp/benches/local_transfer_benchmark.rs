use std::{
    hint::black_box,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use anyhow::{anyhow, Context as _, Result};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use futures::future::try_join_all;
use russh_sftp::client::SftpSession;
use tempfile::TempDir;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    process::{Child, ChildStdin, ChildStdout, Command},
    runtime::Runtime,
    time::timeout,
};

const FILE_COUNT: usize = 8;
const FILE_SIZE: usize = 10 * 1024 * 1024;
const LARGE_FILE_SIZE: usize = 256 * 1024 * 1024;
const SESSION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

struct ChildSftpStream {
    reader: ChildStdout,
    writer: ChildStdin,
}

impl AsyncRead for ChildSftpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buffer)
    }
}

impl AsyncWrite for ChildSftpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

struct LocalSftpHarness {
    session: SftpSession,
    child: Child,
    root: TempDir,
}

impl LocalSftpHarness {
    async fn start() -> Result<Self> {
        let server_path = local_sftp_server_path()?;
        let root = tempfile::tempdir().context("create benchmark directory")?;
        let mut child = Command::new(server_path)
            .current_dir(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            // The harness owns the subsystem process and must not leave it
            // alive when a benchmark panics or exits early.
            .kill_on_drop(true)
            .spawn()
            .context("start local SFTP subsystem")?;
        let writer = child
            .stdin
            .take()
            .context("local SFTP subsystem should expose stdin")?;
        let reader = child
            .stdout
            .take()
            .context("local SFTP subsystem should expose stdout")?;
        let session = match SftpSession::new(ChildSftpStream { reader, writer }).await {
            Ok(session) => session,
            Err(error) => {
                // Initialization failures still reap the owned subsystem.
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(error).context("initialize local SFTP session");
            }
        };
        Ok(Self {
            session,
            child,
            root,
        })
    }

    fn root(&self) -> &Path {
        self.root.path()
    }

    async fn shutdown(self) -> Result<()> {
        let Self {
            session,
            mut child,
            root: _root,
        } = self;
        let close_result = session.close().await;
        drop(session);
        if let Err(error) = close_result {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error).context("close local SFTP session");
        }

        match timeout(SESSION_SHUTDOWN_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(status)) => Err(anyhow!("local SFTP subsystem exited with {}", status)),
            Ok(Err(error)) => Err(error).context("wait for local SFTP subsystem"),
            Err(_) => {
                // A bounded kill-and-wait path guarantees process cleanup.
                child
                    .kill()
                    .await
                    .context("stop unresponsive local SFTP subsystem")?;
                child
                    .wait()
                    .await
                    .context("reap local SFTP subsystem after kill")?;
                Err(anyhow!(
                    "local SFTP subsystem did not stop after session close"
                ))
            }
        }
    }
}

fn local_sftp_server_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("RUSSH_SFTP_SERVER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(anyhow!(
            "RUSSH_SFTP_SERVER does not point to an SFTP subsystem executable"
        ));
    }

    #[cfg(target_os = "macos")]
    const CANDIDATES: &[&str] = &["/usr/libexec/sftp-server"];
    #[cfg(target_os = "linux")]
    const CANDIDATES: &[&str] = &[
        "/usr/lib/openssh/sftp-server",
        "/usr/lib/ssh/sftp-server",
        "/usr/libexec/openssh/sftp-server",
    ];
    #[cfg(target_os = "windows")]
    const CANDIDATES: &[&str] = &["C:\\Windows\\System32\\OpenSSH\\sftp-server.exe"];
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    const CANDIDATES: &[&str] = &[];

    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or_else(|| {
            anyhow!("no local SFTP subsystem found; set RUSSH_SFTP_SERVER to its executable path")
        })
}

async fn upload_many(session: &SftpSession, data: Arc<Vec<u8>>) -> Result<()> {
    let uploads = (0..FILE_COUNT).map(|index| {
        let data = Arc::clone(&data);
        async move {
            let path = format!("upload-{index}.bin");
            let mut file = session.create(&path).await?;
            file.write_all(&data).await?;
            file.shutdown().await?;
            session.remove_file(path).await?;
            Ok::<(), anyhow::Error>(())
        }
    });
    try_join_all(uploads).await?;
    Ok(())
}

async fn upload_single(session: &SftpSession, data: Arc<Vec<u8>>) -> Result<()> {
    let path = "upload-large.bin";
    let mut file = session.create(path).await?;
    file.write_all(&data).await?;
    file.shutdown().await?;
    session.remove_file(path).await?;
    Ok(())
}

async fn download_many(session: &SftpSession) -> Result<()> {
    let downloads = (0..FILE_COUNT).map(|index| async move {
        let mut file = session.open(format!("download-{index}.bin")).await?;
        let mut data = Vec::with_capacity(FILE_SIZE);
        file.read_to_end(&mut data).await?;
        if data.len() != FILE_SIZE {
            return Err(anyhow!("downloaded file length did not match fixture"));
        }
        black_box(data);
        Ok::<(), anyhow::Error>(())
    });
    try_join_all(downloads).await?;
    Ok(())
}

async fn download_single(session: &SftpSession) -> Result<()> {
    let mut file = session.open("download-large.bin").await?;
    let mut data = Vec::with_capacity(LARGE_FILE_SIZE);
    file.read_to_end(&mut data).await?;
    if data.len() != LARGE_FILE_SIZE {
        return Err(anyhow!("downloaded file length did not match fixture"));
    }
    black_box(data);
    Ok(())
}

fn write_download_fixtures(root: &Path) -> Result<()> {
    let small_data = vec![0x5a; FILE_SIZE];
    for index in 0..FILE_COUNT {
        std::fs::write(root.join(format!("download-{index}.bin")), &small_data)
            .context("write small download fixture")?;
    }
    let large_data = vec![0x5a; LARGE_FILE_SIZE];
    std::fs::write(root.join("download-large.bin"), large_data)
        .context("write large download fixture")?;
    Ok(())
}

async fn upload_bulk(session: &SftpSession, data: &[u8]) -> Result<()> {
    let file = session.create("upload-bulk.bin").await?;
    let mut writer = file.into_pipelined_uploader(0, 256 * 1024, 64, 16 * 1024 * 1024);
    writer.write_all_chunk(data).await?;
    writer.shutdown().await?;
    session.remove_file("upload-bulk.bin").await?;
    Ok(())
}

async fn download_bulk(session: &SftpSession) -> Result<()> {
    let file = session.open("download-large.bin").await?;
    let mut reader = file.into_pipelined_downloader_for_range(
        0,
        Some(LARGE_FILE_SIZE as u64),
        256 * 1024,
        64,
        16 * 1024 * 1024,
    );
    let mut offset = 0;
    while let Some(chunk) = reader.next_chunk().await? {
        if chunk.offset != offset || chunk.data.iter().any(|byte| *byte != 0x5a) {
            return Err(anyhow!(
                "bulk download content or offset did not match fixture"
            ));
        }
        offset += chunk.data.len() as u64;
    }
    reader.shutdown().await?;
    if offset != LARGE_FILE_SIZE as u64 {
        return Err(anyhow!("bulk download ended before fixture boundary"));
    }
    Ok(())
}

fn benchmark(c: &mut Criterion) {
    let runtime = Runtime::new().expect("benchmark runtime should start");
    let harness = runtime
        .block_on(LocalSftpHarness::start())
        .expect("local SFTP subsystem should start");
    write_download_fixtures(harness.root()).expect("download fixtures should be written");

    let small_data = Arc::new(vec![0x5a; FILE_SIZE]);
    let large_data = Arc::new(vec![0x5a; LARGE_FILE_SIZE]);

    let mut upload_group = c.benchmark_group("sftp_local_upload");
    upload_group.sample_size(10);
    upload_group.measurement_time(Duration::from_secs(10));
    upload_group.throughput(Throughput::Bytes((FILE_COUNT * FILE_SIZE) as u64));
    upload_group.bench_function("8_files_10mb", |bencher| {
        bencher.iter_batched(
            || Arc::clone(&small_data),
            |data| {
                runtime
                    .block_on(upload_many(&harness.session, data))
                    .expect("small files should upload")
            },
            BatchSize::SmallInput,
        );
    });
    upload_group.throughput(Throughput::Bytes(LARGE_FILE_SIZE as u64));
    upload_group.bench_function("1_file_256mb", |bencher| {
        bencher.iter_batched(
            || Arc::clone(&large_data),
            |data| {
                runtime
                    .block_on(upload_single(&harness.session, data))
                    .expect("large file should upload")
            },
            BatchSize::SmallInput,
        );
    });
    upload_group.finish();

    let mut download_group = c.benchmark_group("sftp_local_download");
    download_group.sample_size(10);
    download_group.measurement_time(Duration::from_secs(10));
    download_group.throughput(Throughput::Bytes((FILE_COUNT * FILE_SIZE) as u64));
    download_group.bench_function("8_files_10mb", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(download_many(&harness.session))
                .expect("small files should download")
        });
    });
    download_group.throughput(Throughput::Bytes(LARGE_FILE_SIZE as u64));
    download_group.bench_function("1_file_256mb", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(download_single(&harness.session))
                .expect("large file should download")
        });
    });
    download_group.finish();

    let mut bulk_group = c.benchmark_group("sftp_local_bulk");
    bulk_group.sample_size(10);
    bulk_group.measurement_time(Duration::from_secs(10));
    bulk_group.throughput(Throughput::Bytes(LARGE_FILE_SIZE as u64));
    bulk_group.bench_function("upload_256mb", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(upload_bulk(&harness.session, &large_data))
                .unwrap()
        });
    });
    bulk_group.bench_function("download_256mb", |bencher| {
        bencher.iter(|| runtime.block_on(download_bulk(&harness.session)).unwrap());
    });
    bulk_group.finish();

    runtime
        .block_on(harness.shutdown())
        .expect("local SFTP subsystem should stop cleanly");
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
