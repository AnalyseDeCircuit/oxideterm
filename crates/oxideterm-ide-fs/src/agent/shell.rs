// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use zeroize::Zeroizing;

const UPLOAD_CHUNK_BYTES: usize = 48 * 1024;
const UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(20);

const MAX_SHELL_FRAME: usize = 150 * 1024 * 1024;

impl AgentTransport {
    pub(super) async fn from_shell(
        mut channel: IdeChannel,
        progress: Option<watch::Sender<AgentInstallProgress>>,
    ) -> Result<Self, AgentError> {
        if let Some(progress) = &progress {
            progress.send_replace(AgentInstallProgress::Checking);
        }
        let token = channel.token().to_owned();
        let mut buffer = Zeroizing::new(Vec::new());
        let startup = async {
            channel
                .write(bootstrap(&token, channel.with_pty()).as_bytes())
                .await
                .map_err(|_| AgentError::ChannelClosed)?;
            let probe = tokio::time::timeout(
                Duration::from_secs(10),
                read_shell_line(&mut channel, &mut buffer, 4096),
            )
            .await
            .map_err(|_| AgentError::Timeout(10))?
            .map_err(|_| AgentError::StartFailed("Agent startup probe channel closed".into()))?;
            let prefix = format!("\x1e{token}:PROBE:");
            let (arch, version) = probe
                .strip_prefix(&prefix)
                .and_then(|probe| probe.split_once(':'))
                .ok_or_else(|| AgentError::StartFailed("Shell bootstrap failed".into()))?;
            if matches!(
                parse_remote_version_output(version),
                RemoteAgentInstallState::Current
            ) {
                channel
                    .write(b"reuse\n")
                    .await
                    .map_err(|_| AgentError::ChannelClosed)?;
            } else {
                let binary = resolve_agent_binary(arch_to_target(arch)?)?;
                let binary = Zeroizing::new(read_agent_binary_payload(&binary).await?);
                let total = binary.len();
                let encoded =
                    Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(&*binary));
                if let Some(progress) = &progress {
                    progress.send_replace(AgentInstallProgress::Uploading {
                        transferred: 0,
                        total,
                    });
                }
                channel
                    .write(b"upload\n")
                    .await
                    .map_err(|_| AgentError::ChannelClosed)?;
                let mut acknowledged = 0;
                for chunk in encoded.as_bytes().chunks(UPLOAD_CHUNK_BYTES) {
                    let transfer = async {
                        channel
                            .write_with_timeout(chunk, UPLOAD_IDLE_TIMEOUT)
                            .await
                            .map_err(|_| AgentError::ChannelClosed)?;
                        channel
                            .write(b"\n")
                            .await
                            .map_err(|_| AgentError::ChannelClosed)?;
                        let ack = read_shell_line(&mut channel, &mut buffer, 128).await?;
                        acknowledged += chunk.len();
                        if *ack != format!("\x1e{token}:UPLOADED:{acknowledged}") {
                            return Err(AgentError::StartFailed(
                                "Invalid agent upload acknowledgment".into(),
                            ));
                        }
                        Ok(())
                    };
                    // Flow control bounds queued data and resets the idle deadline on each remote receipt.
                    tokio::time::timeout(UPLOAD_IDLE_TIMEOUT, transfer)
                        .await
                        .map_err(|_| AgentError::Timeout(UPLOAD_IDLE_TIMEOUT.as_secs()))??;
                    if let Some(progress) = &progress {
                        progress.send_replace(AgentInstallProgress::Uploading {
                            transferred: (acknowledged / 4 * 3).min(total),
                            total,
                        });
                    }
                }
                channel
                    .write(b".\n")
                    .await
                    .map_err(|_| AgentError::ChannelClosed)?;
            }
            if let Some(progress) = &progress {
                progress.send_replace(AgentInstallProgress::Starting);
            }
            let ready = tokio::time::timeout(
                Duration::from_secs(10),
                read_shell_line(&mut channel, &mut buffer, 4096),
            )
            .await
            .map_err(|_| AgentError::Timeout(10))?
            .map_err(|_| AgentError::StartFailed("Agent installation or startup failed".into()))?;
            if *ready != format!("\x1e{token}:READY") {
                return Err(AgentError::StartFailed("Agent did not become ready".into()));
            }
            Ok(())
        };
        if let Err(error) = startup.await {
            // Cleanup completes before the caller reports failure; the shared node stays alive.
            let _ = channel.restore().await;
            return Err(error);
        }
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (write_tx, mut write_rx) = mpsc::channel::<Zeroizing<String>>(16);
        let (watch_tx, _) = broadcast::channel(16);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let task_pending = pending.clone();
        let task_alive = alive.clone();
        let task_watch = watch_tx.clone();
        // This worker owns the PTY lease. Shutdown is explicit and bounded; dropping
        // the last transport requests it, and failure closes only this PTY channel.
        let worker = tokio::spawn(async move {
            let mut stopping = false;
            let graceful = loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.recv() => break true,
                    Some(line) = write_rx.recv() => {
                        let result = async {
                            let mut request: serde_json::Value = serde_json::from_str(&line).map_err(|_| AgentError::ChannelClosed)?;
                            request["session"] = serde_json::json!(token);
                            let mut bytes = Zeroizing::new(serde_json::to_vec(&request).map_err(|_| AgentError::ChannelClosed)?);
                            if bytes.len() >= MAX_SHELL_FRAME { return Err(AgentError::ChannelClosed); }
                            bytes.push(b'\n');
                            channel.write(&bytes).await.map_err(|_| AgentError::ChannelClosed)?;
                            read_shell_frame(&mut channel, &mut buffer, &token).await
                        };
                        let result = {
                            tokio::pin!(result);
                            tokio::select! {
                                result = tokio::time::timeout(Duration::from_secs(300), &mut result) => result,
                                _ = shutdown_rx.recv() => {
                                    stopping = true;
                                    task_alive.store(false, Ordering::Release);
                                    tokio::time::timeout(Duration::from_secs(5), &mut result).await
                                }
                            }
                        };
                        match result {
                            Ok(Ok(line)) => handle_agent_line(&task_pending, &task_watch, &line).await,
                            _ => break false,
                        }
                        if stopping { break true; }
                    }
                    output = channel.recv() => {
                        // Unsolicited bytes cannot be interpreted as a new shell context.
                        if output.is_none() { break false; }
                        if let Some(bytes) = output { buffer.extend_from_slice(&bytes); }
                        if buffer.len() > MAX_SHELL_FRAME { break false; }
                    }
                }
            };
            task_alive.store(false, Ordering::Release);
            if graceful {
                let _ = tokio::time::timeout(Duration::from_secs(5), async {
                    let request = serde_json::json!({"id": 0, "session": token, "method": "sys/shutdown", "params": {}}).to_string() + "\n";
                    channel.write(request.as_bytes()).await.map_err(|_| ())?;
                    while channel.recv().await.is_some() {}
                    Ok::<_, ()>(())
                }).await;
            }
            for (_, tx) in task_pending.lock().await.drain() {
                let _ = tx.send(Err(AgentRpcError {
                    code: -32603,
                    message: "Shell IDE channel closed".into(),
                }));
            }
        });
        Ok(Self {
            _worker: Some(worker),
            request_limit: Some(Arc::new(tokio::sync::Semaphore::new(16))),
            write_tx,
            pending,
            watch_tx,
            shutdown_tx,
            alive,
        })
    }
}

impl Drop for AgentTransport {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.try_send(());
    }
}

fn bootstrap(token: &str, pty: bool) -> String {
    // All variable shell input is generated locally. Remote paths never become shell syntax.
    let relative_path = AGENT_REMOTE_PATH
        .strip_prefix("~/")
        .expect("home-relative agent path");
    let relative_dir = relative_path.rsplit_once('/').expect("agent directory").0;
    let body = format!(
        r#"[ -n "$HOME" ] || exit 1
oxide_agent="$HOME/{relative_path}"
oxide_version=$("$oxide_agent" --version </dev/null 2>/dev/null) || oxide_version=NOT_FOUND
printf '\036%s:PROBE:%s:%s\n' '{token}' "$(uname -m)" "$oxide_version"
IFS= read -r oxide_action || exit 1
case "$oxide_action" in
 reuse) [ -x "$oxide_agent" ] || exit 1;;
 upload)
  (
   umask 077
   oxide_upload="$oxide_agent.upload-{token}"
   oxide_encoded="$oxide_agent.b64-{token}"
   trap 'rm -f -- "$oxide_upload" "$oxide_encoded"' EXIT
   trap 'exit 1' HUP INT TERM
   for oxide_tool in base64 chmod mkdir mv rm; do command -v "$oxide_tool" >/dev/null || exit 1; done
   mkdir -p -- "$HOME/{relative_dir}" || exit 1
   set -C
   : > "$oxide_encoded" || exit 1
   oxide_received=0
   while IFS= read -r oxide_line; do
    case "$oxide_line" in .) break;; '#oxideterm-cancel-{token}') exit 1;; esac
    printf '%s' "$oxide_line" >> "$oxide_encoded" || exit 1
    oxide_received=$((oxide_received + ${{#oxide_line}}))
    printf '\036%s:UPLOADED:%s\n' '{token}' "$oxide_received"
   done
   base64 -d "$oxide_encoded" > "$oxide_upload" || exit 1
   chmod 700 "$oxide_upload" || exit 1
   "$oxide_upload" --version </dev/null >/dev/null 2>&1 || exit 1
   mv -f -- "$oxide_upload" "$oxide_agent" || exit 1
  ) || exit 1;;
 *) exit 1;;
esac
"$oxide_agent" --shell-session '{token}' 2>/dev/null
"#
    );
    let script = bootstrap_script(token, &body, pty);
    format!(
        " /bin/sh -c '{}'{}",
        shell_single_quote(&script),
        if pty { "\r" } else { "\n" }
    )
}

pub(super) fn bootstrap_script(token: &str, body: &str, pty: bool) -> String {
    // Only PTY transports alter terminal modes; every channel shares the same framing.
    let tty_setup = if pty {
        "oxide_tty=$(stty -g) || exit 1\nstty raw -echo || exit 1"
    } else {
        ""
    };
    format!(
        r#"oxide_tty=
oxide_dir=
trap 'oxide_result=$?; test -z "$oxide_dir" || rm -rf -- "$oxide_dir"; if test -z "$oxide_tty" || stty "$oxide_tty"; then printf "\\036%s:RESTORED\\037" "{token}"; fi; exit "$oxide_result"' EXIT
trap 'exit 1' HUP INT TERM
{tty_setup}
[ "$(uname -s)" = Linux ] || exit 1
{body}"#
    )
}

fn take_shell_line(
    buffer: &mut Zeroizing<Vec<u8>>,
    limit: usize,
) -> Result<Option<Zeroizing<String>>, AgentError> {
    const MAX_PREAMBLE_BYTES: usize = 64 * 1024;
    while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
        let start = buffer[..end].iter().position(|b| *b == 0x1e);
        if start.is_some_and(|start| end - start > limit || start > MAX_PREAMBLE_BYTES)
            || start.is_none() && end > MAX_PREAMBLE_BYTES
        {
            return Err(AgentError::ChannelClosed);
        }
        // Interactive shells can emit readline/OSC controls without a newline
        // immediately before the child starts. Only the framed suffix is UTF-8.
        let line = start
            .map(|start| String::from_utf8(buffer[start..end].to_vec()))
            .transpose()
            .map_err(|_| AgentError::ChannelClosed)?
            .map(Zeroizing::new);
        zeroize::Zeroize::zeroize(&mut buffer[..=end]);
        buffer.drain(..=end);
        if line.is_some() {
            return Ok(line);
        }
    }
    if buffer.len() > MAX_PREAMBLE_BYTES + limit {
        return Err(AgentError::ChannelClosed);
    }
    Ok(None)
}

pub(super) async fn read_shell_line(
    channel: &mut IdeChannel,
    buffer: &mut Zeroizing<Vec<u8>>,
    limit: usize,
) -> Result<Zeroizing<String>, AgentError> {
    loop {
        if let Some(line) = take_shell_line(buffer, limit)? {
            return Ok(line);
        }
        let chunk = channel.recv().await.ok_or(AgentError::ChannelClosed)?;
        buffer.extend_from_slice(&chunk);
    }
}

async fn read_shell_frame(
    channel: &mut IdeChannel,
    buffer: &mut Zeroizing<Vec<u8>>,
    token: &str,
) -> Result<Zeroizing<String>, AgentError> {
    let header = read_shell_line(channel, buffer, 128).await?;
    let prefix = format!("\x1e{token}:");
    let len: usize = header
        .strip_prefix(&prefix)
        .and_then(|v| v.parse().ok())
        .filter(|n| *n <= MAX_SHELL_FRAME)
        .ok_or(AgentError::ChannelClosed)?;
    while buffer.len() <= len {
        let chunk = channel.recv().await.ok_or(AgentError::ChannelClosed)?;
        buffer.extend_from_slice(&chunk);
        if buffer.len() > MAX_SHELL_FRAME + 4096 {
            return Err(AgentError::ChannelClosed);
        }
    }
    if buffer[len] != b'\n' {
        return Err(AgentError::ChannelClosed);
    }
    let line = Zeroizing::new(
        String::from_utf8(buffer[..len].to_vec()).map_err(|_| AgentError::ChannelClosed)?,
    );
    zeroize::Zeroize::zeroize(&mut buffer[..=len]);
    buffer.drain(..=len);
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker for a Linux PTY"
    )]
    fn shell_agent_persists_in_home_reuses_install_and_preserves_it_on_failure() {
        let token = "0123456789abcdef0123456789abcdef";
        let agent = format!(
            r#"#!/bin/sh
if [ "$1" = --version ]; then
 printf 'oxideterm-agent fixture compat {}\n'
else
 printf '\036%s:READY\n' "$2"
 IFS= read -r request
 [ "$request" = quit ] || exit 69
fi
"#,
            CURRENT_AGENT_COMPATIBILITY_VERSION
        );
        let input = serde_json::json!({"token":token,"bootstrap":bootstrap(token, true).trim(),"agent":agent,"compatibility":CURRENT_AGENT_COMPATIBILITY_VERSION});
        let python = r#"
import base64,json,os,pty,select,shutil,stat,subprocess,sys,tempfile,termios
p=json.load(sys.stdin)
root=tempfile.mkdtemp(prefix="oxide home' ")
directory=os.path.join(root,'.oxideterm')
os.mkdir(directory)
path=os.path.join(directory,'oxideterm-agent')
with open(path,'w') as f: f.write('#!/bin/sh\necho oxideterm-agent old compat 0\n')
os.chmod(path,0o700)
try:
 for action in ['install','reuse','invalid upload','cancel upload']:
  master,slave=pty.openpty()
  baseline=termios.tcgetattr(master)
  environment=dict(os.environ,HOME=root,TMPDIR='/unavailable/temporary-directory')
  process=subprocess.Popen(['/bin/sh','-c',p['bootstrap']],stdin=slave,stdout=slave,stderr=slave,cwd=root,env=environment)
  os.close(slave)
  buffer=b''
  def send(data):
   while data:
    count=os.write(master,data)
    data=data[count:]
  def receive():
   global buffer
   if not select.select([master],[],[],10)[0]: raise RuntimeError(action+': PTY response timeout')
   buffer+=os.read(master,65536)
  def line():
   global buffer
   while b'\n' not in buffer: receive()
   result,buffer=buffer.split(b'\n',1)
   return result
  try:
   probe=line().decode()
   assert probe.startswith('\x1e'+p['token']+':PROBE:'),probe
   expected='oxideterm-agent old compat 0' if action=='install' else 'oxideterm-agent fixture compat '+str(p['compatibility'])
   assert probe.endswith(':'+expected),probe
   if action=='reuse':
    send(b'reuse\n')
   else:
    send(b'upload\n')
    if action=='cancel upload':
     send(('#oxideterm-cancel-'+p['token']+'\n').encode())
    else:
     payload=p['agent'].encode() if action=='install' else b'not an executable\n'
     encoded=base64.b64encode(payload)
     send(encoded+b'\n.\n')
     assert line()==('\x1e'+p['token']+':UPLOADED:'+str(len(encoded))).encode()
   if action in ['install','reuse']:
    assert line()==('\x1e'+p['token']+':READY').encode()
    send(b'quit\n' if action=='install' else b'fail\n')
   marker=('\x1e'+p['token']+':RESTORED\x1f').encode()
   while marker not in buffer: receive()
   assert process.wait(timeout=10)==(0 if action=='install' else 1 if action!='reuse' else 69)
   assert termios.tcgetattr(master)==baseline
   with open(path) as f: assert f.read()==p['agent']
   assert stat.S_IMODE(os.stat(path).st_mode)==0o700
   assert os.listdir(directory)==['oxideterm-agent']
   if action=='install': installed_inode=os.stat(path).st_ino
   else: assert os.stat(path).st_ino==installed_inode
  finally:
   if process.poll() is None: process.kill();process.wait()
   os.close(master)
 print('persistent agent reused and retained after failures')
finally:
 shutil.rmtree(root)
"#;
        let output = super::super::tests::run_linux(
            "python3",
            &["-c", python],
            input.to_string().as_bytes(),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            b"persistent agent reused and retained after failures\n"
        );
    }

    #[test]
    fn startup_frame_after_shell_control_sequences_is_not_discarded() {
        let mut buffer = Zeroizing::new(b"\x1b[?2004l\r\x1b]133;C\x07\x1e0123456789abcdef0123456789abcdef:PROBE:x86_64\nfollowing bytes".to_vec());
        let line = take_shell_line(&mut buffer, 4096).unwrap();
        assert_eq!(
            line.as_deref().map(String::as_str),
            Some("\x1e0123456789abcdef0123456789abcdef:PROBE:x86_64")
        );
        assert_eq!(&*buffer, b"following bytes");
    }

    #[test]
    fn startup_frame_split_after_non_utf8_preamble_preserves_following_response() {
        let mut buffer =
            Zeroizing::new(b"echoed command\r\n\xff\x1b[?2004l\r\x1e01234567".to_vec());
        assert!(take_shell_line(&mut buffer, 128).unwrap().is_none());
        buffer.extend_from_slice(b"89abcdef0123456789abcdef:READY\nresponse bytes");
        let line = take_shell_line(&mut buffer, 128).unwrap().unwrap();
        assert_eq!(&*line, "\x1e0123456789abcdef0123456789abcdef:READY");
        assert_eq!(&*buffer, b"response bytes");
    }

    #[tokio::test]
    async fn bound_file_requests_never_resolve_another_context_or_fallback_to_sftp() {
        let registry = oxideterm_ssh::SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let mut fs = NodeAgentIdeFileSystem::new(router, NodeAgentMode::Enabled);
        let (write_tx, mut requests) = mpsc::channel::<Zeroizing<String>>(1);
        let (shutdown_tx, mut shutdown) = mpsc::channel(1);
        let (watch_tx, _) = broadcast::channel(1);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        fs.binding = Some(Arc::new(RemoteBinding {
            id: "shell:container-root".into(),
            owner_id: fs.owner_id,
            terminal: true,
            ssh_shell: false,
            owns_transport: true,
            audit: None,
            session: Arc::new(AgentSession::new(
                AgentTransport {
                    _worker: None,
                    request_limit: None,
                    write_tx,
                    pending: pending.clone(),
                    watch_tx,
                    shutdown_tx,
                    alive: alive.clone(),
                },
                SysInfoResult {
                    version: "test".into(),
                    compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    arch: "x86_64".into(),
                    os: "linux".into(),
                    pid: 7,
                    capabilities: vec!["file-bytes".into()],
                },
            )),
        }));
        let location = IdeLocation::remote("shell:container-root", "/etc/config");
        let request_fs = fs.clone();
        let read = tokio::spawn(async move { request_fs.read_file(&location, None).await });
        let prefix: serde_json::Value =
            serde_json::from_str(&requests.recv().await.unwrap()).unwrap();
        assert_eq!(prefix["method"], "fs/readFilePrefix");
        assert_eq!(prefix["params"], serde_json::json!({"path":"/etc/config"}));
        pending
            .lock()
            .await
            .remove(&prefix["id"].as_u64().unwrap())
            .unwrap()
            .send(Ok(serde_json::json!("Y29udGFpbmVyCg==")))
            .unwrap();
        let request: serde_json::Value =
            serde_json::from_str(&requests.recv().await.unwrap()).unwrap();
        assert_eq!(request["method"], "fs/readFile");
        assert_eq!(
            request["params"],
            serde_json::json!({"path": "/etc/config", "encoding": "base64"})
        );
        let id = request["id"].as_u64().unwrap();
        let response = pending.lock().await.remove(&id).unwrap();
        response.send(Ok(serde_json::json!({"content":"Y29udGFpbmVyCg==", "encoding":"base64", "size":10, "mtime":7, "hash":"container-hash"}))).unwrap();
        let file = read.await.unwrap().unwrap();
        assert_eq!(&*file.text, "container\n");
        assert_eq!(file.version.etag.as_deref(), Some("container-hash"));
        let binary_fs = fs.clone();
        let binary = tokio::spawn(async move {
            binary_fs
                .read_file(
                    &IdeLocation::remote("shell:container-root", "/bin/program"),
                    None,
                )
                .await
        });
        let prefix: serde_json::Value =
            serde_json::from_str(&requests.recv().await.unwrap()).unwrap();
        assert_eq!(prefix["method"], "fs/readFilePrefix");
        pending
            .lock()
            .await
            .remove(&prefix["id"].as_u64().unwrap())
            .unwrap()
            .send(Ok(serde_json::json!("AAEC")))
            .unwrap();
        assert_eq!(
            binary.await.unwrap().unwrap_err().kind,
            IdeFileErrorKind::Binary
        );
        assert!(
            matches!(requests.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "binary classification must not request the full file"
        );
        assert_eq!(
            fs.read_file(&IdeLocation::remote("host-node", "/etc/config"), None)
                .await
                .unwrap_err()
                .kind,
            IdeFileErrorKind::Disconnected
        );
        // Even a method error that normally permits host fallback stays on this shell.
        for method in [
            "fs/readFile",
            "fs/listDir",
            "fs/stat",
            "fs/remove",
            "search/grep",
        ] {
            let request_fs = fs.clone();
            let operation = tokio::spawn(async move {
                let location = IdeLocation::remote("shell:container-root", "/etc/config");
                match method {
                    "fs/readFile" => request_fs.read_file(&location, None).await.map(|_| ()),
                    "fs/listDir" => request_fs.list_dir(&location).await.map(|_| ()),
                    "fs/stat" => request_fs.stat(&location).await.map(|_| ()),
                    "fs/remove" => {
                        request_fs
                            .delete_item("shell:container-root", "/etc/config", false)
                            .await
                    }
                    _ => request_fs
                        .grep_project("shell:container-root", "value", "/etc", true, 10)
                        .await
                        .map(|_| ()),
                }
            });
            let request: serde_json::Value =
                serde_json::from_str(&requests.recv().await.unwrap()).unwrap();
            assert_eq!(
                request["method"],
                if method == "fs/readFile" {
                    "fs/readFilePrefix"
                } else {
                    method
                }
            );
            let response = pending
                .lock()
                .await
                .remove(&request["id"].as_u64().unwrap())
                .unwrap();
            response
                .send(Err(AgentRpcError {
                    code: -32601,
                    message: "shell method unavailable".into(),
                }))
                .unwrap();
            assert_eq!(
                operation.await.unwrap().unwrap_err().message,
                "shell method unavailable",
                "{method}"
            );
        }
        let external_scope = fs.scoped_owner();
        external_scope.release_all_ide_consumers();
        assert!(matches!(
            shutdown.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        fs.release_ide_session_for_node("shell:container-root");
        assert_eq!(shutdown.try_recv().unwrap(), ());
        alive.store(false, Ordering::Release);
        assert_eq!(
            fs.list_dir(&IdeLocation::remote("shell:container-root", "/etc"))
                .await
                .unwrap_err()
                .kind,
            IdeFileErrorKind::Disconnected
        );
        assert_eq!(
            fs.open_project("shell:container-root", "/etc")
                .await
                .unwrap_err()
                .kind,
            IdeFileErrorKind::Disconnected
        );
    }
}
