struct PooledSshConnection {
    audit: Option<oxideterm_audit::AuditContext>,
    target: client::Handle<NativeClientHandler>,
    command_channel: tokio::sync::Mutex<Option<SshCommandChannel>>,
    _jump_handles: Vec<client::Handle<NativeClientHandler>>,
    remote_forward_handler: RemoteForwardHandlerSlot,
    x11_forward_handler: X11ForwardHandlerSlot,
    x11_dispatcher: X11ForwardDispatcher,
    auth_banners: AuthBannerSink,
    agent_forwarding_accepted: Arc<AtomicBool>,
    agent_forwarding_request: tokio::sync::Mutex<()>,
    last_rtt: RwLock<Option<(Duration, Instant)>>,
}

fn append_limited_command_output(
    output: &mut Vec<u8>,
    data: &[u8],
    max_output_size: usize,
    total_output_size: &mut usize,
    truncated: &mut bool,
) {
    if *total_output_size >= max_output_size {
        *truncated = true;
        return;
    }
    let remaining = max_output_size.saturating_sub(*total_output_size);
    if data.len() > remaining {
        output.extend_from_slice(&data[..remaining]);
        *total_output_size += remaining;
        *truncated = true;
    } else {
        output.extend_from_slice(data);
        *total_output_size += data.len();
    }
}

impl PooledSshConnection {
    fn direct(
        handle: client::Handle<NativeClientHandler>,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
        x11_dispatcher: X11ForwardDispatcher,
        auth_banners: AuthBannerSink,
        agent_forwarding_accepted: Arc<AtomicBool>,
        audit: Option<oxideterm_audit::AuditContext>,
    ) -> Self {
        Self {
            audit,
            target: handle,
            command_channel: tokio::sync::Mutex::new(None),
            _jump_handles: Vec::new(),
            remote_forward_handler,
            x11_forward_handler,
            x11_dispatcher,
            auth_banners,
            agent_forwarding_accepted,
            agent_forwarding_request: tokio::sync::Mutex::new(()),
            last_rtt: RwLock::new(None),
        }
    }

    fn tunneled(
        target: client::Handle<NativeClientHandler>,
        jump_handles: Vec<client::Handle<NativeClientHandler>>,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
        x11_dispatcher: X11ForwardDispatcher,
        auth_banners: AuthBannerSink,
        agent_forwarding_accepted: Arc<AtomicBool>,
        audit: Option<oxideterm_audit::AuditContext>,
    ) -> Self {
        Self {
            audit,
            target,
            command_channel: tokio::sync::Mutex::new(None),
            _jump_handles: jump_handles,
            remote_forward_handler,
            x11_forward_handler,
            x11_dispatcher,
            auth_banners,
            agent_forwarding_accepted,
            agent_forwarding_request: tokio::sync::Mutex::new(()),
            last_rtt: RwLock::new(None),
        }
    }

    pub(super) fn record_rtt(&self, rtt: Duration, measured_at: Instant) {
        *self.last_rtt.write() = Some((rtt, measured_at));
    }

    pub(super) fn recent_rtt(&self, max_age: Duration) -> Option<Duration> {
        let (rtt, measured_at) = (*self.last_rtt.read())?;
        (measured_at.elapsed() <= max_age).then_some(rtt)
    }

    async fn is_closed(&self) -> bool {
        self.target.is_closed()
    }
}

/// Closes an exec channel even when the capture future is dropped mid-flight.
///
/// Dropping russh's `Channel` does not send CHANNEL_CLOSE, so a cancelled
/// caller (timeout, tab close, superseded probe) would otherwise leave the
/// remote command running and the pooled connection holding a half-open
/// channel. The graceful path takes the channel out first, so `Drop` only
/// works for the cancelled case.
struct ChannelCloseGuard(Option<russh::Channel<client::Msg>>);

impl ChannelCloseGuard {
    fn new(channel: russh::Channel<client::Msg>) -> Self {
        Self(Some(channel))
    }

    fn get_mut(&mut self) -> &mut russh::Channel<client::Msg> {
        self.0
            .as_mut()
            .expect("the capture takes the channel only after its read loop")
    }

    fn take(&mut self) -> Option<russh::Channel<client::Msg>> {
        self.0.take()
    }
}

impl Drop for ChannelCloseGuard {
    fn drop(&mut self) {
        let Some(channel) = self.0.take() else {
            return;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = channel.close().await;
            });
        }
    }
}

impl SshConnectionHandle {
    /// Returns the real pooled SSH transport state behind this registry handle.
    ///
    /// Node-first consumers such as SFTP and port forwarding use this to avoid
    /// the old native bug where an `Active` registry entry with a closed
    /// terminal-created russh handle was borrowed as if it were healthy. Tauri
    /// `ConnectionEntry` ownership requires the physical transport to be valid,
    /// independent of whether any terminal pane still exists.
    pub async fn transport_status(&self) -> ConnectionTransportStatus {
        self.transport_status_now()
    }

    /// Returns whether this registry handle currently backs an open physical transport.
    pub fn has_open_transport(&self) -> bool {
        self.transport_status_now() == ConnectionTransportStatus::Open
    }

    pub(crate) fn transport_status_now(&self) -> ConnectionTransportStatus {
        if let Some(pooled) = self.physical::<PooledSshConnection>() {
            if pooled.target.is_closed() {
                ConnectionTransportStatus::Closed
            } else {
                ConnectionTransportStatus::Open
            }
        } else if self.has_physical() {
            // Tests and embedders may install a non-russh physical marker. Treat
            // that as open so the pool contract stays type-agnostic outside the
            // real transport module.
            ConnectionTransportStatus::Open
        } else {
            ConnectionTransportStatus::Missing
        }
    }

    pub(crate) fn recent_rtt(&self, max_age: Duration) -> Option<Duration> {
        let pooled = self.physical::<PooledSshConnection>()?;
        if pooled.target.is_closed() {
            return None;
        }
        pooled.recent_rtt(max_age)
    }

    pub async fn probe_alive(&self, probe_timeout: Duration) -> KeepaliveProbeResult {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return KeepaliveProbeResult::IoError;
        };
        if pooled.is_closed().await {
            return KeepaliveProbeResult::IoError;
        }

        let handle = &pooled.target;
        let started = Instant::now();
        // russh's `send_keepalive(true)` only waits for local command-queue
        // insertion and succeeds immediately on silent half-open sockets.
        // `send_global_request` awaits the peer's reply (either success or
        // `RequestDenied` proves a live round-trip).
        match timeout(
            probe_timeout,
            handle.send_global_request("keepalive@openssh.com", &[], true),
        )
        .await
        {
            Ok(Ok(_)) | Ok(Err(russh::Error::RequestDenied)) => {
                pooled.record_rtt(
                    started.elapsed().max(Duration::from_millis(1)),
                    Instant::now(),
                );
                KeepaliveProbeResult::Ok
            }
            Ok(Err(error)) => {
                if pooled.is_closed().await
                    || matches!(
                        error,
                        russh::Error::Disconnect
                            | russh::Error::SendError
                            | russh::Error::RecvError
                            | russh::Error::IO(_)
                    )
                {
                    KeepaliveProbeResult::IoError
                } else {
                    let error = format!("{error:?}");
                    if error.contains("Disconnect") || error.contains("disconnect") {
                        KeepaliveProbeResult::IoError
                    } else {
                        KeepaliveProbeResult::Timeout
                    }
                }
            }
            Err(_) => KeepaliveProbeResult::Timeout,
        }
    }

    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u16,
        origin_host: &str,
        origin_port: u16,
    ) -> Result<BoxedSshForwardStream, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for port forwarding".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot open a port forward".to_string(),
            ));
        }

        let handle = &pooled.target;
        let stream =
            open_direct_tcpip_stream_with_origin(handle, host, port, origin_host, origin_port)
                .await?;
        Ok(Box::new(stream))
    }

    pub async fn open_x11_channel(
        &self,
        origin_host: &str,
        origin_port: u16,
    ) -> Result<BoxedSshForwardStream, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for X11 forwarding".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot open an X11 channel".to_string(),
            ));
        }

        let channel = pooled
            .target
            .channel_open_x11(origin_host, origin_port as u32)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        Ok(Box::new(channel.into_stream()))
    }

    pub async fn allocate_remote_x11_display(
        &self,
        allocator: &X11RemoteDisplayAllocator,
        command_timeout: Duration,
    ) -> Result<u16, SshTransportError> {
        let command = allocator.probe_command();
        let output = self.run_command(&command, command_timeout, 4096).await?;
        allocator
            .parse_probe_output(&output)
            .map_err(|error| SshTransportError::Channel(error.to_string()))
    }

    pub async fn install_remote_x11_authority(
        &self,
        update: &X11RemoteXauthUpdate,
        command_timeout: Duration,
    ) -> Result<(), SshTransportError> {
        let command = update.command();
        // The command carries the fake X11 cookie. Keep it inside Zeroizing and
        // return only a redacted failure surface if the remote xauth call fails.
        let output = self
            .run_command_capture(command.as_str(), command_timeout, 4096)
            .await?;
        if output.exit_code == Some(0) {
            Ok(())
        } else {
            Err(SshTransportError::Channel(
                "remote xauth update failed".to_string(),
            ))
        }
    }

    pub async fn preflight_host_key_via_direct_tcpip(
        &self,
        host: &str,
        port: u16,
        timeout_secs: u64,
        legacy_compatibility: bool,
        algorithms: &oxideterm_connections::SshAlgorithmPreferences,
    ) -> HostKeyStatus {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return HostKeyStatus::Error {
                message: "no active parent SSH connection is available for host-key preflight"
                    .to_string(),
            };
        };
        if pooled.is_closed().await {
            return HostKeyStatus::Error {
                message: "parent SSH connection is closed and cannot preflight child host key"
                    .to_string(),
            };
        }

        let handle = &pooled.target;
        // Tauri `preflightTreeNode` verifies a child host key through the
        // already-connected parent node. Keep the stream type inside the SSH
        // crate so GPUI can request node-scoped preflight without depending on
        // russh internals.
        match open_direct_tcpip_stream_with_origin(handle, host, port, "127.0.0.1", 0).await {
            Ok(stream) => {
                check_host_key_via_stream(
                    host,
                    port,
                    stream,
                    timeout_secs,
                    legacy_compatibility,
                    algorithms,
                )
                .await
            }
            Err(error) => HostKeyStatus::Error {
                message: error.to_string(),
            },
        }
    }

    pub async fn request_remote_tcpip_forward(
        &self,
        bind_address: &str,
        bind_port: u16,
    ) -> Result<u16, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot request remote port forwarding".to_string(),
            ));
        }

        let handle = &pooled.target;
        let server_port = handle
            .tcpip_forward(bind_address, bind_port as u32)
            .await
            .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;
        resolve_remote_forward_port(bind_port, server_port)
    }

    pub async fn cancel_remote_tcpip_forward(
        &self,
        bind_address: &str,
        bind_port: u16,
    ) -> Result<(), SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        let handle = &pooled.target;
        handle
            .cancel_tcpip_forward(bind_address, bind_port as u32)
            .await
            .map_err(|error| {
                SshTransportError::ConnectionFailed(format!(
                    "failed to cancel remote port forward {bind_address}:{bind_port}: {error}"
                ))
            })
    }

    pub async fn run_command(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<String, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote command execution".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot execute remote commands".to_string(),
            ));
        }

        let mut channel = {
            let handle = &pooled.target;
            handle
                .channel_open_session()
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?
        };
        channel
            .exec(true, command)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;

        let mut output = Vec::new();
        let mut exit_status = None;
        tokio::time::timeout(timeout, async {
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        output.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, ext: 1 } => {
                        output.extend_from_slice(&data);
                    }
                    ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => {
                        exit_status = Some(status);
                    }
                    // EOF only closes the remote output stream. ExitStatus can
                    // still follow, so wait for Close before evaluating the
                    // command result.
                    ChannelMsg::Eof => {}
                    ChannelMsg::Close => break,
                    _ => {}
                }
                if output.len() > max_output_size {
                    output.truncate(max_output_size);
                    break;
                }
            }
        })
        .await
        .map_err(|_| SshTransportError::Timeout)?;
        let _ = channel.close().await;

        if let Some(status) = exit_status
            && status != 0
        {
            return Err(SshTransportError::Channel(format!(
                "remote command exited with status {status}"
            )));
        }

        String::from_utf8(output).map_err(|error| {
            SshTransportError::Channel(format!("remote command output was not UTF-8: {error}"))
        })
    }

    pub async fn run_command_capture(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshCommandOutput, SshTransportError> {
        Ok(self
            .execute_command(command, timeout, max_output_size)
            .await?
            .output)
    }

    pub async fn run_secret_command_capture(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshSecretCommandOutput, SshTransportError> {
        let output = self
            .run_command_capture_bytes(command, timeout, max_output_size)
            .await?;
        // Move both streams directly into zeroizing owners without a plaintext copy.
        Ok(SshSecretCommandOutput {
            stdout: Zeroizing::new(output.stdout),
            stderr: Zeroizing::new(output.stderr),
            exit_code: output.exit_code,
            truncated: output.truncated,
        })
    }

    pub async fn run_secret_pty_command_capture(
        &self,
        command: &str,
        columns: u32,
        rows: u32,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshSecretCommandOutput, SshTransportError> {
        let output = self
            .run_command_capture_bytes_with_pty(
                command,
                timeout,
                max_output_size,
                Some((columns.max(1), rows.max(1))),
            )
            .await?;
        // The PTY can echo secret bootstrap material, so both streams retain zeroizing owners.
        Ok(SshSecretCommandOutput {
            stdout: Zeroizing::new(output.stdout),
            stderr: Zeroizing::new(output.stderr),
            exit_code: output.exit_code,
            truncated: output.truncated,
        })
    }

    async fn run_command_capture_bytes(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshCommandCaptureBytes, SshTransportError> {
        self.run_command_capture_bytes_with_pty(command, timeout, max_output_size, None)
            .await
    }

    async fn run_command_capture_bytes_with_pty(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
        pty_size: Option<(u32, u32)>,
    ) -> Result<SshCommandCaptureBytes, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote command execution".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot execute remote commands".to_string(),
            ));
        }

        Self::capture_exec_on(&pooled, command, timeout, max_output_size, pty_size, None).await
    }

    /// Runs a command on a clean exec channel and writes `stdin` into it.
    ///
    /// The payload never becomes part of the command line, so credentials stay
    /// out of `ps` and the remote shell history. Both streams keep zeroizing
    /// owners because a command can echo what it reads.
    pub async fn run_command_capture_with_stdin(
        &self,
        command: &str,
        stdin: Zeroizing<Vec<u8>>,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<SshSecretCommandOutput, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote command execution".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot execute remote commands".to_string(),
            ));
        }
        let output = Self::capture_exec_on(
            &pooled,
            command,
            timeout,
            max_output_size,
            None,
            Some(stdin),
        )
        .await?;
        Ok(SshSecretCommandOutput {
            stdout: Zeroizing::new(output.stdout),
            stderr: Zeroizing::new(output.stderr),
            exit_code: output.exit_code,
            truncated: output.truncated,
        })
    }

    async fn capture_exec_on(
        pooled: &PooledSshConnection,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
        pty_size: Option<(u32, u32)>,
        stdin: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<SshCommandCaptureBytes, SshTransportError> {
        let mut channel = ChannelCloseGuard::new(
            tokio::time::timeout(timeout, pooled.target.channel_open_session())
                .await
                .map_err(|_| SshTransportError::Timeout)?
                .map_err(|error| SshTransportError::Channel(error.to_string()))?,
        );
        // Zeroizing from the first byte: an aborted, timed-out, or rejected
        // command can still have echoed a secret into either stream, and every
        // early return must wipe what was captured. Successful callers take the
        // buffers out, so the plain-output path pays no wipe.
        let mut stdout = Zeroizing::new(Vec::new());
        let mut stderr = Zeroizing::new(Vec::new());
        let mut exit_status = None;
        let mut total_output_size = 0;
        let mut truncated = false;
        let mut rejected = false;
        let captured = tokio::time::timeout(timeout, async {
            let channel = channel.get_mut();
            if let Some((columns, rows)) = pty_size {
                channel
                    .request_pty(
                        false,
                        "xterm-256color",
                        columns,
                        rows,
                        0,
                        0,
                        DEFAULT_PTY_MODES,
                    )
                    .await
                    .map_err(|error| SshTransportError::Channel(error.to_string()))?;
            }
            channel
                .exec(true, command)
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?;
            // Bounded payloads are written before the read loop: the request
            // never waits on the remote process to consume its input first.
            if let Some(stdin) = stdin {
                channel
                    .data(stdin.as_slice())
                    .await
                    .map_err(|error| SshTransportError::Channel(error.to_string()))?;
                channel
                    .eof()
                    .await
                    .map_err(|error| SshTransportError::Channel(error.to_string()))?;
            }

            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        append_limited_command_output(
                            &mut stdout,
                            &data,
                            max_output_size,
                            &mut total_output_size,
                            &mut truncated,
                        );
                    }
                    ChannelMsg::ExtendedData { data, ext: 1 } => {
                        append_limited_command_output(
                            &mut stderr,
                            &data,
                            max_output_size,
                            &mut total_output_size,
                            &mut truncated,
                        );
                    }
                    ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => {
                        exit_status = Some(status);
                    }
                    // RFC 4254 allows the exit status to arrive after EOF.
                    // Keep the capture channel alive until Close so successful
                    // host-tool probes are not reported with an unknown status.
                    ChannelMsg::Eof => {}
                    ChannelMsg::Failure => {
                        rejected = true;
                        break;
                    }
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok::<(), SshTransportError>(())
        })
        .await;
        // Graceful close first; the guard disarms so a completed capture does
        // not queue a second close.
        if let Some(channel) = channel.take() {
            let _ = tokio::time::timeout(timeout, channel.close()).await;
        }
        captured.map_err(|_| SshTransportError::Timeout)??;

        if rejected {
            return Err(SshTransportError::CommandRejected);
        }

        Ok(SshCommandCaptureBytes {
            stdout: std::mem::take(&mut *stdout),
            stderr: std::mem::take(&mut *stderr),
            exit_code: exit_status.and_then(|status| i32::try_from(status).ok()),
            truncated,
        })
    }

    pub async fn open_session_channel(
        &self,
    ) -> Result<russh::Channel<client::Msg>, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for a session channel".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot open a session channel".to_string(),
            ));
        }

        let handle = &pooled.target;
        handle
            .channel_open_session()
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))
    }

    pub async fn open_persistent_shell_channel(
        &self,
        init_command: &str,
    ) -> Result<SshShellChannel, SshTransportError> {
        self.open_sampling_shell_channel(init_command, false, SHELL_OPEN_TIMEOUT)
            .await
    }

    pub(crate) async fn open_sampling_shell_channel(
        &self,
        init_command: &str,
        with_pty: bool,
        timeout: Duration,
    ) -> Result<SshShellChannel, SshTransportError> {
        if !self.allows_additional_channels() {
            return Err(SshTransportError::CommandChannelUnavailable);
        }
        let pooled = self.physical::<PooledSshConnection>().ok_or_else(|| {
            SshTransportError::ConnectionFailed("no active SSH connection is available".into())
        })?;
        Self::open_sampling_shell_on(&pooled, init_command, with_pty, timeout).await
    }

    async fn open_sampling_shell_on(
        pooled: &PooledSshConnection,
        init_command: &str,
        with_pty: bool,
        timeout: Duration,
    ) -> Result<SshShellChannel, SshTransportError> {
        let deadline = Instant::now() + timeout;
        let channel = tokio::time::timeout_at(deadline, pooled.target.channel_open_session())
            .await
            .map_err(|_| SshTransportError::Timeout)?
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        // Keep ownership outside initialization's timeout so cancellation can close the channel.
        let mut shell = SshShellChannel { channel, with_pty };
        let initialized = tokio::time::timeout_at(
            deadline,
            Self::initialize_sampling_shell(&mut shell, init_command),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)
        .and_then(|result| result);
        if let Err(error) = initialized {
            let _ = tokio::time::timeout(SHELL_CLOSE_TIMEOUT, shell.close()).await;
            return Err(error);
        }
        Ok(shell)
    }

    async fn initialize_sampling_shell(
        shell: &mut SshShellChannel,
        init_command: &str,
    ) -> Result<(), SshTransportError> {
        let with_pty = shell.with_pty;
        if with_pty {
            // Sampling scripts can exceed the terminal's canonical input limit.
            shell
                .channel
                .request_pty(
                    true,
                    "xterm",
                    120,
                    40,
                    0,
                    0,
                    &[(Pty::ECHO, 0), (Pty::ICANON, 0)],
                )
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        }
        shell
            .channel
            .request_shell(with_pty)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        if with_pty {
            // A gateway may finish target login after accepting the shell request.
            // Confirm initialization before sending a script that can exceed 4 KiB.
            let initialization = if init_command.is_empty() {
                // Environment detection has not selected a shell dialect yet.
                "echo __OXIDE_MONITOR_READY__\r\n".to_string()
            } else if init_command.starts_with("set PROMPT=") {
                format!("{init_command}echo __OXIDE_MONITOR_READY__\r\n")
            } else {
                format!(
                    "stty -echo -icanon 2>/dev/null; {init_command}\nprintf '\\n%s\\n' '__OXIDE_MONITOR_READY__'\n"
                )
            };
            let mut attempt = 1;
            loop {
                match shell
                    .sample_until(
                        &initialization,
                        "__OXIDE_MONITOR_READY__",
                        Duration::from_secs(2),
                        64 * 1024,
                    )
                    .await
                {
                    Ok(_) => {
                        return Ok(());
                    }
                    Err(SshTransportError::Timeout) if attempt < 3 => {
                        // Retry only this idempotent probe: early gateway input can be lost.
                        attempt += 1;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        if !init_command.is_empty() {
            shell
                .channel
                .data(init_command.as_bytes())
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok(())
    }

    pub fn shared_remote_forward_handler<T: RemoteForwardHandler + Default>(
        &self,
    ) -> Result<Arc<T>, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        // Each physical transport dispatches every logical node's incoming channels through one router.
        let mut slot = pooled.remote_forward_handler.write();
        let registration = slot.get_or_insert_with(|| RemoteForwardRegistration {
            connection_id: self.connection_id().to_owned(),
            handler: Arc::new(T::default()),
        });
        let handler: Arc<dyn std::any::Any + Send + Sync> = registration.handler.clone();
        handler.downcast::<T>().map_err(|_| {
            SshTransportError::ConnectionFailed("remote forward dispatcher type mismatch".into())
        })
    }

    pub fn set_x11_forward_handler(
        &self,
        handler: Arc<dyn X11ForwardHandler>,
    ) -> Result<(), SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for X11 forwarding".to_string(),
            ));
        };
        *pooled.x11_forward_handler.write() = Some(X11ForwardRegistration {
            connection_id: self.connection_id().to_string(),
            handler,
        });
        Ok(())
    }

    pub fn clear_x11_forward_handler(&self) {
        if let Some(pooled) = self.physical::<PooledSshConnection>() {
            *pooled.x11_forward_handler.write() = None;
        }
    }
}

struct SshCommandCaptureBytes {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
    truncated: bool,
}

fn resolve_remote_forward_port(
    requested_port: u16,
    server_port: u32,
) -> Result<u16, SshTransportError> {
    if server_port == 0 {
        if requested_port == 0 {
            return Err(SshTransportError::ConnectionFailed(
                "remote forwarding server accepted a port allocation request without returning the allocated port"
                    .to_string(),
            ));
        }

        // RFC 4254 includes a port in the success response only for allocation
        // requests. Russh represents an empty explicit-port response as zero.
        return Ok(requested_port);
    }

    u16::try_from(server_port).map_err(|_| {
        SshTransportError::ConnectionFailed(format!(
            "remote forwarding server returned invalid port {server_port}"
        ))
    })
}

#[cfg(test)]
mod remote_forward_port_tests {
    use super::*;

    #[test]
    fn remote_forward_port_resolution_handles_explicit_and_allocated_responses() {
        for (requested, returned, expected) in [
            (58_627, 0, Some(58_627)),
            (0, 42_000, Some(42_000)),
            (0, 0, None),
            (0, u16::MAX as u32 + 1, None),
        ] {
            assert_eq!(
                resolve_remote_forward_port(requested, returned).ok(),
                expected,
                "{requested}/{returned}"
            );
        }
    }

    #[tokio::test]
    async fn probe_alive_requires_peer_reply_and_times_out_on_silent_socket() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        struct Peer;
        impl russh::server::Handler for Peer {
            type Error = russh::Error;
            async fn auth_none(&mut self, _: &str) -> Result<russh::server::Auth, Self::Error> {
                Ok(russh::server::Auth::Accept)
            }
        }

        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_port = proxy_listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let blackhole = Arc::new(AtomicBool::new(false));
        let blackhole_proxy = blackhole.clone();

        let server_task = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let session = russh::server::run_stream(
                Arc::new(russh::server::Config {
                    keys: vec![key],
                    ..Default::default()
                }),
                stream,
                Peer,
            )
            .await
            .unwrap();
            let _ = session.await;
        });

        let proxy_task = tokio::spawn(async move {
            let (client_stream, _) = proxy_listener.accept().await.unwrap();
            let server_stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
            let (mut c_read, mut c_write) = client_stream.into_split();
            let (mut s_read, mut s_write) = server_stream.into_split();
            let bh_c2s = blackhole_proxy.clone();
            let c2s = tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    let n = match c_read.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    if bh_c2s.load(Ordering::Acquire) {
                        continue;
                    }
                    if s_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
            let bh_s2c = blackhole_proxy;
            let s2c = tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    let n = match s_read.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    if bh_s2c.load(Ordering::Acquire) {
                        continue;
                    }
                    if c_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
            let _ = tokio::join!(c2s, s2c);
        });

        let registry = SshConnectionRegistry::default();
        let mut config = SshConfig::password("127.0.0.1", proxy_port, "fixture", "pw");
        config.expected_host_key_fingerprint = Some(fingerprint);
        config.trust_host_key = Some(false);
        let handle = timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config).connect_node_with_registry(
                registry.clone(),
                ConnectionConsumer::Terminal("probe-test".into()),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        assert_eq!(
            handle.probe_alive(Duration::from_secs(2)).await,
            KeepaliveProbeResult::Ok
        );
        assert!(handle.recent_rtt(Duration::from_secs(60)).is_some());

        // Drop all forwarded packets while keeping the TCP sockets open to
        // emulate a half-open network blackout.
        blackhole.store(true, Ordering::Release);
        assert_eq!(
            handle.probe_alive(Duration::from_millis(150)).await,
            KeepaliveProbeResult::Timeout
        );

        // Two consecutive sweep timeouts must tolerate brief weak-network
        // stalls; the third marks the connection LinkDown.
        for _ in 0..(crate::HEARTBEAT_FAIL_THRESHOLD - 1) {
            let changed = registry
                .probe_active_connections(Duration::from_millis(100))
                .await;
            assert!(changed.is_empty());
            assert_eq!(handle.state(), ConnectionState::Active);
        }
        let changed = registry
            .probe_active_connections(Duration::from_millis(100))
            .await;
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].connection_id, handle.connection_id());
        assert_eq!(handle.state(), ConnectionState::LinkDown);

        proxy_task.abort();
        server_task.abort();
    }
}
