// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Debug)]
pub(super) struct ClientRdpFrameState {
    pub(super) graphics_sync: RdpGraphicsSyncState,
    pub(super) graphics_accumulator: RdpGraphicsFrameAccumulator,
    pub(super) pending_base_frame: bool,
    pub(super) pending_base_frame_can_publish_ready: bool,
    pub(super) published_first_desktop_frame: bool,
    pub(super) next_graphics_trace_id: u64,
    pub(super) graphics_diagnostics: RdpGraphicsDiagnostics,
}

impl Default for ClientRdpFrameState {
    fn default() -> Self {
        Self {
            graphics_sync: RdpGraphicsSyncState::default(),
            graphics_accumulator: RdpGraphicsFrameAccumulator::default(),
            pending_base_frame: false,
            pending_base_frame_can_publish_ready: false,
            published_first_desktop_frame: false,
            next_graphics_trace_id: 0,
            graphics_diagnostics: RdpGraphicsDiagnostics::from_env(),
        }
    }
}

impl ClientRdpFrameState {
    fn next_graphics_trace_id(&mut self) -> u64 {
        self.next_graphics_trace_id = self.next_graphics_trace_id.saturating_add(1).max(1);
        self.next_graphics_trace_id
    }
}

#[derive(Debug)]
pub(super) struct RdpGraphicsDiagnostics {
    enabled: bool,
    last_report: Instant,
    graphics_updates: u64,
    skipped_updates: u64,
    base_frames: u64,
    dirty_updates: u64,
    copied_bytes: u64,
    base_frame_bytes: u64,
    dirty_update_bytes: u64,
    dirty_pixels: u64,
    dirty_frame_pixels: u64,
    last_trace_id: u64,
}

impl RdpGraphicsDiagnostics {
    fn from_env() -> Self {
        Self {
            enabled: std::env::var_os(RDP_GRAPHICS_DIAGNOSTICS_ENV).is_some(),
            last_report: Instant::now(),
            graphics_updates: 0,
            skipped_updates: 0,
            base_frames: 0,
            dirty_updates: 0,
            copied_bytes: 0,
            base_frame_bytes: 0,
            dirty_update_bytes: 0,
            dirty_pixels: 0,
            dirty_frame_pixels: 0,
            last_trace_id: 0,
        }
    }

    fn record_graphics_update(&mut self) {
        if self.enabled {
            self.graphics_updates = self.graphics_updates.saturating_add(1);
        }
    }

    fn record_skipped_update(&mut self) {
        if self.enabled {
            self.skipped_updates = self.skipped_updates.saturating_add(1);
            self.maybe_report();
        }
    }

    fn record_base_frame(&mut self, trace_id: u64, size: RemoteDesktopSize, byte_len: usize) {
        if !self.enabled {
            return;
        }
        self.last_trace_id = trace_id;
        self.base_frames = self.base_frames.saturating_add(1);
        let byte_len = byte_len as u64;
        self.copied_bytes = self.copied_bytes.saturating_add(byte_len);
        self.base_frame_bytes = self.base_frame_bytes.saturating_add(byte_len);
        self.dirty_frame_pixels = self.dirty_frame_pixels.saturating_add(frame_pixels(size));
        self.maybe_report();
    }

    fn record_dirty_update(
        &mut self,
        trace_id: u64,
        size: RemoteDesktopSize,
        rect: oxideterm_remote_desktop::RemoteDesktopRect,
        byte_len: usize,
    ) {
        if !self.enabled {
            return;
        }
        self.last_trace_id = trace_id;
        self.dirty_updates = self.dirty_updates.saturating_add(1);
        let byte_len = byte_len as u64;
        self.copied_bytes = self.copied_bytes.saturating_add(byte_len);
        self.dirty_update_bytes = self.dirty_update_bytes.saturating_add(byte_len);
        self.dirty_pixels = self.dirty_pixels.saturating_add(rect_pixels(rect));
        self.dirty_frame_pixels = self.dirty_frame_pixels.saturating_add(frame_pixels(size));
        self.maybe_report();
    }

    fn maybe_report(&mut self) {
        if self.last_report.elapsed() < RDP_GRAPHICS_DIAGNOSTICS_REPORT_INTERVAL {
            return;
        }
        let dirty_ratio = ratio_per_mille(self.dirty_pixels, self.dirty_frame_pixels);
        eprintln!(
            "[oxideterm:rdp-helper-graphics] trace={} graphics_updates={} skipped={} base_frames={} dirty_updates={} copied_bytes={} base_bytes={} dirty_bytes={} dirty_ratio_per_mille={}",
            self.last_trace_id,
            self.graphics_updates,
            self.skipped_updates,
            self.base_frames,
            self.dirty_updates,
            self.copied_bytes,
            self.base_frame_bytes,
            self.dirty_update_bytes,
            dirty_ratio,
        );
        self.last_report = Instant::now();
    }
}

pub(super) async fn connect_native_rdp(
    config: &ClientRdpConfig,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputSender,
) -> connector::ConnectorResult<(ConnectionResult, UpgradedRdpFramed, EgfxSessionBridge)> {
    let socket = TcpStream::connect((config.destination.host(), config.destination.port()))
        .await
        .map_err(|error| connector::custom_err!("TCP connect", error))?;
    socket
        .set_nodelay(true)
        .map_err(|error| connector::custom_err!("set TCP_NODELAY", error))?;
    let client_addr = socket
        .local_addr()
        .map_err(|error| connector::custom_err!("get socket local address", error))?;
    let mut framed = ironrdp_tokio::TokioFramed::new(socket);
    let mut connector = connector::ClientConnector::new(config.connector.clone(), client_addr);
    let egfx_bridge = attach_client_virtual_channels(&mut connector, input_tx, output_tx);
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;
    let (initial_stream, leftover_bytes) = framed.into_inner();
    let (upgraded_stream, tls_cert) =
        ironrdp_tls::upgrade(initial_stream, config.destination.host())
            .await
            .map_err(|error| connector::custom_err!("TLS upgrade", error))?;
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let erased_stream: Box<dyn AsyncReadWrite + Unpin + Send + Sync> = Box::new(upgraded_stream);
    let mut upgraded_framed =
        ironrdp_tokio::TokioFramed::new_with_leftover(erased_stream, leftover_bytes);
    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&tls_cert)
        .ok_or_else(|| connector::general_err!("unable to extract TLS server public key"))?;
    let connection_result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut upgraded_framed,
        &mut ironrdp_tokio::reqwest::ReqwestNetworkClient::new(),
        connector::ServerName::new(config.destination.host().to_string()),
        server_public_key.to_owned(),
        None,
    )
    .await?;
    log_rdp_negotiated_graphics(&config.connector, &connection_result);

    Ok((connection_result, upgraded_framed, egfx_bridge))
}

pub(super) fn attach_client_virtual_channels(
    connector: &mut connector::ClientConnector,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputSender,
) -> EgfxSessionBridge {
    let (graphics_pipeline, egfx_bridge) = new_egfx_channel(output_tx.clone());
    let display_control = DrdynvcClient::new()
        .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new())))
        .with_dynamic_channel(graphics_pipeline);
    connector.attach_static_channel(display_control);

    // CLIPRDR is attached as a normal static channel while the backend itself
    // bridges callbacks into OxideTerm's helper protocol.
    let clipboard = ClientClipboardBackend::new(input_tx, output_tx);
    connector.attach_static_channel(CliprdrClient::new(Box::new(clipboard)));

    // The bridge is session-owned and is dropped after the active RDP loop exits.
    egfx_bridge
}

pub(super) async fn run_native_rdp_active_session(
    framed: UpgradedRdpFramed,
    connection_result: ConnectionResult,
    input_rx: &mut tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    output_tx: &ClientRdpOutputSender,
    egfx_bridge: &EgfxSessionBridge,
) -> SessionResult<ClientRdpControlFlow> {
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let mut image = DecodedImage::new(
        RDP_DECODED_FRAME_PIXEL_FORMAT,
        connection_result.desktop_size.width,
        connection_result.desktop_size.height,
    );
    let mut active_stage = ActiveStage::new(connection_result);
    let mut clipboard_cleanup = tokio::time::interval(RDP_CLIPBOARD_TIMEOUT_POLL_INTERVAL);
    let mut frame_state = ClientRdpFrameState::default();

    let disconnect_reason = 'session: loop {
        flush_pending_rdp_base_frame(output_tx, &image, &mut frame_state)?;
        flush_pending_rdp_graphics_updates(output_tx, &image, &mut frame_state)?;
        let graphics_flush_delay = frame_state.graphics_accumulator.next_flush_delay();

        let outputs = tokio::select! {
            _ = wait_for_graphics_accumulator_flush(graphics_flush_delay) => {
                flush_pending_rdp_graphics_updates(output_tx, &image, &mut frame_state)?;
                Vec::new()
            }
            frame = reader.read_pdu() => {
                let (action, payload) = frame
                    .map_err(|error| {
                        if rdp_frame_read_error_context(&error)
                            == "server closed established RDP session while reading frames"
                        {
                            session::custom_err!(
                                "server closed established RDP session while reading frames",
                                error
                            )
                        } else {
                            session::custom_err!("read RDP frame", error)
                        }
                    })?;
                active_stage.process(&mut image, action, &payload)?
            }
            input = input_rx.recv() => {
                let input = input.ok_or_else(|| session::general_err!("RDP input channel closed"))?;
                match input {
                    RdpInputEvent::Resize {
                        width,
                        height,
                        scale_factor,
                        physical_size,
                    } => {
                        if let Some(response_frame) =
                            active_stage.encode_resize(
                                u32::from(width),
                                u32::from(height),
                                Some(scale_factor),
                                physical_size,
                            )
                        {
                            vec![ActiveStageOutput::ResponseFrame(response_frame?)]
                        } else {
                            // Some servers, notably xrdp/GNOME setups, do not
                            // expose DisplayControl after activation. Keep the
                            // live framebuffer and let the UI scale it locally
                            // instead of tearing down a usable session.
                            send_client_rdp_event(
                                output_tx,
                                unsupported_resize_connected_event(&image),
                            )?;
                            Vec::new()
                        }
                    }
                    RdpInputEvent::FastPath(events) => {
                        active_stage.process_fastpath_input(&mut image, &events)?
                    }
                    RdpInputEvent::Clipboard(message) => {
                        process_clipboard_message(&mut active_stage, message)?
                    }
                    RdpInputEvent::SetClipboardText(text) => {
                        advertise_local_clipboard_text(&mut active_stage, text)?
                    }
                    RdpInputEvent::SetClipboardData(data) => {
                        advertise_local_clipboard_data(&mut active_stage, data)?
                    }
                    RdpInputEvent::RequestFrame => {
                        if !egfx_bridge
                            .request_base_frame()
                            .map_err(|error| session::custom_err!("request EGFX base frame", error))?
                        {
                            send_client_rdp_base_frame(
                                output_tx,
                                &image,
                                &mut frame_state,
                                false,
                            )?;
                        }
                        Vec::new()
                    }
                    RdpInputEvent::Close => active_stage.graceful_shutdown()?,
                }
            }
            _ = clipboard_cleanup.tick() => {
                drive_clipboard_timeouts(&mut active_stage)?
            }
        };

        for output in outputs {
            match output {
                ActiveStageOutput::ResponseFrame(frame) => writer
                    .write_all(&frame)
                    .await
                    .map_err(|error| session::custom_err!("write response", error))?,
                ActiveStageOutput::GraphicsUpdate(region) => {
                    send_client_rdp_graphics_update(output_tx, &image, region, &mut frame_state)?;
                }
                ActiveStageOutput::PointerPosition { x, y } => {
                    send_client_rdp_event(
                        output_tx,
                        RemoteDesktopHelperEvent::Cursor {
                            x: u32::from(x),
                            y: u32::from(y),
                            width: 0,
                            height: 0,
                        },
                    )?;
                }
                ActiveStageOutput::PointerDefault => {
                    send_client_rdp_event(output_tx, RemoteDesktopHelperEvent::CursorDefault)?;
                }
                ActiveStageOutput::PointerHidden => {
                    send_client_rdp_event(output_tx, RemoteDesktopHelperEvent::CursorHidden)?;
                }
                ActiveStageOutput::PointerBitmap(pointer) => {
                    send_client_rdp_event(
                        output_tx,
                        RemoteDesktopHelperEvent::CursorShape {
                            shape: RemoteDesktopCursorShape::new(
                                RemoteDesktopSize {
                                    width: u32::from(pointer.width),
                                    height: u32::from(pointer.height),
                                },
                                u32::from(pointer.hotspot_x),
                                u32::from(pointer.hotspot_y),
                                RemoteDesktopFrameFormat::Rgba8,
                                pointer.bitmap_data.clone(),
                            ),
                        },
                    )?;
                }
                ActiveStageOutput::DeactivateAll(connection_activation) => {
                    handle_deactivate_all(
                        &mut reader,
                        &mut writer,
                        &mut active_stage,
                        &mut image,
                        connection_activation,
                    )
                    .await?;
                    reset_graphics_base_after_reactivation(&mut frame_state);
                    egfx_bridge.prepare_for_reactivation().map_err(|error| {
                        session::custom_err!("reset EGFX state after reactivation", error)
                    })?;
                }
                ActiveStageOutput::Terminate(reason) => break 'session reason,
                ActiveStageOutput::MultitransportRequest(_) | ActiveStageOutput::AutoDetect(_) => {}
            }
        }
    };

    Ok(ClientRdpControlFlow::TerminatedGracefully(
        disconnect_reason,
    ))
}

pub(super) fn reset_graphics_base_after_reactivation(frame_state: &mut ClientRdpFrameState) {
    frame_state.graphics_sync.mark_needs_base();
    frame_state.graphics_accumulator.clear();
    frame_state.pending_base_frame = false;
    frame_state.pending_base_frame_can_publish_ready = false;
}

pub(super) fn flush_pending_rdp_base_frame(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
) -> SessionResult<()> {
    if !frame_state.pending_base_frame {
        return Ok(());
    }

    let publish_ready = frame_state.pending_base_frame_can_publish_ready;
    send_client_rdp_base_frame(output_tx, image, frame_state, publish_ready)
}

pub(super) async fn wait_for_graphics_accumulator_flush(delay: Option<Duration>) {
    match delay {
        Some(delay) => tokio::time::sleep(delay).await,
        None => future::pending::<()>().await,
    }
}

pub(super) fn flush_pending_rdp_graphics_updates(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
) -> SessionResult<()> {
    flush_rdp_graphics_updates(output_tx, image, frame_state, false)
}

#[cfg(test)]
pub(super) fn flush_queued_rdp_graphics_updates(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
) -> SessionResult<()> {
    flush_rdp_graphics_updates(output_tx, image, frame_state, true)
}

pub(super) fn flush_rdp_graphics_updates(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
    force: bool,
) -> SessionResult<()> {
    let rect = if force {
        frame_state.graphics_accumulator.take_rect()
    } else {
        frame_state.graphics_accumulator.take_ready_rect()
    };
    let Some(rect) = rect else {
        return Ok(());
    };
    if frame_state.pending_base_frame
        || frame_state.graphics_sync.needs_base()
        || rect_covers_image(rect, image)
    {
        return send_client_rdp_base_frame(output_tx, image, frame_state, true);
    }

    let trace_id = frame_state.next_graphics_trace_id();
    let event = attach_graphics_trace_id(accumulated_graphics_event(image, rect), trace_id);
    if let RemoteDesktopHelperEvent::FrameUpdate { update } = &event {
        frame_state.graphics_diagnostics.record_dirty_update(
            trace_id,
            update.size,
            update.rect,
            update.bytes.len(),
        );
    }
    send_client_rdp_graphics_event(output_tx, event, frame_state)
}

pub(super) fn send_client_rdp_base_frame(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
    publish_ready: bool,
) -> SessionResult<()> {
    let trace_id = frame_state.next_graphics_trace_id();
    frame_state.graphics_accumulator.clear();
    let event = attach_graphics_trace_id(base_frame_event(image), trace_id);
    if let RemoteDesktopHelperEvent::Frame { frame } = &event {
        frame_state
            .graphics_diagnostics
            .record_base_frame(trace_id, frame.size, frame.bytes.len());
    }
    match output_tx.try_send_graphics(ClientRdpOutput::Event(event)) {
        Ok(()) => {
            frame_state.pending_base_frame = false;
            frame_state.pending_base_frame_can_publish_ready = false;
            frame_state.graphics_sync.mark_synced();
            if publish_ready && !frame_state.published_first_desktop_frame {
                for event in native_rdp_desktop_ready_events(remote_size_for_image(image)) {
                    output_tx
                        .send_control(ClientRdpOutput::Event(event))
                        .map_err(|error| session::custom_err!("send RDP ready event", error))?;
                }
                frame_state.published_first_desktop_frame = true;
            }
            Ok(())
        }
        Err(mpsc::TrySendError::Full(_)) => {
            // Keep retrying a complete frame; dirty updates are not safe again
            // until this recovery boundary is queued successfully.
            frame_state.pending_base_frame = true;
            frame_state.pending_base_frame_can_publish_ready |= publish_ready;
            frame_state.graphics_sync.mark_needs_base();
            Ok(())
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            Err(session::general_err!("RDP output channel closed"))
        }
    }
}

pub(super) fn send_client_rdp_graphics_update(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    region: InclusiveRectangle,
    frame_state: &mut ClientRdpFrameState,
) -> SessionResult<()> {
    frame_state.graphics_diagnostics.record_graphics_update();
    let Some(rect) =
        graphics_update_rect_for_accumulator(image, region, frame_state.graphics_sync)?
    else {
        frame_state.graphics_diagnostics.record_skipped_update();
        return Ok(());
    };

    if frame_state.graphics_sync.needs_base() || rect_covers_image(rect, image) {
        // Base frames are the synchronization boundary. Queue them through the
        // dedicated path so the first real desktop frame can publish Connected
        // only after the UI has a complete framebuffer.
        return send_client_rdp_base_frame(output_tx, image, frame_state, true);
    }

    frame_state.graphics_accumulator.queue_rect(rect);
    if frame_state
        .graphics_accumulator
        .should_promote_to_base(image)
    {
        let pending_regions = frame_state.graphics_accumulator.pending_regions();
        frame_state.graphics_accumulator.clear();
        if remote_rdp_helper_graphics_diagnostics_enabled() {
            eprintln!(
                "[oxideterm:rdp-helper-graphics] pending_regions={pending_regions} promoted_to_base=true"
            );
        }
        return send_client_rdp_base_frame(output_tx, image, frame_state, true);
    }
    flush_pending_rdp_graphics_updates(output_tx, image, frame_state)
}

pub(super) fn send_client_rdp_graphics_event(
    output_tx: &ClientRdpOutputSender,
    event: RemoteDesktopHelperEvent,
    frame_state: &mut ClientRdpFrameState,
) -> SessionResult<()> {
    if matches!(event, RemoteDesktopHelperEvent::Frame { .. }) {
        match output_tx.try_send_graphics(ClientRdpOutput::Event(event)) {
            Ok(()) => {
                frame_state.pending_base_frame = false;
                frame_state.pending_base_frame_can_publish_ready = false;
                frame_state.graphics_sync.mark_synced();
                return Ok(());
            }
            Err(mpsc::TrySendError::Full(_)) => {
                frame_state.pending_base_frame = true;
                frame_state.graphics_sync.mark_needs_base();
                return Ok(());
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(session::general_err!("RDP output channel closed"));
            }
        }
    }
    if frame_state.pending_base_frame || frame_state.graphics_sync.needs_base() {
        frame_state.graphics_sync.mark_needs_base();
        return Ok(());
    }

    match output_tx.try_send_graphics(ClientRdpOutput::Event(event)) {
        Ok(()) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => {
            // Dirty rectangles are relative to the UI's backing frame. If the
            // bridge is saturated, drop the stale delta chain and recover with
            // the latest complete image once capacity returns.
            frame_state.pending_base_frame = true;
            frame_state.graphics_sync.mark_needs_base();
            Ok(())
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            Err(session::general_err!("RDP output channel closed"))
        }
    }
}

pub(super) fn attach_graphics_trace_id(
    event: RemoteDesktopHelperEvent,
    trace_id: u64,
) -> RemoteDesktopHelperEvent {
    match event {
        RemoteDesktopHelperEvent::Frame { frame } => RemoteDesktopHelperEvent::Frame {
            frame: frame.with_trace_id(trace_id),
        },
        RemoteDesktopHelperEvent::FrameUpdate { update } => RemoteDesktopHelperEvent::FrameUpdate {
            update: update.with_trace_id(trace_id),
        },
        event => event,
    }
}

pub(super) fn frame_pixels(size: RemoteDesktopSize) -> u64 {
    u64::from(size.width).saturating_mul(u64::from(size.height))
}

pub(super) fn rect_pixels(rect: oxideterm_remote_desktop::RemoteDesktopRect) -> u64 {
    u64::from(rect.width).saturating_mul(u64::from(rect.height))
}

pub(super) fn ratio_per_mille(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_mul(1000) / denominator
}

pub(super) fn remote_rdp_helper_graphics_diagnostics_enabled() -> bool {
    std::env::var_os(RDP_GRAPHICS_DIAGNOSTICS_ENV).is_some()
}

pub(super) fn send_client_rdp_event(
    output_tx: &ClientRdpOutputSender,
    event: RemoteDesktopHelperEvent,
) -> SessionResult<()> {
    if client_rdp_event_can_be_dropped_under_backpressure(&event) {
        match output_tx.try_send_graphics(ClientRdpOutput::Event(event)) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => return Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(session::general_err!("RDP output channel closed"));
            }
        }
    }

    // Base frames and control-like visual events must not be dropped because
    // the UI relies on them to establish backing state and cursor shape.
    output_tx
        .send_control(ClientRdpOutput::Event(event))
        .map_err(|error| session::custom_err!("send RDP client event", error))
}

pub(super) fn client_rdp_event_can_be_dropped_under_backpressure(
    event: &RemoteDesktopHelperEvent,
) -> bool {
    matches!(event, RemoteDesktopHelperEvent::Cursor { .. })
}
