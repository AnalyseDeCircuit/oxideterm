use super::*;
use oxideterm_ssh::{
    ConnectionTransportStatus, SshConfig, SshConnectionRegistry, SshTransportClient,
};
use russh::{ChannelId, server};
use std::{collections::HashMap, process::Stdio, sync::atomic::AtomicUsize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Process {
    input: tokio::process::ChildStdin,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Process {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Default)]
struct UploadGate {
    received: tokio::sync::Notify,
    proceed: tokio::sync::Notify,
    restored: AtomicBool,
}

struct Peer {
    home: PathBuf,
    upload_gate: Option<Arc<UploadGate>>,
    exec: bool,
    channels: HashMap<ChannelId, tokio::task::JoinHandle<()>>,
    processes: HashMap<ChannelId, Process>,
    sftp: Arc<AtomicUsize>,
}
impl Peer {
    fn start(&mut self, channel: ChannelId, session: &mut server::Session) {
        let mut command = if cfg!(target_os = "linux") {
            let mut command = tokio::process::Command::new("/bin/sh");
            command.env("HOME", &self.home).current_dir(&self.home);
            command
        } else {
            let mut command = tokio::process::Command::new("docker");
            command.args([
                "run",
                "--rm",
                "--network",
                "none",
                "-i",
                "python:3.6",
                "/bin/sh",
            ]);
            command
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let handle = session.handle();
        let receipt = self.upload_gate.take();
        let mut upload_gate = receipt.clone();
        let mut marker_tail = Vec::new();
        let task = tokio::spawn(async move {
            let mut buffer = zeroize::Zeroizing::new(vec![0; 65536]);
            while let Ok(count) = output.read(&mut buffer).await {
                if count == 0 {
                    break;
                }
                marker_tail.extend_from_slice(&buffer[..count]);
                if let Some(gate) = upload_gate.as_ref() {
                    if memchr::memmem::find(&marker_tail, b":UPLOADED:").is_some() {
                        gate.received.notify_one();
                        gate.proceed.notified().await;
                        // Reject the receipt after the held transfer to exercise acknowledged cleanup.
                        let _ = handle
                            .data(channel, b"\x1einvalid-receipt\n".to_vec())
                            .await;
                        upload_gate = None;
                    }
                }
                if memchr::memmem::find(&marker_tail, b":RESTORED").is_some() {
                    if let Some(receipt) = &receipt {
                        receipt.restored.store(true, Ordering::SeqCst);
                    }
                }
                let discard = marker_tail.len().saturating_sub(16);
                marker_tail.drain(..discard);
                if handle
                    .data(channel, buffer[..count].to_vec())
                    .await
                    .is_err()
                {
                    return;
                }
            }
            if let Ok(status) = child.wait().await {
                let _ = handle
                    .exit_status_request(channel, status.code().unwrap_or(1) as u32)
                    .await;
            }
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        self.processes.insert(channel, Process { input, task });
    }
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_none(&mut self, _: &str) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }
    async fn channel_open_session(
        &mut self,
        mut channel: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        // The server handler consumes data; drain the mirrored channel queue to avoid backpressure.
        self.channels.insert(
            channel.id(),
            tokio::spawn(async move { while channel.wait().await.is_some() {} }),
        );
        reply.accept().await;
        Ok(())
    }
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.sftp.fetch_add(1, Ordering::SeqCst);
        session.channel_failure(channel)?;
        session.close(channel)
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if !self.exec {
            session.channel_failure(channel)?;
            return session.close(channel);
        }
        session.channel_success(channel)?;
        let command = std::str::from_utf8(data).unwrap();
        if command.starts_with("echo __OXIDE_PROBE_") {
            session.data(
                channel,
                format!("{}\n", command.trim_start_matches("echo ")).into_bytes(),
            )?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)
        } else {
            assert_eq!(command, "/bin/sh");
            self.start(channel, session);
            Ok(())
        }
    }
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        self.start(channel, session);
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(process) = self.processes.get_mut(&channel) {
            process
                .input
                .write_all(data)
                .await
                .map_err(russh::Error::IO)?;
        }
        Ok(())
    }
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.processes.remove(&channel);
        if let Some(task) = self.channels.remove(&channel) {
            task.abort();
        }
        Ok(())
    }
}

#[tokio::test]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires local Docker for real Linux file commands and agent"
)]
async fn ssh_file_backends_work_without_sftp_through_exec_and_shell() {
    for exec in [true, false] {
        for access in [RemoteAccess::Commands, RemoteAccess::Agent] {
            // Each peer needs a fresh install and file namespace, including on repeated runs.
            let home = tempfile::tempdir().unwrap();
            let remote_root = if cfg!(target_os = "linux") {
                home.path().to_str().unwrap()
            } else {
                "/tmp"
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let key = russh::keys::PrivateKey::random(
                &mut rand10::rng(),
                russh::keys::Algorithm::Ed25519,
            )
            .unwrap();
            let fingerprint = key
                .public_key()
                .fingerprint(russh::keys::HashAlg::Sha256)
                .to_string();
            let sftp = Arc::new(AtomicUsize::new(0));
            let upload_gate =
                (exec && access == RemoteAccess::Agent).then(|| Arc::new(UploadGate::default()));
            let peer = Peer {
                home: home.path().to_path_buf(),
                upload_gate: upload_gate.clone(),
                exec,
                channels: HashMap::new(),
                sftp: sftp.clone(),
                processes: HashMap::new(),
            };
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let connection = server::run_stream(
                    Arc::new(server::Config {
                        keys: vec![key],
                        ..Default::default()
                    }),
                    stream,
                    peer,
                )
                .await
                .unwrap();
                let _ = connection.await;
            });
            let mut config =
                SshConfig::password("127.0.0.1", port, "fixture", "synthetic-password");
            config.expected_host_key_fingerprint = Some(fingerprint);
            config.trust_host_key = Some(false);
            let registry = SshConnectionRegistry::default();
            let node = NodeId::new("file-contract");
            let router = NodeRouter::new(registry.clone());
            router.upsert_node(node.clone(), config.clone());
            let handle = SshTransportClient::new(config)
                .connect_node_with_registry(
                    registry,
                    ConnectionConsumer::NodeRouter(node.0.clone()),
                )
                .await
                .unwrap();
            router
                .bind_connection(&node, handle.connection_id().to_string())
                .unwrap();
            let base = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
            let (progress_tx, progress_rx) = watch::channel(AgentInstallProgress::Checking);
            let startup_fs = base.clone();
            let startup_node = node.0.clone();
            let mut startup = tokio::spawn(async move {
                startup_fs
                    .bind_node(&startup_node, access, Some(progress_tx))
                    .await
            });
            if let Some(gate) = upload_gate.as_ref() {
                tokio::select! {
                    _ = gate.received.notified() => {},
                    result = &mut startup => panic!("startup completed before upload acknowledgment: {:?}", result.map(|result| result.map(|_| ()))),
                }
                assert!(matches!(
                    *progress_rx.borrow(),
                    AgentInstallProgress::Uploading { transferred: 0, .. }
                ));
                // The remote has received a batch, but its acknowledgment is deliberately held.
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(11)).await;
                tokio::time::resume();
                assert!(
                    !startup.is_finished(),
                    "an active upload must not use the 10-second startup deadline"
                );
                gate.proceed.notify_one();
            }
            let result = startup.await.unwrap();
            let fs = if let Some(gate) = upload_gate {
                assert!(matches!(
                    result,
                    Err(IdeFileError {
                        kind: IdeFileErrorKind::Other,
                        ..
                    })
                ));
                assert!(
                    gate.restored.load(Ordering::SeqCst),
                    "failure must wait for the remote cleanup trap"
                );
                assert!(
                    !handle
                        .info()
                        .consumers
                        .iter()
                        .any(|consumer| matches!(consumer, ConnectionConsumer::Ide(_)))
                );
                assert_eq!(
                    handle.transport_status().await,
                    ConnectionTransportStatus::Open
                );
                let (sender, receiver) = watch::channel(AgentInstallProgress::Checking);
                let fs = base.bind_node(&node.0, access, Some(sender)).await.unwrap();
                assert_eq!(*receiver.borrow(), AgentInstallProgress::Starting);
                fs
            } else {
                result.unwrap_or_else(|error| panic!("exec={exec} access={access:?}: {error:?}"))
            };
            if !exec && access == RemoteAccess::Agent {
                assert_eq!(*progress_rx.borrow(), AgentInstallProgress::Starting);
            }
            assert_eq!(fs.uses_ssh_shell(), !exec);
            assert!(!fs.supports_watching());
            let location =
                IdeLocation::remote(node.0.clone(), format!("{remote_root}/ide-file-contract"));
            let version = fs
                .write_file(
                    &location,
                    "hello €\n",
                    &oxideterm_ide_core::TextFileFormat::default(),
                    None,
                    WriteMode::CreateNew,
                )
                .await
                .unwrap();
            assert_eq!(
                &*fs.read_file(&location, None).await.unwrap().text,
                "hello €\n"
            );
            fs.write_file(
                &location,
                "saved\n",
                &oxideterm_ide_core::TextFileFormat::default(),
                Some(&version),
                WriteMode::AtomicReplace,
            )
            .await
            .unwrap();
            assert_eq!(
                &*fs.read_file(&location, None).await.unwrap().text,
                "saved\n"
            );
            let binary_path = format!("{remote_root}/ide-binary-contract");
            let binary = IdeLocation::remote(node.0.clone(), &binary_path);
            fs.write_file(
                &binary,
                "\0binary",
                &oxideterm_ide_core::TextFileFormat::default(),
                None,
                WriteMode::CreateNew,
            )
            .await
            .unwrap();
            assert_eq!(
                fs.read_file(&binary, None).await.unwrap_err().kind,
                IdeFileErrorKind::Binary
            );
            fs.delete_item(node.0.clone(), &binary_path, false)
                .await
                .unwrap();
            let unicode_path = format!("{remote_root}/ide-unicode-contract");
            let unicode = IdeLocation::remote(node.0.clone(), &unicode_path);
            let format = oxideterm_ide_core::TextFileFormat {
                encoding: "UTF-16LE".into(),
                has_bom: false,
                ..Default::default()
            };
            fs.write_file(&unicode, "你好", &format, None, WriteMode::CreateNew)
                .await
                .unwrap();
            assert_eq!(
                fs.read_file(&unicode, Some("UTF-16LE"))
                    .await
                    .unwrap()
                    .text
                    .as_str(),
                "你好"
            );
            fs.delete_item(node.0.clone(), &unicode_path, false)
                .await
                .unwrap();
            if exec && access == RemoteAccess::Commands {
                let ide_consumers = || {
                    handle
                        .info()
                        .consumers
                        .iter()
                        .filter(|consumer| matches!(consumer, ConnectionConsumer::Ide(_)))
                        .count()
                };
                assert_eq!(ide_consumers(), 1);
                let scoped = fs.scoped_owner();
                scoped.ensure_ide_session_for_node(&node).await.unwrap();
                assert_eq!(ide_consumers(), 2);
                scoped.release_all_ide_consumers();
                assert_eq!(ide_consumers(), 1);
                assert_eq!(
                    scoped
                        .ensure_ide_session_for_node(&node)
                        .await
                        .unwrap_err()
                        .kind,
                    IdeFileErrorKind::Disconnected
                );
                assert_eq!(
                    &*fs.read_file(&location, None).await.unwrap().text,
                    "saved\n"
                );
            }
            assert_eq!(sftp.load(Ordering::SeqCst), 0);
            assert!(
                base.bind_node(&node.0, RemoteAccess::Sftp, None)
                    .await
                    .is_err()
            );
            assert_eq!(sftp.load(Ordering::SeqCst), 1);
            fs.release_ide_session_for_node(&node.0);
            assert_eq!(
                handle.transport_status().await,
                ConnectionTransportStatus::Open
            );
            server.abort();
            let _ = server.await;
        }
    }
}
