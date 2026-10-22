// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use zeroize::Zeroizing;

const SEARCH_OUTPUT_LIMIT: usize = 1024 * 1024;
const OUTPUT_LIMIT: usize = 150 * 1024 * 1024;
const TOOLS: &str = "[ \"$(uname -s)\" = Linux ] || exit 69; for t in sh base64 find stat sha256sum cut readlink mktemp cp mv ln rm rmdir mkdir grep cat head chmod dirname bash mkfifo; do command -v \"$t\" >/dev/null || exit 69; done; stat --version >/dev/null 2>&1 && find --version >/dev/null 2>&1 && cp --version >/dev/null 2>&1 && grep --version >/dev/null 2>&1 || exit 69";

// Both execution contexts consume the same generated scripts and byte results.
// Only a terminal transport owns stty state and a restoration handshake.
struct CommandContext {
    channel: IdeChannel,
    buffer: Zeroizing<Vec<u8>>,
}

fn quoted(value: &str) -> Result<String, AgentError> {
    if value.contains('\0') {
        return Err(AgentError::StartFailed("NUL in file argument".into()));
    }
    Ok(format!("'{}'", shell_single_quote(value)))
}

impl CommandContext {
    async fn execute(&mut self, script: &str) -> Result<Zeroizing<Vec<u8>>, AgentError> {
        let Self { channel, buffer } = self;
        let payload = Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(script));
        channel
            .write(payload.as_bytes())
            .await
            .map_err(|_| AgentError::ChannelClosed)?;
        channel
            .write(b"\n")
            .await
            .map_err(|_| AgentError::ChannelClosed)?;
        let header = super::shell::read_shell_line(channel, buffer, 128).await?;
        let prefix = format!("\x1e{}:", channel.token());
        let status = header
            .strip_prefix(&prefix)
            .and_then(|v| v.parse::<i32>().ok())
            .ok_or(AgentError::ChannelClosed)?;
        let encoded = read_payload_line(channel, buffer).await?;
        if status != 0 {
            return Err(command_error(status));
        }
        Ok(Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(&*encoded)
                .map_err(|_| AgentError::ChannelClosed)?,
        ))
    }

    async fn shutdown(&mut self) {
        let _ = self.channel.restore().await;
    }
}

async fn read_payload_line(
    channel: &mut IdeChannel,
    buffer: &mut Zeroizing<Vec<u8>>,
) -> Result<Zeroizing<Vec<u8>>, AgentError> {
    loop {
        if let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let result = Zeroizing::new(buffer[..end].to_vec());
            zeroize::Zeroize::zeroize(&mut buffer[..=end]);
            buffer.drain(..=end);
            return Ok(result);
        }
        if buffer.len() > OUTPUT_LIMIT {
            return Err(AgentError::ChannelClosed);
        }
        buffer.extend_from_slice(&channel.recv().await.ok_or(AgentError::ChannelClosed)?);
    }
}

fn command_error(status: i32) -> AgentError {
    let (code, message) = match status {
        44 => (-32001, "File not found"),
        45 => (-4, "CONFLICT: File changed or already exists"),
        69 => (-32601, "Linux GNU file utilities are required"),
        _ => (-32000, "File command failed"),
    };
    AgentError::Rpc {
        code,
        message: message.into(),
    }
}

fn param<'a>(params: &'a serde_json::Value, key: &str) -> Result<&'a str, AgentError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AgentError::StartFailed("Missing file argument".into()))
}

// Metadata and file bytes use NUL separators, never ls or delimiter-based filenames.
const META: &str = "stat -Lc '%s %Y %a' -- \"$p\"";
const VERSION: &str = "printf '%s\\0' \"$(stat -Lc '%s' -- \"$p\")\" \"$(stat -Lc '%Y' -- \"$p\")\" \"$(sha256sum < \"$p\" | cut -d ' ' -f 1)\"";

struct CommandOperation {
    script: Zeroizing<String>,
    method: String,
    max_results: usize,
}
impl CommandOperation {
    fn new(method: &str, mut params: serde_json::Value) -> Result<Self, AgentError> {
        let script = match method {
            "sys/cwd" => "pwd -P".into(),
            "fs/project" => format!(
                "p={}; cd -- \"$p\" || exit 44; pwd -P",
                quoted(param(&params, "path")?)?
            ),
            "fs/stat" => format!(
                "p={}; if [ ! -e \"$p\" ]; then printf missing; else [ -d \"$p\" ] && printf 'directory ' || printf 'file '; {META}; fi",
                quoted(param(&params, "path")?)?
            ),
            "fs/listDir" => format!(
                "p={}; [ -d \"$p\" ] || exit 44; find \"$p\" -mindepth 1 -maxdepth 1 -printf '%f\\0%p\\0%y\\0%Y\\0%s\\0%T@\\0%m\\0%l\\0'",
                quoted(param(&params, "path")?)?
            ),
            "fs/readFilePrefix" => format!(
                "p={}; [ -f \"$p\" ] || exit 44; head -c {} -- \"$p\"",
                quoted(param(&params, "path")?)?,
                crate::text::FILE_PREFIX_BYTES,
            ),
            "fs/readFile" => format!(
                "p={}; [ -f \"$p\" ] || exit 44; [ \"$(stat -Lc '%s' -- \"$p\")\" -le {} ] || exit 1; {VERSION}; cat -- \"$p\"",
                quoted(param(&params, "path")?)?,
                oxideterm_ide_core::MAX_EDITABLE_FILE_SIZE
            ),
            "fs/createFile" | "fs/writeIdeFile" => write_script(method, &params)?,
            "fs/mkdir" => format!("mkdir -- {}", quoted(param(&params, "path")?)?),
            "fs/remove" => format!(
                "p={}; if [ -d \"$p\" ] && [ ! -L \"$p\" ] && [ {} = false ]; then rmdir -- \"$p\"; else rm {} -- \"$p\"; fi",
                quoted(param(&params, "path")?)?,
                params["recursive"].as_bool().unwrap_or(false),
                if params["recursive"].as_bool() == Some(true) {
                    "-r"
                } else {
                    ""
                },
            ),
            "fs/rename" => format!(
                "[ ! -e {to} ] && [ ! -L {to} ] || exit 45; mv -T -- {from} {to}",
                from = quoted(param(&params, "old_path")?)?,
                to = quoted(param(&params, "new_path")?)?
            ),
            "fs/copy" => format!(
                "[ ! -e {to} ] && [ ! -L {to} ] || exit 45; cp -aT -- {from} {to}",
                from = quoted(param(&params, "source_path")?)?,
                to = quoted(param(&params, "target_path")?)?
            ),
            "search/grep" => search_script(&params)?,
            _ => return Err(command_error(69)),
        };
        let max_results = params["max_results"].as_u64().unwrap_or(1000) as usize;
        if let Some(serde_json::Value::String(content)) = params.get_mut("content") {
            zeroize::Zeroize::zeroize(content);
        }
        Ok(Self {
            script: Zeroizing::new(format!("set -e\n{script}")),
            method: method.into(),
            max_results,
        })
    }
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value, AgentError> {
        use serde_json::json;
        let text = || std::str::from_utf8(bytes).map_err(|_| AgentError::ChannelClosed);
        match self.method.as_str() {
            "sys/cwd" => Ok(json!(text()?.strip_suffix('\n').unwrap_or(text()?))),
            "fs/project" => {
                let path = text()?.strip_suffix('\n').unwrap_or(text()?);
                Ok(
                    json!({"rootPath":path,"name":path.rsplit('/').find(|s|!s.is_empty()).unwrap_or("/"),"isGitRepo":false,"gitBranch":null,"fileCount":0}),
                )
            }
            "fs/stat" => {
                let fields: Vec<_> = text()?.split_whitespace().collect();
                if fields == ["missing"] {
                    return Ok(
                        json!({"exists":false,"file_type":null,"size":null,"mtime":null,"permissions":null}),
                    );
                }
                if fields.len() != 4 {
                    return Err(AgentError::ChannelClosed);
                }
                Ok(
                    json!({"exists":true,"file_type":fields[0],"size":fields[1].parse::<u64>().map_err(|_|AgentError::ChannelClosed)?,"mtime":fields[2].parse::<u64>().map_err(|_|AgentError::ChannelClosed)?,"permissions":fields[3]}),
                )
            }
            "fs/readFilePrefix" => Ok(json!(
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )),
            "fs/readFile" | "fs/createFile" | "fs/writeIdeFile" => {
                let mut fields = bytes.splitn(4, |b| *b == 0);
                let number = |b: Option<&[u8]>| -> Result<u64, AgentError> {
                    std::str::from_utf8(b.ok_or(AgentError::ChannelClosed)?)
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .ok_or(AgentError::ChannelClosed)
                };
                let size = number(fields.next())?;
                let mtime = number(fields.next())?;
                let hash = std::str::from_utf8(fields.next().ok_or(AgentError::ChannelClosed)?)
                    .map_err(|_| AgentError::ChannelClosed)?;
                if self.method == "fs/readFile" {
                    Ok(
                        json!({"size":size,"mtime":mtime,"hash":hash,"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(fields.next().ok_or(AgentError::ChannelClosed)?)}),
                    )
                } else {
                    Ok(
                        json!({"size":size,"mtime":mtime,"hash":hash,"atomic":self.method!="fs/createFile"}),
                    )
                }
            }
            "fs/listDir" => {
                let fields: Vec<_> = bytes.split(|b| *b == 0).collect();
                if fields.last() != Some(&&b""[..]) || (fields.len() - 1) % 8 != 0 {
                    return Err(AgentError::ChannelClosed);
                }
                let mut entries = Vec::new();
                for row in fields[..fields.len() - 1].chunks(8) {
                    let strings: Vec<_> = row
                        .iter()
                        .map(|v| std::str::from_utf8(v).map_err(|_| AgentError::ChannelClosed))
                        .collect::<Result<_, _>>()?;
                    let kind = |v: &str| match v {
                        "d" => "directory",
                        "f" => "file",
                        "l" => "symlink",
                        _ => "other",
                    };
                    entries.push(json!({"name":strings[0],"path":strings[1],"file_type":kind(strings[2]),"is_symlink":strings[2]=="l","symlink_target":strings[7],"target_file_type":kind(strings[3]),"size":strings[4].parse::<u64>().map_err(|_|AgentError::ChannelClosed)?,"mtime":strings[5].split('.').next().and_then(|s|s.parse::<u64>().ok()),"permissions":strings[6],"children":null,"truncated":false}));
                }
                Ok(json!(entries))
            }
            "search/grep" => {
                let mut rest = bytes;
                let mut hits = Vec::new();
                let limit = self.max_results;
                while !rest.is_empty() && hits.len() < limit {
                    let end = rest
                        .iter()
                        .position(|b| *b == 0)
                        .ok_or(AgentError::ChannelClosed)?;
                    let path =
                        std::str::from_utf8(&rest[..end]).map_err(|_| AgentError::ChannelClosed)?;
                    rest = &rest[end + 1..];
                    let end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
                    let line = String::from_utf8_lossy(&rest[..end]);
                    let (number, text) = line.split_once(':').ok_or(AgentError::ChannelClosed)?;
                    hits.push(json!({"path":path,"line":number.parse::<u32>().map_err(|_|AgentError::ChannelClosed)?,"column":1,"text":text}));
                    rest = rest.get(end + 1..).unwrap_or_default();
                }
                Ok(json!(hits))
            }
            _ => Ok(json!({})),
        }
    }
}

// Bound both reads before allocation, preserving NUL-delimited filenames and
// complete lines. The parent owns and terminates the producer when a limit is reached.
fn search_script(params: &serde_json::Value) -> Result<String, AgentError> {
    let limit = params["max_results"].as_u64().unwrap_or(1000);
    Ok(format!(
        r#"[ {limit} -gt 0 ] || exit 0
search_dir=$(mktemp -d) || exit 1
producer=''
trap 'if [ -n "$producer" ]; then kill "$producer" 2>/dev/null || :; wait "$producer" 2>/dev/null || :; fi; rm -rf -- "$search_dir"' EXIT HUP INT TERM
mkfifo "$search_dir/pipe"
grep -r -H -n -Z -I -E --line-buffered {case} -- {pattern} {path} > "$search_dir/pipe" &
producer=$!
limiter_status=0
LC_ALL=C bash -c '
remaining=$1
limit=$2
marker=$3
count=0
while (( remaining > 0 )); do
    IFS= read -r -d "" -n "$remaining" path || break
    (( remaining -= ${{#path}} + 1 ))
    (( remaining > 0 )) || break
    IFS= read -r -n "$remaining" line || break
    (( remaining -= ${{#line}} + 1 ))
    (( remaining >= 0 )) || break
    printf "%s\0%s\n" "$path" "$line"
    (( count += 1 ))
    if (( count >= limit )); then
        : > "$marker"
        exit 0
    fi
done
if (( remaining <= 0 )); then : > "$marker"; fi
' -- {budget} {limit} "$search_dir/limited" < "$search_dir/pipe" || limiter_status=$?
if [ -f "$search_dir/limited" ] || [ "$limiter_status" -ne 0 ]; then kill "$producer" 2>/dev/null || :; fi
status=0
wait "$producer" || status=$?
producer=''
[ "$limiter_status" -eq 0 ] || exit 1
case "$status" in
    0|1) ;;
    141|143) [ -f "$search_dir/limited" ] || exit 1 ;;
    *) exit 1 ;;
esac
"#,
        limit = limit,
        budget = SEARCH_OUTPUT_LIMIT,
        case = if params["case_sensitive"].as_bool() == Some(true) {
            ""
        } else {
            "-i"
        },
        pattern = quoted(param(params, "pattern")?)?,
        path = quoted(param(params, "path")?)?,
    ))
}

fn write_script(method: &str, params: &serde_json::Value) -> Result<String, AgentError> {
    let content = param(params, "content")?;
    let bytes = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|_| AgentError::ChannelClosed)?,
    );
    if bytes.len() as u64 > oxideterm_ide_core::MAX_EDITABLE_FILE_SIZE {
        return Err(AgentError::ChannelClosed);
    }
    let create = method == "fs/createFile" || params["create_new"].as_bool() == Some(true);
    let expected = params["expect_hash"].as_str().map(quoted).transpose()?;
    let check = expected
        .map(|v| {
            format!(
                "[ -f \"$p\" ] && [ \"$(sha256sum < \"$p\" | cut -d ' ' -f 1)\" = {v} ] || exit 45"
            )
        })
        .unwrap_or_default();
    let install = if create {
        "ln -- \"$tmp\" \"$p\" || exit 45; rm -- \"$tmp\""
    } else {
        "if [ -e \"$p\" ]; then cp --attributes-only --preserve=mode,ownership -- \"$p\" \"$tmp\"; fi; mv -T -- \"$tmp\" \"$p\""
    };
    Ok(format!(
        r#"p={path}
if [ -L "$p" ]; then {symlink}; fi
{check}
[ ! -d "$p" ] || exit 1
tmp=$(mktemp -- "$(dirname -- "$p")/.oxideterm-write.XXXXXXXX")
trap 'rm -f -- "$tmp"' EXIT HUP INT TERM
printf '%s' {content} | base64 -d > "$tmp"
{install}
{VERSION}"#,
        path = quoted(param(params, "path")?)?,
        symlink = if create {
            "exit 45"
        } else {
            "p=$(readlink -f -- \"$p\")"
        },
        content = quoted(content)?
    ))
}

fn terminal_bootstrap(token: &str, pty: bool) -> String {
    super::shell::bootstrap_script(
        token,
        &format!(
            r#"oxide_dir=$(mktemp -d) || exit 1
printf '\036{token}:0\n\n'
while IFS= read -r request; do
 [ "$request" != '!quit' ] || exit 0
 [ "$request" != '#oxideterm-cancel-{token}' ] || exit 1
 printf '%s' "$request" | base64 -d > "$oxide_dir/request" || exit 1
 /bin/sh "$oxide_dir/request" > "$oxide_dir/output" 2>/dev/null
 status=$?
 printf '\036{token}:%s\n' "$status"
 base64 -w 0 < "$oxide_dir/output"
 printf '\n'
done
"#
        ),
        pty,
    )
}

impl AgentTransport {
    pub(super) async fn commands_terminal(mut channel: IdeChannel) -> Result<Self, AgentError> {
        let token = channel.token().to_owned();
        let script = terminal_bootstrap(&token, channel.with_pty());
        let startup = async {
            channel
                .write(
                    format!(
                        " /bin/sh -c {}{}",
                        quoted(&script)?,
                        if channel.with_pty() { "\r" } else { "\n" }
                    )
                    .as_bytes(),
                )
                .await
                .map_err(|_| AgentError::ChannelClosed)?;
            let mut buffer = Zeroizing::new(Vec::new());
            let ready = tokio::time::timeout(
                Duration::from_secs(10),
                super::shell::read_shell_line(&mut channel, &mut buffer, 128),
            )
            .await
            .map_err(|_| AgentError::Timeout(10))??;
            if *ready != format!("\x1e{token}:0") {
                return Err(AgentError::ChannelClosed);
            }
            read_payload_line(&mut channel, &mut buffer).await?;
            Ok::<_, AgentError>(buffer)
        }
        .await;
        match startup {
            Ok(buffer) => Self::commands(CommandContext { channel, buffer }).await,
            Err(error) => {
                let _ = channel.restore().await;
                Err(error)
            }
        }
    }

    async fn commands(mut context: CommandContext) -> Result<Self, AgentError> {
        let probe = tokio::time::timeout(Duration::from_secs(10), context.execute(TOOLS))
            .await
            .unwrap_or_else(|_| Err(AgentError::Timeout(10)));
        if let Err(error) = probe {
            context.shutdown().await;
            return Err(error);
        }
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (write_tx, mut write_rx) = mpsc::channel::<Zeroizing<String>>(16);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let (watch_tx, _) = broadcast::channel(1);
        let task_pending = pending.clone();
        let task_alive = alive.clone();
        // The worker owns its exact terminal lease or a node-scoped consumer.
        let worker = tokio::spawn(async move {
            loop {
                let request = tokio::select! { biased; _=shutdown_rx.recv()=>break, request=write_rx.recv()=>match request {Some(v)=>v,None=>break} };
                let Ok(mut request) = serde_json::from_str::<serde_json::Value>(&request) else {
                    break;
                };
                let Some(id) = request["id"].as_u64() else {
                    break;
                };
                let method = request["method"].as_str().unwrap_or_default().to_owned();
                let mut stop = false;
                let result = if method == "sys/info" {
                    Ok(
                        serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"compatibility_version":CURRENT_AGENT_COMPATIBILITY_VERSION,"os":"linux","arch":"commands","pid":0,"capabilities":["file-bytes"]}),
                    )
                } else if method == "sys/ping" {
                    Ok(serde_json::json!({"ok":true}))
                } else if method == "sys/shutdown" {
                    stop = true;
                    Ok(serde_json::json!({}))
                } else {
                    match CommandOperation::new(&method, request["params"].take()) {
                        Err(e) => Err(e),
                        Ok(operation) => {
                            let execution = context.execute(&&*operation.script);
                            tokio::pin!(execution);
                            let output = tokio::select! {
                                result=tokio::time::timeout(Duration::from_secs(300),&mut execution)=>result,
                                _=shutdown_rx.recv()=> {stop=true; tokio::time::timeout(Duration::from_secs(5),&mut execution).await}
                            };
                            match output {
                                Ok(Ok(bytes)) => operation.decode(&bytes),
                                Ok(Err(e)) => Err(e),
                                Err(_) => {
                                    stop = true;
                                    Err(AgentError::Timeout(300))
                                }
                            }
                        }
                    }
                };
                let fatal = matches!(result, Err(AgentError::ChannelClosed));
                if let Some(sender) = task_pending.lock().await.remove(&id) {
                    let _ = sender.send(result.map_err(|e| match e {
                        AgentError::Rpc { code, message } => AgentRpcError { code, message },
                        _ => AgentRpcError {
                            code: -32603,
                            message: "File command failed".into(),
                        },
                    }));
                }
                if stop || fatal {
                    break;
                }
            }
            task_alive.store(false, Ordering::Release);
            context.shutdown().await;
            for (_, sender) in task_pending.lock().await.drain() {
                let _ = sender.send(Err(AgentRpcError {
                    code: -32603,
                    message: "File session closed".into(),
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

#[cfg(test)]
mod tests {
    use super::super::tests::run_linux;
    use super::*;
    use serde_json::json;

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker for GNU tools"
    )]
    fn command_file_probe_returns_only_the_binary_prefix() {
        let operation =
            CommandOperation::new("fs/readFilePrefix", json!({"path":"large.bin"})).unwrap();
        let script = format!(
            "root=$(mktemp -d) || exit 1\ntrap 'rm -rf -- \"$root\"' EXIT\ncd -- \"$root\" || exit 1\ntruncate -s 52428800 large.bin\n{}",
            &*operation.script
        );
        let output = run_linux("/bin/sh", &[], script.as_bytes());
        assert!(output.status.success());
        assert_eq!(output.stdout, vec![0; 8192]);
        assert_eq!(
            operation.decode(&output.stdout).unwrap(),
            json!(base64::engine::general_purpose::STANDARD.encode(vec![0; 8192]))
        );
    }

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker for GNU tools"
    )]
    fn shell_search_bounds_remote_output_and_preserves_complete_records() {
        let cases = [
            ("match", 2, true),
            ("absent", 2, true),
            ("[", 2, false),
            ("match", 0, true),
        ];
        for (pattern, limit, success) in cases {
            let op = CommandOperation::new(
                "search/grep",
                json!({"path":".", "pattern":pattern, "max_results":limit}),
            )
            .unwrap();
            let script = format!(
                "root=$(mktemp -d)\ntrap 'rm -rf -- \"$root\"' EXIT\ncd -- \"$root\"\nprintf 'match one\\nmatch two\\nmatch three\\n' > 'a\nfile'\n(\n{}\n)",
                &*op.script
            );
            let output = run_linux("/bin/sh", &[], script.as_bytes());
            assert_eq!(output.status.success(), success, "{pattern}/{limit}");
            if success {
                let expected = if pattern == "match" && limit > 0 {
                    b"./a\nfile\x001:match one\n./a\nfile\x002:match two\n".as_slice()
                } else {
                    b""
                };
                assert_eq!(output.stdout, expected, "{pattern}/{limit}");
            }
        }
        // A producer that pauses after its first hit must be terminated, not drained to EOF.
        let op = CommandOperation::new(
            "search/grep",
            json!({"path":".", "pattern":"match", "max_results":1}),
        )
        .unwrap();
        let script = format!(
            r#"root=$(mktemp -d)
trap 'rm -rf -- "$root"' EXIT
mkdir "$root/bin"
printf '#!/bin/sh\nprintf "./first\\0001:match\\n"\nexec sleep 30\n' > "$root/bin/grep"
chmod +x "$root/bin/grep"
cat > "$root/request" <<'REQUEST'
{}
REQUEST
PATH="$root/bin:$PATH" timeout 3 /bin/sh "$root/request"
"#,
            &*op.script
        );
        let output = run_linux("/bin/sh", &[], script.as_bytes());
        assert!(
            output.status.success(),
            "limited search failed to stop its producer: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"./first\x001:match\n");

        let op = CommandOperation::new(
            "search/grep",
            json!({"path":".", "pattern":"x", "max_results":100000}),
        )
        .unwrap();
        let script = format!(
            "root=$(mktemp -d)\ntrap 'rm -rf -- \"$root\"' EXIT\ncd -- \"$root\"\nhead -c 32768 /dev/zero | tr '\\000' x > line\nfor n in $(seq 1 64); do cat line; printf '\\n'; done > many\nrm line\n{}",
            &*op.script
        );
        let output = run_linux("/bin/sh", &[], script.as_bytes());
        assert!(output.status.success());
        assert!(output.stdout.len() <= SEARCH_OUTPUT_LIMIT);
        let expected = (1..=31)
            .map(|line| json!({"path":"./many", "line":line, "column":1, "text":"x".repeat(32768)}))
            .collect::<Vec<_>>();
        assert_eq!(op.decode(&output.stdout).unwrap(), json!(expected));

        // An oversized first record must not cross the transport or become a partial hit.
        let op = CommandOperation::new(
            "search/grep",
            json!({"path":".", "pattern":"x", "max_results":10}),
        )
        .unwrap();
        let script = format!(
            "root=$(mktemp -d)\ntrap 'rm -rf -- \"$root\"' EXIT\ncd -- \"$root\"\nhead -c 2097152 /dev/zero | tr '\\000' x > huge\nprintf '\\n' >> huge\n{}",
            &*op.script
        );
        let output = run_linux("/bin/sh", &[], script.as_bytes());
        assert!(output.status.success());
        assert_eq!(output.stdout, b"");
        assert_eq!(op.decode(&output.stdout).unwrap(), json!([]));
    }

    #[test]
    fn command_results_keep_delimiters_in_names_and_reject_incomplete_metadata() {
        let operation = CommandOperation::new("fs/listDir", json!({"path":"/work"})).unwrap();
        let entries = operation
            .decode(b"a\n'b\0/work/a\n'b\0l\0f\04\01700000000.25\0640\0target\0")
            .unwrap();
        assert_eq!(entries[0]["name"], "a\n'b");
        assert_eq!(entries[0]["path"], "/work/a\n'b");
        assert_eq!(entries[0]["file_type"], "symlink");
        assert_eq!(entries[0]["target_file_type"], "file");
        assert_eq!(entries[0]["mtime"], 1700000000u64);
        assert!(operation.decode(b"name\0/path\0f\0").is_err());
        assert!(quoted("bad\0name").is_err());
    }

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker to run GNU tools on Linux"
    )]
    fn linux_file_commands_preserve_targets_permissions_and_conflicts() {
        // macOS uses an isolated Linux container so the actual supported GNU commands run.
        let root = format!(
            "/tmp/oxideterm-command-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = format!("{root}/a' $(touch injected)\nfile");
        let link = format!("{root}/link");
        let expected = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
        let cases = [
            ("fs/listDir", json!({"path":root}), 0),
            ("fs/readFile", json!({"path":path}), 0),
            (
                "fs/writeIdeFile",
                json!({"path":link,"content":"dXBkYXRlZAo=","expect_hash":expected}),
                0,
            ),
            ("fs/readFile", json!({"path":path}), 0),
            ("fs/stat", json!({"path":path}), 0),
            ("fs/createFile", json!({"path":path,"content":""}), 45),
            (
                "fs/writeIdeFile",
                json!({"path":path,"content":"","expect_hash":expected}),
                45,
            ),
            (
                "fs/createFile",
                json!({"path":format!("{root}/new"),"content":"bmV3Cg=="}),
                0,
            ),
            (
                "search/grep",
                json!({"path":root,"pattern":"updated","case_sensitive":true,"max_results":10}),
                0,
            ),
            ("fs/remove", json!({"path":path,"recursive":false}), 0),
            (
                "fs/writeIdeFile",
                json!({"path":path,"content":"","expect_hash":expected}),
                45,
            ),
            ("fs/mkdir", json!({"path":format!("{root}/empty")}), 0),
            (
                "fs/remove",
                json!({"path":format!("{root}/empty"),"recursive":false}),
                0,
            ),
        ];
        let mut script = format!(
            "mkdir -- {root} || exit 1\ntrap 'rm -rf -- {root} {root}.result' EXIT\ncd -- {root} || exit 1\nprintf 'hello\\n' > {}\nchmod 640 -- {}\nln -s -- {} {}\n",
            quoted(&path).unwrap(),
            quoted(&path).unwrap(),
            quoted(&path).unwrap(),
            quoted(&link).unwrap()
        );
        let mut operations = Vec::new();
        for (method, params, _) in &cases {
            let operation = CommandOperation::new(method, params.clone()).unwrap();
            script.push_str(&format!("(\n{}\n) > {root}.result 2>/dev/null\nstatus=$?\nprintf '%s\\n' \"$status\"\nbase64 -w 0 {root}.result\nprintf '\\n'\n",&*operation.script));
            operations.push(operation);
        }
        script.push_str(&format!(
            "test -L {} && test ! -e {root}/injected\n",
            quoted(&link).unwrap()
        ));
        let output = run_linux("/bin/sh", &[], script.as_bytes());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let lines = std::str::from_utf8(&output.stdout)
            .unwrap()
            .split_terminator('\n')
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), cases.len() * 2);
        let mut results = Vec::new();
        for ((case, operation), lines) in cases.iter().zip(operations.iter()).zip(lines.chunks(2)) {
            assert_eq!(
                lines[0].parse::<i32>().unwrap(),
                case.2,
                "{}; output={:?}",
                case.0,
                String::from_utf8_lossy(&output.stdout)
            );
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(lines[1])
                .unwrap();
            results.push(if case.2 == 0 {
                operation.decode(&bytes).unwrap()
            } else {
                json!(null)
            });
        }
        assert_eq!(
            results[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["name"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            ["a' $(touch injected)\nfile", "link"].into_iter().collect()
        );
        assert_eq!(results[1]["content"], "aGVsbG8K");
        assert_eq!(results[1]["hash"], expected);
        assert_eq!(results[3]["content"], "dXBkYXRlZAo=");
        assert_eq!(results[4]["permissions"], "640");
        assert_eq!(results[7]["size"], 4);
        assert_eq!(results[8][0]["text"], "updated");
        assert_eq!(results[8][0]["line"], 1);
    }

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker for a Linux PTY"
    )]
    fn failed_shell_startup_restores_original_shell_and_tty() {
        let token = "0123456789abcdef0123456789abcdef";
        let input = json!({"token":token,"cases":[
            {"name":"failure before tty change", "script":format!("stty() {{ return 1; }}; {}", super::super::shell::bootstrap_script(token,"exit 69", true)),"cancel":false},
            {"name":"failure after tty change", "script":super::super::shell::bootstrap_script(token,"exit 69", true),"cancel":false},
            {"name":"cancel waiting command reader", "script":terminal_bootstrap(token, true),"cancel":true}
        ]});
        let python = r#"
import json,os,pty,select,shlex,subprocess,sys,termios
p=json.load(sys.stdin)
for case in p['cases']:
 master,slave=pty.openpty()
 process=subprocess.Popen(['/bin/sh'],stdin=slave,stdout=slave,stderr=slave,cwd='/tmp')
 os.close(slave)
 buffer=b''
 def send(data):
  while data:
   count=os.write(master,data)
   data=data[count:]
 def until(marker):
  global buffer
  while marker not in buffer:
   if not select.select([master],[],[],10)[0]: raise RuntimeError(case['name']+': PTY response timeout')
   buffer+=os.read(master,65536)
  buffer=buffer.split(marker,1)[1]
 try:
  send(b"printf '\\036PARENT_READY\\037'\n")
  until(b'\x1ePARENT_READY\x1f')
  baseline=termios.tcgetattr(master)
  send(('/bin/sh -c '+shlex.quote(case['script'])+'\n').encode())
  if case['cancel']:
   until(('\x1e'+p['token']+':0\n\n').encode())
   send(('\n#oxideterm-cancel-'+p['token']+'\n').encode())
  until(('\x1e'+p['token']+':RESTORED\x1f').encode())
  assert termios.tcgetattr(master)==baseline, case['name']
  send(b"printf '\\036PARENT_OK:%s\\037' \"$PWD\"\n")
  until(b'\x1ePARENT_OK:/tmp\x1f')
  assert process.poll() is None, case['name']
 finally:
  process.kill();process.wait()
  os.close(master)
print('original shell usable after every failed startup')
"#;
        let output = run_linux("python3", &["-c", python], input.to_string().as_bytes());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            b"original shell usable after every failed startup\n"
        );
    }

    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "requires local Docker for a Linux PTY"
    )]
    fn terminal_command_loop_preserves_cwd_bytes_and_restores_tty() {
        let root = format!(
            "/tmp/oxideterm-pty-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = format!("{root}/file");
        let operations = [
            CommandOperation::new("sys/cwd", json!({})).unwrap(),
            CommandOperation::new("fs/createFile", json!({"path":path,"content":"4oKsDQo="}))
                .unwrap(),
            CommandOperation::new("fs/readFile", json!({"path":path})).unwrap(),
        ];
        let token = "0123456789abcdef0123456789abcdef";
        let input = json!({"root":root,"token":token,"bootstrap":terminal_bootstrap(token, true),"requests":operations.iter().map(|op| base64::engine::general_purpose::STANDARD.encode(op.script.as_bytes())).collect::<Vec<_>>()});
        let python = r#"
import base64,json,os,pty,select,shutil,subprocess,sys,termios
p=json.load(sys.stdin)
os.mkdir(p['root'])
master,slave=pty.openpty()
baseline=termios.tcgetattr(master)
process=subprocess.Popen(['/bin/sh','-c',p['bootstrap']],stdin=slave,stdout=slave,stderr=slave,cwd=p['root'])
os.close(slave)
buffer=b''
def receive():
 global buffer
 if not select.select([master],[],[],10)[0]: raise RuntimeError('PTY response timeout')
 buffer+=os.read(master,65536)
def line():
 global buffer
 while b'\n' not in buffer: receive()
 result,buffer=buffer.split(b'\n',1)
 return result
def send(data):
 while data:
  count=os.write(master,data)
  data=data[count:]
try:
 assert line()==('\x1e'+p['token']+':0').encode()
 assert line()==b''
 responses=[]
 for request in p['requests']:
  encoded=request.encode()+b'\n'
  send(encoded[:7])
  send(encoded[7:])
  assert line()==('\x1e'+p['token']+':0').encode()
  responses.append(line().decode())
 send(b'!quit\n')
 marker=('\x1e'+p['token']+':RESTORED\x1f').encode()
 while marker not in buffer: receive()
 assert process.wait(timeout=10)==0
 print(json.dumps({'responses':responses,'restored':termios.tcgetattr(master)==baseline}))
finally:
 if process.poll() is None: process.kill();process.wait()
 os.close(master)
 shutil.rmtree(p['root'])
"#;
        let output = run_linux("python3", &["-c", python], input.to_string().as_bytes());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["restored"], true);
        let results: Vec<_> = operations
            .iter()
            .zip(result["responses"].as_array().unwrap())
            .map(|(op, bytes)| {
                op.decode(
                    &base64::engine::general_purpose::STANDARD
                        .decode(bytes.as_str().unwrap())
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        assert_eq!(results[0], root);
        assert_eq!(results[1]["size"], 5);
        assert_eq!(results[2]["content"], "4oKsDQo=");
    }
}
