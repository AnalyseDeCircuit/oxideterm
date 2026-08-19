// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Debug)]
pub(super) struct ClientRdpFrameState {
    pub(super) graphics_sync: RdpGraphicsSyncState,
    pub(super) graphics_accumulator: RdpGraphicsFrameAccumulator,
    pub(super) graphics_epoch: u64,
    pub(super) awaiting_reactivation: bool,
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
            graphics_epoch: 0,
            awaiting_reactivation: false,
            pending_base_frame: false,
            pending_base_frame_can_publish_ready: false,
            published_first_desktop_frame: false,
            next_graphics_trace_id: 0,
            graphics_diagnostics: RdpGraphicsDiagnostics::from_env(),
        }
    }
}

impl ClientRdpFrameState {
    fn begin_reactivation(&mut self) -> Option<u64> {
        if self.awaiting_reactivation {
            return None;
        }
        self.graphics_epoch = self.graphics_epoch.saturating_add(1).max(1);
        self.awaiting_reactivation = true;
        self.graphics_sync.mark_needs_base();
        self.graphics_accumulator.clear();
        self.pending_base_frame = false;
        self.pending_base_frame_can_publish_ready = false;
        Some(self.graphics_epoch)
    }

    fn finish_reactivation(&mut self) {
        self.awaiting_reactivation = false;
        reset_graphics_base_after_reactivation(self);
    }

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
    input_rx: &mut tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputSender,
) -> connector::ConnectorResult<(ConnectionResult, UpgradedRdpFramed, EgfxSessionBridge)> {
    let mut deferred_inputs = VecDeque::new();
    let socket = {
        let connect = TcpStream::connect((
            config.transport_destination.host(),
            config.transport_destination.port(),
        ));
        tokio::pin!(connect);
        loop {
            tokio::select! {
                result = &mut connect => {
                    break result.map_err(|error| connector::custom_err!("TCP connect", error))?;
                }
                input = input_rx.recv() => {
                    defer_or_cancel_preconnection_input(input, &mut deferred_inputs)?;
                }
            }
        }
    };
    socket
        .set_nodelay(true)
        .map_err(|error| connector::custom_err!("set TCP_NODELAY", error))?;
    let client_addr = socket
        .local_addr()
        .map_err(|error| connector::custom_err!("get socket local address", error))?;
    let mut framed = ironrdp_tokio::TokioFramed::new(socket);
    let mut connector = connector::ClientConnector::new(config.connector.clone(), client_addr);
    let egfx_bridge = attach_client_virtual_channels(
        &mut connector,
        input_tx.clone(),
        output_tx.clone(),
        config.graphics_epoch,
        config.session_options,
        config.monitor_layout.clone(),
    );
    let should_upgrade = {
        let connect_begin = ironrdp_tokio::connect_begin(&mut framed, &mut connector);
        tokio::pin!(connect_begin);
        loop {
            tokio::select! {
                result = &mut connect_begin => break result?,
                input = input_rx.recv() => {
                    defer_or_cancel_preconnection_input(input, &mut deferred_inputs)?;
                }
            }
        }
    };
    let (initial_stream, leftover_bytes) = framed.into_inner();
    let (upgraded_stream, tls_cert) = {
        let tls_upgrade = ironrdp_tls::upgrade(initial_stream, config.destination.host());
        tokio::pin!(tls_upgrade);
        loop {
            tokio::select! {
                result = &mut tls_upgrade => {
                    break result.map_err(|error| connector::custom_err!("TLS upgrade", error))?;
                }
                input = input_rx.recv() => {
                    defer_or_cancel_preconnection_input(input, &mut deferred_inputs)?;
                }
            }
        }
    };
    let certificate = rdp_server_certificate(&config.destination, &tls_cert)?;
    output_tx
        .send_control(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::ServerCertificate {
                certificate: certificate.clone(),
            },
        ))
        .map_err(|error| connector::custom_err!("publish RDP certificate challenge", error))?;
    wait_for_rdp_authentication(input_rx, &mut deferred_inputs, &mut connector, &certificate)
        .await?;
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let erased_stream: Box<dyn AsyncReadWrite + Unpin + Send + Sync> = Box::new(upgraded_stream);
    let mut upgraded_framed =
        ironrdp_tokio::TokioFramed::new_with_leftover(erased_stream, leftover_bytes);
    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&tls_cert)
        .ok_or_else(|| connector::general_err!("unable to extract TLS server public key"))?;
    let connection_result = {
        let mut network_client = ironrdp_tokio::reqwest::ReqwestNetworkClient::new();
        let connect_finalize = ironrdp_tokio::connect_finalize(
            upgraded,
            connector,
            &mut upgraded_framed,
            &mut network_client,
            connector::ServerName::new(config.destination.host().to_string()),
            server_public_key.to_owned(),
            None,
        );
        tokio::pin!(connect_finalize);
        loop {
            tokio::select! {
                result = &mut connect_finalize => break result?,
                input = input_rx.recv() => {
                    defer_or_cancel_preconnection_input(input, &mut deferred_inputs)?;
                }
            }
        }
    };
    for input in deferred_inputs {
        // Preserve resize and input events generated while the certificate
        // dialog was visible, after authentication has established the session.
        let _ = input_tx.send(input);
    }
    log_rdp_negotiated_graphics(&config.connector, &connection_result);

    Ok((connection_result, upgraded_framed, egfx_bridge))
}

fn defer_or_cancel_preconnection_input(
    input: Option<RdpInputEvent>,
    deferred_inputs: &mut VecDeque<RdpInputEvent>,
) -> connector::ConnectorResult<()> {
    match input {
        Some(RdpInputEvent::Close) | None => {
            Err(connector::general_err!("RDP connection canceled"))
        }
        Some(input) => {
            deferred_inputs.push_back(input);
            Ok(())
        }
    }
}

async fn wait_for_rdp_authentication(
    input_rx: &mut tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    deferred_inputs: &mut VecDeque<RdpInputEvent>,
    connector: &mut connector::ClientConnector,
    certificate: &oxideterm_remote_desktop::RemoteDesktopServerCertificate,
) -> connector::ConnectorResult<()> {
    loop {
        match input_rx.recv().await {
            Some(RdpInputEvent::Authenticate {
                challenge_id,
                sha256_fingerprint,
                username,
                password,
                mut domain,
            }) => {
                let mut username = username.unwrap_or_default();
                if challenge_id != certificate.challenge_id
                    || sha256_fingerprint != certificate.sha256_fingerprint
                {
                    username.zeroize();
                    if let Some(domain) = domain.as_mut() {
                        domain.zeroize();
                    }
                    return Err(connector::general_err!(
                        "RDP certificate challenge no longer matches the active TLS stream"
                    ));
                }
                let Some(password) = password else {
                    username.zeroize();
                    if let Some(domain) = domain.as_mut() {
                        domain.zeroize();
                    }
                    return Err(connector::general_err!("RDP credentials are incomplete"));
                };
                if username.trim().is_empty() || password.is_empty() {
                    username.zeroize();
                    if let Some(domain) = domain.as_mut() {
                        domain.zeroize();
                    }
                    return Err(connector::general_err!("RDP credentials are incomplete"));
                }

                // IronRDP currently owns plain String credentials. This copy is
                // created only after certificate acceptance and remains scoped
                // to the native connector for the authenticated session.
                connector.config.credentials = Credentials::UsernamePassword {
                    username,
                    password: password.expose_secret().to_string(),
                };
                connector.config.domain = domain;
                connector.config.autologon = true;
                return Ok(());
            }
            Some(RdpInputEvent::Close) | None => {
                return Err(connector::general_err!("RDP connection canceled"));
            }
            Some(input) => deferred_inputs.push_back(input),
        }
    }
}

fn rdp_server_certificate(
    destination: &ClientRdpDestination,
    certificate: &x509_cert::Certificate,
) -> connector::ConnectorResult<oxideterm_remote_desktop::RemoteDesktopServerCertificate> {
    let der = certificate
        .to_der()
        .map_err(|error| connector::custom_err!("read RDP TLS certificate", error))?;
    let digest = Sha256::digest(&der);
    let fingerprint = digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":");

    Ok(oxideterm_remote_desktop::RemoteDesktopServerCertificate {
        challenge_id: uuid::Uuid::new_v4().to_string(),
        protocol: RemoteDesktopProtocol::Rdp,
        endpoint: RemoteDesktopEndpoint::new(destination.host(), destination.port()),
        identity_kind: oxideterm_remote_desktop::RemoteDesktopServerIdentityKind::X509Certificate,
        security_method: "tls-credssp".to_string(),
        sha256_fingerprint: fingerprint,
        // native-tls exposes the authenticated certificate bytes but no
        // portable subject/validity parser. The fingerprint remains the
        // authoritative identity shown and pinned by OxideTerm.
        subject: None,
        issuer: None,
        valid_from: None,
        valid_to: None,
    })
}

pub(super) fn attach_client_virtual_channels(
    connector: &mut connector::ClientConnector,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputSender,
    graphics_epoch: u64,
    session_options: RemoteDesktopSessionOptions,
    monitor_layout: RemoteDesktopMonitorLayout,
) -> EgfxSessionBridge {
    let initial_layout = if session_options.display.use_all_monitors {
        build_display_control_layout(&monitor_layout).ok()
    } else {
        None
    };
    let mut dynamic_channels =
        DrdynvcClient::new().with_dynamic_channel(DisplayControlClient::new(move |capabilities| {
            let Some(layout) = initial_layout.as_ref() else {
                return Ok(Vec::new());
            };
            let requested_area = layout
                .monitors()
                .iter()
                .map(|monitor| {
                    let (width, height) = monitor.dimensions();
                    u64::from(width) * u64::from(height)
                })
                .sum::<u64>();
            if requested_area > capabilities.max_monitor_area() {
                return Ok(Vec::new());
            }
            Ok(vec![Box::new(DisplayControlPdu::from(layout.clone()))])
        }));
    let egfx_bridge = if session_options.rdp.disable_graphics_pipeline {
        // DisplayControl remains attached while EGFX is omitted for bitmap fallback.
        EgfxSessionBridge::disabled()
    } else {
        let (graphics_pipeline, bridge) = new_egfx_channel(output_tx.clone(), graphics_epoch);
        dynamic_channels = dynamic_channels.with_dynamic_channel(graphics_pipeline);
        bridge
    };
    if session_options.audio.capture {
        dynamic_channels =
            dynamic_channels.with_dynamic_channel(AudioInputClient::new(input_tx.clone()));
    }
    connector.attach_static_channel(dynamic_channels);

    // CLIPRDR is attached as a normal static channel while the backend itself
    // bridges callbacks into OxideTerm's helper protocol.
    if session_options.clipboard.text
        || session_options.clipboard.images
        || session_options.clipboard.files
    {
        let clipboard = ClientClipboardBackend::new(input_tx, output_tx, session_options.clipboard);
        connector.attach_static_channel(CliprdrClient::new(Box::new(clipboard)));
    }

    // RDPSND is session-owned so disabling playback omits both negotiation and
    // the local device thread.
    if session_options.audio.playback {
        let audio_backend = PcmRdpsndBackend::new();
        connector.attach_static_channel(ironrdp::rdpsnd::client::Rdpsnd::new(Box::new(
            audio_backend,
        )));
    }

    // The bridge is session-owned and is dropped after the active RDP loop exits.
    egfx_bridge
}

fn build_display_control_layout(
    layout: &RemoteDesktopMonitorLayout,
) -> ironrdp_core::EncodeResult<DisplayControlMonitorLayout> {
    let monitors = layout
        .monitors
        .iter()
        .map(|monitor| {
            let mut entry = if monitor.primary {
                MonitorLayoutEntry::new_primary(monitor.width, monitor.height)?
            } else {
                MonitorLayoutEntry::new_secondary(monitor.width, monitor.height)?
                    .with_position(monitor.left, monitor.top)?
            };
            entry = entry
                .with_orientation(match monitor.orientation {
                    oxideterm_remote_desktop::RemoteDesktopMonitorOrientation::Landscape => {
                        MonitorOrientation::Landscape
                    }
                    oxideterm_remote_desktop::RemoteDesktopMonitorOrientation::Portrait => {
                        MonitorOrientation::Portrait
                    }
                    oxideterm_remote_desktop::RemoteDesktopMonitorOrientation::LandscapeFlipped => {
                        MonitorOrientation::LandscapeFlipped
                    }
                    oxideterm_remote_desktop::RemoteDesktopMonitorOrientation::PortraitFlipped => {
                        MonitorOrientation::PortraitFlipped
                    }
                })
                .with_desktop_scale_factor(monitor.desktop_scale_factor)?
                .with_device_scale_factor(match monitor.device_scale_factor {
                    100 => DeviceScaleFactor::Scale100Percent,
                    140 => DeviceScaleFactor::Scale140Percent,
                    180 => DeviceScaleFactor::Scale180Percent,
                    _ => DeviceScaleFactor::Scale100Percent,
                });
            if let (Some(width), Some(height)) =
                (monitor.physical_width_mm, monitor.physical_height_mm)
            {
                entry = entry.with_physical_dimensions(width, height)?;
            }
            Ok(entry)
        })
        .collect::<ironrdp_core::EncodeResult<Vec<_>>>()?;
    DisplayControlMonitorLayout::new(&monitors)
}

#[cfg(test)]
mod monitor_layout_tests {
    use super::*;
    use oxideterm_remote_desktop::{RemoteDesktopMonitor, RemoteDesktopMonitorOrientation};

    #[test]
    fn display_control_layout_preserves_primary_relative_topology() {
        let layout = RemoteDesktopMonitorLayout {
            monitors: vec![
                RemoteDesktopMonitor {
                    stable_id: "primary".to_string(),
                    left: 0,
                    top: 0,
                    width: 1920,
                    height: 1080,
                    primary: true,
                    desktop_scale_factor: 100,
                    device_scale_factor: 100,
                    physical_width_mm: None,
                    physical_height_mm: None,
                    orientation: RemoteDesktopMonitorOrientation::Landscape,
                },
                RemoteDesktopMonitor {
                    stable_id: "left".to_string(),
                    left: -1280,
                    top: 0,
                    width: 1280,
                    height: 1024,
                    primary: false,
                    desktop_scale_factor: 100,
                    device_scale_factor: 100,
                    physical_width_mm: None,
                    physical_height_mm: None,
                    orientation: RemoteDesktopMonitorOrientation::Landscape,
                },
            ],
        };

        let encoded = build_display_control_layout(&layout).unwrap();

        assert_eq!(encoded.monitors().len(), 2);
        assert!(encoded.monitors()[0].is_primary());
        assert_eq!(encoded.monitors()[0].position(), Some((0, 0)));
        assert_eq!(encoded.monitors()[1].position(), Some((-1280, 0)));
        assert_eq!(encoded.monitors()[1].dimensions(), (1280, 1024));
    }
}

fn encode_display_control_layout(
    active_stage: &mut ActiveStage,
    layout: &RemoteDesktopMonitorLayout,
) -> Option<SessionResult<Vec<u8>>> {
    let display_control_channel = active_stage.get_dvc::<DisplayControlClient>()?;
    let channel_id = display_control_channel.channel_id();
    let display_control = display_control_channel.processor();
    if !display_control.ready() {
        return None;
    }
    let layout = match build_display_control_layout(layout) {
        Ok(layout) => DisplayControlPdu::from(layout),
        Err(error) => return Some(Err(session::SessionError::encode(error))),
    };
    let messages =
        match encode_dvc_messages(channel_id, vec![Box::new(layout)], ChannelFlags::empty()) {
            Ok(messages) => messages,
            Err(error) => return Some(Err(session::SessionError::encode(error))),
        };
    Some(
        active_stage
            .process_svc_processor_messages(SvcProcessorMessages::<DrdynvcClient>::new(messages)),
    )
}

fn display_control_resize_available(active_stage: &mut ActiveStage) -> bool {
    active_stage
        .get_dvc::<DisplayControlClient>()
        .is_some_and(|channel| channel.processor().ready())
}

fn publish_rdp_resize_capability(
    active_stage: &mut ActiveStage,
    output_tx: &ClientRdpOutputSender,
    last_reported: &mut Option<NegotiatedCapabilityStatus>,
) -> SessionResult<()> {
    let resize = if display_control_resize_available(active_stage) {
        NegotiatedCapabilityStatus::Supported
    } else {
        NegotiatedCapabilityStatus::Unsupported
    };
    if *last_reported == Some(resize) {
        return Ok(());
    }

    // Report only negotiated DisplayControl evidence. The provider manifest
    // describes helper potential, not what this server actually exposed.
    send_client_rdp_event(
        output_tx,
        RemoteDesktopHelperEvent::CapabilitiesNegotiated {
            capabilities: NegotiatedCapabilities {
                resize,
                ..NegotiatedCapabilities::default()
            },
        },
    )?;
    *last_reported = Some(resize);
    Ok(())
}

fn begin_rdp_frame_transition(
    output_tx: &ClientRdpOutputSender,
    frame_state: &mut ClientRdpFrameState,
    egfx_bridge: &EgfxSessionBridge,
) -> SessionResult<u64> {
    let Some(graphics_epoch) = frame_state.begin_reactivation() else {
        return Ok(frame_state.graphics_epoch);
    };
    egfx_bridge
        .begin_frame_transition(graphics_epoch)
        .map_err(|error| session::custom_err!("begin EGFX frame transition", error))?;
    send_client_rdp_event(
        output_tx,
        RemoteDesktopHelperEvent::FrameStreamReset { graphics_epoch },
    )?;
    Ok(graphics_epoch)
}

fn encode_pending_microphone_packets(
    active_stage: &mut ActiveStage,
) -> Option<SessionResult<Vec<u8>>> {
    let audio_input_channel = active_stage.get_dvc::<AudioInputClient>()?;
    let channel_id = audio_input_channel.channel_id();
    let audio_input = audio_input_channel.processor();
    let messages = audio_input.drain_messages();
    if messages.is_empty() {
        return None;
    }
    let messages = match encode_dvc_messages(channel_id, messages, ChannelFlags::empty()) {
        Ok(messages) => messages,
        Err(error) => return Some(Err(session::SessionError::encode(error))),
    };
    Some(
        active_stage
            .process_svc_processor_messages(SvcProcessorMessages::<DrdynvcClient>::new(messages)),
    )
}

pub(super) async fn run_native_rdp_active_session(
    framed: UpgradedRdpFramed,
    connection_result: ConnectionResult,
    graphics_epoch: u64,
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
    // Keep the factory for every server-driven Deactivation-Reactivation Sequence.
    let activation_factory = connection_result.activation_factory;
    let mut active_stage = ActiveStageBuilder {
        static_channels: connection_result.static_channels,
        user_channel_id: connection_result.user_channel_id,
        io_channel_id: connection_result.io_channel_id,
        message_channel_id: connection_result.message_channel_id,
        share_id: connection_result.share_id,
        compression_type: connection_result.compression_type,
        enable_server_pointer: connection_result.enable_server_pointer,
        pointer_software_rendering: connection_result.pointer_software_rendering,
    }
    .build();
    let mut clipboard_cleanup = tokio::time::interval(RDP_CLIPBOARD_TIMEOUT_POLL_INTERVAL);
    let mut frame_state = ClientRdpFrameState {
        graphics_epoch,
        ..ClientRdpFrameState::default()
    };
    let mut last_reported_resize_capability = None;

    let disconnect_reason = 'session: loop {
        flush_pending_rdp_base_frame(output_tx, &image, &mut frame_state)?;
        flush_pending_rdp_graphics_updates(output_tx, &image, &mut frame_state)?;
        if frame_state.published_first_desktop_frame {
            publish_rdp_resize_capability(
                &mut active_stage,
                output_tx,
                &mut last_reported_resize_capability,
            )?;
        }
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
                            let response_frame = response_frame?;
                            begin_rdp_frame_transition(output_tx, &mut frame_state, egfx_bridge)?;
                            vec![ActiveStageOutput::ResponseFrame(response_frame)]
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
                    RdpInputEvent::SetClipboardFiles { transfer_id, paths } => {
                        advertise_local_clipboard_files(
                            &mut active_stage,
                            transfer_id,
                            paths,
                        )?
                    }
                    RdpInputEvent::CancelClipboardTransfer(transfer_id) => {
                        cancel_clipboard_transfer(&mut active_stage, &transfer_id);
                        Vec::new()
                    }
                    RdpInputEvent::UpdateDisplayLayout(layout) => {
                        if let Some(response_frame) =
                            encode_display_control_layout(&mut active_stage, &layout)
                        {
                            let response_frame = response_frame?;
                            begin_rdp_frame_transition(output_tx, &mut frame_state, egfx_bridge)?;
                            vec![ActiveStageOutput::ResponseFrame(response_frame)]
                        } else {
                            Vec::new()
                        }
                    }
                    RdpInputEvent::MicrophoneReady => {
                        if let Some(response_frame) =
                            encode_pending_microphone_packets(&mut active_stage)
                        {
                            vec![ActiveStageOutput::ResponseFrame(response_frame?)]
                        } else {
                            Vec::new()
                        }
                    }
                    RdpInputEvent::Authenticate { .. } => {
                        // Authentication is consumed before ActiveStage is
                        // created. Ignore a delayed duplicate bound to the
                        // already-established session.
                        Vec::new()
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
                ActiveStageOutput::DeactivateAll => {
                    let graphics_epoch =
                        begin_rdp_frame_transition(output_tx, &mut frame_state, egfx_bridge)?;
                    handle_deactivate_all(
                        &mut reader,
                        &mut writer,
                        &mut active_stage,
                        &mut image,
                        activation_factory.create(),
                    )
                    .await?;
                    frame_state.finish_reactivation();
                    egfx_bridge
                        .prepare_for_reactivation(graphics_epoch)
                        .map_err(|error| {
                            session::custom_err!("reset EGFX state after reactivation", error)
                        })?;
                }
                ActiveStageOutput::Terminate(reason) => break 'session reason,
                ActiveStageOutput::MultitransportRequest(_)
                | ActiveStageOutput::AutoDetect(_)
                | ActiveStageOutput::SaveSessionInfo { .. } => {}
                ActiveStageOutput::AutoReconnectCookie(mut cookie) => {
                    // OxideTerm currently performs credential-backed retries. Erase the unused
                    // reconnect credential immediately instead of retaining it without an owner.
                    cookie.random_bits.zeroize();
                }
            }
        }
        if frame_state.published_first_desktop_frame {
            publish_rdp_resize_capability(
                &mut active_stage,
                output_tx,
                &mut last_reported_resize_capability,
            )?;
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
    if frame_state.awaiting_reactivation || !frame_state.pending_base_frame {
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
    if frame_state.awaiting_reactivation {
        return Ok(());
    }
    let rects = if force {
        frame_state.graphics_accumulator.take_rects()
    } else {
        frame_state.graphics_accumulator.take_ready_rects()
    };
    let Some(rects) = rects else {
        return Ok(());
    };
    if frame_state.pending_base_frame
        || frame_state.graphics_sync.needs_base()
        || rects
            .iter()
            .copied()
            .any(|rect| rect_covers_image(rect, image))
    {
        return send_client_rdp_base_frame(output_tx, image, frame_state, true);
    }

    let trace_id = frame_state.next_graphics_trace_id();
    let event = attach_graphics_metadata(
        accumulated_graphics_event(image, rects),
        frame_state.graphics_epoch,
        trace_id,
    );
    if let RemoteDesktopHelperEvent::FrameUpdate { update } = &event {
        frame_state.graphics_diagnostics.record_dirty_update(
            trace_id,
            update.size,
            update.rect,
            update.bytes.len(),
        );
    } else if let RemoteDesktopHelperEvent::FrameUpdateBatch { batch } = &event {
        for update in &batch.updates {
            frame_state.graphics_diagnostics.record_dirty_update(
                trace_id,
                update.size,
                update.rect,
                update.bytes.len(),
            );
        }
    }
    send_client_rdp_graphics_event(output_tx, event, frame_state)
}

pub(super) fn send_client_rdp_base_frame(
    output_tx: &ClientRdpOutputSender,
    image: &DecodedImage,
    frame_state: &mut ClientRdpFrameState,
    publish_ready: bool,
) -> SessionResult<()> {
    if frame_state.awaiting_reactivation {
        frame_state.graphics_sync.mark_needs_base();
        return Ok(());
    }
    let trace_id = frame_state.next_graphics_trace_id();
    frame_state.graphics_accumulator.clear();
    let event = attach_graphics_metadata(
        base_frame_event(image),
        frame_state.graphics_epoch,
        trace_id,
    );
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
    if frame_state.awaiting_reactivation {
        frame_state.graphics_diagnostics.record_skipped_update();
        return Ok(());
    }
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
    if frame_state.awaiting_reactivation {
        frame_state.graphics_sync.mark_needs_base();
        return Ok(());
    }
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

pub(super) fn attach_graphics_metadata(
    event: RemoteDesktopHelperEvent,
    graphics_epoch: u64,
    trace_id: u64,
) -> RemoteDesktopHelperEvent {
    match event {
        RemoteDesktopHelperEvent::Frame { frame } => RemoteDesktopHelperEvent::Frame {
            frame: frame
                .with_graphics_epoch(graphics_epoch)
                .with_trace_id(trace_id),
        },
        RemoteDesktopHelperEvent::FrameUpdate { update } => RemoteDesktopHelperEvent::FrameUpdate {
            update: update
                .with_graphics_epoch(graphics_epoch)
                .with_trace_id(trace_id),
        },
        RemoteDesktopHelperEvent::FrameUpdateBatch { batch } => {
            RemoteDesktopHelperEvent::FrameUpdateBatch {
                batch: oxideterm_remote_desktop::RemoteDesktopFrameUpdateBatch::new(
                    batch
                        .updates
                        .into_iter()
                        .map(|update| {
                            update
                                .with_graphics_epoch(graphics_epoch)
                                .with_trace_id(trace_id)
                        })
                        .collect(),
                ),
            }
        }
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
