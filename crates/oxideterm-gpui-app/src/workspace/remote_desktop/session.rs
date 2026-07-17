// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn poll_remote_desktop_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scale_factor = Some(remote_desktop_scale_factor_percent(window.scale_factor()));
        let mut changed = self.schedule_remote_desktop_viewport_resizes(scale_factor, cx);
        while let Ok(delivery) = self.remote_desktop_worker_rx.try_recv() {
            match delivery {
                RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation } => {
                    if self.apply_remote_desktop_frame_ready(tab_id, generation, window, cx) {
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::FrameRecoveryRequired { tab_id, generation } => {
                    if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
                        continue;
                    }
                    // Queue saturation is an explicit continuity break. Ask
                    // the helper for one new base before accepting more deltas.
                    self.send_remote_desktop_request(
                        tab_id,
                        RemoteDesktopHelperRequest::RequestFrame,
                    );
                }
                RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation,
                    event,
                } => {
                    if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
                        continue;
                    }
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        match &event {
                            RemoteDesktopHelperEvent::ClipboardText { text } => {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }
                            RemoteDesktopHelperEvent::ClipboardData { data } => {
                                if let Some(item) = remote_desktop_clipboard_item_from_data(data) {
                                    cx.write_to_clipboard(item);
                                }
                            }
                            _ => {}
                        }
                        session.state.apply_event(event);
                        let retired_images = session.state.take_retired_images();
                        let retired_textures = session.state.take_retired_textures();
                        Self::drop_remote_desktop_images(retired_images, window, cx);
                        Self::drop_remote_desktop_textures(retired_textures, window);
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message,
                } => {
                    if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
                        continue;
                    }
                    if self.apply_remote_desktop_frame_ready(tab_id, generation, window, cx) {
                        changed = true;
                    }
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        session
                            .state
                            .apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
                                message,
                                category: Some(RemoteDesktopErrorCategory::Unknown),
                            });
                        let retired_images = session.state.take_retired_images();
                        let retired_textures = session.state.take_retired_textures();
                        Self::drop_remote_desktop_images(retired_images, window, cx);
                        Self::drop_remote_desktop_textures(retired_textures, window);
                        changed = true;
                    }
                }
            }
        }

        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn close_remote_desktop_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut session) = self.remote_desktop_sessions.remove(&tab_id) {
            let images = session.state.take_all_images();
            let textures = session.state.take_all_textures();
            Self::drop_remote_desktop_images(images, window, cx);
            Self::drop_remote_desktop_textures(textures, window);
            // The helper owns external resources. Always send a protocol-level
            // close before dropping the channel so real helpers can disconnect.
            if let Some(request_tx) = session.request_tx {
                let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
                let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
            }
        }
    }

    pub(in crate::workspace) fn release_remote_desktop_inputs_for_tab(&mut self, tab_id: TabId) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            session.last_input_modifiers = RemoteDesktopModifierState::default();
            session.last_lock_keys = None;
            session.pressed_mouse_buttons.clear();
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        }
        self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::ReleaseAllInputs);
    }

    pub(in crate::workspace) fn release_active_remote_desktop_inputs(&mut self) {
        if let Some(tab_id) = self.active_remote_desktop_tab_id() {
            self.release_remote_desktop_inputs_for_tab(tab_id);
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
        self.clear_ai_sidebar_keyboard_focus();
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
            self.sync_remote_desktop_lock_keys(tab_id, window.capslock());
        }
        window.focus(&self.focus_handle, cx);
        if changed {
            cx.notify();
        }
    }

    fn spawn_remote_desktop_worker(
        &self,
        tab_id: TabId,
        generation: u64,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        worker_wake: RemoteDesktopWorkerWake,
        initial_size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> mpsc::Sender<RemoteDesktopHelperRequest> {
        let (request_tx, request_rx) = mpsc::channel();
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(
                    tab_id,
                    generation,
                    profile,
                    provider,
                    password,
                    initial_size,
                    scale_factor,
                    frame_slot,
                    worker_wake,
                    request_rx,
                    delivery_tx,
                );
            })
            .expect("failed to start remote desktop worker");
        request_tx
    }

    fn schedule_remote_desktop_worker_wake(
        &self,
        tab_id: TabId,
        generation: u64,
        worker_wake: RemoteDesktopWorkerWake,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |workspace, cx| {
            loop {
                worker_wake.wait().await;
                let keep_running = workspace
                    .update(cx, |this, cx| {
                        if !this.remote_desktop_worker_generation_matches(tab_id, generation) {
                            return false;
                        }
                        if worker_wake.take() {
                            cx.notify();
                        }
                        !worker_wake.is_stopped()
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn render_remote_desktop_footer(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let snapshot = session.state.snapshot();
        let status = snapshot.status;
        let status_color = remote_desktop_status_color(&self.tokens, status);
        let reconnect_disabled = remote_desktop_reconnect_mode(status).is_none();
        let resize_capability_label = if session.provider.capabilities.resize {
            self.i18n.t("remote_desktop.resize_dynamic")
        } else {
            self.i18n.t("remote_desktop.resize_fixed")
        };
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
                            this.release_remote_desktop_inputs_for_tab(tab_id);
                            this.disconnect_remote_desktop(tab_id, window, cx);
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn force_recover_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_remote_desktop_inputs_for_tab(tab_id);
        let has_live_worker = self
            .remote_desktop_sessions
            .get(&tab_id)
            .is_some_and(|session| session.request_tx.is_some());
        if has_live_worker {
            self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::RequestFrame);
        }
        self.restart_remote_desktop_worker(tab_id, window, cx);
    }

    pub(in crate::workspace) fn send_remote_desktop_request(
        &mut self,
        tab_id: TabId,
        request: RemoteDesktopHelperRequest,
    ) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            if matches!(request, RemoteDesktopHelperRequest::Resize { .. })
                && !session.provider.capabilities.resize
            {
                return;
            }
            if let RemoteDesktopHelperRequest::Resize { size, .. } = &request {
                session.state.mark_resize_requested(*size);
            }
            if let Some(request_tx) = session.request_tx.as_ref() {
                let _ = request_tx.send(request);
            } else if matches!(request, RemoteDesktopHelperRequest::Close) {
                session
                    .state
                    .apply_event(RemoteDesktopHelperEvent::Disconnected { reason: None });
            }
        }
    }

    pub(in crate::workspace) fn disconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return;
        };
        if let Some(request_tx) = session.request_tx.as_ref() {
            let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
            return;
        }

        // When the helper channel is already gone, apply the same disconnected
        // state locally and release any frame images retired by the transition.
        session
            .state
            .apply_event(RemoteDesktopHelperEvent::Disconnected { reason: None });
        let retired_images = session.state.take_retired_images();
        let retired_textures = session.state.take_retired_textures();
        Self::drop_remote_desktop_images(retired_images, window, cx);
        Self::drop_remote_desktop_textures(retired_textures, window);
    }

    pub(in crate::workspace) fn reconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(status) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .map(|session| session.state.snapshot().status)
        else {
            return;
        };

        match remote_desktop_reconnect_mode(status) {
            Some(RemoteDesktopReconnectMode::ProtocolRequest) => {
                self.release_remote_desktop_inputs_for_tab(tab_id);
                self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Reconnect);
            }
            Some(RemoteDesktopReconnectMode::RestartHelper) => {
                self.release_remote_desktop_inputs_for_tab(tab_id);
                self.restart_remote_desktop_worker(tab_id, window, cx);
            }
            None => {}
        }
    }

    pub(in crate::workspace) fn restart_remote_desktop_worker(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((
            profile,
            provider,
            password,
            generation,
            initial_request_size,
            initial_viewport_size,
            scale_factor,
            old_request_tx,
        )) = self.remote_desktop_sessions.get(&tab_id).map(|session| {
            let (initial_request_size, initial_viewport_size) =
                initial_remote_desktop_sizes_for_session(session);
            (
                session.profile.clone(),
                session.provider.clone(),
                session.password.clone(),
                next_remote_desktop_worker_generation(session.worker_generation),
                initial_request_size,
                initial_viewport_size,
                session.last_viewport_scale_factor,
                session.request_tx.clone(),
            )
        })
        else {
            return;
        };
        if let Some(old_request_tx) = old_request_tx {
            let _ = old_request_tx.send(RemoteDesktopHelperRequest::Close);
        }

        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let request_tx = self.spawn_remote_desktop_worker(
            tab_id,
            generation,
            profile.clone(),
            provider,
            password,
            frame_slot.clone(),
            worker_wake.clone(),
            initial_request_size,
            scale_factor,
        );
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            let old_images = session.state.take_all_images();
            let old_textures = session.state.take_all_textures();
            session.state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
                .with_read_only(profile.read_only);
            session.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Reconnecting,
                message: None,
            });
            Self::drop_remote_desktop_images(old_images, window, cx);
            Self::drop_remote_desktop_textures(old_textures, window);
            session.frame_slot = frame_slot;
            session.request_tx = Some(request_tx);
            session.worker_generation = generation;
            session.last_viewport_size = initial_viewport_size;
            session.last_sent_resize = None;
            session.last_viewport_scale_factor = scale_factor;
            session.resize_generation = Arc::new(AtomicU64::new(0));
            session.last_lock_keys = None;
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        }
        // Register the generation before awaiting a possibly pre-signalled
        // wake so a fast helper cannot make the event task exit as stale.
        self.schedule_remote_desktop_worker_wake(tab_id, generation, worker_wake, cx);
    }

    pub(in crate::workspace) fn start_remote_desktop_worker_for_session(
        &mut self,
        tab_id: TabId,
        initial_request_size: RemoteDesktopSize,
        initial_viewport_size: Option<RemoteDesktopSize>,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((profile, provider, password, frame_slot, generation)) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| {
                if session.request_tx.is_some() {
                    return None;
                }
                Some((
                    session.profile.clone(),
                    session.provider.clone(),
                    session.password.clone(),
                    session.frame_slot.clone(),
                    next_remote_desktop_worker_generation(session.worker_generation),
                ))
            })
        else {
            return false;
        };

        let worker_wake = RemoteDesktopWorkerWake::default();
        let request_tx = self.spawn_remote_desktop_worker(
            tab_id,
            generation,
            profile,
            provider,
            password,
            frame_slot,
            worker_wake.clone(),
            initial_request_size,
            scale_factor,
        );
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            session.request_tx = Some(request_tx);
            session.worker_generation = generation;
            session.last_viewport_size = initial_viewport_size;
            session.last_sent_resize = None;
            session.last_viewport_scale_factor = scale_factor;
            session.last_lock_keys = None;
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
            session.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Connecting,
                message: None,
            });
            // Store the worker generation before the event-driven task can
            // consume a wake permit emitted during helper startup.
            self.schedule_remote_desktop_worker_wake(tab_id, generation, worker_wake, cx);
            return true;
        }
        false
    }

    pub(in crate::workspace) fn remote_desktop_worker_generation_matches(
        &self,
        tab_id: TabId,
        generation: u64,
    ) -> bool {
        self.remote_desktop_sessions
            .get(&tab_id)
            .is_some_and(|session| session.worker_generation == generation)
    }

    pub(in crate::workspace) fn schedule_remote_desktop_viewport_resizes(
        &mut self,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = false;
        let mut pending_starts = Vec::new();
        for (tab_id, session) in self.remote_desktop_sessions.iter_mut() {
            if let Some(scale_factor) = scale_factor {
                // The first viewport measurement happens during layout, after
                // render-time polling. Cache the window scale early so the
                // layout probe does not start RDP with logical pixels only.
                session.last_viewport_scale_factor = Some(scale_factor);
            }
            let snapshot = session.state.snapshot();
            let Some(viewport_size) = session.geometry.viewport_size() else {
                continue;
            };
            let viewport_size =
                RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
            let request_size = remote_desktop_requested_size_for_viewport(
                viewport_size,
                session.last_viewport_scale_factor,
            );
            let resize_request = RemoteDesktopResizeRequestState {
                size: request_size,
                scale_factor: session.last_viewport_scale_factor,
            };
            if session.request_tx.is_none() {
                if session.last_viewport_scale_factor.is_none() {
                    continue;
                }
                if matches!(
                    snapshot.status,
                    RemoteDesktopSessionStatus::Idle
                        | RemoteDesktopSessionStatus::Connecting
                        | RemoteDesktopSessionStatus::Reconnecting
                ) {
                    pending_starts.push((
                        *tab_id,
                        request_size,
                        Some(viewport_size),
                        session.last_viewport_scale_factor,
                    ));
                }
                continue;
            }
            if snapshot.status != RemoteDesktopSessionStatus::Connected {
                continue;
            }
            let should_send_resize = remote_desktop_resize_request_needed_for_capability(
                session.provider.capabilities.resize,
                snapshot.size,
                snapshot.pending_resize,
                session.last_viewport_size,
                session.last_sent_resize,
                viewport_size,
                request_size,
                session.last_viewport_scale_factor,
            );
            if Some(viewport_size) == session.last_viewport_size && !should_send_resize {
                continue;
            }
            session.last_viewport_size = Some(viewport_size);
            if !should_send_resize {
                continue;
            }

            session.last_sent_resize = Some(resize_request);
            session.state.mark_resize_requested(request_size);
            changed = true;

            let generation = session.resize_generation.fetch_add(1, Ordering::Relaxed) + 1;
            let resize_generation = session.resize_generation.clone();
            let Some(request_tx) = session.request_tx.clone() else {
                continue;
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
        }
        for (tab_id, request_size, viewport_size, scale_factor) in pending_starts {
            changed |= self.start_remote_desktop_worker_for_session(
                tab_id,
                request_size,
                viewport_size,
                scale_factor,
                cx,
            );
        }
        changed
    }

    pub(in crate::workspace) fn schedule_remote_desktop_initial_layout_probe(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |workspace, cx| {
            for _ in 0..REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS {
                Timer::after(REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL).await;
                let done = workspace
                    .update(cx, |this, cx| {
                        let Some(session) = this.remote_desktop_sessions.get(&tab_id) else {
                            return true;
                        };
                        if session.request_tx.is_some() {
                            return true;
                        }

                        // The viewport probe runs during layout, after the
                        // render-time worker poll. Nudge the workspace briefly
                        // so a measured first viewport can start the helper
                        // without waiting for an unrelated repaint.
                        if this.schedule_remote_desktop_viewport_resizes(None, cx) {
                            cx.notify();
                        }

                        this.remote_desktop_sessions
                            .get(&tab_id)
                            .map(|session| session.request_tx.is_some())
                            .unwrap_or(true)
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn apply_remote_desktop_frame_ready(
        &mut self,
        tab_id: TabId,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
            return false;
        }
        let Some(frame_slot) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .map(|session| session.frame_slot.clone())
        else {
            return false;
        };
        let delay = frame_slot.next_frame_ready_delay();
        if !delay.is_zero() {
            self.schedule_remote_desktop_pending_frame_ready(tab_id, generation, delay, cx);
            return false;
        }
        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return false;
        };
        let mut changed = false;
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
            // Apply a bounded, time-budgeted batch so ordinary dirty bursts can
            // catch up without letting large image uploads monopolize GPUI.
            events.push(event);
            changed = true;
        }
        let drained_events = events.len();
        if drained_events == 0 {
            frame_slot.complete_delivery();
            return false;
        }
        frame_slot.mark_frame_presented();
        let apply_started_at = Instant::now();
        let apply_stats = session.state.apply_frame_events(events);
        let apply_elapsed = apply_started_at.elapsed();
        let retired_images = session.state.take_retired_images();
        let retired_textures = session.state.take_retired_textures();
        let retired_image_count = retired_images.len();
        session.render_diagnostics.record_batch(
            drained_events,
            budget_hit,
            apply_elapsed,
            apply_stats,
            retired_image_count,
        );
        if remote_desktop_diagnostics_enabled() {
            eprintln!(
                "[oxideterm:remote-desktop-render] tab={tab_id:?} protocol={:?} provider={} resize={} clipboard_data={} gen={generation} trace={:?}->{:?} drained={drained_events} budget_hit={budget_hit} apply_us={} full={} updates={} dirty_applied={} dirty_rejected={} dirty_px={} dirty_frame_px={} pending_texture_updates={} pending_texture_bytes={} texture_updates={} textures_created={} retired={} full_update_recoveries={} totals={:?}",
                session.profile.protocol,
                session.provider.id,
                session.provider.capabilities.resize,
                session.provider.capabilities.clipboard_data,
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
                session.render_diagnostics,
            );
        }
        Self::drop_remote_desktop_images(retired_images, window, cx);
        Self::drop_remote_desktop_textures(retired_textures, window);
        if frame_slot.complete_delivery() {
            self.schedule_remote_desktop_followup_frame_ready(tab_id, generation, frame_slot, cx);
        }
        changed
    }

    pub(in crate::workspace) fn schedule_remote_desktop_pending_frame_ready(
        &self,
        tab_id: TabId,
        generation: u64,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        // The slot is already marked as queued. This timer only delays the
        // existing ready notification until the next visual presentation tick.
        cx.spawn(async move |workspace, cx| {
            Timer::after(delay).await;
            let _ = workspace.update(cx, |this, cx| {
                if !this.remote_desktop_worker_generation_matches(tab_id, generation) {
                    return;
                }
                let _ = this
                    .remote_desktop_worker_tx
                    .send(RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation });
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_remote_desktop_followup_frame_ready(
        &self,
        tab_id: TabId,
        generation: u64,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        cx: &mut Context<Self>,
    ) {
        if !frame_slot.mark_frame_ready_queued() {
            return;
        }

        let delay = frame_slot.next_frame_ready_delay();
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        if delay.is_zero() {
            let _ =
                delivery_tx.send(RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation });
            cx.notify();
            return;
        }

        cx.spawn(async move |workspace, cx| {
            Timer::after(delay).await;
            let _ = workspace.update(cx, |this, cx| {
                if !this.remote_desktop_worker_generation_matches(tab_id, generation) {
                    return;
                }
                // The queued flag stays set while this timer waits, so new
                // frame bursts coalesce into the existing delivery slot.
                let _ = this
                    .remote_desktop_worker_tx
                    .send(RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation });
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn drop_remote_desktop_images(
        images: Vec<Arc<gpui::RenderImage>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for image in images {
            // Remote desktop tiles are replaced continuously; GPUI keeps painted
            // images in the sprite atlas until the app explicitly drops them.
            cx.drop_image(image, Some(window));
        }
    }

    pub(in crate::workspace) fn drop_remote_desktop_textures(
        textures: Vec<Arc<gpui::DynamicTexture>>,
        window: &mut Window,
    ) {
        for texture in textures {
            let _ = window.drop_dynamic_texture(texture);
        }
    }
}
