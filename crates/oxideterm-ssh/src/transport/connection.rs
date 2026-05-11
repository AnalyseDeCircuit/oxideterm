struct PooledSshConnection {
    target: Mutex<client::Handle<NativeClientHandler>>,
    _jump_handles: Vec<client::Handle<NativeClientHandler>>,
    remote_forward_handler: RemoteForwardHandlerSlot,
}

impl PooledSshConnection {
    fn direct(
        handle: client::Handle<NativeClientHandler>,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Self {
        Self {
            target: Mutex::new(handle),
            _jump_handles: Vec::new(),
            remote_forward_handler,
        }
    }

    fn tunneled(
        target: client::Handle<NativeClientHandler>,
        jump_handles: Vec<client::Handle<NativeClientHandler>>,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Self {
        Self {
            target: Mutex::new(target),
            _jump_handles: jump_handles,
            remote_forward_handler,
        }
    }

    async fn is_closed(&self) -> bool {
        self.target.lock().await.is_closed()
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
        if let Some(pooled) = self.physical::<PooledSshConnection>() {
            if pooled.is_closed().await {
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

    pub async fn probe_alive(&self, probe_timeout: Duration) -> KeepaliveProbeResult {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return KeepaliveProbeResult::IoError;
        };
        if pooled.is_closed().await {
            return KeepaliveProbeResult::IoError;
        }

        let handle = pooled.target.lock().await;
        // Tauri's app-level heartbeat uses an SSH GLOBAL_REQUEST
        // `keepalive@openssh.com` with want_reply=true, not an exec channel.
        // This native russh fork exposes `send_ping()` for the same frame and
        // waits for the reply so the 5s Tauri timeout remains observable.
        match timeout(probe_timeout, handle.send_ping()).await {
            Ok(Ok(())) => KeepaliveProbeResult::Ok,
            Ok(Err(error)) => {
                let error = format!("{error:?}");
                if error.contains("Disconnect") || error.contains("disconnect") {
                    KeepaliveProbeResult::IoError
                } else {
                    KeepaliveProbeResult::Timeout
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

        let handle = pooled.target.lock().await;
        let stream =
            open_direct_tcpip_stream_with_origin(&handle, host, port, origin_host, origin_port)
                .await?;
        Ok(Box::new(stream))
    }

    pub async fn request_remote_tcpip_forward(
        &self,
        bind_address: &str,
        bind_port: u16,
    ) -> Result<u16, SshTransportError> {
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

        let handle = pooled.target.lock().await;
        handle
            .tcpip_forward(bind_address, bind_port as u32)
            .await
            .map(|port| port as u16)
            .map_err(|error| {
                SshTransportError::ConnectionFailed(format!(
                    "failed to request remote port forward {bind_address}:{bind_port}: {error}"
                ))
            })
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
        let handle = pooled.target.lock().await;
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
            let handle = pooled.target.lock().await;
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
                    ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                        output.extend_from_slice(&data);
                    }
                    ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => {
                        exit_status = Some(status);
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => break,
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

    pub(crate) async fn open_session_channel(
        &self,
    ) -> Result<russh::Channel<client::Msg>, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for SFTP".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot open an SFTP channel".to_string(),
            ));
        }

        let handle = pooled.target.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))
    }

    pub async fn open_persistent_shell_channel(
        &self,
        init_command: &str,
    ) -> Result<SshShellChannel, SshTransportError> {
        let channel = self.open_session_channel().await?;
        channel
            .request_shell(false)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        if !init_command.is_empty() {
            channel
                .data(init_command.as_bytes())
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok(SshShellChannel { channel })
    }

    pub fn set_remote_forward_handler(
        &self,
        handler: Arc<dyn RemoteForwardHandler>,
    ) -> Result<(), SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        *pooled.remote_forward_handler.write() = Some(handler);
        Ok(())
    }

    pub fn clear_remote_forward_handler(&self) {
        if let Some(pooled) = self.physical::<PooledSshConnection>() {
            *pooled.remote_forward_handler.write() = None;
        }
    }
}
