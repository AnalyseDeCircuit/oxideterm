use super::super::*;
use oxideterm_atomic_file::durable_write_with_before_replace;

pub(in crate::workspace) fn tab_background_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::FileManager => "file_manager",
        TabKind::Launcher => "launcher",
        TabKind::Graphics => "graphics",
        TabKind::Runtime => "runtime",
        TabKind::ConnectionPool => "runtime",
        TabKind::ConnectionMonitor => "connection_monitor",
        TabKind::Topology => "topology",
        TabKind::NotificationCenter => "notification_center",
        TabKind::Sftp => "sftp",
        TabKind::Ide => "ide",
        TabKind::Forwards => "forwards",
        TabKind::SessionManager => "session_manager",
        TabKind::PluginManager => "plugin_manager",
        TabKind::Plugin { .. } => "plugin",
        TabKind::CloudSync => "cloud_sync",
        TabKind::RemoteDesktop => "remote_desktop",
        TabKind::Settings => "settings",
    }
}

pub(in crate::workspace) fn current_window_size(window: &Window) -> (f32, f32) {
    let bounds = window.inner_window_bounds().get_bounds();
    (f32::from(bounds.size.width), f32::from(bounds.size.height))
}

pub(in crate::workspace) fn terminal_background_fit(fit: BackgroundFit) -> TerminalBackgroundFit {
    match fit {
        BackgroundFit::Cover => TerminalBackgroundFit::Cover,
        BackgroundFit::Contain => TerminalBackgroundFit::Contain,
        BackgroundFit::Fill => TerminalBackgroundFit::Fill,
        BackgroundFit::Tile => TerminalBackgroundFit::Tile,
    }
}

pub(in crate::workspace) fn background_scope_includes_content(
    scope: BackgroundScope,
    enabled_tabs: &[String],
    background_key: &str,
) -> bool {
    scope == BackgroundScope::Content && enabled_tabs.iter().any(|tab| tab == background_key)
}

pub(in crate::workspace) fn background_scope_includes_window(scope: BackgroundScope) -> bool {
    scope == BackgroundScope::Window
}

pub(in crate::workspace) fn root_locale_from_settings(language: Language) -> Locale {
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

pub(in crate::workspace) fn settings_language_from_locale(locale: Locale) -> Language {
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

pub(in crate::workspace) fn tokens_from_settings(settings: &PersistedSettings) -> ThemeTokens {
    let mut tokens = oxideterm_settings_model::custom_theme_tokens_from_settings(settings)
        .unwrap_or_else(|| ThemeTokens::from_builtin(theme_by_id(&settings.terminal.theme)));
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: 2.0_f32.min(radius.max(1.0)),
    };
    tokens.apply_density(match settings.appearance.ui_density {
        oxideterm_settings::UiDensity::Compact => UiDensityProfile::Compact,
        oxideterm_settings::UiDensity::Comfortable => UiDensityProfile::Comfortable,
        oxideterm_settings::UiDensity::Spacious => UiDensityProfile::Spacious,
    });
    tokens.apply_ui_font_scale(
        settings.appearance.ui_font_size as f32 / oxideterm_settings::DEFAULT_UI_FONT_SIZE as f32,
    );
    tokens.apply_motion(match settings.appearance.animation_speed {
        oxideterm_settings::AnimationSpeed::Off => UiMotionProfile::Off,
        oxideterm_settings::AnimationSpeed::Reduced => UiMotionProfile::Reduced,
        oxideterm_settings::AnimationSpeed::Normal => UiMotionProfile::Normal,
        oxideterm_settings::AnimationSpeed::Fast => UiMotionProfile::Fast,
    });
    tokens
}

pub(in crate::workspace) fn native_vibrancy_mode(mode: FrostedGlassMode) -> NativeVibrancyMode {
    match mode {
        FrostedGlassMode::Off => NativeVibrancyMode::Off,
        // Keep old persisted "css" values usable without exposing the
        // WebView-era option in GPUI settings.
        FrostedGlassMode::Css => NativeVibrancyMode::System,
        FrostedGlassMode::Native | FrostedGlassMode::System => NativeVibrancyMode::System,
        FrostedGlassMode::Mica => NativeVibrancyMode::Mica,
        FrostedGlassMode::Acrylic => NativeVibrancyMode::Acrylic,
    }
}

pub(in crate::workspace) fn effective_vibrancy_mode(
    settings: &PersistedSettings,
    policy: &EffectiveRenderPolicy,
) -> NativeVibrancyMode {
    if policy.allow_vibrancy {
        native_vibrancy_mode(settings.appearance.frosted_glass)
    } else {
        NativeVibrancyMode::Off
    }
}

pub(in crate::workspace) fn render_profile_from_env() -> Option<RenderProfile> {
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

pub(in crate::workspace) fn workspace_background(
    tokens: &ThemeTokens,
    mode: NativeVibrancyMode,
    has_window_background: bool,
) -> Rgba {
    // A window-scoped image may have its own opacity, but it must blend with an
    // opaque app color instead of sampling the macOS desktop through the window.
    if has_window_background {
        return rgb(tokens.ui.bg);
    }
    match mode {
        NativeVibrancyMode::Off => rgb(tokens.ui.bg),
        NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
            rgba((tokens.ui.bg << 8) | alpha_byte(tokens.metrics.window_vibrancy_tint_alpha))
        }
    }
}

pub(in crate::workspace) fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

pub(in crate::workspace) fn sidebar_surface_background(
    color: u32,
    has_window_background: bool,
    opacity: f32,
) -> Rgba {
    if has_window_background {
        rgba((color << 8) | alpha_byte(opacity))
    } else {
        rgb(color)
    }
}

pub(in crate::workspace) fn context_sidebar_inner_surface_background(
    color: u32,
    has_window_background: bool,
) -> Rgba {
    if has_window_background {
        rgba(0x00000000)
    } else {
        rgb(color)
    }
}

pub(in crate::workspace) fn settings_mono_font_family(
    settings: &PersistedSettings,
) -> SharedString {
    let family = settings
        .terminal
        .font_family
        .terminal_family_name(&settings.terminal.custom_font_family);
    settings_css_font_family_head(&family).unwrap_or_else(|| gpui_font_family_name(&family))
}

pub(in crate::workspace) fn terminal_cjk_font_family_preference(family: &str) -> Option<String> {
    let family = family.trim();
    if family.is_empty() {
        None
    } else {
        Some(family.to_string())
    }
}

pub(in crate::workspace) fn reconnect_phase_label(phase: &ReconnectPhase) -> &'static str {
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
pub(in crate::workspace) enum WorkspaceContextMenuDismissal {
    Close,
    KeepOpen,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn workspace_tooltip_icon_button(
        &self,
        icon: LucideIcon,
        icon_size: f32,
        icon_color: Rgba,
        options: oxideterm_gpui_ui::button::IconButtonOptions,
        tooltip: String,
        element_id_prefix: &'static str,
        flex_none: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        let actionable = !(options.disabled || options.loading);
        let tooltip_for_move = tooltip.clone();
        let tooltip_element_id = tooltip.clone();
        let tooltip_request_id = tooltip.clone();
        let tooltip_workspace = workspace.clone();
        let clear_workspace = workspace;

        // FileManager, SFTP, and launcher toolbar buttons all map to Tauri
        // icon buttons with hover tooltips. Keep tooltip ownership and the
        // disabled/loading click guard in one helper so feature surfaces only
        // supply button metrics and the action body.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, icon_size, icon_color),
            options,
        )
        .id((gpui::ElementId::from(element_id_prefix), tooltip_element_id))
        .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
            let _ = tooltip_workspace.update(cx, |this, cx| {
                this.queue_workspace_tooltip(
                    tooltip_request_id.clone(),
                    tooltip_for_move.clone(),
                    f32::from(event.position.x) + 12.0,
                    f32::from(event.position.y) + 16.0,
                    cx,
                );
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            if !*hovered {
                let _ = clear_workspace.update(cx, |this, cx| {
                    this.clear_workspace_tooltip(&tooltip, cx);
                });
            }
        })
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, listener)
        })
        .when(!actionable, |button| {
            // Disabled browser buttons do not activate their parent row. Stop the
            // click at the shared tooltip button so file/SFTP/sidebar toolbars do
            // not each need a local disabled-event patch.
            button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
        })
        .when(flex_none, |button| button.flex_none())
        .into_any_element()
    }

    pub(in crate::workspace) fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        icon_size: f32,
        icon_color: Rgba,
        options: oxideterm_gpui_ui::button::IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let actionable = !(options.disabled || options.loading);

        // Row-level icon buttons do not always have tooltips, but they still
        // share the browser button contract: disabled/loading buttons keep
        // their visual state and never dispatch pointer activation.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, icon_size, icon_color),
            options,
        )
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!actionable, |button| {
            // Match the DOM disabled button contract for inline row actions:
            // inert buttons consume the pointer instead of selecting/opening the row.
            button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
        })
    }

    pub(in crate::workspace) fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: oxideterm_gpui_ui::button::ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let actionable = !(options.button.disabled || options.loading);

        // Tauri Button activation always goes through the native disabled
        // attribute. GPUI callers still own the action body, but the shared
        // wrapper keeps disabled/loading activation guards out of feature code.
        oxideterm_gpui_ui::button::toolbar_button(&self.tokens, label, icon, options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
    }

    pub(in crate::workspace) fn workspace_confirm_footer_action_button(
        &self,
        label: String,
        variant: oxideterm_gpui_ui::button::ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        focused_action: Option<ConfirmDialogAction>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_modal_footer_action_button(
            label,
            variant,
            action,
            disabled,
            focused_action,
            None,
            listener,
            cx,
        )
    }

    pub(in crate::workspace) fn workspace_modal_footer_action_button<T>(
        &self,
        label: String,
        variant: oxideterm_gpui_ui::button::ButtonVariant,
        action: T,
        disabled: bool,
        focused_action: Option<T>,
        min_width: Option<f32>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div
    where
        T: std::marker::Copy + Eq,
    {
        // DialogFooter buttons across settings, AI, FileManager, and import/export
        // use the same shadcn Button contract: disabled buttons are inert, and the
        // focus ring only follows explicit keyboard-owned footer focus.
        self.workspace_toolbar_action_button(
            label,
            None,
            oxideterm_gpui_ui::button::ToolbarButtonOptions {
                button: oxideterm_gpui_ui::button::ButtonOptions {
                    variant,
                    size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                    radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                    disabled,
                },
                min_width,
                focus_visible: focused_action == Some(action),
                ..oxideterm_gpui_ui::button::ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(in crate::workspace) fn workspace_clickable_row_action(
        &self,
        row: gpui::Div,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        // Some Tauri controls are clickable rows rather than Button elements
        // (Cloud Sync record check rows, inline option rows). Keep their
        // disabled activation guard centralized without forcing Button chrome.
        row.when(!disabled, |row| {
            row.cursor_pointer()
                .on_mouse_down(MouseButton::Left, listener)
        })
    }

    pub(in crate::workspace) fn workspace_context_menu_backdrop(
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

    pub(in crate::workspace) fn workspace_context_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
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

    pub(in crate::workspace) fn workspace_context_menu_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Closing Radix menu items share the same row hover/disabled/loading
        // state as persistent checkbox-style items, but run a close callback
        // before the action. Keep that two-step behavior centralized so file,
        // SFTP, session, topology, and terminal menus do not compose guards manually.
        let item = oxideterm_gpui_ui::context_menu::context_menu_actionable_row(
            item, disabled, loading, style,
        );
        self.workspace_context_menu_action(item, disabled, loading, close_menu, listener, cx)
    }

    pub(in crate::workspace) fn workspace_context_menu_persistent_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
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

    pub(in crate::workspace) fn workspace_context_menu_persistent_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
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

    pub(in crate::workspace) fn workspace_context_menu_action_with_dismissal(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        dismissal: WorkspaceContextMenuDismissal,
        close_menu: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
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

    pub(in crate::workspace) fn push_event_log_entry(
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

    pub(in crate::workspace) fn clear_event_log(&mut self) {
        self.notification_center.event_log.clear();
    }

    pub(in crate::workspace) fn cycle_event_log_severity_filter(&mut self) {
        self.notification_center.event_log.cycle_severity_filter();
    }

    pub(in crate::workspace) fn cycle_event_log_category_filter(&mut self) {
        self.notification_center.event_log.cycle_category_filter();
    }

    pub(in crate::workspace) fn event_log_entry_matches_filter(
        &self,
        entry: &WorkspaceEventLogEntry,
    ) -> bool {
        self.notification_center.event_log.matches_filter(entry)
    }

    pub(in crate::workspace) fn push_notification_entry(
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

    pub(in crate::workspace) fn resolve_connection_notifications_for_node(
        &mut self,
        node_id: &NodeId,
    ) {
        self.notification_center
            .notifications
            .resolve_connection_for_node(&node_id.0);
    }

    pub(in crate::workspace) fn recount_notifications(&mut self) {
        self.notification_center.notifications.recount();
    }

    pub(in crate::workspace) fn clear_notifications(&mut self) {
        self.notification_center.notifications.clear();
    }

    pub(in crate::workspace) fn mark_all_notifications_read(&mut self) {
        self.notification_center.notifications.mark_all_read();
    }

    pub(in crate::workspace) fn dismiss_notification(&mut self, id: u64) {
        self.notification_center.notifications.remove(id);
    }

    pub(in crate::workspace) fn cycle_notification_status_filter(&mut self) {
        self.notification_center.notifications.cycle_status_filter();
    }

    pub(in crate::workspace) fn cycle_notification_severity_filter(&mut self) {
        self.notification_center
            .notifications
            .cycle_severity_filter();
    }

    pub(in crate::workspace) fn cycle_notification_kind_filter(&mut self) {
        self.notification_center.notifications.cycle_kind_filter();
    }

    pub(in crate::workspace) fn notification_matches_filter(
        &self,
        entry: &WorkspaceNotificationEntry,
    ) -> bool {
        self.notification_center.notifications.matches_filter(entry)
    }

    pub(in crate::workspace) fn push_reconnect_notice(
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

    pub(in crate::workspace) fn i18n_with(
        &self,
        key: &str,
        replacements: &[(&str, String)],
    ) -> String {
        let mut text = self.i18n.t(key);
        for (name, value) in replacements {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    pub(in crate::workspace) fn ssh_algorithm_diagnostic_parts(
        &self,
        error: &str,
    ) -> Option<(String, String)> {
        let diagnostic = oxideterm_ssh::parse_algorithm_negotiation_error(error)?;
        let kind_label = self.i18n.t(ssh_algorithm_kind_label_key(diagnostic.kind));
        let summary_key = ssh_algorithm_summary_key(diagnostic.kind, &diagnostic.server_algorithms);
        let summary = self.i18n.t(summary_key).replace("{{kind}}", &kind_label);
        let no_common = self
            .i18n
            .t("connections.trace.diagnostics.no_common")
            .replace("{{kind}}", &kind_label);
        let detail = [
            self.i18n_with(
                "connections.trace.diagnostics.client_offered",
                &[(
                    "algorithms",
                    format_algorithm_list(&diagnostic.client_algorithms),
                )],
            ),
            self.i18n_with(
                "connections.trace.diagnostics.server_offered",
                &[(
                    "algorithms",
                    format_algorithm_list(&diagnostic.server_algorithms),
                )],
            ),
            self.i18n_with(
                "connections.trace.diagnostics.missing_match",
                &[("reason", no_common)],
            ),
        ]
        .join("\n");
        Some((summary, detail))
    }

    pub(in crate::workspace) fn ssh_algorithm_diagnostic_message(
        &self,
        error: &str,
    ) -> Option<String> {
        let (summary, detail) = self.ssh_algorithm_diagnostic_parts(error)?;
        Some(format!("{summary}\n{detail}"))
    }

    pub(in crate::workspace) fn connection_failure_notice_for_node(
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
            let description = self
                .ssh_algorithm_diagnostic_message(error)
                .unwrap_or_else(|| error.to_string());
            return Some((
                self.i18n.t("ssh.errors.chain_failed_title"),
                Some(self.i18n_with(
                    "ssh.errors.chain_failed_desc",
                    &[
                        ("position", (position + 1).to_string()),
                        ("total", total.to_string()),
                        ("error", description),
                    ],
                )),
            ));
        }

        Some((
            self.i18n.t("ssh.errors.generic_title"),
            Some(
                self.ssh_algorithm_diagnostic_message(error)
                    .unwrap_or_else(|| error.to_string()),
            ),
        ))
    }

    pub(in crate::workspace) fn connection_trace_plan_for_node(
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
        let path = path
            .into_iter()
            .map(|candidate| {
                let ready = self.connection_trace_node_is_ready(&candidate);
                (candidate, ready)
            })
            .collect::<Vec<_>>();
        self.connection_trace_state.plan_for_path(mode, &path)
    }

    pub(in crate::workspace) fn connection_trace_node_is_ready(&self, node_id: &NodeId) -> bool {
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

    pub(in crate::workspace) fn begin_connection_trace_for_node(
        &mut self,
        node_id: &NodeId,
        plan: Option<&ConnectionTracePlan>,
        parent_id: Option<&NodeId>,
    ) {
        let label = self.ssh_nodes.get(node_id).map(|node| node.title.clone());
        self.connection_trace_state
            .begin(node_id.clone(), label, plan);
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

    pub(in crate::workspace) fn emit_connection_trace_stage(
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

    pub(in crate::workspace) fn finish_connection_trace_success(&mut self, node_id: &NodeId) {
        if self.connection_trace_state.contains(node_id) {
            self.emit_connection_trace_stage(node_id, ConnectionTraceStage::Pty, 86.0, None);
            self.emit_connection_trace_stage(node_id, ConnectionTraceStage::ShellReady, 96.0, None);
            self.emit_connection_trace_event(
                node_id,
                ConnectionTraceStage::Ready,
                ConnectionTraceStatus::Ready,
                100.0,
                None,
            );
            self.connection_trace_state.finish(node_id);
        }
    }

    pub(in crate::workspace) fn finish_connection_trace_failed(
        &mut self,
        node_id: &NodeId,
        detail: Option<String>,
    ) {
        if self.connection_trace_state.contains(node_id) {
            let stage = oxideterm_ssh::connection_trace_failure_stage(detail.as_deref());
            self.emit_connection_trace_event(
                node_id,
                stage,
                ConnectionTraceStatus::Failed,
                100.0,
                detail,
            );
            self.connection_trace_state.finish(node_id);
        }
    }

    pub(in crate::workspace) fn cancel_connection_trace_for_node(&mut self, node_id: &NodeId) {
        if self.connection_trace_state.contains(node_id) {
            self.emit_connection_trace_event(
                node_id,
                ConnectionTraceStage::Authentication,
                ConnectionTraceStatus::Cancelled,
                100.0,
                None,
            );
            self.connection_trace_state.finish(node_id);
        }
    }

    pub(in crate::workspace) fn emit_connection_trace_event(
        &self,
        node_id: &NodeId,
        stage: ConnectionTraceStage,
        status: ConnectionTraceStatus,
        progress: f32,
        detail: Option<String>,
    ) {
        if let Some(event) = self
            .connection_trace_state
            .event(node_id, stage, status, progress, detail)
        {
            let _ = self.connection_trace_tx.send(event);
        }
    }

    pub(in crate::workspace) fn log_reconnect_phase(
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

    pub(in crate::workspace) fn log_connection_event(
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

    pub(in crate::workspace) fn has_active_reconnect_job(&self, node_id: &NodeId) -> bool {
        self.reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
    }

    pub(in crate::workspace) fn cancel_reconnect_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::workspace) fn prepare_modal_interaction_boundary(&mut self) {
        // Tauri dialogs are Radix modal roots: opening one dismisses background
        // popovers and input focus before the overlay starts trapping events.
        self.release_active_remote_desktop_inputs();
        self.close_settings_select();
        self.close_new_connection_select();
        // Cloud Sync provider/config selects are Radix-like transient popovers;
        // a modal boundary must release both the open menu and the trigger
        // focus owner so keyboard rings do not leak behind the dialog.
        self.cloud_sync.view.open_select = None;
        self.focused_settings_input = None;
        self.cloud_sync.view.focused_select = None;
        self.settings_slider_drag = None;
        self.ime_marked_text = None;
        self.workspace_tooltip = None;
        self.workspace_tooltip_pending = None;
        self.workspace_tooltip_generation = self.workspace_tooltip_generation.wrapping_add(1);
    }

    pub(in crate::workspace) fn dismiss_transient_workspace_overlays(&mut self) -> bool {
        let mut changed = false;

        // Match browser/Radix outside-click behavior for non-modal UI only.
        // Auth prompts, confirm dialogs, QuickLook, and SFTP editor shells keep
        // their dedicated dialog_backdrop() close policy and are intentionally
        // excluded from this shared background dismiss path.
        if self.open_settings_select.is_some() {
            self.close_settings_select();
            changed = true;
        }
        if self.open_new_connection_select.is_some() {
            self.close_new_connection_select();
            changed = true;
        }
        if self.cloud_sync.view.open_select.take().is_some() {
            self.cloud_sync.view.select_highlighted = None;
            changed = true;
        }
        if self.cloud_sync.view.focused_select.take().is_some() {
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
        } else if self.ai.models.selector_open
            && self.ai.models.selector_scope == Some(AiModelSelectorScope::TerminalInline)
        {
            // The terminal inline model selector is painted inside the pane
            // instead of the sidebar popover portal, so include it in the same
            // transient-dismiss path used by wheel/outside-pointer behavior.
            self.close_ai_model_selector();
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

    pub(in crate::workspace) fn dismiss_workspace_context_menus(&mut self) -> bool {
        let mut changed = false;

        // Radix ContextMenu uses one close policy for outside pointer and Esc.
        // Keep all native context-menu owners here so feature handlers do not
        // each mutate their own menu state differently.
        if self.connection_monitor.dismiss_topology_menu() {
            changed = true;
        }
        if self.close_session_row_menus() {
            changed = true;
        }
        if self.dismiss_file_manager_context_menu() {
            changed = true;
        }
        if self.dismiss_sftp_context_menu() {
            changed = true;
        }
        if self.dismiss_terminal_broadcast_menu() {
            changed = true;
        }
        if self.close_terminal_git_branch_picker() {
            changed = true;
        }
        if self.close_tab_context_menu() {
            changed = true;
        }

        changed
    }

    pub(in crate::workspace) fn dismiss_transient_workspace_overlays_from_outside_pointer(
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
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn handle_transient_workspace_overlay_escape(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" || event.keystroke.modifiers.platform {
            return false;
        }
        if self.dismiss_transient_workspace_overlays() {
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn restore_session_tree_snapshot(&mut self) {
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
            let config = node.config.or_else(|| {
                saved_origin_config(
                    &self.connection_store,
                    self.settings_store.settings(),
                    &node.origin,
                )
            });
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
                terminal_endpoints: Vec::new(),
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

    pub(in crate::workspace) fn persist_session_tree_snapshot(&self) {
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

pub(in crate::workspace) fn restored_saved_node_rank(origin: &NodeOrigin) -> u32 {
    match origin {
        NodeOrigin::ManualPreset { hop_index, .. } => *hop_index,
        NodeOrigin::Restored { .. } => u32::MAX,
        NodeOrigin::AutoRoute { hop_index, .. } => *hop_index,
        NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => 0,
    }
}

pub(in crate::workspace) fn write_session_tree_snapshot(
    path: &Path,
    snapshot: &PersistedNodeTreeSnapshot,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    durable_write_with_before_replace(path, &bytes, fail_before_session_tree_replace_for_tests)?;
    Ok(())
}

#[cfg(test)]
pub(in crate::workspace) fn fail_before_session_tree_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_SESSION_TREE_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other(
                "injected failure before session tree replace",
            ))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
pub(in crate::workspace) fn fail_before_session_tree_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
pub(in crate::workspace) fn inject_session_tree_replace_failure() {
    FAIL_NEXT_SESSION_TREE_REPLACE.with(|fail| fail.set(true));
}

pub(in crate::workspace) fn persistable_session_tree_config(
    node: &NodeTreeSnapshotNode,
) -> Option<SshConfig> {
    if node.origin.saved_connection_id().is_some() {
        return None;
    }
    (!node.config.has_runtime_auth_secret()).then(|| node.config.clone())
}

pub(in crate::workspace) fn connection_error_is_cancelled(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("cancelled")
        || error.contains("user_cancelled")
        || error.contains("manual disconnect")
        || error.contains("explicit disconnect")
}

pub(in crate::workspace) fn connection_error_is_proxy_hop_unsupported(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("proxy")
        && (error.contains("keyboard-interactive")
            || error.contains("2fa")
            || error.contains("unsupported auth"))
}

pub(in crate::workspace) fn saved_origin_config(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    origin: &NodeOrigin,
) -> Option<SshConfig> {
    match origin {
        NodeOrigin::Restored {
            saved_connection_id,
        } => {
            let connection = store.get(saved_connection_id)?;
            oxideterm_session_adapter::ssh_config_from_saved_connection(store, settings, connection)
        }
        NodeOrigin::ManualPreset {
            saved_connection_id,
            hop_index,
        } => {
            let connection = store.get(saved_connection_id)?;
            oxideterm_session_adapter::ssh_config_for_saved_connection_hop(
                store, settings, connection, *hop_index,
            )
        }
        NodeOrigin::AutoRoute { .. } | NodeOrigin::DrillDown { .. } | NodeOrigin::Direct => None,
    }
}

pub(in crate::workspace) fn ssh_algorithm_kind_label_key(
    kind: SshAlgorithmDiagnosticKind,
) -> &'static str {
    match kind {
        SshAlgorithmDiagnosticKind::KeyExchange => {
            "connections.trace.diagnostics.kind.key_exchange"
        }
        SshAlgorithmDiagnosticKind::HostKey => "connections.trace.diagnostics.kind.host_key",
        SshAlgorithmDiagnosticKind::Cipher => "connections.trace.diagnostics.kind.cipher",
        SshAlgorithmDiagnosticKind::Mac => "connections.trace.diagnostics.kind.mac",
        SshAlgorithmDiagnosticKind::Compression => "connections.trace.diagnostics.kind.compression",
    }
}

pub(in crate::workspace) fn ssh_algorithm_summary_key(
    kind: SshAlgorithmDiagnosticKind,
    server_algorithms: &[String],
) -> &'static str {
    match kind {
        SshAlgorithmDiagnosticKind::KeyExchange => {
            "connections.trace.diagnostics.summary.key_exchange"
        }
        SshAlgorithmDiagnosticKind::HostKey
            if oxideterm_ssh::server_only_offers_ssh_rsa(server_algorithms) =>
        {
            "connections.trace.diagnostics.summary.host_key_ssh_rsa"
        }
        SshAlgorithmDiagnosticKind::HostKey => "connections.trace.diagnostics.summary.host_key",
        SshAlgorithmDiagnosticKind::Cipher
            if oxideterm_ssh::server_offers_legacy_cipher(server_algorithms) =>
        {
            "connections.trace.diagnostics.summary.cipher_legacy"
        }
        SshAlgorithmDiagnosticKind::Cipher => "connections.trace.diagnostics.summary.cipher",
        SshAlgorithmDiagnosticKind::Mac => "connections.trace.diagnostics.summary.mac",
        SshAlgorithmDiagnosticKind::Compression => {
            "connections.trace.diagnostics.summary.compression"
        }
    }
}

pub(in crate::workspace) fn format_algorithm_list(algorithms: &[String]) -> String {
    if algorithms.is_empty() {
        "-".to_string()
    } else {
        algorithms.join(", ")
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn appearance_settings_reach_density_motion_and_radius_tokens() {
        let mut settings = PersistedSettings::default();
        settings.appearance.ui_density = oxideterm_settings::UiDensity::Compact;
        settings.appearance.animation_speed = oxideterm_settings::AnimationSpeed::Off;
        settings.appearance.border_radius = 2;

        let tokens = tokens_from_settings(&settings);

        assert_eq!(tokens.density, UiDensityProfile::Compact);
        assert!(!tokens.motion.enabled);
        assert_eq!(tokens.radii.md, 2.0);
        assert_eq!(tokens.radii.xs, 0.0);
    }

    #[test]
    fn representative_appearance_matrix_reaches_render_tokens() {
        let themes = [
            "default",
            "paper-oxide",
            "github-dark",
            "nord",
            "solarized-light",
        ];
        let densities = [
            oxideterm_settings::UiDensity::Compact,
            oxideterm_settings::UiDensity::Comfortable,
            oxideterm_settings::UiDensity::Spacious,
        ];
        let radii = [0_i64, 6, 16];

        for theme_id in themes {
            for density in densities {
                for radius in radii {
                    let mut settings = PersistedSettings::default();
                    settings.terminal.theme = theme_id.to_string();
                    settings.appearance.ui_density = density;
                    settings.appearance.border_radius = radius;
                    settings.appearance.animation_speed = oxideterm_settings::AnimationSpeed::Off;

                    let tokens = tokens_from_settings(&settings);

                    assert_eq!(
                        tokens.terminal,
                        oxideterm_theme::theme_by_id(theme_id).terminal
                    );
                    assert_eq!(tokens.radii.md, radius as f32);
                    assert!(!tokens.motion.enabled);
                }
            }
        }
    }

    #[test]
    fn content_background_scope_respects_enabled_tabs() {
        let enabled_tabs = vec!["terminal".to_string(), "sftp".to_string()];

        assert!(background_scope_includes_content(
            BackgroundScope::Content,
            &enabled_tabs,
            "sftp",
        ));
        assert!(!background_scope_includes_content(
            BackgroundScope::Content,
            &enabled_tabs,
            "settings",
        ));
    }

    #[test]
    fn window_background_scope_ignores_content_tab_selection() {
        let enabled_tabs = vec!["terminal".to_string()];

        assert!(background_scope_includes_window(BackgroundScope::Window));
        assert!(!background_scope_includes_content(
            BackgroundScope::Window,
            &enabled_tabs,
            "terminal",
        ));
    }

    #[test]
    fn sidebar_background_uses_one_translucent_outer_surface() {
        assert_eq!(
            sidebar_surface_background(0x112233, true, 0.64),
            rgba(0x112233a3)
        );
        assert_eq!(
            context_sidebar_inner_surface_background(0x112233, true),
            rgba(0x00000000)
        );
        assert_eq!(
            context_sidebar_inner_surface_background(0x112233, false),
            rgb(0x112233)
        );
    }

    #[test]
    fn window_image_background_uses_an_opaque_app_base() {
        let tokens = oxideterm_theme::default_tokens();

        assert_eq!(
            workspace_background(&tokens, NativeVibrancyMode::System, true),
            rgb(tokens.ui.bg)
        );
        assert_ne!(
            workspace_background(&tokens, NativeVibrancyMode::System, false),
            rgb(tokens.ui.bg)
        );
    }

    #[test]
    pub(in crate::workspace) fn classifies_ssh_rsa_host_key_as_specific_legacy_case() {
        let algorithms = vec!["ssh-rsa".to_string()];

        assert!(oxideterm_ssh::server_only_offers_ssh_rsa(&algorithms));
        assert_eq!(
            ssh_algorithm_summary_key(SshAlgorithmDiagnosticKind::HostKey, &algorithms),
            "connections.trace.diagnostics.summary.host_key_ssh_rsa"
        );
    }

    #[test]
    pub(in crate::workspace) fn classifies_cbc_cipher_as_legacy_case() {
        let algorithms = vec!["aes128-cbc".to_string(), "3des-cbc".to_string()];

        assert!(oxideterm_ssh::server_offers_legacy_cipher(&algorithms));
        assert_eq!(
            ssh_algorithm_summary_key(SshAlgorithmDiagnosticKind::Cipher, &algorithms),
            "connections.trace.diagnostics.summary.cipher_legacy"
        );
    }
}
