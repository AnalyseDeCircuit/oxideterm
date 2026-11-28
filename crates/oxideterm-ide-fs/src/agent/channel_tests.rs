use super::*;
use oxideterm_ssh::{
    ConnectionTransportStatus, SshConfig, SshConnectionRegistry, SshTransportClient,
};
#[cfg(unix)]
use oxideterm_terminal::{LocalPtyConfig, ShellInfo, TerminalEvent, TerminalSession};
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
    sftp_program: Option<&'static str>,
    upload_gate: Option<Arc<UploadGate>>,
    exec: bool,
    interactive: bool,
    channels: HashMap<ChannelId, tokio::task::JoinHandle<()>>,
    processes: HashMap<ChannelId, Process>,
    sftp: Arc<AtomicUsize>,
}
impl Peer {
    async fn listen(self) -> (u16, russh::keys::PublicKey, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let public_key = key.public_key().clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let connection = server::run_stream(
                Arc::new(server::Config {
                    keys: vec![key],
                    ..Default::default()
                }),
                stream,
                self,
            )
            .await
            .unwrap();
            let _ = connection.await;
        });
        (port, public_key, server)
    }

    async fn connect(
        self,
    ) -> (
        NodeAgentIdeFileSystem,
        NodeId,
        oxideterm_ssh::SshConnectionHandle,
        tokio::task::JoinHandle<()>,
    ) {
        let (port, key, server) = self.listen().await;
        let fingerprint = key.fingerprint(russh::keys::HashAlg::Sha256).to_string();
        let mut config = SshConfig::password("127.0.0.1", port, "fixture", "synthetic-password");
        config.expected_host_key_fingerprint = Some(fingerprint);
        config.trust_host_key = Some(false);
        let registry = SshConnectionRegistry::default();
        let node = NodeId::new("file-contract");
        let router = NodeRouter::new(registry.clone());
        router.upsert_node(node.clone(), config.clone());
        let handle = SshTransportClient::new(config)
            .connect_node_with_registry(registry, ConnectionConsumer::NodeRouter(node.0.clone()))
            .await
            .unwrap();
        router
            .bind_connection(&node, handle.connection_id().to_string())
            .unwrap();
        let base = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled);
        (base, node, handle, server)
    }

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
        if self.interactive {
            // OpenSSH drives a real Linux slave PTY so stty and echo have their usual semantics.
            command.args([
                "-ec",
                r#"mkdir "$HOME/work tree"
cd "$HOME/work tree"
printf 'remote file\n' > note.txt
exec script -qefc 'printf "remote-pty-ready\n"; exec /bin/sh -i' /dev/null"#,
            ]);
        } else if !self.exec {
            // Shell-only gateways can expose dd without GNU's byte-skip and status operands.
            command.args([
                "-ec",
                r#"mkdir -p "$HOME/test-bin"
cat > "$HOME/test-bin/dd" <<'SH'
#!/bin/sh
for arg do
    case "$arg" in iflag=*|status=*) exit 1 ;; esac
done
exec /bin/dd "$@"
SH
chmod +x "$HOME/test-bin/dd"
PATH="$HOME/test-bin:$PATH" exec /bin/sh"#,
            ]);
        }
        self.start_process(channel, session, command);
    }

    fn start_process(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
        mut command: tokio::process::Command,
    ) {
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
        name: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.sftp.fetch_add(1, Ordering::SeqCst);
        if let Some(program) = self.sftp_program.filter(|_| name == "sftp") {
            session.channel_success(channel)?;
            self.start_process(channel, session, tokio::process::Command::new(program));
            return Ok(());
        }
        session.channel_failure(channel)?;
        session.close(channel)
    }
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if self.interactive {
            session.channel_success(channel)
        } else {
            session.channel_failure(channel)
        }
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

#[cfg(unix)]
async fn output_until(terminal: &mut TerminalSession, marker: &[u8]) -> Vec<u8> {
    let activity = terminal.activity_receiver();
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bytes = Vec::new();
        loop {
            terminal.read_pending();
            for event in terminal.take_events() {
                if let TerminalEvent::Output(output) = event {
                    bytes.extend(output);
                }
            }
            if memchr::memmem::find(&bytes, marker).is_some() {
                return bytes;
            }
            assert!(activity.notified().await);
        }
    })
    .await
    .unwrap_or_else(|_| panic!("waiting for {}", String::from_utf8_lossy(marker)))
}

#[cfg(unix)]
fn local_test_terminal() -> TerminalSession {
    let config = LocalPtyConfig {
        shell: Some(ShellInfo::new("test-sh", "Shell", "/bin/sh")),
        load_profile: false,
        ..Default::default()
    };
    let mut terminal = TerminalSession::local_with_config_graphics_and_encoding(
        100,
        24,
        config,
        Default::default(),
        oxideterm_terminal::TerminalEncoding::Utf8,
        100,
    )
    .unwrap();
    terminal.set_output_events_enabled(true);
    terminal
}

#[cfg(unix)]
#[tokio::test]
async fn local_pty_ide_returns_the_unsupported_shell_to_its_user() {
    use std::os::unix::fs::PermissionsExt;

    let bin = tempfile::tempdir().unwrap();
    let uname = bin.path().join("uname");
    std::fs::write(&uname, "#!/bin/sh\nprintf 'Darwin\\n'\n").unwrap();
    std::fs::set_permissions(&uname, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut terminal = local_test_terminal();
    let prepare = format!(
        "export PATH='{}':\"$PATH\"; shell_only=original; printf 'ready-%s\\n' \"$(uname -s)\"\r",
        shell_single_quote(&bin.path().to_string_lossy()),
    );
    terminal.write_input(prepare.as_bytes()).unwrap();
    output_until(&mut terminal, b"ready-Darwin").await;

    let channel = terminal
        .acquire_shell_channel()
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let mut released = channel.release_receiver();
    let fs =
        NodeAgentIdeFileSystem::new(NodeRouter::new(Default::default()), NodeAgentMode::Disabled);
    let error = fs
        .from_shell_channel(channel, RemoteAccess::Commands, None)
        .await
        .err()
        .expect("the active shell reports Darwin");
    assert_eq!(error.kind, IdeFileErrorKind::Unsupported);
    tokio::time::timeout(Duration::from_secs(5), released.wait_for(|done| *done))
        .await
        .unwrap()
        .unwrap();
    terminal
        .write_input(b"printf 'resumed-%s\\n' \"$shell_only\"\r")
        .unwrap();
    output_until(&mut terminal, b"resumed-original").await;
    terminal.shutdown();
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "requires local Docker for a Linux shell behind loopback OpenSSH"
)]
async fn local_pty_ide_reads_and_restores_the_native_ssh_shell() {
    let home = tempfile::tempdir().unwrap();
    let peer = Peer {
        home: home.path().to_owned(),
        sftp_program: None,
        upload_gate: None,
        exec: false,
        interactive: true,
        channels: HashMap::new(),
        processes: HashMap::new(),
        sftp: Arc::new(AtomicUsize::new(0)),
    };
    let (port, key, server) = peer.listen().await;
    let known_hosts = home.path().join("known_hosts");
    std::fs::write(
        &known_hosts,
        format!("[127.0.0.1]:{port} {}\n", key.to_openssh().unwrap()),
    )
    .unwrap();
    let mut terminal = local_test_terminal();
    let login = format!(
        "/usr/bin/ssh -tt -F /dev/null -oBatchMode=yes -oStrictHostKeyChecking=yes -oUserKnownHostsFile='{}' -p {port} fixture@127.0.0.1\r",
        shell_single_quote(&known_hosts.to_string_lossy())
    );
    terminal.write_input(login.as_bytes()).unwrap();
    // script invokes the runner's shell with -c; bash clears an inherited PS1.
    // The marker comes from the slave PTY, independently of the user's prompt.
    output_until(&mut terminal, b"remote-pty-ready").await;
    terminal
        .write_input(b"shell_only=retained; printf 'prepared-%s\\n' yes\r")
        .unwrap();
    output_until(&mut terminal, b"prepared-yes").await;

    let channel = terminal
        .acquire_shell_channel()
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let token = channel.token.clone();
    let mut released = channel.release_receiver();
    let base =
        NodeAgentIdeFileSystem::new(NodeRouter::new(Default::default()), NodeAgentMode::Disabled);
    let (fs, cwd) = base
        .from_shell_channel(channel, RemoteAccess::Commands, None)
        .await
        .unwrap();
    let remote_home = if cfg!(target_os = "linux") {
        home.path()
    } else {
        std::path::Path::new("/root")
    };
    assert_eq!(std::path::Path::new(&cwd), remote_home.join("work tree"));
    let node = fs.terminal_session_id().unwrap();
    let file = IdeLocation::remote(node, format!("{cwd}/note.txt"));
    // If ordinary input reaches the PTY, this line shuts down the live file service.
    terminal.write_input(b"!quit\n").unwrap();
    let data = fs.read_file(&file, None).await.unwrap();
    assert_eq!(data.text.as_str(), "remote file\n");
    fs.write_file(
        &file,
        &"edited".into(),
        &data.format,
        Some(&data.version),
        WriteMode::AtomicReplace,
    )
    .await
    .unwrap();

    fs.release_ide_session_for_node(node);
    tokio::time::timeout(Duration::from_secs(10), released.wait_for(|done| *done))
        .await
        .unwrap()
        .unwrap();
    terminal
        .write_input(b"printf '%s:%s\\n' \"$shell_only\" \"$(cat note.txt)\"\r")
        .unwrap();
    let output = output_until(&mut terminal, b"retained:edited").await;
    assert!(
        memchr::memmem::find(&output, token.as_bytes()).is_none(),
        "IDE protocol reached terminal recording"
    );
    terminal.shutdown();
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "requires a local OpenSSH sftp-server executable"
)]
async fn sftp_search_skips_directory_links_and_binary_text_files() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let git = root.join(".git");
    std::fs::create_dir(&git).unwrap();
    std::fs::write(git.join("private.txt"), "needle").unwrap();
    std::os::unix::fs::symlink(&git, root.join("linked")).unwrap();
    std::fs::write(root.join("binary.txt"), b"needle\0bytes").unwrap();
    let visible = root.join("visible.txt");
    std::fs::write(&visible, "first\nneedle").unwrap();
    let program = ["/usr/libexec/sftp-server", "/usr/lib/openssh/sftp-server"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .expect("OpenSSH sftp-server required");
    let peer = Peer {
        home: root.clone(),
        sftp_program: Some(program),
        upload_gate: None,
        exec: false,
        interactive: false,
        channels: HashMap::new(),
        processes: HashMap::new(),
        sftp: Arc::new(AtomicUsize::new(0)),
    };
    let (base, node, _handle, server) = peer.connect().await;
    let fs = base
        .bind_node(&node.0, RemoteAccess::Sftp, None)
        .await
        .unwrap();
    let mut query =
        IdeSearchQuery::tauri_literal_project_search("needle", root.to_str().unwrap(), 20, 0);
    query.include_hidden = true;
    let hits = fs.search_project(&node.0, query).await.unwrap();
    let hits: Vec<_> = hits
        .iter()
        .map(|hit| (hit.path.as_str(), hit.line))
        .collect();
    assert_eq!(hits, [(visible.to_str().unwrap(), 2)]);
    fs.release_ide_session_for_node(&node.0);
    server.abort();
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
            let sftp = Arc::new(AtomicUsize::new(0));
            let upload_gate =
                (exec && access == RemoteAccess::Agent).then(|| Arc::new(UploadGate::default()));
            let peer = Peer {
                home: home.path().to_path_buf(),
                sftp_program: None,
                upload_gate: upload_gate.clone(),
                exec,
                interactive: false,
                channels: HashMap::new(),
                sftp: sftp.clone(),
                processes: HashMap::new(),
            };
            let (base, node, handle, server) = peer.connect().await;
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
                    &"hello €\n".into(),
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
                &"saved\n".into(),
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
                &"\0binary".into(),
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
            let local = tempfile::tempdir().unwrap();
            let source = local.path().join("上传 binary");
            let destination = local.path().join("download");
            let mut bytes = vec![0xff; 32768];
            bytes.extend([0, 1, 2]);
            std::fs::write(&source, &bytes).unwrap();
            let (_cancel, cancel) = watch::channel(false);
            let (progress, _updates) = watch::channel((0, 0));
            fs.transfer_file(
                binary.clone(),
                source,
                crate::TransferDirection::Upload,
                true,
                cancel.clone(),
                progress.clone(),
            )
            .await
            .unwrap();
            if exec {
                let previous = b"previous private download";
                std::fs::write(&destination, previous).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                let (_cancelled_tx, cancelled) = watch::channel(true);
                let cancelled_error = fs
                    .transfer_file(
                        IdeLocation::remote(
                            node.0.clone(),
                            format!("{remote_root}/missing-download"),
                        ),
                        destination.clone(),
                        crate::TransferDirection::Download,
                        true,
                        cancelled,
                        progress.clone(),
                    )
                    .await
                    .unwrap_err();
                assert_eq!(cancelled_error.kind, IdeFileErrorKind::Cancelled);
                assert_eq!(std::fs::read(&destination).unwrap(), previous);
            }
            fs.transfer_file(
                binary.clone(),
                destination.clone(),
                crate::TransferDirection::Download,
                exec,
                cancel,
                progress,
            )
            .await
            .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), bytes);
            #[cfg(unix)]
            if exec {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&destination)
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
            fs.delete_item(node.0.clone(), &binary_path, false)
                .await
                .unwrap();
            let project = format!("{remote_root}/ide-filenames");
            fs.create_folder(&node.0, &project).await.unwrap();
            fs.create_folder(&node.0, format!("{project}/src"))
                .await
                .unwrap();
            let unicode_path = format!("{project}/src/示例.py");
            let unicode = IdeLocation::remote(node.0.clone(), &unicode_path);
            let format = oxideterm_ide_core::TextFileFormat {
                encoding: "UTF-16LE".into(),
                has_bom: false,
                ..Default::default()
            };
            fs.write_file(
                &unicode,
                &"你好".into(),
                &format,
                None,
                WriteMode::CreateNew,
            )
            .await
            .unwrap();
            let mut index =
                oxideterm_ide_core::ProjectFileIndex::new(IdeLocation::remote(&node.0, &project));
            while let Some(directory) = index.next_directory() {
                index.add_entries(fs.list_dir(&directory).await.unwrap());
            }
            let hits = index.matches("示例", 10);
            assert_eq!(hits, [unicode], "exec={exec} access={access:?}");
            assert_eq!(
                fs.read_file(&hits[0], Some("UTF-16LE"))
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
