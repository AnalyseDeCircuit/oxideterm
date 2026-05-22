fn tab_background_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::FileManager => "file_manager",
        TabKind::Launcher => "launcher",
        TabKind::Graphics => "graphics",
        TabKind::ConnectionPool => "connection_pool",
        TabKind::ConnectionMonitor => "connection_monitor",
        TabKind::Topology => "topology",
        TabKind::NotificationCenter => "notification_center",
        TabKind::Sftp => "sftp",
        TabKind::Ide => "ide",
        TabKind::Forwards => "forwards",
        TabKind::SessionManager => "session_manager",
        TabKind::PluginManager => "plugin_manager",
        TabKind::CloudSync => "cloud_sync",
        TabKind::Settings => "settings",
    }
}

fn current_window_size(window: &Window) -> (f32, f32) {
    let bounds = window.inner_window_bounds().get_bounds();
    (f32::from(bounds.size.width), f32::from(bounds.size.height))
}

fn terminal_background_fit(fit: BackgroundFit) -> TerminalBackgroundFit {
    match fit {
        BackgroundFit::Cover => TerminalBackgroundFit::Cover,
        BackgroundFit::Contain => TerminalBackgroundFit::Contain,
        BackgroundFit::Fill => TerminalBackgroundFit::Fill,
        BackgroundFit::Tile => TerminalBackgroundFit::Tile,
    }
}

fn sftp_runtime_settings_from_settings(
    settings: &PersistedSettings,
) -> SftpTransferRuntimeSettings {
    SftpTransferRuntimeSettings {
        max_concurrent_transfers: settings.sftp.max_concurrent_transfers.max(1) as usize,
        speed_limit_kbps: if settings.sftp.speed_limit_enabled {
            settings.sftp.speed_limit_kbps.max(0) as usize
        } else {
            0
        },
        directory_parallelism: settings.sftp.directory_parallelism.max(1) as usize,
    }
}

fn reconnect_timing_from_settings(settings: &PersistedSettings) -> ReconnectTiming {
    ReconnectTiming {
        retry_base_delay: Duration::from_millis(settings.reconnect.base_delay_ms.max(1) as u64),
        retry_max_delay: Duration::from_millis(settings.reconnect.max_delay_ms.max(1) as u64),
        ..ReconnectTiming::default()
    }
}

fn reconnect_max_attempts_from_settings(settings: &PersistedSettings) -> u32 {
    settings.reconnect.max_attempts.max(1) as u32
}

fn session_terminal_encoding(encoding: SettingsTerminalEncoding) -> SessionTerminalEncoding {
    match encoding {
        SettingsTerminalEncoding::Utf8 => SessionTerminalEncoding::Utf8,
        SettingsTerminalEncoding::Gbk => SessionTerminalEncoding::Gbk,
        SettingsTerminalEncoding::Gb18030 => SessionTerminalEncoding::Gb18030,
        SettingsTerminalEncoding::Big5 => SessionTerminalEncoding::Big5,
        SettingsTerminalEncoding::ShiftJis => SessionTerminalEncoding::ShiftJis,
        SettingsTerminalEncoding::EucJp => SessionTerminalEncoding::EucJp,
        SettingsTerminalEncoding::EucKr => SessionTerminalEncoding::EucKr,
        SettingsTerminalEncoding::Windows1252 => SessionTerminalEncoding::Windows1252,
    }
}

fn locale_from_settings(language: Language) -> Locale {
    match language {
        Language::De => Locale::De,
        Language::En => Locale::En,
        Language::EsEs => Locale::EsEs,
        Language::FrFr => Locale::FrFr,
        Language::It => Locale::It,
        Language::Ja => Locale::Ja,
        Language::Ko => Locale::Ko,
        Language::PtBr => Locale::PtBr,
        Language::Vi => Locale::Vi,
        Language::ZhCn => Locale::ZhCn,
        Language::ZhTw => Locale::ZhTw,
    }
}

fn settings_language_from_locale(locale: Locale) -> Language {
    match locale {
        Locale::De => Language::De,
        Locale::En => Language::En,
        Locale::EsEs => Language::EsEs,
        Locale::FrFr => Language::FrFr,
        Locale::It => Language::It,
        Locale::Ja => Language::Ja,
        Locale::Ko => Language::Ko,
        Locale::PtBr => Language::PtBr,
        Locale::Vi => Language::Vi,
        Locale::ZhCn => Language::ZhCn,
        Locale::ZhTw => Language::ZhTw,
    }
}

fn tokens_from_settings(settings: &PersistedSettings) -> ThemeTokens {
    let mut tokens = settings::custom_theme_tokens_from_settings(settings)
        .unwrap_or_else(|| ThemeTokens::from_builtin(theme_by_id(&settings.terminal.theme)));
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: 2.0_f32.min(radius.max(1.0)),
    };
    tokens
}

fn native_vibrancy_mode(mode: FrostedGlassMode) -> NativeVibrancyMode {
    match mode {
        FrostedGlassMode::Off | FrostedGlassMode::Css => NativeVibrancyMode::Off,
        FrostedGlassMode::Native | FrostedGlassMode::System => NativeVibrancyMode::System,
        FrostedGlassMode::Mica => NativeVibrancyMode::Mica,
        FrostedGlassMode::Acrylic => NativeVibrancyMode::Acrylic,
    }
}

fn effective_vibrancy_mode(
    settings: &PersistedSettings,
    policy: &EffectiveRenderPolicy,
) -> NativeVibrancyMode {
    if policy.allow_vibrancy {
        native_vibrancy_mode(settings.appearance.frosted_glass)
    } else {
        NativeVibrancyMode::Off
    }
}

fn render_profile_from_env() -> Option<RenderProfile> {
    let value = std::env::var("OXIDETERM_RENDER_PROFILE").ok()?;
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" => Some(RenderProfile::Auto),
        "quality" | "high-quality" | "high" => Some(RenderProfile::Quality),
        "low-power" | "lowpower" | "low" => Some(RenderProfile::LowPower),
        "compatibility" | "compat" | "safe" | "safe-mode" => Some(RenderProfile::Compatibility),
        _ => None,
    }
}

fn workspace_background(tokens: &ThemeTokens, mode: NativeVibrancyMode) -> Rgba {
    match mode {
        NativeVibrancyMode::Off => rgb(tokens.ui.bg),
        NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
            rgba((tokens.ui.bg << 8) | alpha_byte(tokens.metrics.window_vibrancy_tint_alpha))
        }
    }
}

fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

fn settings_mono_font_family(settings: &PersistedSettings) -> SharedString {
    let family = settings
        .terminal
        .font_family
        .terminal_family_name(&settings.terminal.custom_font_family);
    settings_css_font_family_head(&family).unwrap_or_else(|| gpui_font_family_name(&family))
}

fn reconnect_phase_label(phase: &ReconnectPhase) -> &'static str {
    match phase {
        ReconnectPhase::Queued => "queued",
        ReconnectPhase::Snapshot => "snapshot",
        ReconnectPhase::GracePeriod => "grace-period",
        ReconnectPhase::SshConnect => "ssh-connect",
        ReconnectPhase::AwaitTerminal => "await-terminal",
        ReconnectPhase::RestoreForwards => "restore-forwards",
        ReconnectPhase::ResumeTransfers => "resume-transfers",
        ReconnectPhase::RestoreIde => "restore-ide",
        ReconnectPhase::Verify => "verify",
        ReconnectPhase::Done => "done",
        ReconnectPhase::Failed => "failed",
        ReconnectPhase::Cancelled => "cancelled",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceContextMenuDismissal {
    Close,
    KeepOpen,
}

impl WorkspaceApp {
    fn workspace_context_menu_backdrop(
        &self,
        menu: impl gpui::IntoElement,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Radix context menus close for primary and secondary outside clicks.
        // Keep all native context-menu backdrops on the same dismissal path so
        // focus restoration and overlay arbitration do not diverge by feature.
        oxideterm_gpui_ui::context_menu::context_menu_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, window, cx| {
                    this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(menu)
    }

    fn workspace_context_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_context_menu_action_with_dismissal(
            item,
            disabled,
            loading,
            WorkspaceContextMenuDismissal::Close,
            close_menu,
            listener,
            cx,
        )
    }

    fn workspace_context_menu_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Closing Radix menu items share the same row hover/disabled/loading
        // state as persistent checkbox-style items, but run a close callback
        // before the action. Keep that two-step behavior centralized so file,
        // SFTP, session, and terminal menus do not compose guards manually.
        let item = oxideterm_gpui_ui::context_menu::context_menu_actionable_row(
            item, disabled, loading, style,
        );
        self.workspace_context_menu_action(item, disabled, loading, close_menu, listener, cx)
    }

    fn workspace_context_menu_persistent_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_context_menu_action_with_dismissal(
            item,
            disabled,
            loading,
            WorkspaceContextMenuDismissal::KeepOpen,
            |_| {},
            listener,
            cx,
        )
    }

    fn workspace_context_menu_persistent_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Checkbox-like Radix menu rows, such as terminal broadcast targets,
        // keep the menu open after activation but must still share the same
        // disabled/loading guard and hover semantics as closing menu items.
        let item = oxideterm_gpui_ui::context_menu::context_menu_actionable_row(
            item, disabled, loading, style,
        );
        self.workspace_context_menu_persistent_action(item, disabled, loading, listener, cx)
    }

    fn workspace_context_menu_action_with_dismissal(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        dismissal: WorkspaceContextMenuDismissal,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Browser/Radix context-menu item activation has a common sequence:
        // ignore disabled/loading rows, apply the menu's dismissal policy, run
        // the action, then stop the pointer from reaching the underlying row or
        // terminal. Checkbox/dropdown menus such as broadcast target selection
        // intentionally keep the popover open while still sharing the guard.
        oxideterm_gpui_ui::context_menu::context_menu_action(
            item,
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                if dismissal == WorkspaceContextMenuDismissal::Close {
                    close_menu(this);
                }
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    fn push_event_log_entry(
        &mut self,
        severity: WorkspaceEventSeverity,
        category: WorkspaceEventCategory,
        node_id: Option<NodeId>,
        connection_id: Option<String>,
        title: impl Into<String>,
        detail: Option<String>,
        source: &'static str,
    ) {
        self.notification_center.event_log.push(
            severity,
            category,
            node_id.map(|node_id| node_id.0),
            connection_id,
            title,
            detail,
            source,
        );
    }

    fn clear_event_log(&mut self) {
        self.notification_center.event_log.clear();
    }

    fn cycle_event_log_severity_filter(&mut self) {
        self.notification_center.event_log.cycle_severity_filter();
    }

    fn cycle_event_log_category_filter(&mut self) {
        self.notification_center.event_log.cycle_category_filter();
    }

    fn event_log_entry_matches_filter(&self, entry: &WorkspaceEventLogEntry) -> bool {
        self.notification_center.event_log.matches_filter(entry)
    }

    fn push_notification_entry(
        &mut self,
        kind: WorkspaceNotificationKind,
        severity: WorkspaceNotificationSeverity,
        title: impl Into<String>,
        body: Option<String>,
        scope: WorkspaceNotificationScope,
        dedupe_key: Option<String>,
    ) {
        self.notification_center
            .notifications
            .push(kind, severity, title, body, scope, dedupe_key);
    }

    fn resolve_connection_notifications_for_node(&mut self, node_id: &NodeId) {
        self.notification_center
            .notifications
            .resolve_connection_for_node(&node_id.0);
    }

    fn recount_notifications(&mut self) {
        self.notification_center.notifications.recount();
    }

    fn clear_notifications(&mut self) {
        self.notification_center.notifications.clear();
    }

    fn mark_all_notifications_read(&mut self) {
        self.notification_center.notifications.mark_all_read();
    }

    fn dismiss_notification(&mut self, id: u64) {
        self.notification_center.notifications.remove(id);
    }

    fn cycle_notification_status_filter(&mut self) {
        self.notification_center.notifications.cycle_status_filter();
    }

    fn cycle_notification_severity_filter(&mut self) {
        self.notification_center
            .notifications
            .cycle_severity_filter();
    }

    fn cycle_notification_kind_filter(&mut self) {
        self.notification_center.notifications.cycle_kind_filter();
    }

    fn notification_matches_filter(&self, entry: &WorkspaceNotificationEntry) -> bool {
        self.notification_center.notifications.matches_filter(entry)
    }

    fn push_reconnect_notice(
        &self,
        title: impl Into<String>,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: title.into(),
            description,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn i18n_with(&self, key: &str, replacements: &[(&str, String)]) -> String {
        let mut text = self.i18n.t(key);
        for (name, value) in replacements {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    fn connection_failure_notice_for_node(
        &self,
        node_id: &NodeId,
        error: &str,
    ) -> Option<(String, Option<String>)> {
        if connection_error_is_cancelled(error) {
            return None;
        }

        if connection_error_is_proxy_hop_unsupported(error) {
            return Some((
                self.i18n.t("connections.toast.proxy_chain_invalid"),
                Some(self.i18n.t("connections.toast.proxy_hop_kbi_unsupported")),
            ));
        }

        if let Some(run) = self.active_connection_chain.as_ref()
            && let Some(position) = run
                .node_ids
                .iter()
                .position(|candidate| candidate == node_id)
        {
            let total = run.node_ids.len();
            return Some((
                self.i18n.t("ssh.errors.chain_failed_title"),
                Some(self.i18n_with(
                    "ssh.errors.chain_failed_desc",
                    &[
                        ("position", (position + 1).to_string()),
                        ("total", total.to_string()),
                        ("error", error.to_string()),
                    ],
                )),
            ));
        }

        Some((
            self.i18n.t("ssh.errors.generic_title"),
            Some(error.to_string()),
        ))
    }

    fn next_connection_trace_attempt_id(&mut self) -> String {
        self.connection_trace_attempt_seq = self.connection_trace_attempt_seq.wrapping_add(1);
        format!("native-connection-{}", self.connection_trace_attempt_seq)
    }

    fn connection_trace_plan_for_node(
        &mut self,
        node_id: &NodeId,
        mode: ConnectionTraceMode,
    ) -> Option<ConnectionTracePlan> {
        let mut path = Vec::new();
        let mut current = Some(node_id.clone());
        while let Some(current_id) = current {
            path.push(current_id.clone());
            current = self
                .node_runtime_store
                .snapshot(&current_id)
                .and_then(|snapshot| snapshot.parent_id);
        }
        path.reverse();
        let start_index = path
            .iter()
            .position(|candidate| !self.connection_trace_node_is_ready(candidate))?;
        Some(ConnectionTracePlan {
            attempt_id: self.next_connection_trace_attempt_id(),
            mode,
            node_ids: path[start_index..].to_vec(),
        })
    }

    fn connection_trace_node_is_ready(&self, node_id: &NodeId) -> bool {
        self.ssh_nodes.get(node_id).is_some_and(|node| {
            matches!(node.readiness, NodeReadiness::Ready)
                && self
                    .node_router
                    .connection_id_for_node(node_id)
                    .and_then(|connection_id| self.ssh_registry.get(&connection_id))
                    .is_some_and(|handle| {
                        matches!(
                            handle.state(),
                            ConnectionState::Active | ConnectionState::Idle
                        )
                    })
        })
    }

    fn begin_connection_trace_for_node(
        &mut self,
        node_id: &NodeId,
        plan: Option<&ConnectionTracePlan>,
        parent_id: Option<&NodeId>,
    ) {
        let Some((attempt_id, mode, step_index, total_steps)) = plan
            .and_then(|plan| {
                let step = plan
                    .node_ids
                    .iter()
                    .position(|candidate| candidate == node_id)?;
                Some((
                    plan.attempt_id.clone(),
                    plan.mode,
                    (step + 1) as u32,
                    plan.node_ids.len() as u32,
                ))
            })
            .or_else(|| {
                Some((
                    self.next_connection_trace_attempt_id(),
                    ConnectionTraceMode::Connect,
                    1,
                    1,
                ))
            })
        else {
            return;
        };
        let label = self.ssh_nodes.get(node_id).map(|node| node.title.clone());
        self.connection_trace_nodes.insert(
            node_id.clone(),
            ConnectionTraceNodeContext {
                attempt_id: attempt_id.clone(),
                label: label.clone(),
                step_index: Some(step_index),
                total_steps: Some(total_steps),
                mode,
            },
        );
        self.emit_connection_trace_stage(node_id, ConnectionTraceStage::Queued, 5.0, None);
        self.emit_connection_trace_stage(node_id, ConnectionTraceStage::Preparing, 15.0, None);
        self.emit_connection_trace_stage(
            node_id,
            ConnectionTraceStage::OpeningTransport,
            28.0,
            None,
        );
        self.emit_connection_trace_stage(node_id, ConnectionTraceStage::HostKey, 38.0, None);
        self.emit_connection_trace_stage(
            node_id,
            ConnectionTraceStage::SshHandshake,
            48.0,
            parent_id.map(|parent_id| format!("via {}", parent_id.0)),
        );
        self.emit_connection_trace_stage(node_id, ConnectionTraceStage::Authentication, 62.0, None);
    }

    fn emit_connection_trace_stage(
        &self,
        node_id: &NodeId,
        stage: ConnectionTraceStage,
        progress: f32,
        detail: Option<String>,
    ) {
        self.emit_connection_trace_event(
            node_id,
            stage,
            ConnectionTraceStatus::Running,
            progress,
            detail,
        );
    }

    fn finish_connection_trace_success(&mut self, node_id: &NodeId) {
        if self.connection_trace_nodes.contains_key(node_id) {
            self.emit_connection_trace_stage(node_id, ConnectionTraceStage::Pty, 86.0, None);
            self.emit_connection_trace_stage(node_id, ConnectionTraceStage::ShellReady, 96.0, None);
            self.emit_connection_trace_event(
                node_id,
                ConnectionTraceStage::Ready,
                ConnectionTraceStatus::Ready,
                100.0,
                None,
            );
            self.connection_trace_nodes.remove(node_id);
        }
    }

    fn finish_connection_trace_failed(&mut self, node_id: &NodeId, detail: Option<String>) {
        if self.connection_trace_nodes.contains_key(node_id) {
            let stage = connection_trace_failure_stage(detail.as_deref());
            self.emit_connection_trace_event(
                node_id,
                stage,
                ConnectionTraceStatus::Failed,
                100.0,
                detail,
            );
            self.connection_trace_nodes.remove(node_id);
        }
    }

    fn cancel_connection_trace_for_node(&mut self, node_id: &NodeId) {
        if self.connection_trace_nodes.contains_key(node_id) {
            self.emit_connection_trace_event(
                node_id,
                ConnectionTraceStage::Authentication,
                ConnectionTraceStatus::Cancelled,
                100.0,
                None,
            );
            self.connection_trace_nodes.remove(node_id);
        }
    }

    fn emit_connection_trace_event(
        &self,
        node_id: &NodeId,
        stage: ConnectionTraceStage,
        status: ConnectionTraceStatus,
        progress: f32,
        detail: Option<String>,
    ) {
        let Some(context) = self.connection_trace_nodes.get(node_id) else {
            return;
        };
        let _ = self.connection_trace_tx.send(ConnectionTraceEvent {
            attempt_id: context.attempt_id.clone(),
            node_id: node_id.clone(),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail,
            label: context.label.clone(),
            step_index: context.step_index,
            total_steps: context.total_steps,
            mode: context.mode,
        });
    }

    fn log_reconnect_phase(
        &mut self,
        node_id: &NodeId,
        phase: ReconnectPhase,
        _detail: Option<String>,
    ) {
        let severity = match phase {
            ReconnectPhase::Failed => WorkspaceEventSeverity::Error,
            ReconnectPhase::Cancelled => WorkspaceEventSeverity::Warn,
            _ => WorkspaceEventSeverity::Info,
        };
        self.push_event_log_entry(
            severity,
            WorkspaceEventCategory::Reconnect,
            Some(node_id.clone()),
            self.node_router.connection_id_for_node(node_id),
            "event_log.events.reconnect_phase",
            Some(reconnect_phase_label(&phase).to_string()),
            "reconnect_orchestrator",
        );
    }

    fn log_connection_event(
        &mut self,
        node_id: &NodeId,
        connection_id: Option<String>,
        title: impl Into<String>,
        severity: WorkspaceEventSeverity,
        detail: Option<String>,
        source: &'static str,
    ) {
        self.push_event_log_entry(
            severity,
            WorkspaceEventCategory::Connection,
            Some(node_id.clone()),
            connection_id,
            title,
            detail,
            source,
        );
    }

    fn has_active_reconnect_job(&self, node_id: &NodeId) -> bool {
        self.reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
    }

    pub(super) fn cancel_reconnect_for_node(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        let mut affected_nodes = self.node_runtime_store.subtree_postorder(node_id);
        if affected_nodes.is_empty() {
            affected_nodes.push(node_id.clone());
        }
        let mut cancelled = 0_u32;
        for affected_node_id in affected_nodes {
            if self
                .reconnect_orchestrator
                .cancel(&affected_node_id.0)
                .is_some()
            {
                cancelled = cancelled.saturating_add(1);
            }
            self.cancel_forward_restore_token(&affected_node_id);
            self.pending_reconnect_node_ids.remove(&affected_node_id);
            self.reconnect_requeue_counts.remove(&affected_node_id);
            self.pending_reconnect_transfer_resumes
                .remove(&affected_node_id);
            self.reconnect_transfer_resume_totals
                .remove(&affected_node_id);
            self.reconnect_transfer_resume_successes
                .remove(&affected_node_id);
            self.pending_ide_restore_transfer_counts
                .remove(&affected_node_id);
            self.reconnect_forward_restore_totals
                .remove(&affected_node_id);
            self.clear_reconnect_pipeline_active(&affected_node_id);
        }
        if cancelled > 0 {
            self.push_event_log_entry(
                WorkspaceEventSeverity::Warn,
                WorkspaceEventCategory::Reconnect,
                Some(node_id.clone()),
                self.node_router.connection_id_for_node(node_id),
                "event_log.events.reconnect_phase",
                Some(reconnect_phase_label(&ReconnectPhase::Cancelled).to_string()),
                "reconnect_orchestrator",
            );
            self.push_reconnect_notice(
                self.i18n.t("connections.reconnect.cancelled"),
                None,
                TerminalNoticeVariant::Default,
            );
            cx.notify();
        }
    }

    pub(super) fn prepare_modal_interaction_boundary(&mut self) {
        // Tauri dialogs are Radix modal roots: opening one dismisses background
        // popovers and input focus before the overlay starts trapping events.
        self.open_settings_select = None;
        self.close_new_connection_select();
        // Cloud Sync provider/config selects are Radix-like transient popovers;
        // a modal boundary must release both the open menu and the trigger
        // focus owner so keyboard rings do not leak behind the dialog.
        self.cloud_sync_open_select = None;
        self.focused_settings_input = None;
        self.cloud_sync_focused_select = None;
        self.settings_slider_drag = None;
        self.ime_marked_text = None;
        self.workspace_tooltip = None;
        self.workspace_tooltip_pending = None;
        self.workspace_tooltip_generation = self.workspace_tooltip_generation.wrapping_add(1);
    }

    pub(super) fn dismiss_transient_workspace_overlays(&mut self) -> bool {
        let mut changed = false;

        // Match browser/Radix outside-click behavior for non-modal UI only.
        // Auth prompts, confirm dialogs, QuickLook, and SFTP editor shells keep
        // their dedicated dialog_backdrop() close policy and are intentionally
        // excluded from this shared background dismiss path.
        if self.open_settings_select.take().is_some() {
            changed = true;
        }
        if self.open_new_connection_select.take().is_some() {
            changed = true;
        }
        if self.cloud_sync_open_select.take().is_some() {
            self.cloud_sync_select_highlighted = None;
            changed = true;
        }
        if self.cloud_sync_focused_select.take().is_some() {
            // Outside pointer focus in the browser leaves the Radix trigger;
            // mirror that owner release so the native focus ring cannot linger.
            changed = true;
        }
        if self.connection_monitor.selector_open {
            self.connection_monitor.selector_open = false;
            self.connection_monitor.selector_highlighted_index = None;
            self.connection_monitor.selector_focus_origin = None;
            changed = true;
        }
        if self.session_manager.show_batch_move {
            self.session_manager.show_batch_move = false;
            changed = true;
        }
        if self.dismiss_workspace_context_menus() {
            changed = true;
        }
        if self.detached_local_terminals_popover_open {
            self.detached_local_terminals_popover_open = false;
            changed = true;
        }
        if self.terminal_quick_commands_open || self.terminal_quick_command_pending.is_some() {
            self.close_terminal_quick_commands_popover();
            changed = true;
        }
        if self.terminal_command_suggestions_open {
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
            changed = true;
        }
        if self.has_ai_sidebar_floating_overlay() {
            self.close_ai_sidebar_popovers();
            changed = true;
        }
        if self.workspace_tooltip.is_some() || self.workspace_tooltip_pending.is_some() {
            self.workspace_tooltip = None;
            self.workspace_tooltip_pending = None;
            self.workspace_tooltip_generation = self.workspace_tooltip_generation.wrapping_add(1);
            changed = true;
        }
        if changed {
            self.ime_marked_text = None;
        }

        changed
    }

    pub(super) fn dismiss_workspace_context_menus(&mut self) -> bool {
        let mut changed = false;

        // Radix ContextMenu uses one close policy for outside pointer and Esc.
        // Keep all native context-menu owners here so feature handlers do not
        // each mutate their own menu state differently.
        if self.connection_monitor.dismiss_topology_menu() {
            changed = true;
        }
        if self
            .session_manager
            .row_context_menu_connection_id
            .take()
            .is_some()
        {
            changed = true;
        }
        if self
            .session_manager
            .folder_tree_context_menu_x
            .take()
            .is_some()
            || self
                .session_manager
                .folder_tree_context_menu_y
                .take()
                .is_some()
        {
            changed = true;
        }
        if self.session_manager.row_menu_connection_id.take().is_some() {
            changed = true;
        }
        if self.file_manager.context_menu.take().is_some() {
            changed = true;
        }
        if self.sftp_view.dismiss_context_menu() {
            changed = true;
        }
        if self.terminal_broadcast_menu_open {
            self.terminal_broadcast_menu_open = false;
            changed = true;
        }

        changed
    }

    pub(super) fn dismiss_transient_workspace_overlays_from_outside_pointer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.dismiss_transient_workspace_overlays();
        if changed {
            // Radix restores focus to the trigger/outside target after closing
            // transient popovers. Native tracks most triggers as workspace
            // state, so returning focus to the Workspace root keeps keyboard
            // routing alive while leaving feature focus flags unchanged.
            window.focus(&self.focus_handle);
            cx.notify();
        }
        changed
    }

    pub(super) fn handle_transient_workspace_overlay_escape(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" || event.keystroke.modifiers.platform {
            return false;
        }
        if self.dismiss_transient_workspace_overlays() {
            window.focus(&self.focus_handle);
            cx.notify();
            return true;
        }
        false
    }

    fn restore_session_tree_snapshot(&mut self) {
        let path = default_session_tree_path();
        let Ok(bytes) = fs::read(&path) else {
            return;
        };
        let Ok(persisted) = serde_json::from_slice::<PersistedNodeTreeSnapshot>(&bytes) else {
            eprintln!("failed to parse session tree snapshot: {}", path.display());
            return;
        };
        let mut restored_nodes = Vec::new();
        let mut restored_ids = HashSet::new();

        for node in persisted.nodes {
            let config = node
                .config
                .or_else(|| saved_origin_config(&self.connection_store, &node.origin));
            let Some(config) = config else {
                continue;
            };
            restored_ids.insert(node.id.clone());
            restored_nodes.push(NodeTreeSnapshotNode {
                id: node.id,
                parent_id: node.parent_id,
                children_ids: node.children_ids,
                depth: node.depth,
                config,
                origin: node.origin,
                state: NodeState::default(),
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                created_at_ms: node.created_at_ms,
                generation: node.generation,
            });
        }

        let restored_roots = persisted
            .root_ids
            .into_iter()
            .filter(|id| restored_ids.contains(id))
            .collect::<Vec<_>>();
        let snapshot = NodeTreeSnapshot {
            version: persisted.version,
            exported_at_ms: persisted.exported_at_ms,
            root_ids: restored_roots,
            nodes: restored_nodes,
        };
        if let Err(error) = self.node_router.apply_tree_snapshot(snapshot.clone()) {
            eprintln!("failed to restore session tree snapshot: {error}");
            return;
        }

        // Rebuild the UI-facing node cache from the SessionTree owner. Runtime
        // ids are deliberately cleared above: after process restart, Tauri also
        // needs reconnect/connect_tree_node to create fresh SSH/SFTP/terminal
        // owners instead of trusting stale ids from disk.
        let mut saved_targets: HashMap<String, (u32, NodeId)> = HashMap::new();
        for node in snapshot.nodes {
            let title = node
                .origin
                .saved_connection_id()
                .and_then(|id| self.connection_store.get(id))
                .map(|connection| connection.name.clone())
                .unwrap_or_else(|| format!("{}@{}", node.config.username, node.config.host));
            if let Some(saved_connection_id) = node.origin.saved_connection_id() {
                let rank = restored_saved_node_rank(&node.origin);
                let entry = saved_targets
                    .entry(saved_connection_id.to_string())
                    .or_insert((rank, node.id.clone()));
                if rank >= entry.0 {
                    *entry = (rank, node.id.clone());
                }
            }
            self.ssh_nodes.insert(
                node.id,
                WorkspaceSshNode {
                    saved_connection_id: node.origin.saved_connection_id().map(str::to_string),
                    config: node.config,
                    title,
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
        for (saved_connection_id, (_, node_id)) in saved_targets {
            self.saved_ssh_nodes.insert(saved_connection_id, node_id);
        }
    }

    fn persist_session_tree_snapshot(&self) {
        let runtime = self.node_router.export_tree_snapshot();
        let nodes = runtime
            .nodes
            .into_iter()
            .filter_map(|node| {
                let config = persistable_session_tree_config(&node);
                if config.is_none() && node.origin.saved_connection_id().is_none() {
                    return None;
                }
                Some(PersistedNodeTreeNode {
                    id: node.id,
                    parent_id: node.parent_id,
                    children_ids: node.children_ids,
                    depth: node.depth,
                    origin: node.origin,
                    config,
                    created_at_ms: node.created_at_ms,
                    generation: node.generation,
                })
            })
            .collect::<Vec<_>>();
        let retained_ids = nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let persisted = PersistedNodeTreeSnapshot {
            version: runtime.version,
            exported_at_ms: runtime.exported_at_ms,
            root_ids: runtime
                .root_ids
                .into_iter()
                .filter(|id| retained_ids.contains(id))
                .collect(),
            nodes,
        };
        let path = default_session_tree_path();
        if let Err(error) = write_session_tree_snapshot(&path, &persisted) {
            eprintln!("failed to persist session tree snapshot: {error}");
        }
    }
}

fn restored_saved_node_rank(origin: &NodeOrigin) -> u32 {
    match origin {
        NodeOrigin::ManualPreset { hop_index, .. } => *hop_index,
        NodeOrigin::Restored { .. } => u32::MAX,
        NodeOrigin::AutoRoute { hop_index, .. } => *hop_index,
        NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => 0,
    }
}

fn write_session_tree_snapshot(path: &PathBuf, snapshot: &PersistedNodeTreeSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn persistable_session_tree_config(node: &NodeTreeSnapshotNode) -> Option<SshConfig> {
    if node.origin.saved_connection_id().is_some() {
        return None;
    }
    config_without_runtime_secret(&node.config).then(|| node.config.clone())
}

fn connection_error_is_cancelled(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("cancelled")
        || error.contains("user_cancelled")
        || error.contains("manual disconnect")
        || error.contains("explicit disconnect")
}

fn connection_error_is_proxy_hop_unsupported(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("proxy")
        && (error.contains("keyboard-interactive")
            || error.contains("2fa")
            || error.contains("unsupported auth"))
}

fn connection_trace_failure_stage(error: Option<&str>) -> ConnectionTraceStage {
    let Some(error) = error else {
        return ConnectionTraceStage::Authentication;
    };
    let error = error.to_ascii_lowercase();

    if error.contains("node not found")
        || error.contains("already connecting")
        || error.contains("already connected")
    {
        return ConnectionTraceStage::Preparing;
    }

    match classify_message(&error) {
        BackendErrorClass::Disconnected
        | BackendErrorClass::PortInUse
        | BackendErrorClass::Timeout => ConnectionTraceStage::OpeningTransport,
        BackendErrorClass::HostKey => ConnectionTraceStage::HostKey,
        // Tauri's backend emits most transport `connect()` failures after the
        // authentication stage has started, so auth/proxy-agent/cancelled
        // failures keep the same terminal stage while detail carries the class.
        BackendErrorClass::Auth
        | BackendErrorClass::Cancelled
        | BackendErrorClass::PermissionDenied
        | BackendErrorClass::Unsupported
        | BackendErrorClass::Conflict
        | BackendErrorClass::NotFound
        | BackendErrorClass::Other => ConnectionTraceStage::Authentication,
    }
}

fn saved_origin_config(store: &ConnectionStore, origin: &NodeOrigin) -> Option<SshConfig> {
    match origin {
        NodeOrigin::Restored {
            saved_connection_id,
        } => {
            let connection = store.get(saved_connection_id)?;
            self::session_manager::ssh_config_from_saved_connection(store, connection)
        }
        NodeOrigin::ManualPreset {
            saved_connection_id,
            hop_index,
        } => {
            let connection = store.get(saved_connection_id)?;
            saved_manual_preset_hop_config(store, connection, *hop_index)
        }
        NodeOrigin::AutoRoute { .. } | NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => None,
    }
}

fn saved_manual_preset_hop_config(
    store: &ConnectionStore,
    connection: &oxideterm_connections::SavedConnection,
    hop_index: u32,
) -> Option<SshConfig> {
    let hop_index = hop_index as usize;
    if hop_index < connection.proxy_chain.len() {
        let hop = &connection.proxy_chain[hop_index];
        return Some(SshConfig {
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth: self::session_manager::auth_method_from_saved_auth(store, &hop.auth)?,
            proxy_chain: None,
            agent_forwarding: hop.agent_forwarding,
            strict_host_key_checking: true,
            ..SshConfig::default()
        });
    }

    if hop_index == connection.proxy_chain.len() {
        let mut target =
            self::session_manager::ssh_config_from_saved_connection(store, connection)?;
        target.proxy_chain = None;
        return Some(target);
    }

    None
}

fn config_without_runtime_secret(config: &SshConfig) -> bool {
    auth_without_runtime_secret(&config.auth)
        && config.proxy_chain.as_ref().is_none_or(|chain| {
            chain
                .iter()
                .all(|hop| auth_without_runtime_secret(&hop.auth))
        })
}

fn auth_without_runtime_secret(auth: &AuthMethod) -> bool {
    match auth {
        AuthMethod::Password { .. } => false,
        AuthMethod::Key { passphrase, .. } | AuthMethod::Certificate { passphrase, .. } => {
            passphrase.is_none()
        }
        AuthMethod::Agent | AuthMethod::KeyboardInteractive => true,
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn trace_failure_stage_matches_tauri_pre_connect_errors() {
        assert_eq!(
            connection_trace_failure_stage(Some("Node abc is already connecting")),
            ConnectionTraceStage::Preparing
        );
        assert_eq!(
            connection_trace_failure_stage(Some("Parent node hop has no SSH connection")),
            ConnectionTraceStage::OpeningTransport
        );
        assert_eq!(
            connection_trace_failure_stage(Some("Connection failed: network unreachable")),
            ConnectionTraceStage::OpeningTransport
        );
    }

    #[test]
    fn trace_failure_stage_keeps_host_key_and_auth_classes() {
        assert_eq!(
            connection_trace_failure_stage(Some("Host key changed for example.com")),
            ConnectionTraceStage::HostKey
        );
        assert_eq!(
            connection_trace_failure_stage(Some("Authentication failed: permission denied")),
            ConnectionTraceStage::Authentication
        );
    }

    #[test]
    fn trace_failure_stage_covers_proxy_hop_and_manual_cancel_classes() {
        assert_eq!(
            connection_trace_failure_stage(Some(
                "proxy_hop_kbi_unsupported: keyboard-interactive authentication is not supported for proxy chain hops"
            )),
            ConnectionTraceStage::Authentication
        );
        assert_eq!(
            connection_trace_failure_stage(Some("USER_CANCELLED")),
            ConnectionTraceStage::Authentication
        );
        assert_eq!(
            connection_trace_failure_stage(Some("retry exhausted after network timeout")),
            ConnectionTraceStage::OpeningTransport
        );
        assert_eq!(
            connection_trace_failure_stage(Some("known_hosts entry mismatch")),
            ConnectionTraceStage::HostKey
        );
    }
}
