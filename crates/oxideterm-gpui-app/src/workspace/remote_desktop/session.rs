// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteDesktopRestartPresentation {
    Reset,
    Preserve,
}

impl RemoteDesktopSessionEntity {
    pub(super) fn install_release_handler(&self, cx: &mut Context<Self>) {
        cx.on_release(|session, _cx| {
            // Entity destruction owns helper shutdown, but never shared SSH
            // nodes, SFTP sessions, or forwarding runtimes.
            if let Some(worker_wake) = session.worker_wake.take() {
                worker_wake.stop();
            }
            session.cancel_automatic_reconnect();
            session.shutdown_worker();
            drop(session.ssh_tunnel.take());
            drop(session.password.take());
        })
        .detach();
    }

    pub(super) fn bind_window(&mut self, window_handle: AnyWindowHandle) {
        let window_changed = self.window_handle != window_handle;
        self.window_handle = window_handle;
        if window_changed && let Some(worker_wake) = self.worker_wake.as_ref() {
            // A wake may have targeted the old window during handoff. Store
            // one fresh permit without polling render.
            worker_wake.mark();
        }
    }

    fn shutdown_worker(&mut self) {
        self.reset_vnc_file_browser_connection();
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
    }

    fn cancel_automatic_reconnect(&mut self) {
        self.automatic_reconnect_worker_generation = None;
        self.automatic_reconnect_attempt = 0;
        drop(self.automatic_reconnect_task.take());
    }

    fn mark_connection_established(&mut self) {
        self.has_connected = true;
        self.automatic_reconnect_worker_generation = None;
        self.automatic_reconnect_attempt = 0;
        drop(self.automatic_reconnect_task.take());
    }

    fn begin_automatic_reconnect(
        &mut self,
        failed_generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.automatic_reconnect_worker_generation == Some(failed_generation) {
            return;
        }
        if let Some(worker_wake) = self.worker_wake.take() {
            worker_wake.stop();
        }
        self.shutdown_worker();
        self.public_mcp_clipboard = None;
        let replacement_frame_slot = RemoteDesktopFrameDeliverySlot::new();
        // Freeze the last presented frame and sever queued deltas from the
        // failed worker before the replacement graphics epoch starts.
        replacement_frame_slot.set_visible(self.frame_slot.is_visible());
        self.frame_slot = replacement_frame_slot;
        self.state.begin_transport_reconnect();
        let retired_images = self.state.take_retired_images();
        let retired_textures = self.state.take_retired_textures();
        self.drop_images(retired_images, window, cx);
        Self::drop_textures(retired_textures, window);

        let attempt = self.automatic_reconnect_attempt;
        self.automatic_reconnect_attempt = self.automatic_reconnect_attempt.saturating_add(1);
        self.automatic_reconnect_worker_generation = Some(failed_generation);
        let delay = remote_desktop_automatic_reconnect_delay(attempt);
        let reconnect_task = cx.spawn(async move |session, cx| {
            Timer::after(delay).await;
            let window_handle = session
                .update(cx, |session, _cx| {
                    (session.automatic_reconnect_worker_generation == Some(failed_generation)
                        && session.worker_generation == failed_generation)
                        .then_some(session.window_handle)
                })
                .ok()
                .flatten();
            let Some(window_handle) = window_handle else {
                return;
            };
            let _ = cx.update_window(window_handle, move |_, window, cx| {
                let _ = session.update(cx, |session, cx| {
                    if session.automatic_reconnect_worker_generation != Some(failed_generation)
                        || session.worker_generation != failed_generation
                    {
                        return;
                    }
                    session.automatic_reconnect_worker_generation = None;
                    session.restart_worker_preserving_frame(window, cx);
                    cx.notify();
                });
            });
        });
        self.automatic_reconnect_task = Some(reconnect_task);
    }

    fn shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_automatic_reconnect();
        if let Some(worker_wake) = self.worker_wake.take() {
            worker_wake.stop();
        }
        self.shutdown_worker();
        self.public_mcp_clipboard = None;
        drop(self.ssh_tunnel.take());
        drop(self.password.take());
        let images = self.state.take_all_images();
        let textures = self.state.take_all_textures();
        self.drop_images(images, window, cx);
        Self::drop_textures(textures, window);
    }

    fn disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_automatic_reconnect();
        self.public_mcp_clipboard = None;
        if let Some(worker) = self.worker.as_ref() {
            worker.send(RemoteDesktopHelperRequest::Close);
            return;
        }
        self.state
            .apply_event(RemoteDesktopHelperEvent::Disconnected { reason: None });
        let retired_images = self.state.take_retired_images();
        let retired_textures = self.state.take_retired_textures();
        self.drop_images(retired_images, window, cx);
        Self::drop_textures(retired_textures, window);
        cx.notify();
    }

    fn force_recover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_automatic_reconnect();
        self.public_mcp_clipboard = None;
        self.release_inputs();
        if self.worker.is_some() {
            self.send_request(RemoteDesktopHelperRequest::RequestFrame);
        }
        self.restart_worker(window, cx);
    }

    fn reconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_automatic_reconnect();
        self.public_mcp_clipboard = None;
        match remote_desktop_reconnect_mode(self.state.snapshot().status) {
            Some(RemoteDesktopReconnectMode::ProtocolRequest) => {
                self.release_inputs();
                self.send_request(RemoteDesktopHelperRequest::Reconnect);
            }
            Some(RemoteDesktopReconnectMode::RestartHelper) => {
                self.release_inputs();
                self.restart_worker(window, cx);
            }
            None => {}
        }
    }

    fn poll_deliveries(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RemoteDesktopDeliveryOutcome {
        let drain = delivery::drain_channel(&self.delivery_rx, REMOTE_DESKTOP_DELIVERY_BUDGET);
        let mut changed = false;
        let mut intents = Vec::new();
        for delivery in drain.items {
            match delivery {
                RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation } => {
                    debug_assert_eq!(tab_id, self.tab_id);
                    if self.frame_slot.is_visible()
                        && self.apply_frame_ready(generation, window, cx)
                    {
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::FrameRecoveryRequired { tab_id, generation } => {
                    debug_assert_eq!(tab_id, self.tab_id);
                    if self.worker_generation != generation {
                        continue;
                    }
                    // Saturation breaks delta continuity, so the session asks
                    // its helper for one new base frame.
                    if let Some(worker) = self.worker.as_ref() {
                        worker.send(RemoteDesktopHelperRequest::RequestFrame);
                    }
                }
                RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation,
                    event,
                } => {
                    debug_assert_eq!(tab_id, self.tab_id);
                    if self.worker_generation != generation {
                        continue;
                    }
                    match event {
                        RemoteDesktopHelperEvent::ServerCertificate { certificate } => {
                            self.handle_certificate(generation, certificate, cx);
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardText { text }
                            if self.profile.session_options.clipboard.text =>
                        {
                            // Keep a session-scoped zeroizing copy for explicitly authorized
                            // Public MCP reads; the platform clipboard remains the UI boundary.
                            self.public_mcp_clipboard = Some(RemoteDesktopPublicClipboard::Text(
                                Zeroizing::new(text.clone()),
                            ));
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardData { data }
                            if self.profile.session_options.clipboard.images =>
                        {
                            self.public_mcp_clipboard = Some(RemoteDesktopPublicClipboard::Image {
                                format: data.format,
                                bytes: Zeroizing::new(data.bytes.clone()),
                            });
                            if let Some(item) = remote_desktop_clipboard_item_from_data(data) {
                                cx.write_to_clipboard(item);
                            }
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardFilesReady { paths, .. }
                            if self.profile.session_options.clipboard.files =>
                        {
                            cx.write_to_clipboard(ClipboardItem {
                                entries: vec![ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                                    paths.into(),
                                ))],
                            });
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardTransferFailed { .. } => {
                            // Helper text may include remote paths or protocol
                            // details. Only a typed, content-free failure crosses
                            // into the workspace notification adapter.
                            intents.push(RemoteDesktopDeliveryIntent::ClipboardTransferFailed);
                            changed = true;
                        }
                        event @ (RemoteDesktopHelperEvent::VncRemoteFilesListed { .. }
                        | RemoteDesktopHelperEvent::VncRemoteFileListFailed { .. }
                        | RemoteDesktopHelperEvent::VncFileTransferProgress { .. }
                        | RemoteDesktopHelperEvent::VncFileTransferCompleted { .. }
                        | RemoteDesktopHelperEvent::VncFileTransferFailed { .. }) => {
                            if let Some(intent) = self.apply_vnc_file_event(event) {
                                intents.push(intent);
                            }
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ConnectionFailure { message, category } => {
                            if remote_desktop_network_failure_allows_automatic_reconnect(
                                self.has_connected,
                                category,
                            ) {
                                self.begin_automatic_reconnect(generation, window, cx);
                            } else {
                                self.state.apply_event(
                                    RemoteDesktopHelperEvent::ConnectionFailure {
                                        message,
                                        category,
                                    },
                                );
                                let retired_images = self.state.take_retired_images();
                                let retired_textures = self.state.take_retired_textures();
                                self.drop_images(retired_images, window, cx);
                                Self::drop_textures(retired_textures, window);
                            }
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::Terminated { exit_code } => {
                            if self.automatic_reconnect_worker_generation == Some(generation) {
                                continue;
                            }
                            self.state
                                .apply_event(RemoteDesktopHelperEvent::Terminated { exit_code });
                            let retired_images = self.state.take_retired_images();
                            let retired_textures = self.state.take_retired_textures();
                            self.drop_images(retired_images, window, cx);
                            Self::drop_textures(retired_textures, window);
                            // The terminal delivery closes the current ownership
                            // epoch and transfers its bounded join to the reaper.
                            self.shutdown_worker();
                            changed = true;
                        }
                        event => {
                            let connection_established = matches!(
                                &event,
                                RemoteDesktopHelperEvent::Connected { .. }
                                    | RemoteDesktopHelperEvent::Status {
                                        status: RemoteDesktopSessionStatus::Connected,
                                        ..
                                    }
                            );
                            self.state.apply_event(event);
                            if connection_established {
                                self.mark_connection_established();
                            }
                            let retired_images = self.state.take_retired_images();
                            let retired_textures = self.state.take_retired_textures();
                            self.drop_images(retired_images, window, cx);
                            Self::drop_textures(retired_textures, window);
                            changed = true;
                        }
                    }
                }
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message,
                } => {
                    debug_assert_eq!(tab_id, self.tab_id);
                    if self.worker_generation != generation {
                        continue;
                    }
                    if self.frame_slot.is_visible() {
                        let _ = self.apply_frame_ready(generation, window, cx);
                    }
                    if self.has_connected {
                        // A helper transport may be replaced only after this
                        // tab has proven that its profile can establish.
                        self.begin_automatic_reconnect(generation, window, cx);
                    } else {
                        self.state
                            .apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
                                message,
                                category: Some(RemoteDesktopErrorCategory::Unknown),
                            });
                        let retired_images = self.state.take_retired_images();
                        let retired_textures = self.state.take_retired_textures();
                        self.drop_images(retired_images, window, cx);
                        Self::drop_textures(retired_textures, window);
                        // An initial helper transport failure remains terminal;
                        // retrying an unproven profile would hide setup errors.
                        self.shutdown_worker();
                    }
                    changed = true;
                }
            }
        }

        if changed {
            cx.notify();
        }
        RemoteDesktopDeliveryOutcome {
            changed,
            backlog_remaining: drain.outcome.backlog_remaining,
            intents,
        }
    }

    fn apply_frame_ready(
        &mut self,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.worker_generation != generation || !self.frame_slot.is_visible() {
            return false;
        }
        let frame_slot = self.frame_slot.clone();
        let delay = frame_slot.next_frame_ready_delay();
        if !delay.is_zero() {
            self.schedule_frame_apply(generation, delay, cx);
            return false;
        }

        let mut events = Vec::new();
        let started_at = Instant::now();
        let mut budget_hit = false;
        for index in 0..REMOTE_DESKTOP_FRAME_READY_DRAIN_LIMIT {
            if index > 0 && started_at.elapsed() >= REMOTE_DESKTOP_FRAME_READY_DRAIN_BUDGET {
                budget_hit = true;
                break;
            }
            let Some(event) = frame_slot.take() else {
                break;
            };
            // Apply one bounded batch so image uploads cannot monopolize GPUI.
            events.push(event);
        }
        let drained_events = events.len();
        if drained_events == 0 {
            frame_slot.complete_delivery();
            return false;
        }

        frame_slot.mark_frame_presented();
        let apply_started_at = Instant::now();
        let apply_stats = self.state.apply_frame_events(events);
        if self.state.snapshot().status == RemoteDesktopSessionStatus::Connected {
            self.mark_connection_established();
        }
        let apply_elapsed = apply_started_at.elapsed();
        let retired_images = self.state.take_retired_images();
        let retired_textures = self.state.take_retired_textures();
        let retired_image_count = retired_images.len();
        self.render_diagnostics.record_batch(
            drained_events,
            budget_hit,
            apply_elapsed,
            apply_stats,
            retired_image_count,
        );
        if remote_desktop_diagnostics_enabled() {
            eprintln!(
                "[oxideterm:remote-desktop-render] tab={:?} protocol={:?} provider={} resize={} clipboard_data={} gen={generation} trace={:?}->{:?} drained={drained_events} budget_hit={budget_hit} apply_us={} full={} updates={} dirty_applied={} dirty_rejected={} dirty_px={} dirty_frame_px={} pending_texture_updates={} pending_texture_bytes={} texture_updates={} textures_created={} retired={} full_update_recoveries={} totals={:?}",
                self.tab_id,
                self.profile.protocol,
                self.provider.id,
                self.provider.capabilities.resize,
                self.provider.capabilities.clipboard_data,
                apply_stats.first_trace_id,
                apply_stats.last_trace_id,
                duration_micros_u64(apply_elapsed),
                apply_stats.full_frames,
                apply_stats.frame_updates,
                apply_stats.dirty_updates_applied,
                apply_stats.dirty_updates_rejected,
                apply_stats.dirty_rect_pixels,
                apply_stats.dirty_frame_pixels,
                apply_stats.pending_texture_updates,
                apply_stats.pending_texture_upload_bytes,
                apply_stats.dirty_tiles_refreshed,
                apply_stats.frame_tiles_created,
                retired_image_count,
                apply_stats.full_update_recoveries,
                self.render_diagnostics,
            );
        }
        self.drop_images(retired_images, window, cx);
        Self::drop_textures(retired_textures, window);
        if frame_slot.complete_delivery() && frame_slot.mark_frame_ready_queued() {
            self.schedule_frame_apply(generation, frame_slot.next_frame_ready_delay(), cx);
        }
        true
    }

    fn spawn_worker(
        &self,
        generation: u64,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password_available: bool,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        worker_wake: RemoteDesktopWorkerWake,
        initial_size: RemoteDesktopSize,
        scale_factor: Option<u32>,
        monitor_layout: RemoteDesktopMonitorLayout,
        delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    ) -> RemoteDesktopWorkerOwner {
        let tab_id = self.tab_id;
        let (request_tx, request_rx) = mpsc::channel();
        let worker_thread = thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(
                    tab_id,
                    generation,
                    profile,
                    provider,
                    password_available,
                    initial_size,
                    scale_factor,
                    monitor_layout,
                    frame_slot,
                    worker_wake,
                    request_rx,
                    delivery_tx,
                );
            })
            .expect("failed to start remote desktop worker");
        RemoteDesktopWorkerOwner::new(request_tx, worker_thread)
    }

    fn start_worker(
        &mut self,
        initial_request_size: RemoteDesktopSize,
        initial_viewport_size: Option<RemoteDesktopSize>,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.worker.is_some() {
            return false;
        }
        let profile = self.profile.clone();
        let provider = self.provider.clone();
        let password_available = self
            .password
            .as_ref()
            .is_some_and(|password| !password.is_empty());
        let frame_slot = self.frame_slot.clone();
        let delivery_tx = self.delivery_tx.clone();
        let generation = next_remote_desktop_worker_generation(self.worker_generation);
        let worker_wake = RemoteDesktopWorkerWake::default();
        let monitor_layout = remote_desktop_monitor_layout(&profile, cx);
        let worker = self.spawn_worker(
            generation,
            profile,
            provider,
            password_available,
            frame_slot,
            worker_wake.clone(),
            initial_request_size,
            scale_factor,
            monitor_layout.clone(),
            delivery_tx,
        );

        self.worker = Some(worker);
        let previous_worker_wake = self.worker_wake.replace(worker_wake.clone());
        self.worker_generation = generation;
        self.certificate_challenge = None;
        self.last_viewport_size = initial_viewport_size;
        self.last_sent_resize = None;
        self.last_viewport_scale_factor = scale_factor;
        self.last_monitor_layout = monitor_layout;
        self.last_lock_keys = None;
        self.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        self.state.apply_event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: None,
        });
        if let Some(previous_worker_wake) = previous_worker_wake {
            previous_worker_wake.stop();
        }
        // Store the generation before consuming a wake emitted during startup.
        self.schedule_worker_wake(generation, worker_wake, cx);
        true
    }

    fn restart_worker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_worker_with_presentation(RemoteDesktopRestartPresentation::Reset, window, cx);
    }

    fn restart_worker_preserving_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_worker_with_presentation(
            RemoteDesktopRestartPresentation::Preserve,
            window,
            cx,
        );
    }

    fn restart_worker_with_presentation(
        &mut self,
        presentation: RemoteDesktopRestartPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (initial_request_size, initial_viewport_size) =
            initial_remote_desktop_sizes_for_session(self);
        let profile = self.profile.clone();
        let provider = self.provider.clone();
        let password_available = self
            .password
            .as_ref()
            .is_some_and(|password| !password.is_empty());
        let generation = next_remote_desktop_worker_generation(self.worker_generation);
        let scale_factor = self.last_viewport_scale_factor;
        self.shutdown_worker();

        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        // Helper replacement preserves presentation visibility independently
        // from the worker lifetime.
        frame_slot.set_visible(self.frame_slot.is_visible());
        let worker_wake = RemoteDesktopWorkerWake::default();
        let monitor_layout = remote_desktop_monitor_layout(&profile, cx);
        let worker = self.spawn_worker(
            generation,
            profile.clone(),
            provider,
            password_available,
            frame_slot.clone(),
            worker_wake.clone(),
            initial_request_size,
            scale_factor,
            monitor_layout.clone(),
            self.delivery_tx.clone(),
        );
        let previous_worker_wake = self.worker_wake.replace(worker_wake.clone());
        let (old_images, old_textures) =
            if presentation == RemoteDesktopRestartPresentation::Preserve {
                // Automatic transport recovery keeps the last complete frame but
                // starts the replacement helper with a fresh graphics epoch.
                self.state.begin_transport_reconnect();
                (
                    self.state.take_retired_images(),
                    self.state.take_retired_textures(),
                )
            } else {
                let old_images = self.state.take_all_images();
                let old_textures = self.state.take_all_textures();
                self.state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
                    .with_read_only(profile.read_only);
                self.state.apply_event(RemoteDesktopHelperEvent::Status {
                    status: RemoteDesktopSessionStatus::Reconnecting,
                    message: None,
                });
                (old_images, old_textures)
            };
        self.frame_slot = frame_slot;
        self.worker = Some(worker);
        self.worker_generation = generation;
        self.certificate_challenge = None;
        self.last_viewport_size = initial_viewport_size;
        self.last_sent_resize = None;
        self.last_viewport_scale_factor = scale_factor;
        self.last_monitor_layout = monitor_layout;
        self.resize_generation = Arc::new(AtomicU64::new(0));
        self.last_lock_keys = None;
        self.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        if let Some(previous_worker_wake) = previous_worker_wake {
            previous_worker_wake.stop();
        }
        self.drop_images(old_images, window, cx);
        Self::drop_textures(old_textures, window);
        self.schedule_worker_wake(generation, worker_wake, cx);
    }

    fn sync_monitor_layout(&mut self, cx: &mut Context<Self>) {
        if !self.profile.session_options.display.use_all_monitors {
            return;
        }
        let layout = remote_desktop_monitor_layout(&self.profile, cx);
        if layout == self.last_monitor_layout {
            return;
        }
        if let Some(worker) = self.worker.as_ref() {
            worker.send(RemoteDesktopHelperRequest::UpdateDisplayLayout {
                layout: layout.clone(),
            });
        }
        self.last_monitor_layout = layout;
    }

    fn resize_capability(&self) -> NegotiatedCapabilityStatus {
        self.state
            .snapshot()
            .negotiated_capabilities
            .as_ref()
            .map(|capabilities| capabilities.resize)
            .unwrap_or(NegotiatedCapabilityStatus::Unknown)
    }

    fn send_resize_immediately(
        &mut self,
        size: RemoteDesktopSize,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.snapshot().status != RemoteDesktopSessionStatus::Connected
            || !self.resize_capability().is_supported()
        {
            return false;
        }
        let Some(worker) = self.worker.as_ref() else {
            return false;
        };
        let size = RemoteDesktopSize::clamped(size.width, size.height);
        let resize_request = RemoteDesktopResizeRequestState { size, scale_factor };

        // Explicit actions cancel any pending layout debounce and cross the
        // existing session-owned helper channel without changing its lifetime.
        self.resize_generation.fetch_add(1, Ordering::Relaxed);
        self.last_sent_resize = Some(resize_request);
        self.state.mark_resize_requested(size);
        worker.send(RemoteDesktopHelperRequest::Resize { size, scale_factor });
        cx.notify();
        true
    }

    fn fit_current_viewport_immediately(
        &mut self,
        scale_factor: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        self.last_viewport_scale_factor = Some(scale_factor);
        let Some(viewport_size) = self.geometry.viewport_size() else {
            return false;
        };
        let viewport_size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
        self.last_viewport_size = Some(viewport_size);
        let request_size = remote_desktop_requested_size_for_viewport(
            viewport_size,
            self.last_viewport_scale_factor,
        );
        self.send_resize_immediately(request_size, self.last_viewport_scale_factor, cx)
    }

    fn set_follow_window_size(
        &mut self,
        enabled: bool,
        scale_factor: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.follow_window_size == enabled {
            return false;
        }
        self.follow_window_size = enabled;
        if enabled {
            self.fit_current_viewport_immediately(scale_factor, cx);
        }
        true
    }

    fn request_preset_size(
        &mut self,
        size: RemoteDesktopSize,
        scale_factor: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        // A fixed preset and automatic viewport following are mutually
        // exclusive; otherwise the next local layout pass would overwrite it.
        self.follow_window_size = false;
        self.last_viewport_scale_factor = Some(scale_factor);
        self.send_resize_immediately(size, Some(scale_factor), cx)
    }

    pub(super) fn schedule_viewport_resize(
        &mut self,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(scale_factor) = scale_factor {
            // Layout is measured after render; keep the first physical scale
            // before deciding whether the helper can start.
            self.last_viewport_scale_factor = Some(scale_factor);
        }
        let snapshot = self.state.snapshot();
        let Some(viewport_size) = self.geometry.viewport_size() else {
            return false;
        };
        let viewport_size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
        let request_size = remote_desktop_requested_size_for_viewport(
            viewport_size,
            self.last_viewport_scale_factor,
        );
        let resize_request = RemoteDesktopResizeRequestState {
            size: request_size,
            scale_factor: self.last_viewport_scale_factor,
        };
        if self.worker.is_none() {
            if self.last_viewport_scale_factor.is_none() {
                return false;
            }
            if matches!(
                snapshot.status,
                RemoteDesktopSessionStatus::Idle
                    | RemoteDesktopSessionStatus::Connecting
                    | RemoteDesktopSessionStatus::Reconnecting
            ) {
                return self.start_worker(
                    request_size,
                    Some(viewport_size),
                    self.last_viewport_scale_factor,
                    cx,
                );
            }
            return false;
        }
        if snapshot.status != RemoteDesktopSessionStatus::Connected {
            return false;
        }
        if !self.follow_window_size {
            return false;
        }
        let should_send_resize = remote_desktop_resize_request_needed_for_capability(
            self.resize_capability().is_supported(),
            snapshot.size,
            snapshot.pending_resize,
            self.last_viewport_size,
            self.last_sent_resize,
            viewport_size,
            request_size,
            self.last_viewport_scale_factor,
        );
        if Some(viewport_size) == self.last_viewport_size && !should_send_resize {
            return false;
        }
        self.last_viewport_size = Some(viewport_size);
        if !should_send_resize {
            return false;
        }

        self.last_sent_resize = Some(resize_request);
        self.state.mark_resize_requested(request_size);
        let generation = self.resize_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let resize_generation = self.resize_generation.clone();
        let Some(request_tx) = self
            .worker
            .as_ref()
            .and_then(RemoteDesktopWorkerOwner::request_sender_cloned)
        else {
            return true;
        };
        thread::Builder::new()
            .name("remote-desktop-resize-debounce".to_string())
            .spawn(move || {
                thread::sleep(REMOTE_DESKTOP_RESIZE_DEBOUNCE);
                if resize_generation.load(Ordering::Relaxed) == generation {
                    let _ = request_tx.send(RemoteDesktopHelperRequest::Resize {
                        size: resize_request.size,
                        scale_factor: resize_request.scale_factor,
                    });
                }
            })
            .ok();
        true
    }

    pub(super) fn schedule_initial_layout_probe(
        &mut self,
        initial_scale_factor: u32,
        cx: &mut Context<Self>,
    ) {
        self.last_viewport_scale_factor = Some(initial_scale_factor);
        if self.schedule_viewport_resize(None, cx) {
            cx.notify();
        }
        if self.worker.is_some() {
            return;
        }

        cx.spawn(async move |session, cx| {
            for _ in 0..REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS {
                Timer::after(REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL).await;
                let done = session
                    .update(cx, |session, cx| {
                        if session.worker.is_some() {
                            return true;
                        }
                        // The worker must start from the measured viewport even
                        // before it has any delivery capable of waking the UI.
                        if session.schedule_viewport_resize(None, cx) {
                            cx.notify();
                        }
                        session.worker.is_some()
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_window_event(&self, event: RemoteDesktopSessionEvent, cx: &mut Context<Self>) {
        cx.spawn(async move |session, cx| {
            let generation = match event {
                RemoteDesktopSessionEvent::DeliveryReady { generation }
                | RemoteDesktopSessionEvent::FrameApplyReady { generation } => generation,
                RemoteDesktopSessionEvent::ClipboardTransferFailed
                | RemoteDesktopSessionEvent::VncFileTransferCompleted
                | RemoteDesktopSessionEvent::VncFileTransferFailed(_) => return,
            };
            let window_handle = session
                .update(cx, |session, _cx| {
                    (session.worker_generation == generation).then_some(session.window_handle)
                })
                .ok()
                .flatten();
            let Some(window_handle) = window_handle else {
                return;
            };

            let _ = cx.update_window(window_handle, move |_, window, cx| {
                let _ = session.update(cx, |session, cx| {
                    if session.worker_generation != generation {
                        return;
                    }
                    match event {
                        RemoteDesktopSessionEvent::DeliveryReady { .. } => {
                            let outcome = session.poll_deliveries(window, cx);
                            session.sync_monitor_layout(cx);
                            if outcome.changed {
                                cx.notify();
                            }
                            for intent in outcome.intents {
                                match intent {
                                    RemoteDesktopDeliveryIntent::ClipboardTransferFailed => {
                                        cx.emit(RemoteDesktopSessionEvent::ClipboardTransferFailed);
                                    }
                                    RemoteDesktopDeliveryIntent::VncFileTransferCompleted => {
                                        cx.emit(
                                            RemoteDesktopSessionEvent::VncFileTransferCompleted,
                                        );
                                    }
                                    RemoteDesktopDeliveryIntent::VncFileTransferFailed(kind) => {
                                        cx.emit(RemoteDesktopSessionEvent::VncFileTransferFailed(
                                            kind,
                                        ));
                                    }
                                }
                            }
                            if outcome.backlog_remaining
                                && let Some(worker_wake) = session.worker_wake.as_ref()
                            {
                                worker_wake.mark();
                            }
                        }
                        RemoteDesktopSessionEvent::FrameApplyReady { .. } => {
                            if session.apply_frame_ready(generation, window, cx) {
                                cx.notify();
                            }
                        }
                        RemoteDesktopSessionEvent::ClipboardTransferFailed
                        | RemoteDesktopSessionEvent::VncFileTransferCompleted
                        | RemoteDesktopSessionEvent::VncFileTransferFailed(_) => {}
                    }
                });
            });
        })
        .detach();
    }

    fn drop_images(
        &self,
        images: Vec<Arc<gpui::RenderImage>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for image in images {
            // Dynamic remote tiles remain in the sprite atlas until explicitly released.
            cx.drop_image(image, Some(window));
        }
    }

    fn drop_textures(textures: Vec<Arc<gpui::DynamicTexture>>, window: &mut Window) {
        for texture in textures {
            let _ = window.drop_dynamic_texture(texture);
        }
    }

    fn set_frame_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.ui_frame_visible = visible;
        self.apply_frame_visibility(cx);
    }

    pub(super) fn apply_frame_visibility(&mut self, cx: &mut Context<Self>) {
        let effective_visible = self.ui_frame_visible || self.public_mcp_frame_observers > 0;
        let visibility_changed = self.frame_slot.is_visible() != effective_visible;
        let recovery_required = self.frame_slot.set_visible(effective_visible);
        if recovery_required
            && let Some(request_tx) = self
                .worker
                .as_ref()
                .and_then(RemoteDesktopWorkerOwner::request_sender)
        {
            // The worker remains alive while hidden; request one base frame so
            // sparse deltas do not become an unbounded off-screen history.
            let _ = request_tx.send(RemoteDesktopHelperRequest::RequestFrame);
        }
        if effective_visible && visibility_changed && self.frame_slot.has_queued_frame_events() {
            self.schedule_frame_apply(self.worker_generation, Duration::ZERO, cx);
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_remote_desktop_session_event(
        &mut self,
        tab_id: TabId,
        session_entity: &Entity<RemoteDesktopSessionEntity>,
        event: &RemoteDesktopSessionEvent,
        cx: &mut Context<Self>,
    ) {
        debug_assert_eq!(session_entity.read(cx).tab_id, tab_id);
        match event {
            RemoteDesktopSessionEvent::ClipboardTransferFailed => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.clipboard_file_failed"),
                    None,
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
            RemoteDesktopSessionEvent::VncFileTransferCompleted => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.file_download_completed"),
                    None,
                    TerminalNoticeVariant::Success,
                    cx,
                );
                return;
            }
            RemoteDesktopSessionEvent::VncFileTransferFailed(kind) => {
                let key = match kind {
                    RemoteDesktopFileTransferFailureKind::Remote => {
                        "remote_desktop.file_download_failed_remote"
                    }
                    RemoteDesktopFileTransferFailureKind::Local => {
                        "remote_desktop.file_download_failed_local"
                    }
                    RemoteDesktopFileTransferFailureKind::Protocol => {
                        "remote_desktop.file_download_failed_protocol"
                    }
                    RemoteDesktopFileTransferFailureKind::Canceled => {
                        "remote_desktop.file_download_canceled"
                    }
                };
                self.push_command_palette_toast(
                    self.i18n.t(key),
                    None,
                    if *kind == RemoteDesktopFileTransferFailureKind::Canceled {
                        TerminalNoticeVariant::Default
                    } else {
                        TerminalNoticeVariant::Error
                    },
                    cx,
                );
                return;
            }
            RemoteDesktopSessionEvent::DeliveryReady { .. }
            | RemoteDesktopSessionEvent::FrameApplyReady { .. } => {}
        }

        let visible = self.remote_desktop_tab_visible(tab_id, cx);
        // The root supplies cross-tab visibility, then the Entity owns the
        // window-affine delivery and lifecycle transition.
        session_entity.update(cx, |session, cx| {
            session.set_frame_visibility(visible, cx);
            session.schedule_window_event(*event, cx);
        });
    }

    pub(in crate::workspace) fn remote_desktop_session_entity(
        &self,
        tab_id: TabId,
        cx: &App,
    ) -> Option<Entity<RemoteDesktopSessionEntity>> {
        self.remote_desktop.read(cx).session(tab_id)
    }

    pub(in crate::workspace) fn bind_remote_desktop_window(
        &mut self,
        tab_id: TabId,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            // Window affinity is a lifecycle property: delivery and resource
            // cleanup must follow the tab across detach and dock transitions.
            session.update(cx, |session, _cx| {
                session.bind_window(window_handle);
            });
        }
    }

    pub(in crate::workspace) fn remote_desktop_tab_visible(&self, tab_id: TabId, cx: &App) -> bool {
        let tab_host = self.tab_host.read(cx);
        let main_tab_visible =
            self.active_tab_id(cx) == Some(tab_id) && !tab_host.is_outside_main_window(tab_id);
        let detached_tab_visible = tab_host.is_detached(tab_id);
        remote_desktop_tab_visible(main_tab_visible, detached_tab_visible)
    }

    pub(in crate::workspace) fn resume_remote_desktop_frame_delivery(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        // Hidden tabs retain one coalesced frame. Visibility resumes that frame
        // without restarting or disconnecting either protocol worker.
        session_entity.update(cx, |session, cx| session.set_frame_visibility(true, cx));
    }

    pub(in crate::workspace) fn sync_remote_desktop_frame_visibility(
        &self,
        tab_id: TabId,
        cx: &mut App,
    ) {
        let visible = self.remote_desktop_tab_visible(tab_id, cx);
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                session.set_frame_visibility(visible, cx);
            });
        }
    }

    pub(in crate::workspace) fn close_remote_desktop_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_public_mcp_desktop_for_closed_tab(tab_id);
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.shutdown(window, cx));
        }
        if self.remote_desktop_resize_menu_tab_id == Some(tab_id) {
            self.remote_desktop_resize_menu_tab_id = None;
        }
        self.remote_desktop
            .update(cx, |remote_desktop, _cx| remote_desktop.remove(tab_id));
    }

    pub(in crate::workspace) fn release_remote_desktop_inputs_for_tab(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.release_inputs());
        }
    }

    pub(in crate::workspace) fn release_active_remote_desktop_inputs(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.active_remote_desktop_tab_id(cx) {
            self.release_remote_desktop_inputs_for_tab(tab_id, cx);
        }
    }

    pub(in crate::workspace) fn focus_remote_desktop_keyboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The remote surface stops root mouse propagation, so it must run the
        // same blur path that an outside workspace click would normally run.
        self.blur_text_inputs(cx);

        let mut changed = self.clear_ime_selection();
        changed |= self.ime_marked_text.take().is_some();
        changed |= self.pending_platform_text_commit.take().is_some();

        let ai_focus_changed = self.ai_entity.read(cx).chat_ui().input_focused
            || self.ai_entity.read(cx).chat_ui().footer_focus.is_some()
            || self.ai_entity.read(cx).model_selector_open()
            || self.ai_entity.read(cx).model_selector_search_focused();
        self.clear_ai_sidebar_keyboard_focus(cx);
        changed |= ai_focus_changed;

        if let Some(tab_id) = self.active_remote_desktop_tab_id(cx) {
            self.sync_remote_desktop_lock_keys(tab_id, window.capslock(), cx);
        }
        window.focus(&self.focus_handle, cx);
        if changed {
            cx.notify();
        }
    }

    fn fit_remote_desktop_to_current_viewport(
        &mut self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        let scale_factor = remote_desktop_scale_factor_percent(window.scale_factor());
        session.update(cx, |session, cx| {
            session.fit_current_viewport_immediately(scale_factor, cx);
        });
    }

    fn toggle_remote_desktop_resize_menu(&mut self, tab_id: TabId) {
        self.remote_desktop_resize_menu_tab_id =
            (self.remote_desktop_resize_menu_tab_id != Some(tab_id)).then_some(tab_id);
    }

    fn set_remote_desktop_follow_window_size(
        &mut self,
        tab_id: TabId,
        enabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        let scale_factor = remote_desktop_scale_factor_percent(window.scale_factor());
        session.update(cx, |session, cx| {
            if session.set_follow_window_size(enabled, scale_factor, cx) {
                cx.notify();
            }
        });
    }

    fn set_remote_desktop_preset_size(
        &mut self,
        tab_id: TabId,
        size: RemoteDesktopSize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        let scale_factor = remote_desktop_scale_factor_percent(window.scale_factor());
        session.update(cx, |session, cx| {
            session.request_preset_size(size, scale_factor, cx);
        });
    }

    fn render_remote_desktop_resize_control(
        &self,
        tab_id: TabId,
        menu_open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let fit_button = self.workspace_toolbar_action_button(
            self.i18n.t("remote_desktop.fit_current_window"),
            Some(Self::render_lucide_icon(
                LucideIcon::Monitor,
                12.0,
                rgb(theme.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(24.0),
                padding_x: Some(8.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.fit_remote_desktop_to_current_viewport(tab_id, window, cx);
                cx.stop_propagation();
            }),
        );
        let menu_button = self.workspace_icon_action_button(
            LucideIcon::ChevronDown,
            12.0,
            rgb(theme.text_muted),
            IconButtonOptions {
                radius: ButtonRadius::Md,
                has_background: menu_open,
                ..IconButtonOptions::compact(24.0)
            },
            move |this, _event, _window, cx| {
                this.toggle_remote_desktop_resize_menu(tab_id);
                cx.stop_propagation();
                cx.notify();
            },
            cx,
        );
        let trigger = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .child(fit_button)
            .child(menu_button);

        // Keep the split-button bounds warm so the first menu open can be
        // placed above the footer without relying on pointer coordinates.
        select_anchor_probe(
            SelectAnchorId::RemoteDesktopResizeMenu(tab_id.0),
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_remote_desktop_resize_menu(
        &self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::RemoteDesktopResizeMenu(tab_id.0))
            .copied()
        else {
            return div().into_any_element();
        };
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return div().into_any_element();
        };
        let session = session_entity.read(cx);
        let snapshot = session.state.snapshot();
        let follow_window_size = session.follow_window_size;
        let current_size = snapshot.size;
        let viewport = window.viewport_size();
        let actionable_or_value_count = 2 + REMOTE_DESKTOP_COMMON_RESOLUTIONS.len();
        let menu_label_count = 2;
        let menu_separator_count = 2.0;
        let menu_height = context_menu_item_height_estimate(&self.tokens)
            * (actionable_or_value_count + menu_label_count) as f32
            + context_menu_separator_height_estimate(&self.tokens) * menu_separator_count
            + self.tokens.metrics.ui_menu_padding * 2.0;
        let placement = browser_behavior::clamp_context_menu_position(
            f32::from(anchor.bounds.right()) - REMOTE_DESKTOP_RESIZE_MENU_WIDTH,
            f32::from(anchor.bounds.top()) - menu_height - REMOTE_DESKTOP_RESIZE_MENU_GAP,
            f32::from(viewport.width),
            f32::from(viewport.height),
            REMOTE_DESKTOP_RESIZE_MENU_WIDTH,
            menu_height,
            REMOTE_DESKTOP_RESIZE_MENU_VIEWPORT_PADDING,
        );
        let follow_item = dropdown_menu_item(
            &self.tokens,
            self.i18n.t("remote_desktop.follow_window_size"),
            DropdownMenuItemKind::Checkbox(follow_window_size),
            false,
            false,
        );
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(REMOTE_DESKTOP_RESIZE_MENU_WIDTH)),
        )
        .child(self.workspace_context_menu_persistent_styled_action(
            follow_item,
            false,
            false,
            oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle::default(),
            move |this, _event, window, cx| {
                this.set_remote_desktop_follow_window_size(tab_id, !follow_window_size, window, cx);
            },
            cx,
        ))
        .child(dropdown_menu_separator(&self.tokens))
        .child(dropdown_menu_label(
            &self.tokens,
            self.i18n.t("remote_desktop.current_remote_size"),
            false,
        ));
        let current_size_label = current_size
            .map(remote_desktop_resolution_label)
            .unwrap_or_else(|| self.i18n.t("remote_desktop.size_unavailable"));
        menu = menu
            .child(dropdown_menu_item(
                &self.tokens,
                current_size_label,
                DropdownMenuItemKind::Plain,
                false,
                true,
            ))
            .child(dropdown_menu_separator(&self.tokens))
            .child(dropdown_menu_label(
                &self.tokens,
                self.i18n.t("remote_desktop.common_resolutions"),
                false,
            ));
        for size in REMOTE_DESKTOP_COMMON_RESOLUTIONS {
            let selected = !follow_window_size && current_size == Some(size);
            let item = dropdown_menu_item(
                &self.tokens,
                remote_desktop_resolution_label(size),
                DropdownMenuItemKind::Radio(selected),
                false,
                false,
            );
            menu = menu.child(self.workspace_context_menu_styled_action(
                item,
                false,
                false,
                oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle::default(),
                |this| this.remote_desktop_resize_menu_tab_id = None,
                move |this, _event, window, cx| {
                    this.set_remote_desktop_preset_size(tab_id, size, window, cx);
                },
                cx,
            ));
        }

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(overlay_content_boundary(menu)),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(in crate::workspace) fn render_remote_desktop_footer(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return div().into_any_element();
        };
        let session = session_entity.read(cx);
        let theme = self.tokens.ui;
        let snapshot = session.state.snapshot();
        let status = snapshot.status;
        let status_color = remote_desktop_status_color(&self.tokens, status);
        let reconnect_disabled = remote_desktop_reconnect_mode(status).is_none();
        let resize_capability = snapshot
            .negotiated_capabilities
            .as_ref()
            .map(|capabilities| capabilities.resize)
            .unwrap_or_else(|| {
                if session.provider.capabilities.resize {
                    NegotiatedCapabilityStatus::Unknown
                } else {
                    NegotiatedCapabilityStatus::Unsupported
                }
            });
        let resize_capability_label = match resize_capability {
            NegotiatedCapabilityStatus::Supported => {
                self.i18n.t("remote_desktop.fit_current_window")
            }
            NegotiatedCapabilityStatus::Unsupported => self.i18n.t("remote_desktop.resize_fixed"),
            NegotiatedCapabilityStatus::Unknown => self.i18n.t("remote_desktop.resize_unknown"),
        };
        let resize_menu_open = self.remote_desktop_resize_menu_tab_id == Some(tab_id);
        let vnc_file_download_available = session.vnc_file_download_available();
        let vnc_capability_presentation =
            (snapshot.protocol == RemoteDesktopProtocol::Vnc).then(|| {
                self.remote_desktop_vnc_capability_presentation(
                    snapshot.negotiated_capabilities.as_ref(),
                )
            });
        let label = format!(
            "{} · {}:{}",
            session.provider.name, session.profile.endpoint.host, session.profile.endpoint.port
        );

        div()
            .flex_none()
            .h(px(36.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .bg(rgb(theme.bg_panel))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(remote_desktop_protocol_chip(
                &self.tokens,
                snapshot.protocol,
            ))
            .child(
                div()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(rgb(status_color))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .when(resize_capability.is_supported(), |footer| {
                footer.child(self.render_remote_desktop_resize_control(
                    tab_id,
                    resize_menu_open,
                    cx,
                ))
            })
            .when(!resize_capability.is_supported(), |footer| {
                footer.child(remote_desktop_capability_chip(
                    &self.tokens,
                    resize_capability_label,
                ))
            })
            .when_some(
                vnc_capability_presentation,
                |footer, (capability_label, capability_tooltip)| {
                    let tooltip_for_move = capability_tooltip.clone();
                    footer.child(
                        remote_desktop_capability_chip(&self.tokens, capability_label)
                            .id("remote-desktop-vnc-capabilities")
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, _window, cx| {
                                    this.queue_workspace_tooltip(
                                        "remote-desktop-vnc-capabilities",
                                        tooltip_for_move.clone(),
                                        f32::from(event.position.x) + 12.0,
                                        f32::from(event.position.y) + 16.0,
                                        cx,
                                    );
                                },
                            ))
                            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                                if !*hovered {
                                    this.clear_workspace_tooltip(
                                        "remote-desktop-vnc-capabilities",
                                        cx,
                                    );
                                }
                            })),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .when(vnc_file_download_available, |actions| {
                        actions.child(self.workspace_toolbar_action_button(
                            self.i18n.t("remote_desktop.file_browser"),
                            Some(Self::render_lucide_icon(
                                LucideIcon::FolderOpen,
                                12.0,
                                rgb(theme.text_muted),
                            )),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                height: Some(24.0),
                                padding_x: Some(8.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_vnc_file_browser(tab_id, cx);
                            }),
                        ))
                    })
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.force_recover"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Wrench,
                            12.0,
                            rgb(theme.text_muted),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: !remote_desktop_force_recover_enabled(status),
                            },
                            height: Some(24.0),
                            padding_x: Some(8.0),
                            font_size: Some(self.tokens.metrics.ui_text_xs),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, window, cx| {
                            this.force_recover_remote_desktop(tab_id, window, cx);
                            cx.notify();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.reconnect"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::RefreshCw,
                            12.0,
                            rgb(theme.text_muted),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: reconnect_disabled,
                            },
                            height: Some(24.0),
                            padding_x: Some(8.0),
                            font_size: Some(self.tokens.metrics.ui_text_xs),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, window, cx| {
                            this.reconnect_remote_desktop(tab_id, window, cx);
                            cx.notify();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.disconnect"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Power,
                            12.0,
                            rgb(theme.text_muted),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            height: Some(24.0),
                            padding_x: Some(8.0),
                            font_size: Some(self.tokens.metrics.ui_text_xs),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, window, cx| {
                            this.release_remote_desktop_inputs_for_tab(tab_id, cx);
                            this.disconnect_remote_desktop(tab_id, window, cx);
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    fn remote_desktop_vnc_capability_presentation(
        &self,
        capabilities: Option<&NegotiatedCapabilities>,
    ) -> (String, String) {
        let unknown_capabilities = NegotiatedCapabilities::default();
        let is_pending = capabilities.is_none();
        let capabilities = capabilities.unwrap_or(&unknown_capabilities);
        let supported = self.i18n.t("remote_desktop.capability_supported");
        let unsupported = self.i18n.t("remote_desktop.capability_unsupported");
        let unknown = self.i18n.t("remote_desktop.capability_unknown");
        let status = |value: NegotiatedCapabilityStatus| match value {
            NegotiatedCapabilityStatus::Supported => supported.as_str(),
            NegotiatedCapabilityStatus::Unsupported => unsupported.as_str(),
            NegotiatedCapabilityStatus::Unknown => unknown.as_str(),
        };
        let feature_statuses = [
            capabilities.resize,
            capabilities.multi_monitor,
            capabilities.extended_clipboard,
            capabilities.tight,
            capabilities.jpeg,
            capabilities.continuous_updates,
            capabilities.fence,
            capabilities.last_rect,
            capabilities.h264,
            capabilities.qemu_audio,
            capabilities.vendor_files,
            capabilities.extended_key_events,
            capabilities.extended_mouse_buttons,
            capabilities.lock_key_sync,
        ];
        let supported_count = feature_statuses
            .iter()
            .filter(|value| value.is_supported())
            .count();
        let label = if is_pending {
            self.i18n.t("remote_desktop.capabilities_pending")
        } else {
            self.i18n
                .t("remote_desktop.capabilities_summary")
                .replace("{{supported}}", &supported_count.to_string())
                .replace("{{total}}", &feature_statuses.len().to_string())
        };
        let security_method = capabilities
            .selected_security_method
            .as_deref()
            .or_else(|| capabilities.security_methods.first().map(String::as_str))
            .unwrap_or(unknown.as_str());
        let clipboard_formats = match capabilities.extended_clipboard {
            NegotiatedCapabilityStatus::Supported
                if !capabilities.extended_clipboard_formats.is_empty() =>
            {
                capabilities.extended_clipboard_formats.join(", ")
            }
            value => status(value).to_string(),
        };
        let lines = [
            format!(
                "{}: {security_method}",
                self.i18n.t("remote_desktop.capability_security_method")
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_encryption"),
                status(capabilities.encrypted)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_identity_verified"),
                status(capabilities.peer_identity_verified)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_resize"),
                status(capabilities.resize)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_multi_monitor"),
                status(capabilities.multi_monitor)
            ),
            format!(
                "{}: {clipboard_formats}",
                self.i18n.t("remote_desktop.capability_extended_clipboard")
            ),
            format!("Tight: {}", status(capabilities.tight)),
            format!("JPEG: {}", status(capabilities.jpeg)),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_continuous_updates"),
                status(capabilities.continuous_updates)
            ),
            format!("Fence: {}", status(capabilities.fence)),
            format!("LastRect: {}", status(capabilities.last_rect)),
            format!("H.264: {}", status(capabilities.h264)),
            format!("QEMU Audio: {}", status(capabilities.qemu_audio)),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_vendor_files"),
                status(capabilities.vendor_files)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_extended_keys"),
                status(capabilities.extended_key_events)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_extended_mouse"),
                status(capabilities.extended_mouse_buttons)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_lock_sync"),
                status(capabilities.lock_key_sync)
            ),
        ];

        (label, lines.join("\n"))
    }

    pub(in crate::workspace) fn force_recover_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.force_recover(window, cx));
        }
    }

    pub(in crate::workspace) fn disconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.disconnect(window, cx));
        }
    }

    pub(in crate::workspace) fn reconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.reconnect(window, cx));
        }
    }
}

fn remote_desktop_monitor_layout(
    profile: &RemoteDesktopConnectionProfile,
    cx: &App,
) -> RemoteDesktopMonitorLayout {
    if !profile.session_options.display.use_all_monitors {
        return RemoteDesktopMonitorLayout::default();
    }

    let displays = cx.displays();
    let primary_id = cx
        .primary_display()
        .map(|display| display.id())
        .or_else(|| {
            displays
                .iter()
                .find(|display| {
                    let bounds = display.physical_bounds();
                    i32::from(bounds.origin.x) == 0 && i32::from(bounds.origin.y) == 0
                })
                .map(|display| display.id())
        })
        .or_else(|| displays.first().map(|display| display.id()));
    let Some(primary_id) = primary_id else {
        return RemoteDesktopMonitorLayout::default();
    };
    let Some(primary_bounds) = displays
        .iter()
        .find(|display| display.id() == primary_id)
        .map(|display| display.physical_bounds())
    else {
        return RemoteDesktopMonitorLayout::default();
    };
    let primary_left = i32::from(primary_bounds.origin.x);
    let primary_top = i32::from(primary_bounds.origin.y);

    let mut monitors = displays
        .into_iter()
        .filter_map(|display| {
            let bounds = display.physical_bounds();
            let width = u32::try_from(i32::from(bounds.size.width)).ok()?;
            let height = u32::try_from(i32::from(bounds.size.height)).ok()?;
            if width < 200 || height < 200 {
                return None;
            }
            let width = width.min(8192) & !1;
            let height = height.min(8192);
            let primary = display.id() == primary_id;
            let desktop_scale_factor = (display.scale_factor()
                * REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER)
                .round()
                .clamp(
                    REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT as f32,
                    REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT as f32,
                ) as u32;
            let device_scale_factor = match desktop_scale_factor {
                0..=120 => 100,
                121..=160 => 140,
                _ => 180,
            };

            Some(RemoteDesktopMonitor {
                stable_id: display
                    .uuid()
                    .map(|uuid| uuid.to_string())
                    .unwrap_or_else(|_| format!("display-{}", u64::from(display.id()))),
                left: i32::from(bounds.origin.x).saturating_sub(primary_left),
                top: i32::from(bounds.origin.y).saturating_sub(primary_top),
                width,
                height,
                primary,
                desktop_scale_factor,
                device_scale_factor,
                physical_width_mm: None,
                physical_height_mm: None,
                orientation: if width >= height {
                    RemoteDesktopMonitorOrientation::Landscape
                } else {
                    RemoteDesktopMonitorOrientation::Portrait
                },
            })
        })
        .collect::<Vec<_>>();
    monitors.sort_by_key(|monitor| (!monitor.primary, monitor.top, monitor.left));

    RemoteDesktopMonitorLayout { monitors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualContext, point, size};
    use oxideterm_remote_desktop::{
        RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate, RemoteDesktopRect,
    };

    struct RemoteDesktopSessionTestRoot;

    impl Render for RemoteDesktopSessionTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui::test]
    fn initial_layout_probe_starts_rdp_and_vnc_before_first_delivery(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| RemoteDesktopSessionTestRoot);
        let tokens = ThemeTokens::from_builtin(theme_by_id("default"));
        let viewport_width = 960;
        let viewport_height = 540;
        let initial_scale_factor = 200;

        for (tab_number, protocol) in [
            (21, RemoteDesktopProtocol::Rdp),
            (22, RemoteDesktopProtocol::Vnc),
        ] {
            let profile = preview_remote_desktop_profile(protocol);
            let provider = builtin_preview_provider_registry()
                .unwrap()
                .get_for_protocol(protocol)
                .cloned()
                .unwrap();
            let window_handle = cx.window_handle();
            let session = cx.new(|cx| {
                let session = RemoteDesktopSessionEntity::new(
                    TabId(tab_number),
                    profile,
                    provider,
                    None,
                    std::env::temp_dir().join(format!(
                        "oxideterm-initial-layout-{}-test-certificates.json",
                        protocol.provider_id()
                    )),
                    RemoteDesktopFrameDeliverySlot::new(),
                    window_handle,
                );
                session.install_release_handler(cx);
                session
            });

            // The placeholder canvas is the only source of the initial
            // viewport before either protocol can produce a worker delivery.
            cx.draw(
                point(px(0.0), px(0.0)),
                size(px(viewport_width as f32), px(viewport_height as f32)),
                |_window, cx| {
                    let session = session.read(cx);
                    remote_desktop_surface_with_geometry(
                        &tokens,
                        &session.state,
                        Some(session.geometry.clone()),
                    )
                },
            );
            assert_eq!(
                session.read_with(cx, |session, _cx| session.geometry.viewport_size()),
                Some(RemoteDesktopSize::clamped(viewport_width, viewport_height))
            );
            session.update(cx, |session, cx| {
                session.schedule_initial_layout_probe(initial_scale_factor, cx);
            });
            cx.cx.run_until_parked();

            let (worker_started, measured_viewport, recorded_scale_factor) =
                session.read_with(cx, |session, _cx| {
                    (
                        session.worker.is_some(),
                        session.last_viewport_size,
                        session.last_viewport_scale_factor,
                    )
                });
            assert!(worker_started, "{protocol:?} worker did not start");
            assert_eq!(
                measured_viewport,
                Some(RemoteDesktopSize::clamped(viewport_width, viewport_height))
            );
            assert_eq!(recorded_scale_factor, Some(initial_scale_factor));

            drop(session);
            cx.cx.run_until_parked();
        }
    }

    #[gpui::test]
    fn hidden_rdp_and_vnc_sessions_apply_control_deliveries(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
        for (tab_number, protocol) in [
            (31, RemoteDesktopProtocol::Rdp),
            (32, RemoteDesktopProtocol::Vnc),
        ] {
            let tab_id = TabId(tab_number);
            let mut profile = preview_remote_desktop_profile(protocol);
            profile.session_options.clipboard.images = true;
            profile.session_options.clipboard.files = true;
            let provider = builtin_preview_provider_registry()
                .unwrap()
                .get_for_protocol(protocol)
                .cloned()
                .unwrap();
            let session = cx.new(|_cx| {
                let mut session = RemoteDesktopSessionEntity::new(
                    tab_id,
                    profile,
                    provider,
                    None,
                    std::env::temp_dir().join(format!(
                        "oxideterm-hidden-control-{}-test-certificates.json",
                        protocol.provider_id()
                    )),
                    RemoteDesktopFrameDeliverySlot::new(),
                    window.into(),
                );
                session.worker_generation = 1;
                session
            });
            let mut events = cx.events(&session);

            // Clipboard and lifecycle messages remain reliable while frame
            // presentation is suspended.
            session.update(cx, |session, cx| {
                session.set_frame_visibility(false, cx);
                for event in [
                    RemoteDesktopHelperEvent::ClipboardText {
                        text: format!("hidden-{protocol:?}"),
                    },
                    RemoteDesktopHelperEvent::ClipboardTransferFailed {
                        transfer_id: "hidden-transfer".to_string(),
                        message: "content-free test failure".to_string(),
                    },
                ] {
                    session
                        .delivery_tx
                        .send(RemoteDesktopWorkerDelivery::Event {
                            tab_id,
                            generation: 1,
                            event,
                        })
                        .unwrap();
                }
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();

            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some(format!("hidden-{protocol:?}"))
            );
            assert_eq!(
                events.try_recv().unwrap(),
                RemoteDesktopSessionEvent::ClipboardTransferFailed
            );

            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::TransportFailed {
                        tab_id,
                        generation: 1,
                        message: "hidden transport failed".to_string(),
                    })
                    .unwrap();
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();
            assert_eq!(
                cx.read(|cx| session.read(cx).state.snapshot().status),
                RemoteDesktopSessionStatus::Failed
            );

            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation: 1,
                        event: RemoteDesktopHelperEvent::Disconnected { reason: None },
                    })
                    .unwrap();
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();
            assert_eq!(
                cx.read(|cx| session.read(cx).state.snapshot().status),
                RemoteDesktopSessionStatus::Disconnected
            );

            let clipboard_path =
                std::env::temp_dir().join(format!("hidden-{protocol:?}-clipboard.txt"));
            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation: 1,
                        event: RemoteDesktopHelperEvent::ClipboardFilesReady {
                            transfer_id: "hidden-files".to_string(),
                            paths: vec![clipboard_path.clone()],
                        },
                    })
                    .unwrap();
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();
            let clipboard_item = cx.read_from_clipboard().unwrap();
            assert!(matches!(
                clipboard_item.entries().first(),
                Some(ClipboardEntry::ExternalPaths(paths))
                    if paths.paths() == std::slice::from_ref(&clipboard_path)
            ));

            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation: 1,
                        event: RemoteDesktopHelperEvent::ClipboardData {
                            data: RemoteDesktopClipboardData::new(
                                RemoteDesktopClipboardFormat::ImagePng,
                                vec![1, 2, 3, 4],
                            ),
                        },
                    })
                    .unwrap();
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();
            assert!(matches!(
                cx.read_from_clipboard()
                    .and_then(|item| item.entries.into_iter().next()),
                Some(ClipboardEntry::Image(_))
            ));
        }
    }

    #[gpui::test]
    fn established_network_failure_preserves_frame_until_retry_is_cancelled(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
        let tab_id = TabId(37);
        let profile = preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(RemoteDesktopProtocol::Rdp)
            .cloned()
            .unwrap();
        let session = cx.new(|_cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                tab_id,
                profile,
                provider,
                None,
                std::env::temp_dir().join("oxideterm-reconnect-test-certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_generation = 1;
            session.has_connected = true;
            session.state.apply_event(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    RemoteDesktopSize {
                        width: 2,
                        height: 1,
                    },
                    RemoteDesktopFrameFormat::Bgra8,
                    vec![1, 2, 3, 0xff, 4, 5, 6, 0xff],
                ),
            });
            session
        });

        window
            .update(cx, |_root, window, cx| {
                session.update(cx, |session, cx| {
                    session
                        .delivery_tx
                        .send(RemoteDesktopWorkerDelivery::Event {
                            tab_id,
                            generation: 1,
                            event: RemoteDesktopHelperEvent::ConnectionFailure {
                                message: "network interrupted".to_string(),
                                category: Some(RemoteDesktopErrorCategory::Network),
                            },
                        })
                        .unwrap();
                    session.poll_deliveries(window, cx);
                });
            })
            .unwrap();

        let reconnect_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
        assert_eq!(
            reconnect_snapshot.status,
            RemoteDesktopSessionStatus::Reconnecting
        );
        assert!(reconnect_snapshot.has_frame);
        assert!(cx.read(|cx| {
            session
                .read(cx)
                .automatic_reconnect_worker_generation
                .is_some()
        }));

        window
            .update(cx, |_root, window, cx| {
                session.update(cx, |session, cx| session.disconnect(window, cx));
            })
            .unwrap();

        assert_eq!(
            cx.read(|cx| session.read(cx).state.snapshot().status),
            RemoteDesktopSessionStatus::Disconnected
        );
        assert!(cx.read(|cx| {
            let session = session.read(cx);
            session.automatic_reconnect_worker_generation.is_none()
                && session.automatic_reconnect_task.is_none()
        }));
    }

    #[gpui::test]
    fn hidden_rdp_and_vnc_sessions_resume_with_latest_frame(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
        let frame_size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        for (tab_number, protocol) in [
            (41, RemoteDesktopProtocol::Rdp),
            (42, RemoteDesktopProtocol::Vnc),
        ] {
            let tab_id = TabId(tab_number);
            let profile = preview_remote_desktop_profile(protocol);
            let provider = builtin_preview_provider_registry()
                .unwrap()
                .get_for_protocol(protocol)
                .cloned()
                .unwrap();
            let frame_slot = RemoteDesktopFrameDeliverySlot::new();
            let session = cx.new(|_cx| {
                let mut session = RemoteDesktopSessionEntity::new(
                    tab_id,
                    profile,
                    provider,
                    None,
                    std::env::temp_dir().join(format!(
                        "oxideterm-hidden-frame-{}-test-certificates.json",
                        protocol.provider_id()
                    )),
                    frame_slot.clone(),
                    window.into(),
                );
                session.worker_generation = 1;
                session
            });

            session.update(cx, |session, cx| {
                session.set_frame_visibility(false, cx);
            });
            let frame_decision = frame_slot.push(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    frame_size,
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0; 8],
                ),
            });
            assert!(frame_decision.frame_ready);
            assert!(
                !frame_slot
                    .push(RemoteDesktopHelperEvent::FrameUpdate {
                        update: RemoteDesktopFrameUpdate::new(
                            frame_size,
                            RemoteDesktopRect::new(1, 0, 1, 1),
                            RemoteDesktopFrameFormat::Rgba8,
                            vec![9, 8, 7, 0xff],
                        ),
                    })
                    .frame_ready
            );
            session.update(cx, |session, cx| {
                session
                    .delivery_tx
                    .send(RemoteDesktopWorkerDelivery::FrameReady {
                        tab_id,
                        generation: 1,
                    })
                    .unwrap();
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();

            let hidden_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
            assert!(
                !hidden_snapshot.has_frame,
                "{protocol:?} uploaded while hidden"
            );
            assert_eq!(cx.read(|cx| session.read(cx).state.texture_generation()), 0);
            assert!(frame_slot.has_queued_frame_events());

            session.update(cx, |session, cx| {
                session.set_frame_visibility(true, cx);
                // The production subscription routes the emitted apply event
                // back to this window-affine delivery method.
                session.schedule_window_event(
                    RemoteDesktopSessionEvent::FrameApplyReady { generation: 1 },
                    cx,
                );
            });
            cx.run_until_parked();

            let visible_snapshot = cx.read(|cx| session.read(cx).state.snapshot());
            assert!(visible_snapshot.has_frame, "{protocol:?} did not resume");
            assert_eq!(visible_snapshot.size, Some(frame_size));
            assert_eq!(cx.read(|cx| session.read(cx).state.texture_generation()), 1);
            assert!(!frame_slot.has_queued_frame_events());
        }
    }

    #[gpui::test]
    fn repeated_shutdown_closes_only_once(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
        let protocol = RemoteDesktopProtocol::Rdp;
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let observed_wake = worker_wake.clone();
        let (request_tx, request_rx) = mpsc::channel();
        let session = cx.new(|_cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(12),
                profile,
                provider,
                Some(RemoteDesktopSecret::from("shutdown-test-secret")),
                std::env::temp_dir().join("oxideterm-shutdown-test-certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_wake = Some(worker_wake);
            session.worker = Some(RemoteDesktopWorkerOwner::new(
                request_tx,
                thread::spawn(|| {}),
            ));
            session
        });

        // Repeated close paths must not duplicate helper shutdown or retain
        // session-owned credentials.
        window
            .update(cx, |_root, window, cx| {
                session.update(cx, |session, cx| session.shutdown(window, cx));
                session.update(cx, |session, cx| session.shutdown(window, cx));
            })
            .unwrap();

        assert!(observed_wake.is_stopped());
        assert!(matches!(
            request_rx.recv().unwrap(),
            RemoteDesktopHelperRequest::ReleaseAllInputs
        ));
        assert!(matches!(
            request_rx.recv().unwrap(),
            RemoteDesktopHelperRequest::Close
        ));
        assert!(request_rx.try_recv().is_err());
        let password_consumed = cx.read(|cx| session.read(cx).password.is_none());
        assert!(password_consumed);
    }
}
