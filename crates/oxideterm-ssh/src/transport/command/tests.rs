use super::*;
use crate::{
    ConnectionConsumer, RemoteEnvInfo, SshConfig, SshConnectionRegistry, SshTransportClient,
};
use russh::{ChannelId, server};
use std::sync::atomic::AtomicUsize;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Exec,
    Shell,
    Pty,
}

#[derive(Default)]
struct Counts {
    probes: AtomicUsize,
    payloads: AtomicUsize,
    authentications: AtomicUsize,
}

struct Peer {
    mode: Mode,
    counts: Arc<Counts>,
    channels: HashMap<ChannelId, Channel<server::Msg>>,
    ptys: HashSet<ChannelId>,
}

fn quoted_marker(command: &str, prefix: &str) -> String {
    let start = command.find(prefix).unwrap();
    command[start..]
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .next()
        .unwrap()
        .to_owned()
}

impl server::Handler for Peer {
    type Error = russh::Error;

    async fn auth_none(&mut self, _: &str) -> Result<server::Auth, Self::Error> {
        self.counts.authentications.fetch_add(1, Ordering::SeqCst);
        Ok(server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.ptys.insert(channel);
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let command = std::str::from_utf8(data).unwrap();
        if command.starts_with("echo __OXIDE_PROBE_") {
            self.counts.probes.fetch_add(1, Ordering::SeqCst);
        }
        if self.mode != Mode::Exec {
            session.channel_failure(channel)?;
            return session.close(channel);
        }
        session.channel_success(channel)?;
        if command.starts_with("echo __OXIDE_PROBE_") {
            session.data(
                channel,
                format!("{}\n", command.trim_start_matches("echo ")).into_bytes(),
            )?;
            session.exit_status_request(channel, 0)?;
        } else {
            self.counts.payloads.fetch_add(1, Ordering::SeqCst);
            if command.contains("drop-response") {
                return session.close(channel);
            }
            if command.contains("withhold-response") {
                return Ok(());
            }
            session.data(channel, b"answer\n".to_vec())?;
            session.extended_data(channel, 1, b"warning\n".to_vec())?;
            session.exit_status_request(channel, 7)?;
        }
        session.eof(channel)?;
        session.close(channel)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if self.mode == Mode::Pty && (!self.ptys.contains(&channel) || !data.ends_with(b"\r")) {
            return session.close(channel);
        }
        let command = std::str::from_utf8(data).unwrap();
        if command.contains("__OXIDE_PROBE_")
            || command.contains("__OXIDE_READY_")
            || command.contains("__OXIDE_MONITOR_READY__")
        {
            let prefix = if command.contains("__OXIDE_PROBE_") {
                "__OXIDE_PROBE_"
            } else if command.contains("__OXIDE_READY_") {
                "__OXIDE_READY_"
            } else {
                "__OXIDE_MONITOR_READY__"
            };
            let marker = quoted_marker(command, prefix);
            session.data(channel, format!("\r\n{marker}\r\n").into_bytes())?;
        } else if command.contains("__OXIDE_BEGIN_") {
            self.counts.payloads.fetch_add(1, Ordering::SeqCst);
            if command.contains("drop-response") {
                return session.close(channel);
            }
            if command.contains("withhold-response") {
                return Ok(());
            }
            let begin = quoted_marker(command, "__OXIDE_BEGIN_");
            let end = quoted_marker(command, "__OXIDE_DONE_");
            // Echo is separate from the actual frame and can contain both marker strings.
            session.data(channel, data.to_vec())?;
            let newline = if self.ptys.contains(&channel) {
                "\r\n"
            } else {
                "\n"
            };
            session.data(
                channel,
                format!("{newline}{begin}{newline}answer{newline}").into_bytes(),
            )?;
            if self.ptys.contains(&channel) {
                session.data(channel, b"warning\r\n".to_vec())?;
            } else {
                session.extended_data(channel, 1, b"warning\n".to_vec())?;
            }
            let trailer = format!("{newline}{end}:7{newline}");
            let split = trailer.len() - 4;
            session.data(channel, trailer.as_bytes()[..split].to_vec())?;
            session.data(channel, trailer.as_bytes()[split..].to_vec())?;
            if !self.ptys.contains(&channel) {
                // The stderr trailer can arrive after the stdout exit record.
                session.extended_data(channel, 1, format!("\n{end}_ERR\n").into_bytes())?;
            }
        }
        Ok(())
    }
}

async fn fixture(
    mode: Mode,
) -> (
    SshConnectionHandle,
    Arc<Counts>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let key = russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
        .unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let counts = Arc::new(Counts::default());
    let peer = Peer {
        mode,
        counts: counts.clone(),
        channels: HashMap::new(),
        ptys: HashSet::new(),
    };
    let task = tokio::spawn(async move {
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
    let mut config = SshConfig::password("127.0.0.1", port, "fixture", "synthetic-password");
    config.expected_host_key_fingerprint = Some(fingerprint);
    config.trust_host_key = Some(false);
    let handle = SshTransportClient::new(config)
        .connect_node_with_registry(
            SshConnectionRegistry::default(),
            ConnectionConsumer::NodeRouter("fixture".into()),
        )
        .await
        .unwrap();
    handle.set_remote_env(RemoteEnvInfo {
        os_type: "Linux".into(),
        os_version: None,
        kernel: None,
        arch: None,
        shell: Some("/bin/sh".into()),
        home: None,
        zdotdir: None,
        xdg_config_home: None,
        detected_at: 0,
    });
    (handle, counts, task)
}

#[tokio::test]
async fn command_selection_preserves_results_and_caches_per_connection() {
    for (mode, expected) in [
        (Mode::Exec, SshCommandChannel::Exec),
        (Mode::Shell, SshCommandChannel::Shell),
        (Mode::Pty, SshCommandChannel::PtyShell),
    ] {
        let (handle, counts, server) = fixture(mode).await;
        for _ in 0..2 {
            let result = handle
                .execute_command("business-command", Duration::from_secs(2), 8192)
                .await
                .unwrap();
            assert_eq!(result.channel, expected);
            assert_eq!(result.output.exit_code, Some(7));
            assert!(!result.output.truncated);
            if mode == Mode::Pty {
                assert!(result.stderr_merged);
                assert_eq!(result.output.stdout, "answer\nwarning\n");
                assert_eq!(result.output.stderr, "");
            } else {
                assert!(!result.stderr_merged);
                assert_eq!(result.output.stdout, "answer\n");
                assert_eq!(result.output.stderr, "warning\n");
            }
        }
        assert_eq!(counts.probes.load(Ordering::SeqCst), 1);
        assert_eq!(counts.payloads.load(Ordering::SeqCst), 2);
        assert_eq!(counts.authentications.load(Ordering::SeqCst), 1);
        handle.clear_physical().await;
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn command_read_failure_never_replays_payload() {
    for mode in [Mode::Exec, Mode::Shell, Mode::Pty] {
        for payload in ["drop-response", "withhold-response"] {
            let (handle, counts, server) = fixture(mode).await;
            let result = handle
                .execute_command(payload, Duration::from_secs(1), 8192)
                .await;
            assert!(matches!(
                result,
                Err(SshTransportError::CommandOutcomeUnknown)
            ));
            assert_eq!(counts.payloads.load(Ordering::SeqCst), 1);
            handle.clear_physical().await;
            server.abort();
            let _ = server.await;
        }
    }
}

#[test]
fn framed_capture_drains_truncated_output_until_real_status() {
    let mut capture = FramedCapture::new("begin".into(), "end".into(), 5, true, false);
    capture.data(b"echo begin end:9\r\nbeg");
    capture.data(b"in\r\n");
    for chunk in "abcdefghij".repeat(2000).as_bytes().chunks(13) {
        capture.data(chunk);
    }
    capture.data(b"\r\n\r\nend:");
    capture.data(b"23\r\nignored");
    let output = capture.finish();
    assert_eq!(output.stdout, "abcde");
    assert_eq!(output.exit_code, Some(23));
    assert!(output.truncated);
}

#[cfg(unix)]
#[test]
fn unix_frame_executes_quoted_payload_once_and_preserves_exit() {
    let begin = "__BEGIN_TEST__";
    let end = "__DONE_TEST__";
    let script = framed_script(
        "printf \"single'quote\\n\"; printf 'failure\\n' >&2; exit 9",
        begin,
        end,
        false,
        false,
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(script.as_str())
        .output()
        .unwrap();
    let mut capture = FramedCapture::new(begin.into(), end.into(), 8192, false, false);
    capture.data(&output.stdout);
    capture.stderr_data(&output.stderr);
    let output = capture.finish();
    assert_eq!(output.stdout, "single'quote\n");
    assert_eq!(output.stderr, "failure\n");
    assert_eq!(output.exit_code, Some(9));
}

#[tokio::test]
async fn command_cache_is_discarded_when_physical_transport_is_replaced() {
    let (handle, first_counts, first_server) = fixture(Mode::Exec).await;
    assert_eq!(
        handle
            .execute_command("business-command", Duration::from_secs(2), 8192)
            .await
            .unwrap()
            .channel,
        SshCommandChannel::Exec
    );
    let (replacement, replacement_counts, replacement_server) = fixture(Mode::Pty).await;
    handle.set_physical(replacement.physical::<PooledSshConnection>().unwrap());
    assert_eq!(
        handle
            .execute_command("business-command", Duration::from_secs(2), 8192)
            .await
            .unwrap()
            .channel,
        SshCommandChannel::PtyShell
    );
    assert_eq!(first_counts.payloads.load(Ordering::SeqCst), 1);
    assert_eq!(replacement_counts.payloads.load(Ordering::SeqCst), 1);
    assert_eq!(replacement_counts.probes.load(Ordering::SeqCst), 1);
    handle.clear_physical().await;
    replacement.clear_physical().await;
    first_server.abort();
    replacement_server.abort();
    let _ = first_server.await;
    let _ = replacement_server.await;
}

#[test]
fn frame_separator_preserves_payload_carriage_return() {
    let mut unix = FramedCapture::new("begin".into(), "end".into(), 8192, false, false);
    unix.data(b"begin\npayload\r\nend:0\n");
    assert_eq!(unix.finish().stdout, "payload\r");
    let mut windows = FramedCapture::new("begin".into(), "end".into(), 8192, false, true);
    windows.data(b"begin\r\npayload\r\nend:0\r\n");
    assert_eq!(windows.finish().stdout, "payload");
}

#[test]
fn command_debug_omits_remote_output() {
    let result = SshCommandExecution {
        channel: SshCommandChannel::Exec,
        stderr_merged: false,
        output: SshCommandOutput {
            stdout: "synthetic-secret-output".into(),
            stderr: "synthetic-private-error".into(),
            exit_code: Some(1),
            truncated: false,
        },
    };
    let debug = format!("{result:?}");
    assert!(!debug.contains("synthetic-secret-output"));
    assert!(!debug.contains("synthetic-private-error"));
}
