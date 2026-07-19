const PROXY_COMMAND_BUFFER_BYTES: usize = 64 * 1024;

pub(super) struct ProxyCommandStream {
    stream: tokio::io::DuplexStream,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for ProxyCommandStream {
    fn drop(&mut self) {
        // The stream owns the helper lifetime. Closing an SSH transport therefore asks
        // the worker to kill and reap its child instead of leaving a detached process.
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

impl AsyncRead for ProxyCommandStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(context, buffer)
    }
}

impl AsyncWrite for ProxyCommandStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.stream).poll_write(context, buffer)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stream).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stream).poll_shutdown(context)
    }
}

pub(super) async fn dial_proxy_command(
    config: &crate::ProxyCommandConfig,
) -> Result<ProxyCommandStream, SshTransportError> {
    let crate::ProxyCommandConfig::Direct { program, args } = config else {
        let message = match config {
            crate::ProxyCommandConfig::AuthorizationRequired => {
                "ProxyCommand execution requires explicit authorization in settings"
            }
            crate::ProxyCommandConfig::Unavailable => {
                "ProxyCommand is no longer available from the imported SSH config"
            }
            crate::ProxyCommandConfig::Direct { .. } => unreachable!(),
        };
        return Err(SshTransportError::ConnectionFailed(message.to_string()));
    };
    validate_direct_command(program, args)?;

    let mut command = tokio::process::Command::new(program.as_str());
    command
        .args(args.iter().map(|argument| argument.as_str()))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| {
        // Never include the executable, arguments, or child stderr in an error because
        // ProxyCommand values may contain credentials supplied by external tooling.
        SshTransportError::ConnectionFailed("failed to start SSH ProxyCommand".to_string())
    })?;
    let child_stdin = child.stdin.take().ok_or_else(|| {
        SshTransportError::ConnectionFailed("ProxyCommand stdin is unavailable".to_string())
    })?;
    let child_stdout = child.stdout.take().ok_or_else(|| {
        SshTransportError::ConnectionFailed("ProxyCommand stdout is unavailable".to_string())
    })?;
    let (ssh_stream, worker_stream) = tokio::io::duplex(PROXY_COMMAND_BUFFER_BYTES);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    spawn_proxy_command_worker(child, child_stdin, child_stdout, worker_stream, shutdown_rx);

    Ok(ProxyCommandStream {
        stream: ssh_stream,
        shutdown: Some(shutdown_tx),
    })
}

fn validate_direct_command(
    program: &str,
    args: &[zeroize::Zeroizing<String>],
) -> Result<(), SshTransportError> {
    const SHELL_OPERATORS: [&str; 8] = ["|", "||", "&&", ";", ">", ">>", "<", "<<"];
    if program.trim().is_empty()
        || SHELL_OPERATORS.contains(&program)
        || args
            .iter()
            .any(|argument| SHELL_OPERATORS.contains(&argument.as_str()))
    {
        return Err(SshTransportError::ConnectionFailed(
            "ProxyCommand must be one direct executable with arguments; shell operators are not supported"
                .to_string(),
        ));
    }
    Ok(())
}

fn spawn_proxy_command_worker(
    mut child: tokio::process::Child,
    mut child_stdin: tokio::process::ChildStdin,
    mut child_stdout: tokio::process::ChildStdout,
    worker_stream: tokio::io::DuplexStream,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let (mut worker_read, mut worker_write) = tokio::io::split(worker_stream);
        let mut input = tokio::spawn(async move {
            tokio::io::copy(&mut worker_read, &mut child_stdin).await
        });
        let mut output = tokio::spawn(async move {
            tokio::io::copy(&mut child_stdout, &mut worker_write).await
        });

        let child_exited = tokio::select! {
            _ = &mut shutdown => false,
            _ = &mut input => false,
            _ = &mut output => false,
            _ = child.wait() => true,
        };
        input.abort();
        output.abort();
        if !child_exited {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    })
}

#[cfg(test)]
mod proxy_command_tests {
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn direct_proxy_command_rejects_shell_operators() {
        let args = vec![Zeroizing::new("target".to_string()), Zeroizing::new("|".to_string())];

        let error = validate_direct_command("nc", &args).unwrap_err();

        assert!(error.to_string().contains("shell operators"));
        assert!(!error.to_string().contains("target"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn proxy_command_stream_bridges_bytes_without_a_shell() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let command = crate::ProxyCommandConfig::direct(vec![Zeroizing::new(
            "/bin/cat".to_string(),
        )])
        .unwrap();
        let mut stream = dial_proxy_command(&command).await.unwrap();

        stream.write_all(b"proxy-command-round-trip").await.unwrap();
        let mut output = vec![0; "proxy-command-round-trip".len()];
        stream.read_exact(&mut output).await.unwrap();

        assert_eq!(output, b"proxy-command-round-trip");
    }
}
