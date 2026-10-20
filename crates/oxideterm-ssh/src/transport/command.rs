use super::*;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshCommandChannel {
    Exec,
    Shell,
    PtyShell,
}

pub struct SshCommandExecution {
    pub output: SshCommandOutput,
    pub channel: SshCommandChannel,
    /// PTYs carry stderr on their terminal output stream.
    pub stderr_merged: bool,
}

impl std::fmt::Debug for SshCommandExecution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshCommandExecution")
            .field("channel", &self.channel)
            .field("stderr_merged", &self.stderr_merged)
            .field("exit_code", &self.output.exit_code)
            .field("truncated", &self.output.truncated)
            .finish_non_exhaustive()
    }
}

impl SshConnectionHandle {
    /// Selects a verified channel before submitting the command. Never replays it.
    pub async fn execute_command(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshCommandExecution, SshTransportError> {
        let pooled = self.physical::<PooledSshConnection>().ok_or_else(|| {
            SshTransportError::ConnectionFailed("no active SSH connection is available".into())
        })?;
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed".into(),
            ));
        }
        // The cache lives on the physical transport, so reconnect cannot reuse it.
        let channel = select_channel(&pooled).await?;
        if channel == SshCommandChannel::Exec {
            let captured = Self::capture_exec_on(&pooled, command, timeout, max_output_size, None)
                .await
                .map_err(|error| match error {
                    SshTransportError::CommandRejected => error,
                    _ => SshTransportError::CommandOutcomeUnknown,
                })?;
            if captured.exit_code.is_none() {
                return Err(SshTransportError::CommandOutcomeUnknown);
            }
            return Ok(SshCommandExecution {
                output: SshCommandOutput {
                    stdout: String::from_utf8_lossy(&captured.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&captured.stderr).into_owned(),
                    exit_code: captured.exit_code,
                    truncated: captured.truncated,
                },
                channel,
                stderr_merged: false,
            });
        }
        let environment = match self.remote_env() {
            Some(environment) => environment,
            None => {
                let environment =
                    crate::connection_registry::detect_remote_env_for_handle(self).await;
                self.set_remote_env(environment.clone());
                environment
            }
        };
        let windows = match environment.os_type.as_str() {
            "Windows" => true,
            "Linux" | "macOS" | "FreeBSD" | "OpenBSD" | "NetBSD" | "SunOS" | "Windows_MinGW"
            | "Windows_MSYS" | "Windows_Cygwin" => false,
            _ => return Err(SshTransportError::CommandChannelUnavailable),
        };
        let with_pty = channel == SshCommandChannel::PtyShell;
        let init = if windows {
            "set PROMPT=\r\nif (Get-Module PSReadLine) { Set-PSReadLineOption -HistorySaveStyle SaveNothing }\r\n"
        } else {
            "unset HISTFILE; export PS1='' PS2=''; stty -echo 2>/dev/null\n"
        };
        let mut shell =
            Self::open_sampling_shell_on(&pooled, init, with_pty, SHELL_OPEN_TIMEOUT).await?;
        if !with_pty {
            let ready = format!("__OXIDE_READY_{}__", uuid::Uuid::new_v4().simple());
            let initialized = shell
                .sample_until(&format!("echo {ready}\n"), &ready, SHELL_OPEN_TIMEOUT, 8192)
                .await;
            if initialized.is_err() {
                let _ = tokio::time::timeout(PROBE_TIMEOUT, shell.close()).await;
                return Err(SshTransportError::CommandChannelUnavailable);
            }
        }
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let begin = format!("__OXIDE_BEGIN_{nonce}__");
        let end = format!("__OXIDE_DONE_{nonce}__");
        let script = framed_script(command, &begin, &end, windows, with_pty);
        let input = Zeroizing::new(if with_pty {
            script.replace("\r\n", "\n").replace('\n', "\r")
        } else {
            script.to_string()
        });
        let mut capture = FramedCapture::new(begin, end, max_output_size, with_pty, windows);
        // Everything below may have executed the command. No alternative channel
        // is attempted on send failure, timeout, truncation, or disconnect.
        let result = tokio::time::timeout(timeout, async {
            shell
                .channel
                .data(input.as_bytes())
                .await
                .map_err(|_| SshTransportError::CommandOutcomeUnknown)?;
            loop {
                match shell.channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        capture.data(&data);
                        if capture.complete() {
                            return Ok(capture.finish());
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                        capture.stderr_data(&data);
                        if capture.complete() {
                            return Ok(capture.finish());
                        }
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                        return Err(SshTransportError::CommandOutcomeUnknown);
                    }
                    Some(ChannelMsg::Failure) => {
                        return Err(SshTransportError::CommandOutcomeUnknown);
                    }
                    _ => {}
                }
            }
        })
        .await;
        let _ = tokio::time::timeout(PROBE_TIMEOUT, shell.close()).await;
        Ok(SshCommandExecution {
            output: result.map_err(|_| SshTransportError::CommandOutcomeUnknown)??,
            channel,
            stderr_merged: with_pty,
        })
    }
}

async fn select_channel(
    pooled: &PooledSshConnection,
) -> Result<SshCommandChannel, SshTransportError> {
    // Serialize only harmless capability discovery; business commands stay independent.
    let mut cached = pooled.command_channel.lock().await;
    if let Some(channel) = *cached {
        return Ok(channel);
    }
    let marker = format!("__OXIDE_PROBE_{}__", uuid::Uuid::new_v4().simple());
    let probe = format!("echo {marker}");
    if let Ok(output) =
        SshConnectionHandle::capture_exec_on(pooled, &probe, PROBE_TIMEOUT, 8192, None).await
        && output.exit_code == Some(0)
        && !output.truncated
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == marker)
    {
        *cached = Some(SshCommandChannel::Exec);
        return Ok(SshCommandChannel::Exec);
    }
    for with_pty in [false, true] {
        let opened =
            SshConnectionHandle::open_sampling_shell_on(pooled, "", with_pty, SHELL_OPEN_TIMEOUT)
                .await;
        let Ok(mut shell) = opened else { continue };
        let sampled = shell
            .sample_until(&format!("{probe}\n"), &marker, PROBE_TIMEOUT, 8192)
            .await;
        let _ = tokio::time::timeout(PROBE_TIMEOUT, shell.close()).await;
        if let Ok(output) = sampled
            && output.split(['\r', '\n']).any(|line| line.trim() == marker)
        {
            let channel = if with_pty {
                SshCommandChannel::PtyShell
            } else {
                SshCommandChannel::Shell
            };
            *cached = Some(channel);
            return Ok(channel);
        }
    }
    Err(SshTransportError::CommandChannelUnavailable)
}

fn unix_quote(value: &str) -> Zeroizing<String> {
    let mut quoted = Zeroizing::new(String::from("'"));
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn encoded_powershell(script: &str) -> Zeroizing<String> {
    use base64::Engine as _;
    let bytes = Zeroizing::new(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(bytes.as_slice()))
}

fn framed_script(
    command: &str,
    begin: &str,
    end: &str,
    windows: bool,
    merged: bool,
) -> Zeroizing<String> {
    if windows {
        // A child process keeps explicit `exit` inside the payload from skipping the trailer.
        let payload = encoded_powershell(command);
        let stderr_end = if merged {
            String::new()
        } else {
            format!("; [Console]::Error.WriteLine(''); [Console]::Error.WriteLine('{end}_ERR')")
        };
        let script = Zeroizing::new(format!(
            "Write-Output ''; Write-Output '{begin}'; & powershell -NoProfile -NonInteractive -EncodedCommand {}; $code=$LASTEXITCODE; Write-Output ''; Write-Output ('{end}:'+$code){stderr_end}",
            payload.as_str()
        ));
        let encoded = encoded_powershell(&script);
        Zeroizing::new(format!(
            "powershell -NoProfile -NonInteractive -EncodedCommand {}\r\n",
            encoded.as_str()
        ))
    } else {
        let payload = unix_quote(command);
        let stderr_end = if merged {
            String::new()
        } else {
            format!("; printf '\\n{end}_ERR\\n' >&2")
        };
        let body = Zeroizing::new(format!(
            "printf '\\n{begin}\\n'; sh -c {}; __oxide_code=$?; printf '\\n{end}:%s\\n' \"$__oxide_code\"{stderr_end}",
            payload.as_str()
        ));
        let body = unix_quote(&body);
        Zeroizing::new(format!("sh -c {}\n", body.as_str()))
    }
}

struct FramedCapture {
    begin: String,
    end: String,
    line: Zeroizing<Vec<u8>>,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
    stdout_tail: Zeroizing<Vec<u8>>,
    stderr_tail: Zeroizing<Vec<u8>>,
    stderr_line: Zeroizing<Vec<u8>>,
    stderr_long_line: bool,
    stderr_done: bool,
    limit: usize,
    total: usize,
    started: bool,
    long_line: bool,
    truncated: bool,
    pty: bool,
    previous_cr: bool,
    crlf_separator: bool,
    exit_code: Option<i32>,
}

impl FramedCapture {
    fn new(begin: String, end: String, limit: usize, pty: bool, windows: bool) -> Self {
        Self {
            begin,
            end,
            line: Zeroizing::new(Vec::new()),
            stdout: Zeroizing::new(Vec::new()),
            stderr: Zeroizing::new(Vec::new()),
            stdout_tail: Zeroizing::new(Vec::new()),
            stderr_tail: Zeroizing::new(Vec::new()),
            stderr_line: Zeroizing::new(Vec::new()),
            stderr_long_line: false,
            stderr_done: false,
            limit,
            total: 0,
            started: false,
            long_line: false,
            truncated: false,
            pty,
            previous_cr: false,
            crlf_separator: windows && !pty,
            exit_code: None,
        }
    }

    fn stdout(&mut self, data: &[u8]) {
        self.stdout_tail.extend_from_slice(data);
        let payload_len = self
            .stdout_tail
            .len()
            .saturating_sub(if self.crlf_separator { 2 } else { 1 });
        if payload_len == 0 {
            return;
        }
        // Hold back the framing newline so it never consumes the payload budget.
        append_limited_command_output(
            &mut self.stdout,
            &self.stdout_tail[..payload_len],
            self.limit,
            &mut self.total,
            &mut self.truncated,
        );
        self.stdout_tail.drain(..payload_len);
    }

    fn stderr(&mut self, data: &[u8]) {
        self.stderr_tail.extend_from_slice(data);
        let payload_len = self
            .stderr_tail
            .len()
            .saturating_sub(if self.crlf_separator { 2 } else { 1 });
        if payload_len == 0 {
            return;
        }
        append_limited_command_output(
            &mut self.stderr,
            &self.stderr_tail[..payload_len],
            self.limit,
            &mut self.total,
            &mut self.truncated,
        );
        self.stderr_tail.drain(..payload_len);
    }

    fn complete(&self) -> bool {
        self.exit_code.is_some() && (self.pty || self.stderr_done)
    }

    fn stderr_data(&mut self, bytes: &[u8]) {
        let marker = format!("{}_ERR", self.end);
        for &byte in bytes {
            if self.stderr_done {
                break;
            }
            if byte == b'\n' {
                let line = Zeroizing::new(std::mem::take(&mut *self.stderr_line));
                if !self.stderr_long_line
                    && line.strip_suffix(b"\r").unwrap_or(&line) == marker.as_bytes()
                {
                    self.stderr_tail.clear();
                    self.stderr_done = true;
                } else {
                    self.stderr(&line);
                    self.stderr(b"\n");
                }
                self.stderr_long_line = false;
            } else {
                self.stderr_line.push(byte);
                if self.stderr_line.len() > marker.len() + 2 {
                    let line = Zeroizing::new(std::mem::take(&mut *self.stderr_line));
                    self.stderr(&line);
                    self.stderr_long_line = true;
                }
            }
        }
    }

    fn data(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.exit_code.is_some() {
                break;
            }
            let byte = if self.pty {
                let previous_cr = self.previous_cr;
                self.previous_cr = byte == b'\r';
                if byte == b'\n' && previous_cr {
                    continue;
                }
                if byte == b'\r' { b'\n' } else { byte }
            } else {
                byte
            };
            if byte == b'\n' {
                self.line_complete();
            } else {
                self.line.push(byte);
                // Long payload/echo lines must not bypass the output memory bound.
                if self.line.len() > self.end.len() + 32 {
                    if self.started {
                        let line = Zeroizing::new(std::mem::take(&mut *self.line));
                        self.stdout(&line);
                    }
                    self.line.clear();
                    self.long_line = true;
                }
            }
        }
    }

    fn line_complete(&mut self) {
        let line = Zeroizing::new(std::mem::take(&mut *self.line));
        let record = line.strip_suffix(b"\r").unwrap_or(&line);
        if !self.long_line && record == self.begin.as_bytes() {
            self.started = true;
        } else if self.started && !self.long_line {
            let status = std::str::from_utf8(record)
                .ok()
                .and_then(|line| line.strip_prefix(self.end.as_str()))
                .and_then(|value| value.strip_prefix(':'))
                .and_then(|value| value.parse::<i32>().ok());
            if let Some(status) = status {
                self.stdout_tail.clear();
                self.exit_code = Some(status);
            } else {
                self.stdout(&line);
                self.stdout(b"\n");
            }
        } else if self.started {
            self.stdout(&line);
            self.stdout(b"\n");
        }
        self.long_line = false;
    }

    fn finish(mut self) -> SshCommandOutput {
        SshCommandOutput {
            stdout: String::from_utf8_lossy(&self.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&self.stderr).into_owned(),
            exit_code: self.exit_code.take(),
            truncated: self.truncated,
        }
    }
}

#[cfg(test)]
mod tests;
