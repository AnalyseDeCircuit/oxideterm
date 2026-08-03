// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn response_frame_output(frame: Vec<u8>) -> SessionResult<Vec<ActiveStageOutput>> {
    if frame.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![ActiveStageOutput::ResponseFrame(frame)])
    }
}

pub(super) fn run_client_rdp_loop(
    writer: &SharedEventWriter,
    request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_rx: ClientRdpOutputReceiver,
    config: &mut RdpWorkerConfig,
    read_only: bool,
) -> Result<ClientRdpSessionExit, String> {
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();
    loop {
        let mut handled_requests = false;
        let mut reconnect_requested = false;
        let mut coalesced_requests = Vec::new();
        let mut request_coalescer = ClientRdpRequestCoalescer::default();
        // Bound request draining so display output still advances during input bursts.
        for _ in 0..RDP_CLIENT_REQUEST_DRAIN_LIMIT {
            match request_rx.try_recv() {
                Ok(RemoteDesktopHelperRequest::Close) => return Ok(ClientRdpSessionExit::Closed),
                Ok(RemoteDesktopHelperRequest::Reconnect) => reconnect_requested = true,
                Ok(request) => {
                    handled_requests = true;
                    if matches!(
                        request,
                        RemoteDesktopHelperRequest::Resize { .. }
                            | RemoteDesktopHelperRequest::UpdateDisplayLayout { .. }
                    ) {
                        // The pinned IronRDP session cannot safely preserve all
                        // decoder state across DisplayControl reactivation.
                        remember_rdp_reconnect_state(&request, config);
                        reconnect_requested = true;
                        continue;
                    }
                    request_coalescer.push(request, &mut coalesced_requests);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(ClientRdpSessionExit::Closed),
            }
        }
        request_coalescer.flush(&mut coalesced_requests);
        if reconnect_requested {
            // Coalesce every queued display request into one replacement
            // connection so a fit action cannot cause consecutive reconnects.
            begin_rdp_reconnect(writer, config)?;
            return Ok(ClientRdpSessionExit::ReconnectRequested);
        }
        for request in coalesced_requests {
            remember_rdp_reconnect_state(&request, config);
            forward_client_rdp_request(
                input_tx,
                &mut input_database,
                &mut keyboard_mapper,
                request,
                read_only,
            )?;
        }

        // User input should not sit behind a burst of frame events. Drain the
        // helper output after forwarding queued requests so high-update
        // desktops cannot add avoidable keyboard and pointer latency.
        let output_drain = drain_client_rdp_outputs(writer, &output_rx)?;
        if let Some(exit) = output_drain.exit {
            return Ok(exit);
        }

        if output_drain.drained < RDP_CLIENT_OUTPUT_DRAIN_LIMIT && !handled_requests {
            thread::sleep(RDP_CLIENT_LOOP_POLL_INTERVAL);
        }
    }
}

pub(super) fn drain_client_rdp_outputs(
    writer: &SharedEventWriter,
    output_rx: &ClientRdpOutputReceiver,
) -> Result<ClientRdpOutputDrain, String> {
    let mut drain = ClientRdpOutputDrain::default();
    while drain.drained < RDP_CLIENT_OUTPUT_DRAIN_LIMIT {
        match output_rx.control_rx.try_recv() {
            Ok(output) => {
                drain.drained += 1;
                handle_client_rdp_output(writer, output, &mut drain)?;
                if drain.exit.is_some() {
                    return Ok(drain);
                }
            }
            Err(mpsc::TryRecvError::Empty) => match output_rx.graphics_rx.try_recv() {
                Ok(output) => {
                    drain.drained += 1;
                    handle_client_rdp_output(writer, output, &mut drain)?;
                    if drain.exit.is_some() {
                        return Ok(drain);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(drain),
                Err(mpsc::TryRecvError::Disconnected) => return Ok(drain),
            },
            Err(mpsc::TryRecvError::Disconnected) => match output_rx.graphics_rx.try_recv() {
                Ok(output) => {
                    drain.drained += 1;
                    handle_client_rdp_output(writer, output, &mut drain)?;
                    if drain.exit.is_some() {
                        return Ok(drain);
                    }
                }
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                    drain.exit = Some(ClientRdpSessionExit::RemoteEnded(None));
                    return Ok(drain);
                }
            },
        }
    }
    Ok(drain)
}

pub(super) fn handle_client_rdp_output(
    writer: &SharedEventWriter,
    output: ClientRdpOutput,
    drain: &mut ClientRdpOutputDrain,
) -> Result<(), String> {
    match output {
        ClientRdpOutput::Event(event) => send_event(writer, event)?,
        ClientRdpOutput::ConnectionFailure(error) => {
            // Keep the typed connector error available until the helper event
            // is built; string messages are only the display surface, not the
            // classification source.
            drain.exit = Some(ClientRdpSessionExit::ConnectionFailed {
                message: format_connector_error("RDP connection failed", &error),
                category: connector_error_category(&error),
            });
        }
        ClientRdpOutput::ProtocolFailure(message) => {
            drain.exit = Some(ClientRdpSessionExit::ConnectionFailed {
                message,
                category: RemoteDesktopErrorCategory::Protocol,
            });
        }
        ClientRdpOutput::Terminated(message) => {
            drain.exit = Some(ClientRdpSessionExit::RemoteEnded(Some(message)));
        }
        ClientRdpOutput::OutputEnded => {
            drain.exit = Some(ClientRdpSessionExit::RemoteEnded(None));
        }
    }
    Ok(())
}

pub(super) fn remember_rdp_reconnect_state(
    request: &RemoteDesktopHelperRequest,
    config: &mut RdpWorkerConfig,
) {
    if let RemoteDesktopHelperRequest::Resize { size, scale_factor } = request {
        // Reconnects rebuild the IronRDP connector from RdpWorkerConfig, so the
        // last requested display size must live there instead of only in the
        // active client thread.
        config.size = normalized_rdp_desktop_size(*size);
        config.scale_factor = rdp_connector_scale_factor(*scale_factor);
    } else if let RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } = request {
        // Multi-monitor topology is part of initial activation, so a controlled
        // reconnect must rebuild the connector with the latest local layout.
        config.monitor_layout = layout.clone();
    }
}

fn begin_rdp_reconnect(
    writer: &SharedEventWriter,
    config: &mut RdpWorkerConfig,
) -> Result<(), String> {
    config.graphics_epoch = config.graphics_epoch.saturating_add(1).max(1);
    // The barrier reaches the UI before the replacement client can publish a
    // base frame, so queued pixels from the retired connection are discarded.
    send_event(
        writer,
        RemoteDesktopHelperEvent::FrameStreamReset {
            graphics_epoch: config.graphics_epoch,
        },
    )
}

pub(super) fn normalized_rdp_desktop_size(size: RemoteDesktopSize) -> RemoteDesktopSize {
    let size = RemoteDesktopSize::clamped(size.width, size.height);
    let (width, height) = MonitorLayoutEntry::adjust_display_size(size.width, size.height);
    RemoteDesktopSize { width, height }
}

pub(super) fn rdp_connector_scale_factor(scale_factor: Option<u32>) -> u32 {
    rdp_valid_scale_factor(scale_factor).unwrap_or(RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT)
}

pub(super) fn rdp_displaycontrol_scale_factor(scale_factor: Option<u32>) -> u32 {
    rdp_valid_scale_factor(scale_factor).unwrap_or(RDP_DISPLAYCONTROL_DEFAULT_SCALE_FACTOR_PERCENT)
}

pub(super) fn rdp_valid_scale_factor(scale_factor: Option<u32>) -> Option<u32> {
    match scale_factor {
        Some(scale_factor)
            if (RDP_MIN_SCALE_FACTOR_PERCENT..=RDP_MAX_SCALE_FACTOR_PERCENT)
                .contains(&scale_factor) =>
        {
            Some(scale_factor)
        }
        _ => None,
    }
}

pub(super) fn sanitize_rdp_disconnect_reason(reason: Option<&str>) -> Option<String> {
    let reason = reason?.trim();
    if reason.is_empty() || reason.contains("/Users/") || reason.contains(".cargo/git/checkouts") {
        return Some("RDP session ended.".to_string());
    }
    Some(format!("RDP session ended: {reason}."))
}

pub(super) fn rdp_frame_read_error_context(error: &impl fmt::Display) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("eof")
        || message.contains("connection reset")
        || message.contains("broken pipe")
    {
        // The server accepted the session and then closed the transport. Keep
        // this distinct from authentication and connector failures.
        "server closed established RDP session while reading frames"
    } else {
        "read RDP frame"
    }
}

pub(super) async fn handle_deactivate_all<ReadStream, WriteStream>(
    reader: &mut ironrdp_tokio::TokioFramed<ReadStream>,
    writer: &mut ironrdp_tokio::TokioFramed<WriteStream>,
    active_stage: &mut ActiveStage,
    image: &mut DecodedImage,
    mut connection_activation: Box<
        ironrdp::connector::connection_activation::ConnectionActivationSequence,
    >,
) -> SessionResult<()>
where
    ReadStream: AsyncRead + Send + Sync + Unpin,
    WriteStream: AsyncWrite + Send + Sync + Unpin,
{
    let mut buffer = WriteBuf::new();
    loop {
        let written = single_sequence_step_read(reader, &mut *connection_activation, &mut buffer)
            .await
            .map_err(|error| {
                session::custom_err!("read deactivation-reactivation sequence step", error)
            })?;
        if written.size().is_some() {
            writer.write_all(buffer.filled()).await.map_err(|error| {
                session::custom_err!("write deactivation-reactivation sequence step", error)
            })?;
        }

        if let ConnectionActivationState::Finalized {
            io_channel_id,
            user_channel_id,
            desktop_size,
            share_id,
            enable_server_pointer,
            pointer_software_rendering,
        } = connection_activation.connection_activation_state()
        {
            // The server can assign new channel IDs after reactivation; reset
            // both the decoded image and active stage before accepting pixels.
            *image = DecodedImage::new(
                RDP_DECODED_FRAME_PIXEL_FORMAT,
                desktop_size.width,
                desktop_size.height,
            );
            active_stage.set_fastpath_processor(
                fast_path::ProcessorBuilder {
                    io_channel_id,
                    user_channel_id,
                    share_id,
                    enable_server_pointer,
                    pointer_software_rendering,
                    bulk_decompressor: None,
                }
                .build(),
            );
            active_stage.set_share_id(share_id);
            active_stage.set_enable_server_pointer(enable_server_pointer);
            return Ok(());
        }
    }
}
