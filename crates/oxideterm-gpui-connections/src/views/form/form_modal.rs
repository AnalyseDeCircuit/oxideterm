use super::*;

/// Contains only values needed to build one modal frame after releasing the Entity borrow.
struct ConnectionFormModalSnapshot {
    transport: NewConnectionTransport,
    name: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    gssapi_server_identity: String,
    gssapi_delegate_credentials: bool,
    gssapi_credentials_available: Option<bool>,
    password_present: bool,
    remote_desktop_profile_id: Option<String>,
    mosh_profile_id: Option<String>,
    standalone_sftp_profile_id: Option<String>,
    serial_profile_id: Option<String>,
    local_profile_id: Option<String>,
    telnet_profile_id: Option<String>,
    ble_profile_id: Option<String>,
    saved_password_keychain_id: Option<String>,
    password_loaded: bool,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    save_password: bool,
    empty_password: bool,
    group: String,
    notes: String,
    post_connect_command: String,
    pre_connect_command: String,
    proxy_command_enabled: bool,
    proxy_command: zeroize::Zeroizing<String>,
    proxy_command_configured: bool,
    legacy_ssh_compatibility: bool,
    agent_forwarding: bool,
    identity_agent: String,
    agent_available: Option<bool>,
    error: Option<String>,
    feedback_success: bool,
    pending: bool,
    serial_port_path: String,
    serial_baud_rate: String,
    mosh_server_executable: String,
    mosh_udp_host: String,
    mosh_udp_port: String,
    mosh_locale: String,
    connect_timeout_seconds_text: String,
    keep_alive_interval_seconds_text: String,
    keep_alive_count_max_text: String,
    active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
    totp_picker_expanded: bool,
    totp_credential_id: Option<String>,
    proxy_hops_len: usize,
    upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    x11_enabled: bool,
    single_channel_mode: bool,
    dedicated_new_terminal_connection: bool,
    open_sftp_on_connect: bool,
    appearance_customized: bool,
    ssh_algorithms_custom_count: usize,
    terminal_customized: bool,
    // True when the selected BLE service offers both notify and write, i.e. it
    // can back a read/write terminal channel.
    ble_can_open: bool,
}

impl ConnectionFormModalSnapshot {
    fn from_form(form: &NewConnectionForm) -> Self {
        Self {
            transport: form.transport,
            name: form.name.clone(),
            host: form.host.clone(),
            port: form.port.clone(),
            username: form.username.clone(),
            auth_tab: form.auth_tab,
            gssapi_enabled: form.gssapi_enabled,
            gssapi_server_identity: form.gssapi_server_identity.clone(),
            gssapi_delegate_credentials: form.gssapi_delegate_credentials,
            gssapi_credentials_available: form.gssapi_credentials_available,
            password_present: !form.password.is_empty(),
            remote_desktop_profile_id: form.remote_desktop_profile_id.clone(),
            mosh_profile_id: form.mosh_profile_id.clone(),
            standalone_sftp_profile_id: form.standalone_sftp_profile_id.clone(),
            serial_profile_id: form.serial_profile_id.clone(),
            local_profile_id: form.local_profile_id.clone(),
            telnet_profile_id: form.telnet_profile_id.clone(),
            ble_profile_id: form.ble.profile_id.clone(),
            saved_password_keychain_id: form.saved_password_keychain_id.clone(),
            password_loaded: form.password_loaded,
            key_path: form.key_path.clone(),
            managed_key_id: form.managed_key_id.clone(),
            cert_path: form.cert_path.clone(),
            save_password: form.save_password,
            empty_password: form.empty_password,
            group: form.group.clone(),
            notes: form.notes.clone(),
            post_connect_command: form.post_connect_command.clone(),
            pre_connect_command: form.pre_connect_command.clone(),
            proxy_command_enabled: form.proxy_command_enabled,
            // Rendering owns one bounded zeroizing copy; persisted forms retain only a keychain id.
            proxy_command: zeroize::Zeroizing::new(form.proxy_command.as_str().to_owned()),
            proxy_command_configured: form.proxy_command_keychain_id.is_some(),
            legacy_ssh_compatibility: form.legacy_ssh_compatibility,
            agent_forwarding: form.agent_forwarding,
            identity_agent: form.identity_agent.clone(),
            agent_available: form.agent_available,
            error: form.error.clone(),
            feedback_success: form.feedback_is_success(),
            pending: form.pending,
            serial_port_path: form.serial_port_path.clone(),
            serial_baud_rate: form.serial_baud_rate.clone(),
            mosh_server_executable: form.mosh_server_executable.clone(),
            mosh_udp_host: form.mosh_udp_host.clone(),
            mosh_udp_port: form.mosh_udp_port.clone(),
            mosh_locale: form.mosh_locale.clone(),
            connect_timeout_seconds_text: form.connect_timeout_seconds_text.clone(),
            keep_alive_interval_seconds_text: form.keep_alive_interval_seconds_text.clone(),
            keep_alive_count_max_text: form.keep_alive_count_max_text.clone(),
            active_drawer: form.active_drawer,
            totp_picker_expanded: form.totp_picker_expanded,
            totp_credential_id: form.totp_credential_id.clone(),
            proxy_hops_len: form.proxy_hops.len(),
            upstream_proxy_policy: form.upstream_proxy_policy,
            x11_enabled: form.x11_forwarding.enabled,
            single_channel_mode: form.ssh_channel_strategy.requires_dedicated_consumers(),
            dedicated_new_terminal_connection: form.dedicated_new_terminal_connection,
            open_sftp_on_connect: form.open_sftp_on_connect,
            appearance_customized: !form.icon.trim().is_empty()
                || !form.color.trim().is_empty()
                || !form.icon_background_color.trim().is_empty(),
            ssh_algorithms_custom_count: [
                !form.ssh_algorithms.kex.is_empty(),
                !form.ssh_algorithms.host_key.is_empty(),
                !form.ssh_algorithms.cipher.is_empty(),
                !form.ssh_algorithms.mac.is_empty(),
                !form.ssh_algorithms.compression.is_empty(),
            ]
            .into_iter()
            .filter(|customized| *customized)
            .count(),
            terminal_customized: form.terminal != Default::default()
                || !form.pre_connect_command.trim().is_empty()
                || !form.post_connect_command.trim().is_empty(),
            #[cfg(target_family = "wasm")]
            ble_can_open: false,
            #[cfg(not(target_family = "wasm"))]
            ble_can_open: form
                .ble
                .selected_service_uuid
                .and_then(|uuid| {
                    form.ble
                        .device_details
                        .as_ref()
                        .and_then(|details| details.services.iter().find(|s| s.uuid == uuid))
                })
                .map(|service| {
                    let mut notify = false;
                    let mut write = false;
                    for ch in &service.characteristics {
                        for prop in &ch.properties {
                            match *prop {
                                "notify" => notify = true,
                                "write" | "write-without-response" => write = true,
                                _ => {}
                            }
                        }
                    }
                    notify && write
                })
                .unwrap_or(false),
        }
    }
}

impl ConnectionFormView {
    pub(in crate::views) fn render_new_connection_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(ConnectionFormModalSnapshot::from_form)
        else {
            return div().into_any_element();
        };
        if form.gssapi_enabled && form.gssapi_credentials_available.is_none() {
            self.ensure_kerberos_credentials_availability(cx);
        }
        let theme = self.tokens.ui;
        let mode = new_connection_form_mode(
            self.connection_form_state(cx)
                .editing_saved_connection_id
                .as_deref(),
            self.connection_form_state(cx)
                .duplicating_saved_connection_id
                .as_deref(),
            self.connection_form_state(cx)
                .saved_connection_prompt_action,
        );
        let prompt_mode = mode == NewConnectionFormMode::SavedConnectionPrompt;
        let duplicate_mode = mode == NewConnectionFormMode::DuplicateTemplate;
        let edit_properties_mode = mode.submits_saved_connection_properties();
        let reauthentication_mode = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.standalone_connection_id.is_some());
        let remote_desktop_edit_mode = form.remote_desktop_profile_id.is_some();
        let mosh_edit_mode = form.mosh_profile_id.is_some();
        let serial_edit_mode = form.serial_profile_id.is_some();
        let telnet_edit_mode = form.telnet_profile_id.is_some();
        let standalone_sftp_edit_mode = form.standalone_sftp_profile_id.is_some();
        let ftp_edit_mode = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.ftp_profile_id.is_some());
        // Saved non-SSH profiles edit persisted assets without acquiring a
        // runtime owner. BLE joins this list so its editor hides the
        // transport selector like every other saved profile type.
        let saved_profile_edit_mode = remote_desktop_edit_mode
            || mosh_edit_mode
            || form.local_profile_id.is_some()
            || serial_edit_mode
            || telnet_edit_mode
            || standalone_sftp_edit_mode
            || ftp_edit_mode
            || form.ble_profile_id.is_some();
        let drill_down_mode = self
            .connection_form_state(cx)
            .drill_down_parent_node_id
            .is_some();
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let local_terminal_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::LocalTerminal;
        let serial_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Serial;
        let ble_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Ble;
        let telnet_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Telnet;
        let mosh_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Mosh;
        let standalone_sftp_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::StandaloneSftp;
        if standalone_sftp_mode {
            return self.render_standalone_sftp_connection_modal(
                window,
                !saved_profile_edit_mode,
                cx,
            );
        }
        let remote_desktop_protocol =
            if !prompt_mode && !duplicate_mode && !edit_properties_mode && !drill_down_mode {
                match form.transport {
                    NewConnectionTransport::Rdp => {
                        Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
                    }
                    NewConnectionTransport::Vnc => {
                        Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
                    }
                    _ => None,
                }
            } else {
                None
            };
        let wsl_graphics_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::WslGraphics;
        let local_transport_mode = serial_mode || telnet_mode || ble_mode;
        let ftp_mode = form.transport == NewConnectionTransport::Ftp;
        let remote_desktop_mode = remote_desktop_protocol.is_some();
        let ssh_submission_mode = !local_terminal_mode
            && !ftp_mode
            && !local_transport_mode
            && !remote_desktop_mode
            && !wsl_graphics_mode
            && !mosh_mode
            && !standalone_sftp_mode;
        let shows_transport_selector = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !saved_profile_edit_mode
            && !drill_down_mode;
        let shows_icon_field = connection_icon_field_visible(mode, drill_down_mode, form.transport);
        let title = if local_terminal_mode {
            self.i18n
                .t("modals.new_connection.transport_local_terminal")
        } else if drill_down_mode {
            self.i18n.t("ssh.drill_down.title")
        } else if prompt_mode {
            self.i18n
                .t("sessionManager.connect_prompt.title")
                .replace("{{name}}", &form.name)
        } else if duplicate_mode {
            self.i18n
                .t("sessionManager.edit_properties.duplicate_title")
        } else if edit_properties_mode || saved_profile_edit_mode {
            self.i18n.t("sessionManager.edit_properties.title")
        } else if ftp_mode {
            self.i18n.t("modals.new_connection.transport_ftp")
        } else if ble_mode {
            self.i18n.t("ssh.form.title")
        } else if mosh_mode {
            self.i18n.t("mosh.form.title")
        } else if standalone_sftp_mode {
            self.i18n.t("sftp.standalone.form_title")
        } else {
            self.i18n.t("ssh.form.title")
        };
        let description = if local_terminal_mode {
            self.i18n
                .t("modals.new_connection.local_terminal_description")
        } else if drill_down_mode {
            let parent_host = self
                .connection_form_state(cx)
                .drill_down_parent_node_id
                .as_ref()
                .and_then(|node_id| self.environment.node_titles.get(node_id))
                .cloned()
                .unwrap_or_default();
            self.i18n
                .t("ssh.drill_down.description")
                .replace("{{host}}", &parent_host)
                .replace("<host>", "")
                .replace("</host>", "")
        } else if prompt_mode {
            format!("{}@{}:{}", form.username, form.host, form.port)
        } else if duplicate_mode {
            self.i18n
                .t("sessionManager.edit_properties.duplicate_description")
        } else if edit_properties_mode || saved_profile_edit_mode {
            self.i18n.t("sessionManager.edit_properties.description")
        } else if telnet_mode {
            self.i18n.t("modals.new_connection.telnet_description")
        } else if ftp_mode {
            self.i18n.t("modals.new_connection.ftp_description")
        } else if mosh_mode {
            self.i18n.t("mosh.form.description")
        } else if standalone_sftp_mode {
            self.i18n.t("sftp.standalone.form_description")
        } else if ble_mode {
            self.i18n.t("modals.new_connection.ble_description")
        } else if serial_mode {
            self.i18n.t("modals.new_connection.serial_description")
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            self.i18n.t("modals.new_connection.rdp_description")
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
        {
            self.i18n.t("modals.new_connection.vnc_description")
        } else if wsl_graphics_mode {
            self.i18n
                .t("modals.new_connection.wsl_graphics_description")
        } else {
            self.i18n.t("ssh.form.subtitle")
        };
        let connect_timeout_valid = form
            .connect_timeout_seconds_text
            .trim()
            .parse::<u64>()
            .is_ok_and(|seconds| seconds > 0);
        let ssh_keepalive_valid = form
            .keep_alive_interval_seconds_text
            .trim()
            .parse::<u32>()
            .is_ok_and(|seconds| seconds > 0)
            && form
                .keep_alive_count_max_text
                .trim()
                .parse::<u32>()
                .is_ok_and(|count| count > 0);
        let has_required_fields = if local_terminal_mode {
            true
        } else if serial_mode {
            !form.serial_port_path.trim().is_empty()
                && form
                    .serial_baud_rate
                    .trim()
                    .parse::<u32>()
                    .is_ok_and(|baud| baud > 0)
        } else if telnet_mode {
            !form.host.trim().is_empty() && form.port.trim().parse::<u16>().is_ok()
        } else if mosh_mode {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && !form.mosh_server_executable.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
                && connect_timeout_valid
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && (remote_desktop_edit_mode
                    || form.empty_password
                    || form.password_present
                    || form.saved_password_keychain_id.is_some())
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else if remote_desktop_mode {
            !form.host.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else if wsl_graphics_mode {
            true
        } else if ble_mode {
            // The primary action is enabled only once a service with both
            // notify and write characteristics is selected in the details
            // table; that pairing is the minimum for a read/write channel.
            form.ble_can_open
        } else {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok()
                && connect_timeout_valid
                && ssh_keepalive_valid
        };
        let primary_disabled = form.pending || !has_required_fields;
        // Editing a saved BLE profile can persist metadata changes without a
        // live scan because the stored device identity is reused on save.
        let save_disabled =
            form.pending || !(has_required_fields || (ble_mode && form.ble_profile_id.is_some()));
        let form_visible = self.connection_form_state(cx).presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let viewport_margin = composer::studio_viewport_margin(window);
        let (modal_width, studio_fixed_height) = if drill_down_mode {
            let max_w = (f32::from(window.viewport_size().width) - viewport_margin * 2.0).max(0.0);
            // Drill-down renders 4 footer actions (Cancel, Save, Connect,
            // Save & Connect); 640px fits verbose European labels on one row.
            (640.0_f32.min(max_w), None)
        } else if prompt_mode {
            let max_w = (f32::from(window.viewport_size().width) - viewport_margin * 2.0).max(0.0);
            (520.0_f32.min(max_w), None)
        } else {
            let (width, height) = composer::studio_modal_size(
                window,
                &self.tokens,
                shows_transport_selector,
                modal_max_height,
            );
            (width, Some(height))
        };
        let modal_max_height = modal_max_height
            .min((f32::from(window.viewport_size().height) - viewport_margin * 2.0).max(0.0));
        let studio_layout = composer::StudioLayoutMetrics::compute(
            modal_width,
            &self.tokens,
            shows_transport_selector,
        );
        let light = theme.is_light();
        let modal_shadows = vec![
            gpui::BoxShadow {
                color: rgba(if light { 0x00000018 } else { 0x00000066 }).into(),
                offset: point(px(0.0), px(20.0)),
                blur_radius: px(44.0),
                spread_radius: px(-8.0),
                inset: false,
            },
            gpui::BoxShadow {
                color: rgba(if light { 0x0000000c } else { 0x00000038 }).into(),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(12.0),
                spread_radius: px(-2.0),
                inset: false,
            },
        ];
        let uses_ssh_summary_navigation = ssh_submission_mode
            && !prompt_mode
            && !drill_down_mode
            && !mosh_mode
            && !standalone_sftp_mode
            && !serial_mode
            && !ble_mode
            && !local_terminal_mode
            && !telnet_mode
            && !ftp_mode
            && !wsl_graphics_mode
            && !remote_desktop_mode;
        let uses_summary_navigation = !prompt_mode
            && !drill_down_mode
            && !wsl_graphics_mode
            && !standalone_sftp_mode
            && (uses_ssh_summary_navigation
                || mosh_mode
                || remote_desktop_mode
                || serial_mode
                || ble_mode
                || telnet_mode
                || ftp_mode
                || local_terminal_mode);
        let show_general_page = !uses_summary_navigation || form.active_drawer.is_none();
        let subpage_header = (uses_summary_navigation && form.active_drawer.is_some()).then(|| {
            use crate::form_state::ConnectionFormDrawer;
            let active_drawer = form.active_drawer.unwrap();
            let root_label = if serial_mode {
                self.i18n.t("modals.new_connection.serial_section_title")
            } else if ble_mode {
                self.i18n.t("modals.new_connection.ble_form_title")
            } else {
                self.i18n.t("ssh.form.basic_information")
            };
            let (parent_drawer, parent_label, current_label) = match active_drawer {
                ConnectionFormDrawer::Route => (
                    None,
                    root_label,
                    if remote_desktop_mode {
                        self.i18n
                            .t("modals.new_connection.remote_desktop_ssh_gateway")
                    } else {
                        self.i18n.t("ssh.form.connection_route")
                    },
                ),
                ConnectionFormDrawer::SshSecurity => {
                    (None, root_label, self.i18n.t("ssh.form.ssh_options"))
                }
                ConnectionFormDrawer::SshAlgorithms => {
                    if uses_ssh_summary_navigation {
                        (
                            Some(ConnectionFormDrawer::SshSecurity),
                            self.i18n.t("ssh.form.ssh_options"),
                            self.i18n.t("ssh.form.ssh_algorithms"),
                        )
                    } else {
                        (None, root_label, self.i18n.t("ssh.form.ssh_algorithms"))
                    }
                }
                ConnectionFormDrawer::MoshOptions => {
                    (None, root_label, self.i18n.t("mosh.form.advanced"))
                }
                ConnectionFormDrawer::FtpOptions => {
                    (None, root_label, self.i18n.t("ssh.form.connection_route"))
                }
                ConnectionFormDrawer::VncPreferences => (
                    None,
                    root_label,
                    self.i18n.t("modals.new_connection.vnc_preferences_title"),
                ),
                ConnectionFormDrawer::RemoteFeatures => (
                    None,
                    root_label,
                    self.i18n
                        .t("modals.new_connection.remote_desktop_features_title"),
                ),
                ConnectionFormDrawer::BleTransportOptions => (
                    None,
                    root_label,
                    self.i18n
                        .t("modals.new_connection.ble_transport_options_title"),
                ),
                ConnectionFormDrawer::Terminal => {
                    (None, root_label, self.i18n.t("ssh.form.terminal_options"))
                }
                ConnectionFormDrawer::Notes => {
                    (None, root_label, self.i18n.t("ssh.form.appearance"))
                }
                _ => (None, root_label.clone(), root_label),
            };
            div()
                .flex_none()
                .w_full()
                .h(px(40.0))
                .px(px(studio_layout.content_padding_x - 6.0))
                .bg(rgb(theme.bg_elevated))
                .border_b_1()
                .border_color(rgba((theme.border << 8) | 0xb3))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("ssh-subpage-back")
                        .debug_selector(|| "ssh-subpage-back".to_string())
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::ChevronLeft,
                            14.0,
                            rgb(theme.text_muted),
                        ))
                        .child(parent_label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_new_connection_select(cx);
                                this.reset_studio_form_scroll();
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.active_drawer = parent_drawer;
                                        form.ssh_algorithm_menu = None;
                                        form.field_focused = false;
                                        clear_connection_selection(form);
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgba((theme.text_muted << 8) | 0x88))
                        .child("/"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(current_label),
                )
                .into_any_element()
        });
        // A long scrolling form uses an opaque theme surface: underlying text
        // must not bleed through, and live backdrop blur would add GPU work
        // to every scroll frame.
        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    // Tauri NewConnectionModal is a Radix Dialog; overlay
                    // pointer-down calls onOpenChange(false), which closes and
                    // restores focus to the active pane in native.
                    this.close_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                &self.tokens,
                "new-connection-form-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(
                oxideterm_gpui_ui::motion::form_transition(
                    &self.tokens,
                    "new-connection-form-enter",
                    modal_container(&self.tokens)
                .bg(rgb(theme.bg_elevated))
                .shadow(modal_shadows)
                .overflow_hidden()
                .w(px(modal_width))
                .when_some(studio_fixed_height, |container, h| container.h(px(h)))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(self.render_studio_top_chrome(
                    title,
                    Some(description),
                    shows_transport_selector,
                    !uses_summary_navigation && shows_icon_field,
                    cx,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex()
                        .when(shows_transport_selector, |content| {
                            content.child(self.render_studio_protocol_sidebar(studio_layout, cx))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_h(px(0.0))
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .when_some(subpage_header, |pane, header| pane.child(header))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h(px(0.0))
                                        .min_w(px(0.0))
                                        .relative()
                                        .child(
                                            div()
                                                .id("new-connection-modal-form-scroll")
                                                .size_full()
                                                .px(px(studio_layout.content_padding_x))
                                                .py(px(if uses_summary_navigation && show_general_page {
                                                    14.0
                                                } else {
                                                    18.0
                                                }))
                                                .overflow_y_scroll()
                                                .track_scroll(
                                                    &self.selectable_text_scroll_handle(
                                                        "new-connection-modal-form-scroll",
                                                    ),
                                                )
                                                .on_scroll(cx.listener(
                                                    |this, _event, _window, cx| {
                                                        // Tauri/Radix closes select content when the modal
                                                        // scroll body moves its trigger. Native caches the
                                                        // trigger anchor explicitly, so clear both popup
                                                        // ownership and the stale group-select bounds here.
                                                        if this.connection_form_state(cx).open_select.is_some() {
                                                            this.close_new_connection_select(cx);
                                                            this.clear_new_connection_select_anchor(cx);
                                                            cx.notify();
                                                        }
                                                    },
                                                ))
                                                .child(
                                            div()
                                        .flex()
                                        .flex_col()
                                        .min_w(px(0.0))
                                        .gap(px(if uses_summary_navigation && show_general_page {
                                            10.0
                                        } else {
                                            self.tokens.metrics.modal_section_gap
                                        }))
                                        .when(mosh_mode && show_general_page, |content| {
                                            content.child(self.render_mosh_plugin_status(cx))
                                        })
                                        .when(local_terminal_mode, |content| {
                                            content.child(self.render_local_terminal_form_branch(
                                                form.active_drawer,
                                                studio_layout,
                                                cx,
                                            ))
                                        })
                                        .when(serial_mode, |content| {
                                            content.child(self.render_serial_form_branch(
                                                form.active_drawer,
                                                studio_layout,
                                                cx,
                                            ))
                                        })
                                        .when(ble_mode, |content| {
                                            #[cfg(target_family = "wasm")]
                                            {
                                                content.child(self.render_web_ble_form_branch(cx))
                                            }
                                            #[cfg(not(target_family = "wasm"))]
                                            {
                                                content.child(self.render_ble_form_branch(
                                                    form.active_drawer,
                                                    studio_layout,
                                                    window,
                                                    cx,
                                                ))
                                            }
                                        })
                                        .when(telnet_mode, |content| {
                                            content.child(self.render_telnet_form_branch(
                                                form.active_drawer,
                                                studio_layout,
                                                cx,
                                            ))
                                        })
                                        .when(ftp_mode, |content| {
                                            content.child(self.render_ftp_form_branch(
                                                form.active_drawer,
                                                studio_layout,
                                                cx,
                                            ))
                                        })
                                        .when(wsl_graphics_mode, |content| {
                                            content.child(self.render_wsl_graphics_form_branch(cx))
                                        })
                                        .when_some(remote_desktop_protocol, |content, protocol| {
                                            content.child(self.render_remote_desktop_form_branch(
                                                protocol,
                                                form.active_drawer,
                                                studio_layout,
                                                cx,
                                            ))
                                        })
                                        .when(
                                            !serial_mode
                                                && !ble_mode
                                                && !local_terminal_mode
                                                && !telnet_mode
                                                && !ftp_mode
                                                && !wsl_graphics_mode
                                                && !remote_desktop_mode,
                                            |content| {
                                                content
                                .when(standalone_sftp_mode, |content| {
                                    content.child(
                                        div()
                                            .p_3()
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(
                                                (theme.warning << 8)
                                                    | TAURI_PROMPT_FEEDBACK_BORDER_ALPHA,
                                            ))
                                            .bg(rgba(
                                                (theme.warning << 8)
                                                    | TAURI_PROMPT_FEEDBACK_ALPHA,
                                            ))
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .text_color(rgb(theme.text))
                                            .child(self.i18n.t("sftp.standalone.usage_notice")),
                                    )
                                })
                                .when(
                                    (ssh_submission_mode || mosh_mode || standalone_sftp_mode)
                                        && !prompt_mode
                                        && !drill_down_mode
                                        && show_general_page,
                                    |content| {
                                        let group_label = if edit_properties_mode
                                            || mosh_edit_mode
                                            || standalone_sftp_edit_mode
                                        {
                                            self.i18n.t("sessionManager.edit_properties.group")
                                        } else {
                                            self.i18n.t("ssh.form.group")
                                        };
                                        let target_fields = div()
                                            .w_full()
                                            .flex()
                                            .flex_col()
                                            .gap(px(10.0))
                                            .child(self.render_identity_row(
                                                self.i18n.t("ssh.form.name"),
                                                &form.name,
                                                self.i18n.t("ssh.form.name_placeholder"),
                                                NewConnectionField::Name,
                                                Some((group_label, &form.group)),
                                                uses_summary_navigation && shows_icon_field,
                                                studio_layout,
                                                cx,
                                            ))
                                            .child(self.render_unified_address_bar(
                                                &form.username,
                                                &form.host,
                                                if standalone_sftp_mode {
                                                    self.i18n
                                                        .t("sftp.standalone.host_placeholder")
                                                } else {
                                                    self.i18n.t("ssh.form.host_placeholder")
                                                },
                                                &form.port,
                                                studio_layout.content_width,
                                                cx,
                                            ));
                                        content
                                            .child(target_fields)
                                            .child(self.form_hairline())
                                    },
                                )
                                .when(drill_down_mode, |content| {
                                    content
                                        .child(self.render_drill_saved_next_hop_picker(cx))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(self.tokens.metrics.form_host_port_gap))
                                                .child(div().flex_1().child(
                                                    self.render_connection_field(
                                                        self.i18n.t("ssh.drill_down.target_host"),
                                                        &form.host,
                                                        self.i18n
                                                            .t("ssh.drill_down.target_host_placeholder"),
                                                        NewConnectionField::Host,
                                                        false,
                                                        cx,
                                                    ),
                                                ))
                                                .child(
                                                    div()
                                                        .w(px(self.tokens.metrics.form_port_width))
                                                        .child(self.render_connection_field(
                                                            self.i18n.t("ssh.drill_down.port"),
                                                            &form.port,
                                                            SSH_DEFAULT_PORT_TEXT.to_string(),
                                                            NewConnectionField::Port,
                                                            false,
                                                            cx,
                                                        )),
                                                ),
                                        )
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.drill_down.username"),
                                            &form.username,
                                            self.i18n.t("ssh.drill_down.username_placeholder"),
                                            NewConnectionField::Username,
                                            false,
                                            cx,
                                        ))
                                })
                                .when_some(
                                    if prompt_mode {
                                        form.error.clone()
                                    } else {
                                        None
                                    },
                                    |content, error| {
                                        content.child(self.render_prompt_feedback_box(
                                            error,
                                            form.feedback_success,
                                        ))
                                    },
                                )
                                .when(show_general_page, |content| content.child({
                                    let selector = self.render_auth_selector(
                                        form.auth_tab,
                                        if prompt_mode {
                                            AuthSelectorContext::Prompt
                                        } else if drill_down_mode {
                                            AuthSelectorContext::DrillDown
                                        } else if mode == NewConnectionFormMode::EditProperties
                                            || mosh_edit_mode
                                            || standalone_sftp_edit_mode
                                        {
                                            AuthSelectorContext::EditProperties
                                        } else {
                                            AuthSelectorContext::Standard
                                        },
                                        false,
                                        cx,
                                    );
                                    let show_totp_select = form.totp_picker_expanded
                                        || form.totp_credential_id.is_some();
                                    let totp_active = form.totp_credential_id.is_some();
                                    let totp_toggle_chip = (!prompt_mode && !drill_down_mode).then(|| {
                                        div()
                                            .id("composer-totp-toggle")
                                            .h(px(self.tokens.metrics.ui_tabs_list_height + 2.0))
                                            .px(px(10.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(if totp_active || show_totp_select {
                                                rgb(theme.accent)
                                            } else {
                                                rgba((theme.border << 8) | 0x80)
                                            })
                                            .bg(if totp_active || show_totp_select {
                                                rgba((theme.accent << 8) | 0x18)
                                            } else {
                                                rgba((theme.bg_sunken << 8) | 0xc8)
                                            })
                                            .cursor_pointer()
                                            .hover(|el| el.bg(rgb(theme.bg_hover)))
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(if totp_active || show_totp_select {
                                                theme.accent
                                            } else {
                                                theme.text_muted
                                            }))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::KeyRound,
                                                12.0,
                                                rgb(if totp_active || show_totp_select {
                                                    theme.accent
                                                } else {
                                                    theme.text_muted
                                                }),
                                            ))
                                            .child("TOTP")
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.update_connection_form_state(cx, |state| {
                                                        if let Some(form) = state.form.as_mut() {
                                                            form.totp_picker_expanded =
                                                                !form.totp_picker_expanded;
                                                        }
                                                    });
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }),
                                            )
                                    });
                                    let authentication = div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(if uses_summary_navigation {
                                            10.0
                                        } else {
                                            14.0
                                        }))
                                        .child(if studio_layout.content_width < 420.0 {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(8.0))
                                                .child(div().w_full().min_w_0().child(selector))
                                                .when_some(totp_toggle_chip, |col, chip| {
                                                    col.child(
                                                        div()
                                                            .w_full()
                                                            .flex()
                                                            .justify_end()
                                                            .child(chip),
                                                    )
                                                })
                                        } else {
                                            div()
                                                .flex()
                                                .items_end()
                                                .gap(px(10.0))
                                                .child(div().flex_1().min_w_0().child(selector))
                                                .when_some(totp_toggle_chip, |row, chip| {
                                                    row.child(chip)
                                                })
                                        })
                                        .when(show_totp_select, |content| {
                                            content.child(self.render_connection_totp_select(false, cx))
                                        })
                                .when(form.auth_tab == SshAuthTab::Password, |content| {
                                    if edit_properties_mode
                                        && form.saved_password_keychain_id.is_some()
                                    {
                                        let content = if form.password_loaded {
                                            content.child(self.render_connection_secret_field(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.saved_password",
                                                ),
                                                String::new(),
                                                NewConnectionField::Password,
                                                cx,
                                            ))
                                        } else {
                                            content.child(
                                                self.render_edit_saved_password_field(cx),
                                            )
                                        };
                                        content.child(self.render_connection_hint(
                                            self.i18n.t(
                                                "sessionManager.edit_properties.password_hint",
                                            ),
                                        ))
                                    } else if edit_properties_mode {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    } else if prompt_mode {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    } else if drill_down_mode {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.drill_down.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    } else if mosh_edit_mode || standalone_sftp_edit_mode {
                                        content
                                            .child(self.render_connection_secret_field(
                                                self.i18n.t("ssh.form.password"),
                                                String::new(),
                                                NewConnectionField::Password,
                                                cx,
                                            ))
                                            .when(
                                                form.saved_password_keychain_id.is_some(),
                                                |content| {
                                                    content.child(self.render_connection_hint(
                                                        self.i18n.t(
                                                            "sessionManager.edit_properties.password_hint",
                                                        ),
                                                    ))
                                                },
                                            )
                                    } else {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    }
                                })
                                .when(form.auth_tab == SshAuthTab::Password && !prompt_mode, |content| {
                                    let shows_save_checkbox = !edit_properties_mode
                                        && !drill_down_mode
                                        && !mosh_edit_mode
                                        && !standalone_sftp_edit_mode;
                                    let empty_password_switch = self.render_connection_switch_row(
                                        "ssh-empty-password",
                                        self.i18n.t("ssh.form.use_empty_password"),
                                        form.empty_password,
                                        false,
                                        |form| {
                                            form.empty_password = !form.empty_password;
                                            form.field_focused = false;
                                            form.saved_password_keychain_id = None;
                                            form.password_from_store = false;
                                            form.password_loaded = true;
                                            if form.empty_password {
                                                form.password.zeroize();
                                            }
                                        },
                                        cx,
                                    );
                                    content.child(
                                        if shows_save_checkbox
                                            && uses_summary_navigation
                                            && !studio_layout.compact_content
                                        {
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(18.0))
                                            .child(div().flex_1().min_w_0().child(
                                                self.render_connection_switch_row(
                                                    "ssh-save-password",
                                                    self.i18n.t("ssh.form.save_password"),
                                                    form.save_password,
                                                    false,
                                                    |form| form.save_password = !form.save_password,
                                                    cx,
                                                ),
                                            ))
                                            .child(div().flex_1().min_w_0().child(empty_password_switch))
                                    } else {
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .when(shows_save_checkbox, |rows| {
                                                rows.child(self.render_connection_switch_row(
                                                    "ssh-save-password",
                                                    self.i18n.t("ssh.form.save_password"),
                                                    form.save_password,
                                                    false,
                                                    |form| form.save_password = !form.save_password,
                                                    cx,
                                                ))
                                                .child(self.form_hairline())
                                            })
                                            .child(empty_password_switch)
                                    })
                                })
                                .when(prompt_mode, |content| {
                                    content.child(self.render_connection_switch_row(
                                        "ssh-save-credentials",
                                        self.i18n.t("ssh.form.save_credentials_after_login"),
                                        form.save_password,
                                        false,
                                        |form| form.save_password = !form.save_password,
                                        cx,
                                    ))
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::DefaultKey
                                        && !prompt_mode
                                        && !edit_properties_mode
                                        && !mosh_edit_mode
                                        && !standalone_sftp_edit_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.default_key_desc"),
                                            ))
                                            .child(self.render_connection_secret_field(
                                                self.i18n.t("ssh.form.passphrase"),
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                cx,
                                            ))
                                    },
                                )
                                .when(
                                    form.auth_tab == SshAuthTab::SshKey
                                        || ((prompt_mode
                                            || edit_properties_mode
                                            || mosh_edit_mode
                                            || standalone_sftp_edit_mode)
                                            && form.auth_tab == SshAuthTab::DefaultKey),
                                    |content| {
                                        let key_label = if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.key_path")
                                        } else if edit_properties_mode
                                            || mosh_edit_mode
                                            || standalone_sftp_edit_mode
                                        {
                                            self.i18n.t("sessionManager.edit_properties.key_path")
                                        } else {
                                            self.i18n.t("ssh.form.key_file")
                                        };
                                        let key_placeholder = if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.key_path_placeholder")
                                        } else {
                                            "~/.ssh/id_ed25519".to_string()
                                        };
                                        let key_field = if prompt_mode {
                                            self.render_connection_field(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder,
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder,
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        };
                                        content
                                            .child(key_field)
                                            .child(self.render_connection_secret_field(
                                                if drill_down_mode {
                                                    self.i18n.t("ssh.drill_down.passphrase")
                                                } else {
                                                    self.i18n.t("ssh.form.passphrase")
                                                },
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                cx,
                                            ))
                                            .when(
                                                edit_properties_mode
                                                    || mosh_edit_mode
                                                    || standalone_sftp_edit_mode,
                                                |content| {
                                                    content.child(self.render_connection_hint(
                                                        self.i18n.t(
                                                            "sessionManager.edit_properties.passphrase_hint",
                                                        ),
                                                    ))
                                                },
                                            )
                                    },
                                )
                                .when(form.auth_tab == SshAuthTab::ManagedKey, |content| {
                                    content
                                        .child(self.render_managed_key_select(
                                            self.i18n.t("ssh.form.managed_key"),
                                            &form.managed_key_id,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.managed_key_hint"),
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::Certificate, |content| {
                                    let content = if prompt_mode {
                                        content
                                    } else {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.certificate_note"),
                                        ))
                                    };
                                    content
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        })
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                cx,
                                            )
                                        })
                                        .child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            cx,
                                        ))
                                        .when(
                                            edit_properties_mode
                                                || mosh_edit_mode
                                                || standalone_sftp_edit_mode,
                                            |content| {
                                                content.child(self.render_connection_hint(
                                                    self.i18n.t(
                                                        "sessionManager.edit_properties.passphrase_hint",
                                                    ),
                                                ))
                                            },
                                        )
                                })
                                .when(form.auth_tab == SshAuthTab::Agent, |content| {
                                    let content = content
                                        .child(self.render_connection_hint(if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.agent_desc")
                                        } else {
                                            self.i18n.t("ssh.form.agent_desc")
                                        }))
                                        .when(!prompt_mode, |content| {
                                            content
                                                .child(self.render_connection_field(
                                                    self.i18n.t("ssh.form.agent_endpoint"),
                                                    &form.identity_agent,
                                                    self.i18n
                                                        .t("ssh.form.agent_endpoint_placeholder"),
                                                    NewConnectionField::IdentityAgent,
                                                    false,
                                                    cx,
                                                ))
                                                .child(self.render_connection_hint(
                                                    self.i18n.t("ssh.form.agent_endpoint_hint"),
                                                ))
                                                .when(!drill_down_mode, |content| {
                                                    content.child(self.render_agent_status(
                                                        form.agent_available,
                                                    ))
                                                })
                                        });
                                    if drill_down_mode {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.drill_down.agent_hint"),
                                        ))
                                    } else if !prompt_mode {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.agent_hint"),
                                        ))
                                    } else {
                                        content
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::TwoFactor
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_desc"),
                                            ))
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_hint"),
                                            ))
                                            .child(self.render_connection_hint_with_color(
                                                self.i18n.t("ssh.form.two_factor_warning"),
                                                self.tokens.ui.warning,
                                            ))
                                    },
                                )
                                .when(
                                    !prompt_mode
                                        && !drill_down_mode
                                        && !standalone_sftp_mode
                                        && !uses_summary_navigation,
                                    |content| {
                                        content.child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .flex_col()
                                                .gap(px(10.0))
                                                .child(self.form_hairline())
                                                .child(self.render_connection_switch_row(
                                                    "ssh-kerberos",
                                                    self.i18n.t("ssh.form.kerberos_preferred"),
                                                    form.gssapi_enabled,
                                                    false,
                                                    |form| {
                                                        form.gssapi_enabled = !form.gssapi_enabled;
                                                    },
                                                    cx,
                                                ))
                                                .when(form.gssapi_enabled, |sec| {
                                                    sec.child(self.render_connection_hint(
                                                        self.i18n.t("ssh.form.gssapi_desc"),
                                                    ))
                                                    .child(self.render_connection_field(
                                                        self.i18n.t(
                                                            "ssh.form.gssapi_server_identity",
                                                        ),
                                                        &form.gssapi_server_identity,
                                                        self.i18n.t(
                                                            "ssh.form.gssapi_server_identity_placeholder",
                                                        ),
                                                        NewConnectionField::GssapiServerIdentity,
                                                        false,
                                                        cx,
                                                    ))
                                                    .child(
                                                        self.render_kerberos_credentials_status(
                                                            form.gssapi_credentials_available,
                                                        ),
                                                    )
                                                    .child(
                                                        self.render_connection_switch_row_with_warning(
                                                            "kerberos-delegation-help",
                                                            "kerberos-delegation-tooltip",
                                                            "ssh.form.gssapi_delegate_credentials",
                                                            "ssh.form.gssapi_delegation_warning",
                                                            form.gssapi_delegate_credentials,
                                                            false,
                                                            |form| {
                                                                form.gssapi_delegate_credentials =
                                                                    !form.gssapi_delegate_credentials;
                                                            },
                                                            cx,
                                                        ),
                                                    )
                                                }),
                                        )
                                    },
                                )
                                .into_any_element();
                                    authentication
                                }))
                                .when(uses_ssh_summary_navigation && show_general_page, |content| {
                                    use crate::form_state::ConnectionFormDrawer;
                                    let route_highlighted = form.proxy_command_enabled
                                        || form.proxy_hops_len > 0
                                        || form.upstream_proxy_policy
                                            != NewConnectionUpstreamProxyPolicy::UseGlobal;
                                    let route_summary = if form.proxy_command_enabled {
                                        self.i18n.t("ssh.form.proxy_command")
                                    } else if form.proxy_hops_len > 0 {
                                        self.i18n
                                            .t("ssh.form.proxy_chain_count")
                                            .replace("{{count}}", &form.proxy_hops_len.to_string())
                                            .trim_end_matches(['.', '。'])
                                            .to_string()
                                    } else {
                                        match form.upstream_proxy_policy {
                                            NewConnectionUpstreamProxyPolicy::UseGlobal => {
                                                self.i18n.t("modals.upstream_proxy.use_global")
                                            }
                                            NewConnectionUpstreamProxyPolicy::Direct => {
                                                self.i18n.t("modals.upstream_proxy.direct")
                                            }
                                            NewConnectionUpstreamProxyPolicy::Custom => {
                                                self.i18n.t("modals.upstream_proxy.custom")
                                            }
                                        }
                                    };
                                    let mut security_parts: Vec<String> = Vec::new();
                                    if form.agent_forwarding {
                                        security_parts.push(self.i18n.t("ssh.auth.agent"));
                                    }
                                    if form.x11_enabled {
                                        security_parts.push("X11".to_string());
                                    }
                                    if form.gssapi_enabled {
                                        security_parts.push("Kerberos".to_string());
                                    }
                                    if form.open_sftp_on_connect {
                                        security_parts.push("SFTP".to_string());
                                    }
                                    if form.legacy_ssh_compatibility {
                                        security_parts
                                            .push(self.i18n.t("ssh.form.ssh_algorithms_legacy"));
                                    }
                                    if form.ssh_algorithms_custom_count > 0 {
                                        security_parts.push(
                                            self.i18n
                                                .t("ssh.form.ssh_algorithms_custom_count")
                                                .replace(
                                                    "{{count}}",
                                                    &form.ssh_algorithms_custom_count.to_string(),
                                                ),
                                        );
                                    }
                                    let security_highlighted = !security_parts.is_empty()
                                        || form.single_channel_mode
                                        || form.dedicated_new_terminal_connection;
                                    let security_summary = if !security_parts.is_empty() {
                                        security_parts.join(" · ")
                                    } else if security_highlighted {
                                        self.i18n.t("ssh.form.ssh_algorithms_custom")
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_default")
                                    };
                                    let terminal_summary = if form.terminal_customized {
                                        self.i18n.t("ssh.form.ssh_algorithms_custom")
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_default")
                                    };
                                    let notes_trimmed = form.notes.trim();
                                    let notes_highlighted =
                                        !notes_trimmed.is_empty() || form.appearance_customized;
                                    let notes_summary = if !notes_trimmed.is_empty() {
                                        notes_trimmed
                                            .lines()
                                            .next()
                                            .unwrap_or(notes_trimmed)
                                            .to_string()
                                    } else if form.appearance_customized {
                                        self.i18n.t("ssh.form.ssh_algorithms_custom")
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_default")
                                    };

                                    content.child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "ssh-summary-Route",
                                                LucideIcon::Network,
                                                self.i18n.t("ssh.form.connection_route"),
                                                route_summary,
                                                route_highlighted,
                                                ConnectionFormDrawer::Route,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "ssh-summary-SshSecurity",
                                                LucideIcon::Shield,
                                                self.i18n.t("ssh.form.ssh_options"),
                                                security_summary,
                                                security_highlighted,
                                                ConnectionFormDrawer::SshSecurity,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "ssh-summary-Terminal",
                                                LucideIcon::Terminal,
                                                self.i18n.t("ssh.form.terminal_options"),
                                                terminal_summary,
                                                form.terminal_customized,
                                                ConnectionFormDrawer::Terminal,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "ssh-summary-Notes",
                                                LucideIcon::Sparkles,
                                                self.i18n.t("ssh.form.appearance"),
                                                notes_summary,
                                                notes_highlighted,
                                                ConnectionFormDrawer::Notes,
                                                cx,
                                            )),
                                    )
                                })
                                .when(mosh_mode && show_general_page, |content| {
                                    use crate::form_state::ConnectionFormDrawer;
                                    let route_highlighted = form.proxy_hops_len > 0;
                                    let route_summary = if form.proxy_hops_len > 0 {
                                        self.i18n
                                            .t("ssh.form.proxy_chain_count")
                                            .replace("{{count}}", &form.proxy_hops_len.to_string())
                                            .trim_end_matches(['.', '。'])
                                            .to_string()
                                    } else {
                                        self.i18n.t("modals.upstream_proxy.direct")
                                    };
                                    let mosh_custom = !form.mosh_udp_host.trim().is_empty()
                                        || !form.mosh_udp_port.trim().is_empty()
                                        || !form.mosh_locale.trim().is_empty()
                                        || !form.pre_connect_command.trim().is_empty()
                                        || form.mosh_server_executable.trim() != "mosh-server"
                                        || form.gssapi_enabled;
                                    let mosh_summary = if form.gssapi_enabled {
                                        "Kerberos".to_string()
                                    } else if mosh_custom {
                                        self.i18n.t("ssh.form.ssh_algorithms_custom")
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_default")
                                    };
                                    let algorithms_summary = if form.ssh_algorithms_custom_count > 0 {
                                        self.i18n
                                            .t("ssh.form.ssh_algorithms_custom_count")
                                            .replace(
                                                "{{count}}",
                                                &form.ssh_algorithms_custom_count.to_string(),
                                            )
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_follow_preset")
                                    };
                                    let notes_trimmed = form.notes.trim();
                                    let notes_highlighted =
                                        !notes_trimmed.is_empty() || form.appearance_customized;
                                    let notes_summary = if !notes_trimmed.is_empty() {
                                        notes_trimmed
                                            .lines()
                                            .next()
                                            .unwrap_or(notes_trimmed)
                                            .to_string()
                                    } else if form.appearance_customized {
                                        self.i18n.t("ssh.form.ssh_algorithms_custom")
                                    } else {
                                        self.i18n.t("ssh.form.ssh_algorithms_default")
                                    };
                                    content.child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "mosh-summary-Route",
                                                LucideIcon::Network,
                                                self.i18n.t("ssh.form.connection_route"),
                                                route_summary,
                                                route_highlighted,
                                                ConnectionFormDrawer::Route,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "mosh-summary-MoshOptions",
                                                LucideIcon::Settings,
                                                self.i18n.t("mosh.form.advanced"),
                                                mosh_summary,
                                                mosh_custom,
                                                ConnectionFormDrawer::MoshOptions,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "mosh-summary-SshAlgorithms",
                                                LucideIcon::Lock,
                                                self.i18n.t("ssh.form.ssh_algorithms"),
                                                algorithms_summary,
                                                form.ssh_algorithms_custom_count > 0,
                                                ConnectionFormDrawer::SshAlgorithms,
                                                cx,
                                            ))
                                            .child(self.form_hairline())
                                            .child(self.render_connection_disclosure_row(
                                                "mosh-summary-Notes",
                                                LucideIcon::Sparkles,
                                                self.i18n.t("ssh.form.appearance"),
                                                notes_summary,
                                                notes_highlighted,
                                                ConnectionFormDrawer::Notes,
                                                cx,
                                            )),
                                    )
                                })
                                .when(
                                    !prompt_mode
                                        && !drill_down_mode
                                        && !mosh_mode
                                        && !standalone_sftp_mode
                                        && form.active_drawer.is_some(),
                                    |content| {
                                        use crate::form_state::ConnectionFormDrawer;
                                        let page_body = match form.active_drawer {
                                            Some(ConnectionFormDrawer::Route) => Some(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                                    .child(self.render_proxy_command_section(
                                                        form.proxy_command_enabled,
                                                        &form.proxy_command,
                                                        form.proxy_command_configured,
                                                        false,
                                                        cx,
                                                    ))
                                                    .when(!form.proxy_command_enabled, |route| {
                                                        route
                                                            .child(self.render_upstream_proxy_policy_section(false, cx))
                                                            .child(self.render_proxy_chain_section(false, cx))
                                                    })
                                                    .into_any_element(),
                                            ),
                                            Some(ConnectionFormDrawer::SshSecurity) => Some(
                                                self.render_connection_ssh_options(
                                                    form.agent_forwarding,
                                                    form.legacy_ssh_compatibility,
                                                    studio_layout.content_width,
                                                    cx,
                                                ),
                                            ),
                                            Some(ConnectionFormDrawer::SshAlgorithms) => {
                                                Some(self.render_ssh_algorithms_page(cx))
                                            }
                                            Some(ConnectionFormDrawer::Terminal) => Some(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                                    .child(self.render_connection_terminal_options(
                                                        studio_layout.narrow_content,
                                                        cx,
                                                    ))
                                                    .child(self.form_hairline())
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .gap(px(12.0))
                                                            .child(self.render_connection_field(
                                                                self.i18n.t("ssh.form.pre_connect_command"),
                                                                &form.pre_connect_command,
                                                                self.i18n.t(
                                                                    "ssh.form.pre_connect_command_placeholder",
                                                                ),
                                                                NewConnectionField::PreConnectCommand,
                                                                false,
                                                                cx,
                                                            ))
                                                            .child(self.render_connection_field(
                                                                self.i18n.t("ssh.form.post_connect_command"),
                                                                &form.post_connect_command,
                                                                self.i18n.t(
                                                                    "ssh.form.post_connect_command_placeholder",
                                                                ),
                                                                NewConnectionField::PostConnectCommand,
                                                                false,
                                                                cx,
                                                            )),
                                                    )
                                                    .into_any_element(),
                                            ),
                                            Some(ConnectionFormDrawer::Notes) => Some(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                                    .when(shows_icon_field, |col| {
                                                        col.child(self.render_connection_appearance_options(cx))
                                                            .child(div().h(px(1.0)).bg(rgb(theme.border)))
                                                    })
                                                    .child(self.render_connection_notes_fields(&form.notes, cx))
                                                    .into_any_element(),
                                            ),
                                            _ => None,
                                        };
                                        content.when_some(page_body, |c, body| c.child(body))
                                    },
                                )
                                .when(mosh_mode && form.active_drawer.is_some(), |content| {
                                    use crate::form_state::ConnectionFormDrawer;
                                    let page_body = match form.active_drawer {
                                        Some(ConnectionFormDrawer::Route) => Some(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(self.tokens.metrics.modal_section_gap))
                                                .child(self.render_connection_hint(
                                                    self.i18n.t("mosh.form.jump_route_hint"),
                                                ))
                                                .child(self.render_proxy_chain_section(false, cx))
                                                .into_any_element(),
                                        ),
                                        Some(ConnectionFormDrawer::MoshOptions) => {
                                            Some(self.render_mosh_advanced_fields(
                                                &form.mosh_server_executable,
                                                &form.mosh_udp_host,
                                                &form.mosh_udp_port,
                                                &form.mosh_locale,
                                                studio_layout,
                                                cx,
                                            ))
                                        }
                                        Some(ConnectionFormDrawer::SshAlgorithms) => {
                                            Some(self.render_ssh_algorithms_page(cx))
                                        }
                                        Some(ConnectionFormDrawer::Notes) => Some(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(self.tokens.metrics.modal_section_gap))
                                                .when(shows_icon_field, |col| {
                                                    col.child(self.render_connection_appearance_options(cx))
                                                        .child(div().h(px(1.0)).bg(rgb(theme.border)))
                                                })
                                                .child(self.render_connection_notes_fields(&form.notes, cx))
                                                .into_any_element(),
                                        ),
                                        _ => None,
                                    };
                                    content.when_some(page_body, |c, body| c.child(body))
                                })
                                    })
                                ),
                                )
                                .child(
                                    oxideterm_gpui_ui::Scrollbar::new(
                                        &self.selectable_text_scroll_handle(
                                            "new-connection-modal-form-scroll",
                                        ),
                                    )
                                    .axis(oxideterm_gpui_ui::ScrollbarAxis::Vertical),
                                ),
                        )
                        ),
                )
                .child({
                    let show_test_button = !reauthentication_mode
                        && !edit_properties_mode
                        && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                        && !drill_down_mode
                        && (ssh_submission_mode || standalone_sftp_mode);
                    let footer_feedback = if prompt_mode {
                        None
                    } else {
                        form.error.clone()
                    };
                    let has_left_cluster = show_test_button || footer_feedback.is_some();
                    let left_cluster = has_left_cluster.then(|| {
                        div()
                            .when(footer_feedback.is_some(), |left| {
                                left.flex_1().min_w(px(140.0))
                            })
                            .when(footer_feedback.is_none(), |left| left.flex_none())
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .when(show_test_button, |left| {
                                left.child(self.render_connection_button(
                                    self.i18n.t("ssh.form.test"),
                                    false,
                                    ConnectionButtonAction::Test,
                                    primary_disabled,
                                    cx,
                                ))
                            })
                            .when_some(footer_feedback, |left, error| {
                                let feedback_color = if form.feedback_success {
                                    theme.success
                                } else {
                                    theme.error
                                };
                                let icon = if form.feedback_success {
                                    LucideIcon::CheckCircle
                                } else {
                                    LucideIcon::AlertCircle
                                };
                                left.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(feedback_color))
                                        .child(Self::render_lucide_icon(
                                            icon,
                                            13.0,
                                            rgb(feedback_color),
                                        ))
                                        .child(div().min_w_0().truncate().child(error)),
                                )
                            })
                    });
                    let right_cluster = div()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(self.render_connection_button(
                            self.i18n.t("ssh.form.cancel"),
                            false,
                            ConnectionButtonAction::Cancel,
                            false,
                            cx,
                        ))
                        .when(reauthentication_mode, |footer| {
                            footer.child(self.render_connection_button(
                                self.i18n.t("ssh.form.connect"),
                                true,
                                ConnectionButtonAction::Connect,
                                primary_disabled,
                                cx,
                            ))
                        })
                        .when(
                            !reauthentication_mode
                                && !edit_properties_mode
                                && !saved_profile_edit_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                                && !remote_desktop_mode
                                && !wsl_graphics_mode,
                            |footer| {
                                let primary_label = if local_transport_mode || local_terminal_mode {
                                    self.i18n.t("modals.new_connection.local_save_and_open")
                                } else if standalone_sftp_mode {
                                    self.i18n.t("sftp.standalone.save_and_open")
                                } else if form.pending && drill_down_mode {
                                    self.i18n.t("ssh.drill_down.connecting")
                                } else {
                                    self.i18n.t("ssh.form.save_and_connect")
                                };
                                footer
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        save_disabled,
                                        cx,
                                    ))
                                    .child(self.render_split_primary_action_button(
                                        primary_label,
                                        primary_disabled,
                                        cx,
                                    ))
                            },
                        )
                        .when(
                            !reauthentication_mode
                                && (edit_properties_mode
                                    || saved_profile_edit_mode
                                    || self.connection_form_state(cx).saved_connection_prompt_action.is_some()),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    if self.connection_form_state(cx).saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Test)
                                    {
                                        self.i18n.t("ssh.form.test")
                                    } else if self.connection_form_state(cx).saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Connect)
                                    {
                                        self.i18n.t("ssh.form.connect")
                                    } else if edit_properties_mode
                                        && self
                                            .connection_form_state(cx).editing_saved_connection_connect_after_save_node_id
                                            .is_some()
                                    {
                                        self.i18n
                                            .t("sessionManager.edit_properties.save_and_reconnect")
                                    } else if edit_properties_mode || saved_profile_edit_mode {
                                        self.i18n.t("sessionManager.edit_properties.save")
                                    } else {
                                        self.i18n.t("modals.new_connection.local_open")
                                    },
                                    true,
                                    if (edit_properties_mode || saved_profile_edit_mode)
                                        && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                                    {
                                        ConnectionButtonAction::Save
                                    } else {
                                        ConnectionButtonAction::Connect
                                    },
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            remote_desktop_mode
                                && !reauthentication_mode
                                && !edit_properties_mode
                                && !remote_desktop_edit_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none(),
                            |footer| {
                                footer
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_split_primary_action_button(
                                        self.i18n.t("ssh.form.save_and_connect"),
                                        primary_disabled,
                                        cx,
                                    ))
                            },
                        )
                        .when(
                            wsl_graphics_mode
                                && !edit_properties_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none(),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("modals.new_connection.wsl_graphics_open"),
                                    true,
                                    ConnectionButtonAction::Connect,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        );

                    modal_footer(&self.tokens)
                        .flex_none()
                        // Allow the footer to grow if narrow viewports or long
                        // localized labels wrap buttons onto a second row.
                        .h_auto()
                        .min_h(px(self.tokens.metrics.modal_footer_height))
                        .flex_wrap()
                        .bg(rgb(theme.bg_panel))
                        .px(px(studio_layout.content_padding_x))
                        .py(px(13.0))
                        .when(has_left_cluster, |footer| footer.justify_between())
                        .when(!has_left_cluster, |footer| footer.justify_end())
                        .gap(px(12.0))
                        .when_some(left_cluster, |footer, left| footer.child(left))
                        .child(right_cluster)
                }),
                    form_visible,
                ),
        )
        .when(!form_visible, |backdrop| {
            // The exiting payload remains mounted only for painting; this top
            // event shield prevents stale form actions during delayed cleanup.
            backdrop.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll(|_event, _window, cx| cx.stop_propagation()),
            )
        })
        .into_any_element()
    }

    #[cfg(target_family = "wasm")]
    fn render_drill_saved_next_hop_picker(&self, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    #[cfg(not(target_family = "wasm"))]
    fn render_drill_saved_next_hop_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(parent_node_id) = self
            .connection_form_state(cx)
            .drill_down_parent_node_id
            .clone()
        else {
            return div().into_any_element();
        };
        let parent_title = self
            .environment
            .node_titles
            .get(&parent_node_id)
            .cloned()
            .unwrap_or_default();
        let description = self
            .i18n
            .t("sessions.saved_next_hop.description")
            .replace("{{host}}", &parent_title);
        let connections = self.environment.connections.clone();
        let has_connections = !connections.is_empty();
        let mut list = div().flex().flex_col().gap(px(4.0));
        for connection in connections {
            list = list.child(self.render_drill_saved_next_hop_row(
                parent_node_id.clone(),
                connection,
                cx,
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgba((theme.bg_card << 8) | 0x66))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessions.saved_next_hop.title")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(description),
                    ),
            )
            .when(!has_connections, |section| {
                section.child(
                    self.render_connection_hint(self.i18n.t("sessions.saved_next_hop.empty")),
                )
            })
            .when(has_connections, |section| {
                section.child(
                    div()
                        .id("drill-saved-next-hop-scroll")
                        .max_h(px(180.0))
                        .overflow_y_scroll()
                        .track_scroll(
                            &self.selectable_text_scroll_handle("drill-saved-next-hop-scroll"),
                        )
                        .child(list),
                )
            })
            .into_any_element()
    }

    #[cfg(not(target_family = "wasm"))]
    fn render_drill_saved_next_hop_row(
        &self,
        parent_node_id: oxideterm_ssh::NodeId,
        connection: oxideterm_connections::ConnectionInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let connection_id = connection.id.clone();
        let detail = format!(
            "{}@{}:{}",
            connection.username, connection.host, connection.port
        );
        let proxy_hop_count = connection.proxy_chain.len();
        let proxy_badge = self
            .i18n
            .t("sessions.saved_next_hop.proxy_chain_badge")
            .replace("{{count}}", &proxy_hop_count.to_string());

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(8.0))
            .py(px(6.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                13.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(connection.name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(detail),
                    ),
            )
            .when(proxy_hop_count > 0, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((theme.accent << 8) | 0x1a))
                        .px(px(6.0))
                        .py(px(2.0))
                        .text_size(px(10.0))
                        .text_color(rgb(theme.accent))
                        .child(proxy_badge),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.request(
                        FormAction::ConnectNextHop {
                            parent: parent_node_id.clone(),
                            connection_id: connection_id.clone(),
                        },
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
