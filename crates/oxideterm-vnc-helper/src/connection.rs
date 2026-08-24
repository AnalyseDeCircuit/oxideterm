// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};

use super::*;

pub(super) const VNC_IO_COMMAND_CAPACITY: usize = 128;
const VNC_ACTIVE_SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(20);
const VNC_IO_MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);
const VNC_IO_JOIN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_VNC_DESKTOP_NAME_BYTES: usize = 1024 * 1024;

impl VncSessionSharedState {
    pub(super) fn new(width: u16, height: u16) -> Self {
        Self {
            width: AtomicU16::new(width),
            height: AtomicU16::new(height),
            force_next_base_frame: AtomicBool::new(false),
            qemu_extended_key_events: AtomicU8::new(VNC_CAPABILITY_UNKNOWN),
            extended_mouse_buttons: AtomicU8::new(VNC_CAPABILITY_UNKNOWN),
            remote_lock_keys: AtomicU8::new(0),
            pending_lock_keys: AtomicU8::new(0),
        }
    }

    pub(super) fn size(&self) -> (u16, u16) {
        (
            self.width.load(Ordering::Acquire),
            self.height.load(Ordering::Acquire),
        )
    }

    pub(super) fn store_size(&self, width: u16, height: u16) {
        self.width.store(width, Ordering::Release);
        self.height.store(height, Ordering::Release);
    }

    pub(super) fn request_base_frame(&self) {
        // RequestFrame is a UI recovery path, so the next framebuffer payload
        // must rebuild the front-end backing buffer instead of remaining dirty.
        self.force_next_base_frame.store(true, Ordering::Release);
    }

    pub(super) fn cancel_base_frame_request(&self) {
        self.force_next_base_frame.store(false, Ordering::Release);
    }

    pub(super) fn take_base_frame_request(&self) -> bool {
        self.force_next_base_frame.swap(false, Ordering::AcqRel)
    }
}

impl VncConnection {
    pub(super) fn complete(
        config: &VncSessionConfig,
        preflight: &mut VncSecurityPreflight,
        username: Option<&str>,
        password: Option<&RemoteDesktopSecret>,
        event_writer: SharedEventWriter,
        diagnostics: VncDiagnostics,
        canceled: Arc<AtomicBool>,
        generation: u64,
        active_generation: Arc<std::sync::atomic::AtomicU64>,
    ) -> VncResult<Self> {
        let desktop_layout = initial_vnc_desktop_layout(
            config.initial_size,
            config.session_options.display.use_all_monitors,
            &config.monitor_layout,
        )
        .map_err(VncError::configuration)?;
        let mut transport = preflight.finish_authentication(
            username,
            password,
            config.session_options.vnc.session_mode,
        )?;
        transport.set_phase_timeout(Some(VNC_IO_MESSAGE_TIMEOUT));
        let (width, height, tight_interaction) =
            read_server_init(&mut transport, preflight.tight_active)?;
        diagnostics.log(format!("server_init framebuffer={width}x{height}"));
        write_pixel_format(&mut transport)?;
        let encoding_preferences = VncEncodingPreferences::from_options(config.session_options.vnc);
        let h264 = match VncH264State::from_env() {
            Ok(h264) => h264,
            Err(error) => {
                diagnostics.log(format!("Open H.264 unavailable: {error}"));
                None
            }
        };
        write_encodings(&mut transport, encoding_preferences, h264.is_some())?;
        diagnostics.log("encodings advertised from negotiated VNC preferences");
        transport.set_phase_timeout(None);
        // The active owner must revisit queued keyboard and pointer commands
        // promptly even while the server has no framebuffer messages pending.
        transport
            .set_read_poll_interval(VNC_ACTIVE_SOCKET_POLL_INTERVAL)
            .map_err(|error| {
                VncError::network(format!(
                    "VNC active-session read timeout setup failed: {error}"
                ))
            })?;

        let (writer, io_rx) = std::sync::mpsc::sync_channel(VNC_IO_COMMAND_CAPACITY);
        let session_state = Arc::new(VncSessionSharedState::new(width, height));
        let desktop_resize = Arc::new(Mutex::new(VncDesktopResizeState::new(desktop_layout)));
        let file_capabilities = tight_interaction
            .as_ref()
            .map(TightFileCapabilities::from_interaction)
            .unwrap_or_default();
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities {
            security_methods: preflight.offered_security_methods.clone(),
            selected_security_method: Some(preflight.security.as_str().to_string()),
            encrypted: known_capability(preflight.encrypted()),
            peer_identity_verified: known_capability(preflight.peer_identity_verified()),
            tight: if preflight.tight_active {
                NegotiatedCapabilityStatus::Supported
            } else {
                NegotiatedCapabilityStatus::Unknown
            },
            vendor_files: if preflight.tight_active {
                known_capability(
                    file_capabilities.list
                        || file_capabilities.download
                        || file_capabilities.upload,
                )
            } else {
                NegotiatedCapabilityStatus::Unknown
            },
            vendor_file_list: if preflight.tight_active {
                known_capability(file_capabilities.list)
            } else {
                NegotiatedCapabilityStatus::Unknown
            },
            vendor_file_download: if preflight.tight_active {
                known_capability(file_capabilities.download)
            } else {
                NegotiatedCapabilityStatus::Unknown
            },
            vendor_file_upload: if preflight.tight_active {
                known_capability(file_capabilities.upload)
            } else {
                NegotiatedCapabilityStatus::Unknown
            },
            ..NegotiatedCapabilities::default()
        }));
        Ok(Self {
            transport: Some(transport),
            writer,
            io_rx: Some(io_rx),
            event_writer,
            diagnostics,
            canceled,
            generation,
            active_generation,
            session_state,
            audio: Arc::new(Mutex::new(QemuAudioSession::new(
                config.session_options.audio.playback,
            ))),
            clipboard: Arc::new(Mutex::new(VncClipboardSession::default())),
            vendor_files: Arc::new(Mutex::new(VncVendorFileSession::new(file_capabilities))),
            capabilities,
            desktop_resize,
            h264,
            reader_handle: None,
            reader_done: None,
            width,
            height,
        })
    }

    pub(super) fn start_reader(&mut self) -> Result<(), String> {
        let transport = self
            .transport
            .take()
            .ok_or_else(|| "VNC transport is unavailable.".to_string())?;
        let io_rx = self
            .io_rx
            .take()
            .ok_or_else(|| "VNC I/O command receiver is unavailable.".to_string())?;
        let writer = self.writer.clone();
        let event_writer = self.event_writer.clone();
        let diagnostics = self.diagnostics;
        let canceled = self.canceled.clone();
        let generation = self.generation;
        let active_generation = self.active_generation.clone();
        let session_state = self.session_state.clone();
        let audio = self.audio.clone();
        let clipboard = self.clipboard.clone();
        let vendor_files = self.vendor_files.clone();
        let capabilities = self.capabilities.clone();
        let desktop_resize = self.desktop_resize.clone();
        let h264 = self.h264.take();
        let width = self.width;
        let height = self.height;
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(format!("oxideterm-vnc-io-{generation}"))
            .spawn(move || {
                run_vnc_io_owner(
                    transport,
                    io_rx,
                    writer,
                    event_writer,
                    diagnostics,
                    canceled,
                    generation,
                    active_generation,
                    session_state,
                    audio,
                    clipboard,
                    vendor_files,
                    capabilities,
                    desktop_resize,
                    h264,
                    width,
                    height,
                );
                let _ = done_tx.send(());
            })
            .map_err(|error| format!("VNC I/O owner start failed: {error}"))?;
        self.reader_handle = Some(handle);
        self.reader_done = Some(done_rx);
        Ok(())
    }

    pub(super) fn capabilities_event(&self) -> Result<RemoteDesktopHelperEvent, String> {
        vnc_capabilities_event(&self.capabilities)
    }

    pub(super) fn request_framebuffer_update(&self, incremental: bool) -> Result<(), String> {
        let (width, height) = self.session_state.size();
        request_framebuffer_update(&self.writer, incremental, width, height)
    }

    pub(super) fn request_full_frame_recovery(&self) -> Result<(), String> {
        self.session_state.request_base_frame();
        if let Err(error) = self.request_framebuffer_update(false) {
            self.session_state.cancel_base_frame_request();
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn request_desktop_layout(&self, layout: VncDesktopLayout) -> Result<(), String> {
        request_vnc_desktop_layout(
            &self.desktop_resize,
            &self.writer,
            &self.event_writer,
            layout,
        )
    }

    pub(super) fn send_pointer(&self, x: u16, y: u16, buttons: u16) -> Result<(), String> {
        let message = vnc_pointer_event_message(
            x,
            y,
            buttons,
            self.session_state.extended_mouse_button_support()
                == NegotiatedCapabilityStatus::Supported,
        );
        write_vnc_message(&self.writer, &message)
    }

    pub(super) fn send_key(&self, keysym: u32, down: bool) -> Result<(), String> {
        write_vnc_message(&self.writer, &vnc_standard_key_event_message(keysym, down))
    }

    pub(super) fn send_key_event(&self, event: VncKeyEvent) -> Result<(), String> {
        if self.session_state.qemu_extended_key_event_support()
            == NegotiatedCapabilityStatus::Supported
            && let Some(message) = qemu_extended_key_event_message(event)
        {
            return write_vnc_message(&self.writer, &message);
        }
        self.send_key(event.keysym, event.down)
    }

    pub(super) fn synchronize_lock_keys(
        &self,
        target: RemoteDesktopLockKeys,
    ) -> Result<(), String> {
        // Preserve the desired state until the server supplies its first LED
        // snapshot; toggling from an unknown state can invert a correct lock.
        self.session_state.store_pending_lock_keys(target);
        flush_pending_vnc_lock_key_sync(&self.session_state, &self.writer)
    }

    pub(super) fn send_client_cut_text(&self, text: &str) -> Result<(), String> {
        let messages = self
            .clipboard
            .lock()
            .map_err(|_| "VNC clipboard state lock is poisoned.".to_string())?
            .set_local_text(text.to_string())?;
        for message in messages {
            write_vnc_message(&self.writer, &message)?;
        }
        Ok(())
    }

    pub(super) fn send_client_clipboard_data(
        &self,
        data: &RemoteDesktopClipboardData,
    ) -> Result<(), String> {
        let messages = self
            .clipboard
            .lock()
            .map_err(|_| "VNC clipboard state lock is poisoned.".to_string())?
            .set_local_data(data)?;
        for message in messages {
            write_vnc_message(&self.writer, &message)?;
        }
        Ok(())
    }

    pub(super) fn send_clipboard_files(
        &self,
        transfer_id: &str,
        paths: &[std::path::PathBuf],
    ) -> Result<(), String> {
        let payload = self
            .vendor_files
            .lock()
            .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())?
            .upload_payload(transfer_id, paths)?;
        // Move the complete bounded transfer through one queue slot so a
        // saturated owner cannot accept a partial sequence.
        write_vnc_owned_message(&self.writer, payload)
    }

    pub(super) fn cancel_clipboard_transfer(&self, transfer_id: String) -> Result<(), String> {
        let message = self
            .vendor_files
            .lock()
            .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())?
            .cancel_upload(transfer_id);
        if let Some(message) = message {
            write_vnc_message(&self.writer, &message)?;
        }
        Ok(())
    }

    pub(super) fn request_remote_files(
        &self,
        request_id: String,
        path: String,
    ) -> Result<(), String> {
        let message = self
            .vendor_files
            .lock()
            .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())?
            .request_list(request_id, path)?;
        write_vnc_owned_message(&self.writer, message)
    }

    pub(super) fn download_remote_files(
        &self,
        transfer_id: String,
        remote_paths: Vec<String>,
        destination: std::path::PathBuf,
        conflict_policy: RemoteDesktopFileConflictPolicy,
    ) -> Result<(), String> {
        let actions = self
            .vendor_files
            .lock()
            .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())?
            .start_download(transfer_id, remote_paths, destination, conflict_policy)?;
        self.dispatch_vendor_file_actions(actions)
    }

    pub(super) fn cancel_file_transfer(&self, transfer_id: String) -> Result<(), String> {
        let actions = self
            .vendor_files
            .lock()
            .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())?
            .cancel_download(transfer_id);
        self.dispatch_vendor_file_actions(actions)
    }

    fn dispatch_vendor_file_actions(&self, actions: VncVendorFileActions) -> Result<(), String> {
        for message in actions.messages {
            write_vnc_owned_message(&self.writer, message)?;
        }
        for event in actions.events {
            send_event(&self.event_writer, event)?;
        }
        Ok(())
    }

    pub(super) fn shutdown_and_join(&mut self) -> Result<(), String> {
        if let Ok(mut audio) = self.audio.lock() {
            audio.shutdown(&self.writer);
        }
        if self.writer.try_send(VncIoCommand::Shutdown).is_err() {
            // Cancellation remains the fallback if the bounded owner queue is saturated.
            self.canceled.store(true, Ordering::Release);
        }

        let mut forced_shutdown = false;
        if let Some(done) = self.reader_done.take() {
            match done.recv_timeout(VNC_IO_JOIN_TIMEOUT) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
                Err(RecvTimeoutError::Timeout) => {
                    // A partial server message can delay the graceful command.
                    // Force the cancellable transport, whose read/write calls
                    // have their own finite socket deadlines, before joining.
                    forced_shutdown = true;
                    self.canceled.store(true, Ordering::Release);
                }
            }
        }
        self.canceled.store(true, Ordering::Release);
        if let Some(handle) = self.reader_handle.take() {
            handle
                .join()
                .map_err(|_| "VNC I/O owner panicked during shutdown.".to_string())?;
        }
        if forced_shutdown {
            self.diagnostics
                .log("VNC I/O owner required forced bounded shutdown");
        }
        Ok(())
    }
}

fn known_capability(supported: bool) -> NegotiatedCapabilityStatus {
    if supported {
        NegotiatedCapabilityStatus::Supported
    } else {
        NegotiatedCapabilityStatus::Unsupported
    }
}

fn run_vnc_io_owner(
    mut transport: Box<dyn VncTransport>,
    io_rx: Receiver<VncIoCommand>,
    writer: SyncSender<VncIoCommand>,
    event_writer: SharedEventWriter,
    diagnostics: VncDiagnostics,
    canceled: Arc<AtomicBool>,
    generation: u64,
    active_generation: Arc<std::sync::atomic::AtomicU64>,
    session_state: Arc<VncSessionSharedState>,
    audio: Arc<Mutex<QemuAudioSession>>,
    clipboard: Arc<Mutex<VncClipboardSession>>,
    vendor_files: Arc<Mutex<VncVendorFileSession>>,
    capabilities: SharedVncCapabilities,
    desktop_resize: SharedVncDesktopResize,
    h264: Option<VncH264State>,
    width: u16,
    height: u16,
) {
    let mut framebuffer = match VncFramebuffer::try_new(width, height) {
        Ok(framebuffer) => framebuffer,
        Err(error) => {
            publish_vnc_disconnect(
                &event_writer,
                diagnostics,
                &canceled,
                generation,
                &active_generation,
                error,
            );
            transport.shutdown_transport();
            return;
        }
    };
    // Decoder contexts are owned by the one task that reads RFB messages and
    // are dropped before the transport owner exits.
    let mut decode_state = VncDecodeState::new(h264);
    let mut performance_state = VncPerformanceState::default();
    let mut counters = VncReaderDiagnosticsCounters::default();
    let mut sent_initial_frame = false;

    loop {
        // Drain final protocol writes before observing cancellation so audio
        // disable and the following shutdown command preserve channel order.
        match drain_vnc_io_commands(&mut transport, &io_rx) {
            Ok(true) => {
                if let Some(message) = performance_state.disable_continuous_updates_message(
                    framebuffer.width as u16,
                    framebuffer.height as u16,
                ) {
                    let _ = transport.write_all(&message);
                }
                transport.shutdown_transport();
                break;
            }
            Ok(false) => {}
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        }
        if canceled.load(Ordering::Acquire)
            || active_generation.load(Ordering::Acquire) != generation
        {
            if let Some(message) = performance_state.disable_continuous_updates_message(
                framebuffer.width as u16,
                framebuffer.height as u16,
            ) {
                let _ = transport.write_all(&message);
            }
            transport.shutdown_transport();
            break;
        }

        transport.set_phase_timeout(None);
        let message_type = match read_u8(&mut transport) {
            Ok(message_type) => message_type,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                continue;
            }
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    format!("VNC server message read failed: {error}"),
                );
                break;
            }
        };
        transport.set_phase_timeout(Some(VNC_IO_MESSAGE_TIMEOUT));
        let is_vendor_file_message = match vendor_files.lock() {
            Ok(files) => files.accepts_server_message(message_type),
            Err(_) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    "VNC file transfer state lock is poisoned.".to_string(),
                );
                break;
            }
        };
        if is_vendor_file_message {
            let file_actions = match vendor_files
                .lock()
                .map_err(|_| "VNC file transfer state lock is poisoned.".to_string())
                .and_then(|mut files| files.observe_server_message(message_type, &mut transport))
            {
                Ok(events) => events,
                Err(error) => {
                    publish_vnc_disconnect(
                        &event_writer,
                        diagnostics,
                        &canceled,
                        generation,
                        &active_generation,
                        error,
                    );
                    break;
                }
            };
            transport.set_phase_timeout(None);
            let mut write_failed = None;
            for message in file_actions.messages {
                if let Err(error) = transport.write_all(&message) {
                    write_failed = Some(format!("VNC file transfer control write failed: {error}"));
                    break;
                }
            }
            if let Some(error) = write_failed {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
            if active_generation.load(Ordering::Acquire) == generation {
                for event in file_actions.events {
                    let _ = send_event(&event_writer, event);
                }
            }
            continue;
        }
        let event = match read_vnc_event_after_type(&mut transport, &mut decode_state, message_type)
        {
            Ok(event) => event,
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        };
        transport.set_phase_timeout(None);

        if let VncServerEvent::ClipboardExtended(message) = &event {
            let outcome = match clipboard
                .lock()
                .map_err(|_| "VNC clipboard state lock is poisoned.".to_string())
                .and_then(|mut clipboard| {
                    clipboard.observe_server_message(VncClipboardMessage::Extended(message.clone()))
                }) {
                Ok(outcome) => outcome,
                Err(error) => {
                    publish_vnc_disconnect(
                        &event_writer,
                        diagnostics,
                        &canceled,
                        generation,
                        &active_generation,
                        error,
                    );
                    break;
                }
            };
            let mut clipboard_write_failed = None;
            for message in outcome.messages {
                if let Err(error) = transport.write_all(&message) {
                    clipboard_write_failed =
                        Some(format!("VNC clipboard control write failed: {error}"));
                    break;
                }
            }
            if let Some(error) = clipboard_write_failed {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
            if active_generation.load(Ordering::Acquire) == generation {
                for helper_event in outcome.helper_events {
                    let _ = send_event(&event_writer, helper_event);
                }
            }
            if outcome.capabilities_changed {
                let clipboard_formats = match clipboard.lock() {
                    Ok(clipboard) => clipboard
                        .server_capabilities()
                        .map(ExtendedClipboardCapabilities::format_labels)
                        .unwrap_or_default(),
                    Err(_) => {
                        publish_vnc_disconnect(
                            &event_writer,
                            diagnostics,
                            &canceled,
                            generation,
                            &active_generation,
                            "VNC clipboard state lock is poisoned.".to_string(),
                        );
                        break;
                    }
                };
                match update_vnc_capabilities(&capabilities, |snapshot| {
                    // Client advertisement is not evidence; only server caps
                    // transition this value from Unknown to Supported.
                    snapshot.extended_clipboard = NegotiatedCapabilityStatus::Supported;
                    snapshot.extended_clipboard_formats = clipboard_formats;
                }) {
                    Ok(Some(event)) if active_generation.load(Ordering::Acquire) == generation => {
                        let _ = send_event(&event_writer, event);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        publish_vnc_disconnect(
                            &event_writer,
                            diagnostics,
                            &canceled,
                            generation,
                            &active_generation,
                            error,
                        );
                        break;
                    }
                }
            }
        }

        session_state.observe_input_extensions(&event);
        if let Err(error) = flush_pending_vnc_lock_key_sync(&session_state, &writer) {
            publish_vnc_disconnect(
                &event_writer,
                diagnostics,
                &canceled,
                generation,
                &active_generation,
                error,
            );
            break;
        }
        let qemu_audio_supported = match handle_qemu_audio_event(&event, &audio, &writer) {
            Ok(supported) => supported,
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        };
        let input_capabilities = session_state.input_extension_capabilities();
        match update_vnc_capabilities(&capabilities, |snapshot| {
            if qemu_audio_supported {
                snapshot.qemu_audio = NegotiatedCapabilityStatus::Supported;
            }
            snapshot.extended_key_events = input_capabilities.extended_key_events;
            snapshot.extended_mouse_buttons = input_capabilities.extended_mouse_buttons;
            snapshot.lock_key_sync = input_capabilities.lock_key_sync;
        }) {
            Ok(Some(event)) if active_generation.load(Ordering::Acquire) == generation => {
                let _ = send_event(&event_writer, event);
            }
            Ok(_) => {}
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        }
        if let Err(error) = handle_vnc_desktop_size_event(
            &event,
            &desktop_resize,
            &capabilities,
            &writer,
            &event_writer,
        ) {
            publish_vnc_disconnect(
                &event_writer,
                diagnostics,
                &canceled,
                generation,
                &active_generation,
                error,
            );
            break;
        }
        match observe_vnc_performance_capabilities(&event, &capabilities) {
            Ok(Some(event)) if active_generation.load(Ordering::Acquire) == generation => {
                let _ = send_event(&event_writer, event);
            }
            Ok(_) => {}
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        }
        let mut performance_control_error = None;
        for message in performance_state.observe_server_event(
            &event,
            framebuffer.width as u16,
            framebuffer.height as u16,
        ) {
            if let Err(error) = transport.write_all(&message) {
                performance_control_error =
                    Some(format!("VNC extension control write failed: {error}"));
                break;
            }
        }
        if let Some(error) = performance_control_error {
            publish_vnc_disconnect(
                &event_writer,
                diagnostics,
                &canceled,
                generation,
                &active_generation,
                error,
            );
            break;
        }
        // Flush extension responses before requesting the next framebuffer so
        // SetDesktopSize and audio control messages preserve wire ordering.
        match drain_vnc_io_commands(&mut transport, &io_rx) {
            Ok(true) => {
                if let Some(message) = performance_state.disable_continuous_updates_message(
                    framebuffer.width as u16,
                    framebuffer.height as u16,
                ) {
                    let _ = transport.write_all(&message);
                }
                transport.shutdown_transport();
                break;
            }
            Ok(false) => {}
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        }
        let summary = vnc_server_event_summary(&event);
        counters.server_messages = counters.server_messages.saturating_add(1);
        counters.helper_side_events = counters
            .helper_side_events
            .saturating_add(summary.side_events);
        counters.dirty_rects = counters.dirty_rects.saturating_add(summary.dirty_rects);
        counters.dirty_pixels = counters.dirty_pixels.saturating_add(summary.dirty_pixels);

        if active_generation.load(Ordering::Acquire) == generation {
            for helper_event in vnc_helper_events(&event) {
                let _ = send_event(&event_writer, helper_event);
            }
        }
        let framebuffer_change = match framebuffer.try_apply(event) {
            Ok(change) => change,
            Err(error) => {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    error,
                );
                break;
            }
        };
        if let Some(change) = framebuffer_change {
            session_state.store_size(framebuffer.width as u16, framebuffer.height as u16);
            let framebuffer_resized = matches!(change, VncFramebufferChange::Full);
            let frame_event = vnc_frame_event_for_change(
                &framebuffer,
                change,
                &mut sent_initial_frame,
                session_state.take_base_frame_request(),
            );
            record_vnc_frame_diagnostics(&mut counters, diagnostics, &frame_event, summary);
            if active_generation.load(Ordering::Acquire) == generation {
                let _ = send_event(&event_writer, frame_event);
            }
            if framebuffer_resized
                && let Some(message) = performance_state.framebuffer_resized_message(
                    framebuffer.width as u16,
                    framebuffer.height as u16,
                )
                && let Err(error) = transport.write_all(&message)
            {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    format!("VNC continuous update resize failed: {error}"),
                );
                break;
            }
        }

        if !performance_state.continuous_updates_active() {
            let request = framebuffer_update_request_message(
                true,
                framebuffer.width as u16,
                framebuffer.height as u16,
            );
            if let Err(error) = transport.write_all(&request) {
                publish_vnc_disconnect(
                    &event_writer,
                    diagnostics,
                    &canceled,
                    generation,
                    &active_generation,
                    format!("VNC framebuffer request write failed: {error}"),
                );
                break;
            }
        }
    }
    transport.shutdown_transport();
    if let Ok(mut audio) = audio.lock() {
        audio.stop_local();
    }
}

/// Queues a deferred lock-key synchronization after the server state is known.
pub(super) fn flush_pending_vnc_lock_key_sync(
    session_state: &VncSessionSharedState,
    writer: &SharedVncWriter,
) -> Result<(), String> {
    let Some((current, target)) = session_state.take_pending_lock_key_sync() else {
        return Ok(());
    };
    let mut message = Vec::new();
    for event in vnc_lock_key_sync_events(current, target) {
        let key_message = if session_state.qemu_extended_key_event_support()
            == NegotiatedCapabilityStatus::Supported
        {
            qemu_extended_key_event_message(event)
                .unwrap_or_else(|| vnc_standard_key_event_message(event.keysym, event.down))
        } else {
            vnc_standard_key_event_message(event.keysym, event.down)
        };
        message.extend_from_slice(&key_message);
    }
    if !message.is_empty()
        && let Err(error) = write_vnc_message(writer, &message)
    {
        // Retain the desired state so a later request or LED observation
        // can retry without guessing the server's current lock state.
        session_state.store_pending_lock_keys(target);
        return Err(error);
    }
    session_state.store_remote_lock_keys(target);
    Ok(())
}

fn drain_vnc_io_commands(
    transport: &mut Box<dyn VncTransport>,
    io_rx: &Receiver<VncIoCommand>,
) -> Result<bool, String> {
    loop {
        match io_rx.try_recv() {
            Ok(VncIoCommand::Write(message)) => transport
                .write_all(&message)
                .map_err(|error| format!("VNC message write failed: {error}"))?,
            Ok(VncIoCommand::Shutdown) => {
                return Ok(true);
            }
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                transport.shutdown_transport();
                return Err("VNC command channel closed.".to_string());
            }
        }
    }
}

fn publish_vnc_disconnect(
    event_writer: &SharedEventWriter,
    diagnostics: VncDiagnostics,
    canceled: &Arc<AtomicBool>,
    generation: u64,
    active_generation: &Arc<std::sync::atomic::AtomicU64>,
    error: String,
) {
    if canceled.load(Ordering::Acquire) || active_generation.load(Ordering::Acquire) != generation {
        return;
    }
    diagnostics.log("VNC I/O owner reported a protocol or network disconnect");
    let _ = send_event(
        event_writer,
        RemoteDesktopHelperEvent::Disconnected {
            reason: Some(error),
        },
    );
}

#[cfg(test)]
mod io_owner_tests {
    use std::net::{TcpListener, TcpStream};

    use super::*;

    /// Captures owner writes without opening a network connection.
    struct RecordingTransport {
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl Read for RecordingTransport {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }

    impl Write for RecordingTransport {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl VncTransport for RecordingTransport {
        fn shutdown_transport(&self) {}

        fn set_phase_timeout(&mut self, _timeout: Option<Duration>) {}

        fn set_read_poll_interval(&mut self, _interval: Duration) -> io::Result<()> {
            Ok(())
        }

        fn peer_certificate_der(&self) -> VncResult<Option<Vec<u8>>> {
            Ok(None)
        }
    }

    #[test]
    fn io_owner_drains_final_audio_write_before_shutdown() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut transport: Box<dyn VncTransport> = Box::new(RecordingTransport {
            bytes: bytes.clone(),
        });
        let (writer, receiver) = std::sync::mpsc::sync_channel(2);
        writer
            .send(VncIoCommand::Write(vec![255, 1, 0, 1]))
            .unwrap();
        writer.send(VncIoCommand::Shutdown).unwrap();

        assert!(drain_vnc_io_commands(&mut transport, &receiver).unwrap());
        assert_eq!(*bytes.lock().unwrap(), vec![255, 1, 0, 1]);
    }

    #[test]
    fn idle_socket_owner_accepts_write_and_shutdown_commands() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(25)))
            .unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let canceled = Arc::new(AtomicBool::new(false));
        let transport: Box<dyn VncTransport> =
            Box::new(CancellableTcpStream::new(client, canceled.clone()));
        let (writer, receiver) = std::sync::mpsc::sync_channel(4);
        let active_generation = Arc::new(AtomicU64::new(1));
        let owner_writer = writer.clone();
        let owner = thread::spawn({
            let active_generation = active_generation.clone();
            move || {
                run_vnc_io_owner(
                    transport,
                    receiver,
                    owner_writer,
                    Arc::new(Mutex::new(io::stdout())),
                    VncDiagnostics::default(),
                    canceled,
                    1,
                    active_generation,
                    Arc::new(VncSessionSharedState::new(1, 1)),
                    Arc::new(Mutex::new(QemuAudioSession::new(false))),
                    Arc::new(Mutex::new(VncClipboardSession::default())),
                    Arc::new(Mutex::new(VncVendorFileSession::default())),
                    Arc::new(Mutex::new(NegotiatedCapabilities::default())),
                    Arc::new(Mutex::new(VncDesktopResizeState::new(
                        VncDesktopLayout::single(RemoteDesktopSize {
                            width: 1,
                            height: 1,
                        })
                        .unwrap(),
                    ))),
                    None,
                    1,
                    1,
                );
            }
        });

        writer.send(VncIoCommand::Write(vec![4, 1, 2, 3])).unwrap();
        let mut received = [0; 4];
        server.read_exact(&mut received).unwrap();
        assert_eq!(received, [4, 1, 2, 3]);
        writer.send(VncIoCommand::Shutdown).unwrap();
        owner.join().unwrap();
    }
}

fn record_vnc_frame_diagnostics(
    counters: &mut VncReaderDiagnosticsCounters,
    diagnostics: VncDiagnostics,
    event: &RemoteDesktopHelperEvent,
    summary: VncServerEventSummary,
) {
    match event {
        RemoteDesktopHelperEvent::Frame { frame } => {
            counters.helper_frames = counters.helper_frames.saturating_add(1);
            diagnostics.log(format!(
                "frame kind=base size={}x{} helper_frames={} helper_updates={} server_messages={} dirty_rects_total={} dirty_rects_batch={} dirty_pixels={} side_events={}",
                frame.size.width,
                frame.size.height,
                counters.helper_frames,
                counters.helper_frame_updates,
                counters.server_messages,
                counters.dirty_rects,
                summary.dirty_rects,
                counters.dirty_pixels,
                counters.helper_side_events
            ));
        }
        RemoteDesktopHelperEvent::FrameUpdate { update } => {
            counters.helper_frame_updates = counters.helper_frame_updates.saturating_add(1);
            diagnostics.log(format!(
                "frame kind=update rect={}x{} helper_frames={} helper_updates={} server_messages={} dirty_rects_total={} dirty_rects_batch={} dirty_pixels={} side_events={}",
                update.rect.width,
                update.rect.height,
                counters.helper_frames,
                counters.helper_frame_updates,
                counters.server_messages,
                counters.dirty_rects,
                summary.dirty_rects,
                counters.dirty_pixels,
                counters.helper_side_events
            ));
        }
        RemoteDesktopHelperEvent::FrameUpdateBatch { batch } => {
            counters.helper_frame_updates = counters
                .helper_frame_updates
                .saturating_add(batch.updates.len() as u64);
            diagnostics.log(format!(
                "frame kind=batch regions={} bytes={} helper_frames={} helper_updates={} server_messages={} dirty_rects_total={} dirty_rects_batch={} dirty_pixels={} side_events={}",
                batch.updates.len(),
                batch.byte_len(),
                counters.helper_frames,
                counters.helper_frame_updates,
                counters.server_messages,
                counters.dirty_rects,
                summary.dirty_rects,
                counters.dirty_pixels,
                counters.helper_side_events
            ));
        }
        _ => {}
    }
}

pub(super) fn vnc_auth_key(password: &RemoteDesktopSecret) -> Zeroizing<[u8; 8]> {
    let mut key = Zeroizing::new([0u8; 8]);
    for (slot, byte) in key
        .iter_mut()
        .zip(password.expose_secret().as_bytes().iter().copied().take(8))
    {
        *slot = byte.reverse_bits();
    }
    key
}

pub(super) fn encrypt_vnc_challenge(
    key: &Zeroizing<[u8; 8]>,
    response: &mut Zeroizing<[u8; 16]>,
) -> Result<(), String> {
    let cipher = Des::new_from_slice(key.as_slice())
        .map_err(|_| "VNC password cipher setup failed.".to_string())?;
    for block in response.chunks_exact_mut(8) {
        let mut cipher_block = Block::<Des>::default();
        cipher_block.copy_from_slice(block);
        cipher.encrypt_block(&mut cipher_block);
        block.copy_from_slice(&cipher_block);
    }
    Ok(())
}

pub(super) fn read_server_init(
    stream: &mut impl Read,
    tight_active: bool,
) -> VncResult<(u16, u16, Option<TightInteractionCapabilities>)> {
    let init = read_exact_array::<24, _>(stream)
        .map_err(|error| VncError::network(format!("VNC init read failed: {error}")))?;
    let width = be_u16(&init[0..2]);
    let height = be_u16(&init[2..4]);
    validate_vnc_framebuffer_size(width, height)?;
    let name_len = be_u32(&init[20..24]) as usize;
    if name_len > MAX_VNC_DESKTOP_NAME_BYTES {
        return Err(VncError::protocol(
            "VNC desktop name exceeds the helper limit.",
        ));
    }
    if name_len > 0 {
        read_exact_vec(stream, name_len)
            .map_err(|error| VncError::network(format!("VNC desktop name read failed: {error}")))?;
    }
    let tight_interaction = if tight_active {
        Some(read_tight_interaction_capabilities(stream).map_err(VncError::protocol)?)
    } else {
        None
    };
    Ok((width, height, tight_interaction))
}

fn validate_vnc_framebuffer_size(width: u16, height: u16) -> VncResult<()> {
    if width == 0 || height == 0 {
        return Err(VncError::protocol(
            "VNC framebuffer dimensions must be greater than zero.",
        ));
    }
    let bytes = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| VncError::protocol("VNC framebuffer byte count overflowed."))?;
    if bytes > MAX_VNC_FRAME_BYTES {
        return Err(VncError::protocol(
            "VNC framebuffer exceeds the helper memory limit.",
        ));
    }
    Ok(())
}

fn write_pixel_format(stream: &mut impl Write) -> VncResult<()> {
    let mut message = Vec::with_capacity(20);
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&[
        32, 24, 0, 1, // 32-bit little-endian true color.
        0, 255, 0, 255, 0, 255, // color max values.
        16, 8, 0, // red, green, blue shifts => BGRA bytes.
        0, 0, 0,
    ]);
    stream
        .write_all(&message)
        .map_err(|error| VncError::network(format!("VNC pixel format write failed: {error}")))
}

fn write_encodings(
    stream: &mut impl Write,
    preferences: VncEncodingPreferences,
    h264_available: bool,
) -> VncResult<()> {
    stream
        .write_all(&set_encodings_message(preferences, h264_available))
        .map_err(|error| VncError::network(format!("VNC encoding write failed: {error}")))
}

pub(super) fn set_encodings_message(
    preferences: VncEncodingPreferences,
    h264_available: bool,
) -> Vec<u8> {
    let encodings = advertised_vnc_encodings(preferences, h264_available);
    let mut message = Vec::with_capacity(4 + encodings.len() * 4);
    message.push(2);
    message.push(0);
    push_be_u16(&mut message, encodings.len() as u16);
    for encoding in encodings {
        push_be_i32(&mut message, encoding);
    }
    message
}
