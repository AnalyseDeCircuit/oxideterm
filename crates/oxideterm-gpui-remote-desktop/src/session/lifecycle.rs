// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl RemoteDesktopSessionEntity {
    fn refresh_plugin_provider(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(lookup) = self.provider_lookup.as_ref() else {
            return true;
        };
        let provider = lookup(&self.provider.id, cx);
        if let Some(provider) = provider {
            self.provider = provider;
            true
        } else {
            self.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Disconnected,
                message: None,
            });
            cx.emit(RemoteDesktopSessionEvent::PluginRequired {
                protocol: self.profile.protocol,
            });
            cx.notify();
            false
        }
    }

    pub fn audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        self.audit_context.clone()
    }

    fn start_connection_audit(
        &mut self,
        action: &str,
        source: Option<oxideterm_audit::AuditSource>,
    ) {
        if action == "desktop_connect" {
            if let Some(audit) = self
                .ssh_tunnel
                .as_mut()
                .and_then(|lease| lease.connect_audit.take())
            {
                self.connect_audit = Some(audit);
                return;
            }
        }
        self.connect_audit = desktop_connection_audit(self.audit_context.as_ref(), action, source);
    }

    fn finish_connection_audit(&mut self, outcome: oxideterm_audit::AuditOutcome) {
        if let Some(audit) = self.connect_audit.take() {
            audit.finish(
                outcome,
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
        }
    }

    pub fn observe_desktop(
        &self,
        category: oxideterm_audit::AuditCategory,
        action: &str,
        detail: &str,
        outcome: oxideterm_audit::AuditOutcome,
        bytes: Option<u64>,
    ) {
        if let Some(context) = self.audit_context.as_ref() {
            let context = context.for_request();
            let mut audit = context.operation(category, action, Some(detail));
            let authorization = if matches!(
                context.source,
                oxideterm_audit::AuditSource::Mcp | oxideterm_audit::AuditSource::Plugin
            ) {
                oxideterm_audit::AuditAuthorization::Approved
            } else {
                oxideterm_audit::AuditAuthorization::NotRequired
            };
            audit.authorization(authorization, None);
            audit.finish(
                outcome,
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                bytes,
            );
        }
    }
    pub fn install_release_handler(&self, cx: &mut Context<Self>) {
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
            session.credential_prompt_task.take();
        })
        .detach();
    }

    pub fn bind_window(&mut self, window_handle: AnyWindowHandle) {
        let window_changed = self.window_handle != window_handle;
        self.window_handle = window_handle;
        if window_changed {
            self.clipboard_activation = None;
            self.resize_anchor = None;
        }
        if window_changed && let Some(worker_wake) = self.worker_wake.as_ref() {
            // A wake may have targeted the old window during handoff. Store
            // one fresh permit without polling render.
            worker_wake.mark();
        }
    }

    pub(super) fn shutdown_worker(&mut self) {
        self.interrupt_gesture();
        self.last_clipboard_files = None;
        self.reset_vnc_file_browser_connection();
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
    }

    pub(super) fn cancel_automatic_reconnect(&mut self) {
        self.automatic_reconnect_worker_generation = None;
        self.automatic_reconnect_attempt = 0;
        drop(self.automatic_reconnect_task.take());
    }

    fn mark_connection_established(&mut self) {
        self.finish_connection_audit(oxideterm_audit::AuditOutcome::Succeeded);
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
                    session.restart_worker_preserving_frame(
                        oxideterm_audit::AuditSource::System,
                        window,
                        cx,
                    );
                    cx.notify();
                });
            });
        });
        self.automatic_reconnect_task = Some(reconnect_task);
    }

    pub fn shutdown(&mut self, mut window: Option<&mut Window>, cx: &mut Context<Self>) {
        let had_worker = self.worker.is_some();
        if self.connect_audit.is_some() {
            self.finish_connection_audit(oxideterm_audit::AuditOutcome::Cancelled);
        }
        self.cancel_automatic_reconnect();
        if let Some(worker_wake) = self.worker_wake.take() {
            worker_wake.stop();
        }
        self.shutdown_worker();
        if had_worker {
            self.observe_desktop(
                oxideterm_audit::AuditCategory::Connection,
                "desktop_disconnect",
                "helper close requested",
                oxideterm_audit::AuditOutcome::Sent,
                None,
            );
        }
        self.public_mcp_clipboard = None;
        drop(self.ssh_tunnel.take());
        drop(self.password.take());
        self.credential_prompt_task.take();
        let images = self.state.take_all_images();
        let textures = self.state.take_all_textures();
        for image in images {
            cx.drop_image(image, window.as_deref_mut());
        }
        // A destroyed window has already released its texture atlas.
        if let Some(window) = window {
            Self::drop_textures(textures, window);
        }
    }

    fn poll_deliveries(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RemoteDesktopDeliveryOutcome {
        let drain = delivery::drain_channel(&self.delivery_rx, REMOTE_DESKTOP_DELIVERY_BUDGET);
        let mut changed = false;
        let mut frame_changed = false;
        let mut intents = Vec::new();
        for delivery in drain.items {
            match delivery {
                RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation } => {
                    debug_assert_eq!(tab_id, self.tab_id);
                    if self.frame_slot.is_visible()
                        && self.apply_frame_ready(generation, window, cx)
                    {
                        frame_changed = true;
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
                            self.observe_desktop(
                                oxideterm_audit::AuditCategory::File,
                                "desktop_clipboard_transfer",
                                "remote_to_local text",
                                oxideterm_audit::AuditOutcome::Succeeded,
                                Some(text.len() as u64),
                            );
                            // Keep a session-scoped zeroizing copy for explicitly authorized
                            // Public MCP reads; the platform clipboard remains the UI boundary.
                            self.public_mcp_clipboard = Some(RemoteDesktopPublicClipboard::Text(
                                Zeroizing::new(text.clone()),
                            ));
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            self.last_clipboard_files = None;
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardData { data }
                            if self.profile.session_options.clipboard.images =>
                        {
                            self.observe_desktop(
                                oxideterm_audit::AuditCategory::File,
                                "desktop_clipboard_transfer",
                                "remote_to_local image",
                                oxideterm_audit::AuditOutcome::Succeeded,
                                Some(data.bytes.len() as u64),
                            );
                            self.public_mcp_clipboard = Some(RemoteDesktopPublicClipboard::Image {
                                format: data.format,
                                bytes: Zeroizing::new(data.bytes.clone()),
                            });
                            if let Some(item) = remote_desktop_clipboard_item_from_data(data) {
                                cx.write_to_clipboard(item);
                                self.last_clipboard_files = None;
                            }
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardFilesReady { paths, .. }
                            if self.profile.session_options.clipboard.files =>
                        {
                            let path_hash = gpui::hash(&paths);
                            self.observe_desktop(
                                oxideterm_audit::AuditCategory::File,
                                "desktop_file_transfer",
                                &format!("remote_to_local files={}", paths.len()),
                                oxideterm_audit::AuditOutcome::Sent,
                                None,
                            );
                            cx.write_to_clipboard(ClipboardItem {
                                entries: vec![ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                                    paths.into(),
                                ))],
                            });
                            self.last_clipboard_files =
                                Some((path_hash, cx.clipboard_sequence_number()));
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ClipboardTransferFailed {
                            transfer_id, ..
                        } => {
                            if let Some(audit) = self.file_audits.remove(&transfer_id) {
                                audit.finish(
                                    oxideterm_audit::AuditOutcome::Failed,
                                    oxideterm_audit::AuditEvidence::Protocol,
                                    None,
                                    None,
                                );
                            }
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
                            match &event {
                                RemoteDesktopHelperEvent::VncFileTransferCompleted {
                                    transfer_id,
                                    paths,
                                    skipped_files,
                                    ..
                                } => {
                                    if let Some(mut audit) = self.file_audits.remove(transfer_id) {
                                        let bytes = self
                                            .vnc_files
                                            .transfer
                                            .as_ref()
                                            .filter(|transfer| &transfer.transfer_id == transfer_id)
                                            .map(|transfer| transfer.transferred_bytes);
                                        audit.summary(&format!(
                                            "remote_to_local files={} skipped={skipped_files}",
                                            paths.len()
                                        ));
                                        audit.finish(
                                            oxideterm_audit::AuditOutcome::Succeeded,
                                            oxideterm_audit::AuditEvidence::Protocol,
                                            None,
                                            bytes,
                                        );
                                    }
                                }
                                RemoteDesktopHelperEvent::VncFileTransferFailed {
                                    transfer_id,
                                    kind,
                                } => {
                                    if let Some(audit) = self.file_audits.remove(transfer_id) {
                                        let bytes = self
                                            .vnc_files
                                            .transfer
                                            .as_ref()
                                            .filter(|transfer| &transfer.transfer_id == transfer_id)
                                            .map(|transfer| transfer.transferred_bytes);
                                        audit.finish(
                                            if *kind
                                                == RemoteDesktopFileTransferFailureKind::Canceled
                                            {
                                                oxideterm_audit::AuditOutcome::Cancelled
                                            } else {
                                                oxideterm_audit::AuditOutcome::Failed
                                            },
                                            oxideterm_audit::AuditEvidence::Protocol,
                                            None,
                                            bytes,
                                        );
                                    }
                                }
                                _ => {}
                            }
                            if let Some(intent) = self.apply_vnc_file_event(event) {
                                intents.push(intent);
                            }
                            changed = true;
                        }
                        RemoteDesktopHelperEvent::ConnectionFailure { message, category } => {
                            self.finish_connection_audit(oxideterm_audit::AuditOutcome::Failed);
                            if self.has_connected
                                && self.automatic_reconnect_worker_generation != Some(generation)
                            {
                                self.observe_desktop(
                                    oxideterm_audit::AuditCategory::Connection,
                                    "desktop_disconnect",
                                    "connection failed",
                                    oxideterm_audit::AuditOutcome::Failed,
                                    None,
                                );
                            }
                            if category == Some(RemoteDesktopErrorCategory::Authentication)
                                && self.password.is_none()
                                && self.credential_prompt_generation != Some(generation)
                            {
                                self.credential_prompt_generation = Some(generation);
                                intents.push(RemoteDesktopDeliveryIntent::CredentialsRequired {
                                    generation,
                                });
                            }
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
                            self.finish_connection_audit(oxideterm_audit::AuditOutcome::Failed);
                            if self.has_connected
                                && self.state.snapshot().status
                                    != RemoteDesktopSessionStatus::Disconnected
                            {
                                self.observe_desktop(
                                    oxideterm_audit::AuditCategory::Connection,
                                    "desktop_disconnect",
                                    "helper terminated",
                                    oxideterm_audit::AuditOutcome::Unknown,
                                    None,
                                );
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
                            let disconnected = matches!(
                                &event,
                                RemoteDesktopHelperEvent::Disconnected { .. }
                                    | RemoteDesktopHelperEvent::Status {
                                        status: RemoteDesktopSessionStatus::Disconnected,
                                        ..
                                    }
                            );
                            if disconnected {
                                self.finish_connection_audit(oxideterm_audit::AuditOutcome::Failed);
                                if self.state.snapshot().status
                                    == RemoteDesktopSessionStatus::Connected
                                {
                                    self.observe_desktop(
                                        oxideterm_audit::AuditCategory::Connection,
                                        "desktop_disconnect",
                                        "helper disconnected",
                                        oxideterm_audit::AuditOutcome::Succeeded,
                                        None,
                                    );
                                }
                            }
                            self.state.apply_event(event);
                            if connection_established {
                                self.mark_connection_established();
                                if window.is_window_active() {
                                    self.sync_local_file_clipboard(cx);
                                }
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
                    self.finish_connection_audit(oxideterm_audit::AuditOutcome::Failed);
                    if self.has_connected
                        && self.automatic_reconnect_worker_generation != Some(generation)
                    {
                        self.observe_desktop(
                            oxideterm_audit::AuditCategory::Connection,
                            "desktop_disconnect",
                            "transport failed",
                            oxideterm_audit::AuditOutcome::Failed,
                            None,
                        );
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
            changed: changed || frame_changed,
            backlog_remaining: drain.outcome.backlog_remaining,
            intents,
        }
    }

    pub(super) fn apply_frame_ready(
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
        let previous_status = self.state.status();
        let apply_stats = self.state.apply_frame_events(events);
        let status = self.state.status();
        if status == RemoteDesktopSessionStatus::Connected {
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
        // The session is also read by the connection sidebar, including when
        // its desktop is detached. Frames must only invalidate the mounted panel.
        if status != previous_status {
            cx.notify();
        } else if let Some(panel) = &self.panel {
            App::notify(cx, panel.entity_id());
        }
        true
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
        if !self.refresh_plugin_provider(cx) {
            return false;
        }
        self.start_connection_audit("desktop_connect", None);
        let profile = self.profile.clone();
        let provider = self.provider.clone();
        let password_available = self.password.is_some();
        let frame_slot = self.frame_slot.clone();
        let delivery_tx = self.delivery_tx.clone();
        let generation = next_remote_desktop_worker_generation(self.worker_generation);
        let worker_wake = RemoteDesktopWorkerWake::default();
        let monitor_layout = remote_desktop_monitor_layout(&profile, cx);
        let worker = RemoteDesktopWorkerOwner::spawn(
            self.tab_id,
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

    pub(super) fn restart_worker_preserving_frame(
        &mut self,
        source: oxideterm_audit::AuditSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.refresh_plugin_provider(cx) {
            return;
        }
        self.start_connection_audit("desktop_reconnect", Some(source));
        let (initial_request_size, initial_viewport_size) =
            initial_remote_desktop_sizes_for_session(self);
        let profile = self.profile.clone();
        let provider = self.provider.clone();
        let password_available = self.password.is_some();
        let generation = next_remote_desktop_worker_generation(self.worker_generation);
        let scale_factor = self.last_viewport_scale_factor;
        self.shutdown_worker();

        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        // Helper replacement preserves presentation visibility independently
        // from the worker lifetime.
        frame_slot.set_visible(self.frame_slot.is_visible());
        let worker_wake = RemoteDesktopWorkerWake::default();
        let monitor_layout = remote_desktop_monitor_layout(&profile, cx);
        let worker = RemoteDesktopWorkerOwner::spawn(
            self.tab_id,
            generation,
            profile,
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
        // Automatic transport recovery keeps the last complete frame but starts
        // the replacement helper with a fresh graphics epoch.
        self.state.begin_transport_reconnect();
        let old_images = self.state.take_retired_images();
        let old_textures = self.state.take_retired_textures();
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
        if !self.profile.session_options.display.use_all_monitors
            || !self.provider.capabilities.multi_monitor
        {
            return;
        }
        let layout = remote_desktop_monitor_layout(&self.profile, cx);
        self.apply_monitor_layout(layout, cx);
    }

    pub(super) fn uses_monitor_layout(&self) -> bool {
        self.profile.session_options.display.use_all_monitors
            && self.provider.capabilities.multi_monitor
            && self.last_monitor_layout.monitors.len() > 1
    }

    fn apply_monitor_layout(&mut self, layout: RemoteDesktopMonitorLayout, cx: &mut Context<Self>) {
        // A transient empty topology during hotplug must not erase the last usable desktop.
        if layout.monitors.is_empty() {
            return;
        }
        if layout == self.last_monitor_layout {
            return;
        }
        self.resize_generation.fetch_add(1, Ordering::Relaxed);
        self.last_sent_resize = None;
        let mut requested = layout.clone();
        if requested.monitors.len() == 1 {
            let Some(viewport) = self.geometry.viewport_size().or(self.last_viewport_size) else {
                return;
            };
            let size = remote_desktop_requested_size_for_viewport(
                viewport,
                self.last_viewport_scale_factor,
            );
            let monitor = &mut requested.monitors[0];
            monitor.width = size.width.clamp(
                RemoteDesktopSize::MIN_WIDTH,
                RemoteDesktopSize::MAX_DIMENSION,
            ) & !1;
            monitor.height = size.height.clamp(
                RemoteDesktopSize::MIN_WIDTH,
                RemoteDesktopSize::MAX_DIMENSION,
            );
            monitor.left = 0;
            monitor.top = 0;
            monitor.primary = true;
            monitor.desktop_scale_factor = self.last_viewport_scale_factor.unwrap_or(100);
            monitor.physical_width_mm = None;
            monitor.physical_height_mm = None;
            monitor.orientation = if monitor.width >= monitor.height {
                RemoteDesktopMonitorOrientation::Landscape
            } else {
                RemoteDesktopMonitorOrientation::Portrait
            };
        }
        if let Some(worker) = self.worker.as_ref() {
            worker.send(RemoteDesktopHelperRequest::UpdateDisplayLayout { layout: requested });
        }
        self.last_monitor_layout = layout;
        cx.notify();
    }

    pub(super) fn resize_capability(&self) -> NegotiatedCapabilityStatus {
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
        if self.uses_monitor_layout()
            || self.state.snapshot().status != RemoteDesktopSessionStatus::Connected
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

    pub(super) fn fit_current_viewport_immediately(
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

    pub(super) fn set_follow_window_size(
        &mut self,
        enabled: bool,
        scale_factor: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.uses_monitor_layout() || self.follow_window_size == enabled {
            return false;
        }
        self.follow_window_size = enabled;
        if enabled {
            self.fit_current_viewport_immediately(scale_factor, cx);
        }
        true
    }

    pub(super) fn request_preset_size(
        &mut self,
        size: RemoteDesktopSize,
        scale_factor: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.uses_monitor_layout() {
            return false;
        }
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
        // Local layout passes also run without incoming RDP frames (for example after hotplug).
        if self.worker.is_some() {
            self.sync_monitor_layout(cx);
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
        if self.uses_monitor_layout() || !self.follow_window_size {
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

    pub fn schedule_initial_layout_probe(
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

    pub(super) fn schedule_window_event(
        &self,
        event: DesktopWindowDelivery,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |session, cx| {
            let generation = match event {
                DesktopWindowDelivery::Mailbox(generation)
                | DesktopWindowDelivery::Frame(generation) => generation,
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
                        DesktopWindowDelivery::Mailbox(_) => {
                            let outcome = session.poll_deliveries(window, cx);
                            if outcome.changed {
                                cx.notify();
                            }
                            for intent in outcome.intents {
                                match intent {
                                    RemoteDesktopDeliveryIntent::CredentialsRequired {
                                        generation,
                                    } => {
                                        cx.emit(RemoteDesktopSessionEvent::CredentialsRequired {
                                            generation,
                                        });
                                    }
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
                        DesktopWindowDelivery::Frame(_) => {
                            session.apply_frame_ready(generation, window, cx);
                        }
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

    pub fn set_frame_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.ui_frame_visible = visible;
        if !visible {
            self.clipboard_activation = None;
        }
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
            let desktop_scale_factor = remote_desktop_monitor_scale_percent(display.scale_factor());
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
mod tests;
