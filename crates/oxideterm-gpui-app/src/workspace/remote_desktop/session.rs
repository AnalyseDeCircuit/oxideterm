// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl RemoteDesktopSessionEntity {
    pub(super) fn install_release_handler(&self, cx: &mut Context<Self>) {
        cx.on_release(|session, _cx| {
            // Entity destruction owns helper shutdown, but never shared SSH
            // nodes, SFTP sessions, or forwarding runtimes.
            if let Some(worker_wake) = session.worker_wake.take() {
                worker_wake.stop();
            }
            if let Some(request_tx) = session.request_tx.take() {
                let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
                let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
            }
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

    fn shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(worker_wake) = self.worker_wake.take() {
            worker_wake.stop();
        }
        if let Some(request_tx) = self.request_tx.take() {
            let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
            let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
        }
        drop(self.password.take());
        let images = self.state.take_all_images();
        let textures = self.state.take_all_textures();
        self.drop_images(images, window, cx);
        Self::drop_textures(textures, window);
    }

    fn disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(request_tx) = self.request_tx.as_ref() {
            let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
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
        self.release_inputs();
        if self.request_tx.is_some() {
            self.send_request(RemoteDesktopHelperRequest::RequestFrame);
        }
        self.restart_worker(window, cx);
    }

    fn reconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        visible: bool,
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
                    if visible && self.apply_frame_ready(generation, window, cx) {
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
                    if let Some(request_tx) = self.request_tx.as_ref() {
                        let _ = request_tx.send(RemoteDesktopHelperRequest::RequestFrame);
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
                            // Move clipboard content directly to the platform
                            // boundary instead of cloning it through workspace state.
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardData { data }
                            if self.profile.session_options.clipboard.images =>
                        {
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
                        event => {
                            self.state.apply_event(event);
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
                    if visible {
                        let _ = self.apply_frame_ready(generation, window, cx);
                    }
                    self.state
                        .apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
                            message,
                            category: Some(RemoteDesktopErrorCategory::Unknown),
                        });
                    let retired_images = self.state.take_retired_images();
                    let retired_textures = self.state.take_retired_textures();
                    self.drop_images(retired_images, window, cx);
                    Self::drop_textures(retired_textures, window);
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
        if self.worker_generation != generation {
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
    ) -> mpsc::Sender<RemoteDesktopHelperRequest> {
        let tab_id = self.tab_id;
        let (request_tx, request_rx) = mpsc::channel();
        thread::Builder::new()
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
        request_tx
    }

    fn start_worker(
        &mut self,
        initial_request_size: RemoteDesktopSize,
        initial_viewport_size: Option<RemoteDesktopSize>,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_tx.is_some() {
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
        let request_tx = self.spawn_worker(
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

        self.request_tx = Some(request_tx);
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
        if let Some(old_request_tx) = self.request_tx.take() {
            let _ = old_request_tx.send(RemoteDesktopHelperRequest::Close);
        }

        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let monitor_layout = remote_desktop_monitor_layout(&profile, cx);
        let request_tx = self.spawn_worker(
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
        let old_images = self.state.take_all_images();
        let old_textures = self.state.take_all_textures();
        self.state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        self.state.apply_event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Reconnecting,
            message: None,
        });
        self.frame_slot = frame_slot;
        self.request_tx = Some(request_tx);
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
        if let Some(request_tx) = self.request_tx.as_ref() {
            let _ = request_tx.send(RemoteDesktopHelperRequest::UpdateDisplayLayout {
                layout: layout.clone(),
            });
        }
        self.last_monitor_layout = layout;
    }

    fn schedule_viewport_resize(
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
        if self.request_tx.is_none() {
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
        let should_send_resize = remote_desktop_resize_request_needed_for_capability(
            self.provider.capabilities.resize,
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
        let Some(request_tx) = self.request_tx.clone() else {
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
        if self.request_tx.is_some() {
            return;
        }

        cx.spawn(async move |session, cx| {
            for _ in 0..REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS {
                Timer::after(REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL).await;
                let done = session
                    .update(cx, |session, cx| {
                        if session.request_tx.is_some() {
                            return true;
                        }
                        // The worker must start from the measured viewport even
                        // before it has any delivery capable of waking the UI.
                        if session.schedule_viewport_resize(None, cx) {
                            cx.notify();
                        }
                        session.request_tx.is_some()
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_window_event(
        &self,
        event: RemoteDesktopSessionEvent,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |session, cx| {
            let generation = match event {
                RemoteDesktopSessionEvent::DeliveryReady { generation }
                | RemoteDesktopSessionEvent::FrameApplyReady { generation } => generation,
                RemoteDesktopSessionEvent::ClipboardTransferFailed => return,
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
                            let scale_factor =
                                Some(remote_desktop_scale_factor_percent(window.scale_factor()));
                            let mut outcome = session.poll_deliveries(visible, window, cx);
                            outcome.changed |= session.schedule_viewport_resize(scale_factor, cx);
                            session.sync_monitor_layout(cx);
                            if outcome.changed {
                                cx.notify();
                            }
                            for intent in outcome.intents {
                                match intent {
                                    RemoteDesktopDeliveryIntent::ClipboardTransferFailed => {
                                        cx.emit(RemoteDesktopSessionEvent::ClipboardTransferFailed);
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
                            if visible && session.apply_frame_ready(generation, window, cx) {
                                cx.notify();
                            }
                        }
                        RemoteDesktopSessionEvent::ClipboardTransferFailed => {}
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
        if matches!(event, RemoteDesktopSessionEvent::ClipboardTransferFailed) {
            self.push_command_palette_toast(
                self.i18n.t("remote_desktop.clipboard_file_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }

        let visible = self.remote_desktop_tab_visible(tab_id, cx);
        // The root supplies cross-tab visibility, then the Entity owns the
        // window-affine delivery and lifecycle transition.
        session_entity.update(cx, |session, cx| {
            session.schedule_window_event(*event, visible, cx);
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
        let main_tab_visible = self.main_window_tabs.active_tab_id == Some(tab_id)
            && !tab_host.is_outside_main_window(tab_id);
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
        let has_queued_frame = session_entity.read(cx).frame_slot.has_queued_frame_events();
        if has_queued_frame {
            // Hidden tabs retain a coalesced frame slot. Visibility resumes that existing
            // delivery without restarting or disconnecting the remote session.
            session_entity.update(cx, |session, cx| {
                session.schedule_frame_apply(session.worker_generation, Duration::ZERO, cx);
            });
        }
    }

    pub(in crate::workspace) fn close_remote_desktop_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.shutdown(window, cx));
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
        if let Some(tab_id) = self.active_remote_desktop_tab_id() {
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

        let ai_focus_changed = self.ai.chat.input_focused
            || self.ai.chat.footer_focus.is_some()
            || self.ai.models.selector_open
            || self.ai.models.selector_search_focused;
        self.clear_ai_sidebar_keyboard_focus(cx);
        changed |= ai_focus_changed;

        if self.terminal_command_bar_focused
            || self.terminal_command_suggestions_open
            || self.terminal_command_suggestion_highlighted.is_some()
        {
            // Remote desktop clicks are a keyboard ownership boundary. Clear
            // Workspace-local text owners so Enter and IME control keys route
            // to the helper after the surface gains focus.
            self.terminal_command_bar_focused = false;
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
            changed = true;
        }

        if let Some(tab_id) = self.active_remote_desktop_tab_id() {
            self.sync_remote_desktop_lock_keys(tab_id, window.capslock(), cx);
        }
        window.focus(&self.focus_handle, cx);
        if changed {
            cx.notify();
        }
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
        let resize_capability_label = match snapshot
            .negotiated_capabilities
            .as_ref()
            .map(|capabilities| capabilities.resize)
        {
            Some(NegotiatedCapabilityStatus::Supported) => {
                self.i18n.t("remote_desktop.resize_dynamic")
            }
            Some(NegotiatedCapabilityStatus::Unsupported) => {
                self.i18n.t("remote_desktop.resize_fixed")
            }
            Some(NegotiatedCapabilityStatus::Unknown) => {
                self.i18n.t("remote_desktop.resize_unknown")
            }
            None if snapshot.protocol == RemoteDesktopProtocol::Vnc => {
                self.i18n.t("remote_desktop.resize_unknown")
            }
            None if session.provider.capabilities.resize => {
                self.i18n.t("remote_desktop.resize_dynamic")
            }
            None => self.i18n.t("remote_desktop.resize_fixed"),
        };
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
            .child(remote_desktop_capability_chip(
                &self.tokens,
                resize_capability_label,
            ))
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
                        session.request_tx.is_some(),
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
    fn hidden_session_still_applies_reliable_lifecycle_delivery(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopSessionTestRoot);
        let protocol = RemoteDesktopProtocol::Rdp;
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let session = cx.new(|_cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(11),
                profile,
                provider,
                None,
                std::env::temp_dir().join("oxideterm-hidden-test-certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_generation = 1;
            session
        });

        // Hidden sessions suppress frame uploads, not reliable lifecycle events.
        session.update(cx, |session, cx| {
            session
                .delivery_tx
                .send(RemoteDesktopWorkerDelivery::Event {
                    tab_id: TabId(11),
                    generation: 1,
                    event: RemoteDesktopHelperEvent::Disconnected { reason: None },
                })
                .unwrap();
            session.schedule_window_event(
                RemoteDesktopSessionEvent::DeliveryReady { generation: 1 },
                false,
                cx,
            );
        });
        cx.run_until_parked();

        let status = cx.read(|cx| session.read(cx).state.snapshot().status);
        assert_eq!(status, RemoteDesktopSessionStatus::Disconnected);
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
            session.request_tx = Some(request_tx);
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
