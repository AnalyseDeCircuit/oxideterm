impl WorkspaceApp {
    fn render_forward_type_badge(&self, forward_type: ForwardType) -> AnyElement {
        let (bg, text) = match forward_type {
            ForwardType::Local => (TW_BLUE_900, TW_BLUE_400),
            ForwardType::Remote => (TW_PURPLE_900, TW_PURPLE_400),
            ForwardType::Dynamic => (TW_YELLOW_900, TW_YELLOW_400),
        };
        div()
            .h(px(FORWARDS_TYPE_BADGE_H))
            .px_2()
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .bg(forwards_palette_alpha(bg, FORWARDS_TW_ALPHA_30))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(forwards_palette_color(text))
            .child(self.render_forward_ui_text(forward_type_key(forward_type, &self.i18n)))
            .into_any_element()
    }

    fn render_forward_status(
        &self,
        status: &ForwardStatus,
        stats: Option<ForwardStats>,
    ) -> AnyElement {
        let (dot, text_color) = match status {
            ForwardStatus::Active => (TW_GREEN_500, self.tokens.ui.text_muted),
            ForwardStatus::Stopped => (self.tokens.ui.text_muted, self.tokens.ui.text_muted),
            ForwardStatus::Suspended => (TW_ORANGE_500, TW_ORANGE_400),
            ForwardStatus::Starting => (TW_BLUE_500, self.tokens.ui.text_muted),
            ForwardStatus::Error => (TW_RED_500, self.tokens.ui.text_muted),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(text_color))
            .child(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(forwards_palette_color(dot)),
            )
            .child(self.render_forward_ui_text(self.i18n.t(forward_status_key(status))))
            .when_some(stats, |row, stats| {
                row.child(
                    div()
                        .ml_2()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            12.0,
                            rgb(self.tokens.ui.text_muted),
                        ))
                        .child(format!(
                            "{}/{} | ↑{} ↓{}",
                            stats.active_connections,
                            stats.connection_count,
                            format_bytes(stats.bytes_sent),
                            format_bytes(stats.bytes_received)
                        )),
                )
            })
            .into_any_element()
    }

    fn render_forward_button(
        &self,
        label: String,
        icon: Option<LucideIcon>,
        variant: ForwardButtonVariant,
        enabled: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let (bg, border, text, hover_bg) = match variant {
            ForwardButtonVariant::Primary => (
                rgb(theme.text),
                forwards_transparent(),
                theme.bg,
                forwards_theme_with_alpha(theme.text, 0xe6),
            ),
            ForwardButtonVariant::Secondary => (
                forwards_theme_panel_bg(theme.bg_panel, has_background),
                forwards_theme_border(theme.border, has_background),
                theme.text,
                forwards_theme_hover_bg(theme.bg_hover, has_background),
            ),
            ForwardButtonVariant::Ghost => (
                forwards_transparent(),
                forwards_transparent(),
                theme.text,
                forwards_theme_hover_bg(theme.bg_hover, has_background),
            ),
        };
        let icon = icon.map(|icon| Self::render_lucide_icon(icon, 14.0, rgb(text)));
        toolbar_button(
            &self.tokens,
            String::new(),
            icon,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: UiButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                show_label: false,
                background: Some(bg),
                border: Some(border),
                text_color: Some(rgb(text)),
                hover_background: Some(hover_bg),
                height: Some(36.0),
                padding_x: Some(16.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
        )
        // Forwards labels need the same CJK font fallback as the Tauri UI.
        // Keep that text element outside the shared primitive's plain label.
        .child(self.render_forward_ui_text(label))
        .when(enabled, |button| button.on_mouse_down(MouseButton::Left, listener))
    }

    fn render_forward_icon_button(
        &self,
        icon: LucideIcon,
        color: u32,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, 13.0, forwards_palette_color(color)),
            IconButtonOptions {
                size: 28.0,
                radius: ButtonRadius::Md,
                has_background,
                idle_opacity: 1.0,
                ..IconButtonOptions::compact(28.0)
            },
        )
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_forward_ui_text(&self, text: String) -> gpui::Div {
        let has_cjk = forwards_text_has_cjk(&text);
        div()
            .when(has_cjk, |label| {
                label.font_family(forwards_cjk_ui_font_family(
                    &self.settings_store.settings().appearance.ui_font_family,
                ))
            })
            .child(text)
    }

    fn forward_mono_font(&self) -> SharedString {
        settings_mono_font_family(self.settings_store.settings())
    }

    fn render_forwards_section_title(&self, label: String) -> AnyElement {
        div()
            .when(forwards_text_has_cjk(&label), |title| {
                title.font_family(forwards_cjk_ui_font_family(
                    &self.settings_store.settings().appearance.ui_font_family,
                ))
            })
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(label.to_uppercase())
            .into_any_element()
    }

    fn render_forwards_separator(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(forwards_theme_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    fn render_forwards_error(&self, error: &str) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.xs))
            .border_1()
            .border_color(forwards_palette_alpha(TW_RED_900, FORWARDS_TW_ALPHA_50))
            .bg(forwards_palette_alpha(TW_RED_950, FORWARDS_TW_ALPHA_30))
            .p_3()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(forwards_palette_color(TW_RED_400))
                    .child("⚠ Error"),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(self.forward_mono_font())
                    .text_color(rgb(self.tokens.ui.text))
                    .child(error.to_string()),
            )
            .into_any_element()
    }

    fn render_port_detection_banner(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        new_ports: Vec<DetectedPort>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .children(new_ports.into_iter().map(|port| {
                let dismiss_port = port.port;
                let forward_port = port.clone();
                let forward_node_id = node_id.clone();
                div()
                    .min_h(px(36.0))
                    .w_full()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(forwards_palette_alpha(TW_BLUE_500, FORWARDS_TW_ALPHA_30))
                    .bg(forwards_palette_alpha(TW_BLUE_500, FORWARDS_TW_ALPHA_05))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Radio,
                                14.0,
                                forwards_palette_color(TW_BLUE_400),
                            ))
                            .child(
                                div()
                                    .truncate()
                                    .flex()
                                    .items_center()
                                    .child(self.render_forward_ui_text(
                                        self.i18n.t("forwards.detection.detected"),
                                    ))
                                    .child(" ")
                                    .child(
                                        div()
                                            .font_family(settings_mono_font_family(
                                                self.settings_store.settings(),
                                            ))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(forwards_palette_color(TW_BLUE_300))
                                            .child(format!(":{}", port.port)),
                                    )
                                    .when_some(port.process_name.as_ref(), |text, process| {
                                        text.child(
                                            div()
                                                .ml_1()
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(format!(
                                                    "({process}{})",
                                                    port.pid
                                                        .map(|pid| format!(" #{pid}"))
                                                        .unwrap_or_default()
                                                )),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                self.render_forward_button(
                                    self.i18n.t("forwards.detection.forward"),
                                    Some(LucideIcon::ArrowRight),
                                    ForwardButtonVariant::Ghost,
                                    true,
                                    has_background,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.create_local_forward_for_detected_port(
                                            tab_id,
                                            forward_node_id.clone(),
                                            forward_port.clone(),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                                .h(px(24.0))
                                .px_2()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(forwards_palette_color(TW_BLUE_400)),
                            )
                            .child(self.render_forward_icon_button(
                                LucideIcon::X,
                                self.tokens.ui.text_muted,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.dismiss_detected_port(dismiss_port);
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )),
                    )
            }))
            .into_any_element()
    }

    fn render_remote_ports_section(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        forwards: &[ForwardRule],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let visible_ports: Vec<DetectedPort> = self
            .forwarding_view
            .detected_ports
            .iter()
            .filter(|port| port.port != 22)
            .cloned()
            .collect();
        let visible_port_count = visible_ports.len();
        let forwarded_ports: std::collections::HashSet<u16> = forwards
            .iter()
            .filter(|rule| {
                rule.forward_type == ForwardType::Local
                    && matches!(rule.status, ForwardStatus::Active | ForwardStatus::Starting)
            })
            .map(|rule| rule.target_port)
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Radio,
                        16.0,
                        forwards_palette_color(TW_EMERALD_400),
                    ))
                    .child(self.render_forwards_section_title(
                        self.i18n.t("forwards.detection.remotePorts"),
                    ))
                    .when(!self.forwarding_view.detected_ports.is_empty(), |header| {
                        header.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(format!("({})", visible_ports.len())),
                        )
                    }),
            )
            .child(
                div()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .overflow_hidden()
                    .bg(forwards_theme_card_bg(theme.bg_card, has_background))
                    .child(
                        self.forward_row_base(
                            FORWARDS_TABLE_HEADER_H,
                            forwards_theme_panel_bg(theme.bg_panel, has_background),
                            ForwardRowCorners::Top,
                        )
                        .border_b_1()
                        .border_color(forwards_theme_border(theme.border, has_background))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child(self.forward_cell(1.0, self.i18n.t("forwards.detection.port")))
                        .child(self.forward_cell(1.4, self.i18n.t("forwards.detection.bindAddr")))
                        .child(self.forward_cell(1.4, self.i18n.t("forwards.detection.process")))
                        .child(
                            div()
                                .w(px(128.0))
                                .pr(px(16.0))
                                .text_align(gpui::TextAlign::Right)
                                .child(self.render_forward_ui_text(
                                    self.i18n.t("forwards.detection.action"),
                                )),
                        ),
                    )
                    .when(visible_ports.is_empty(), |table| {
                        table.child(
                            div()
                                .h(px(72.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_b(px(self.tokens.radii.lg))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(if self.forwarding_view.port_scan_pending {
                                    self.i18n.t("forwards.detection.scanning")
                                } else if let Some(error) =
                                    self.forwarding_view.port_scan_error.as_ref()
                                {
                                    error.clone()
                                } else if self.forwarding_view.has_scanned_ports {
                                    self.i18n.t("forwards.detection.noPorts")
                                } else {
                                    self.i18n.t("forwards.detection.scanning")
                                }),
                        )
                    })
                    .children(visible_ports.into_iter().enumerate().map(|(index, port)| {
                        let already_forwarded = forwarded_ports.contains(&port.port);
                        self.render_detected_port_row(
                            node_id.clone(),
                            tab_id,
                            port,
                            already_forwarded,
                            index + 1 == visible_port_count,
                            has_background,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    fn render_detected_port_row(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        port: DetectedPort,
        already_forwarded: bool,
        rounded_bottom: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_port = port.clone();
        self.forward_row_base(
            FORWARDS_TABLE_ROW_H,
            forwards_theme_sunken_bg(theme.bg_sunken, has_background),
            if rounded_bottom {
                ForwardRowCorners::Bottom
            } else {
                ForwardRowCorners::None
            },
        )
        .border_b_1()
        .border_color(forwards_theme_border_half(theme.border, has_background))
        .hover(move |row| row.bg(forwards_theme_hover_bg(theme.bg_hover, has_background)))
        .text_size(px(self.tokens.metrics.ui_text_sm))
        .child(
            self.forward_cell_element(
                1.0,
                div()
                    .font_family(self.forward_mono_font())
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(forwards_palette_color(TW_EMERALD_400))
                    .child(port.port.to_string())
                    .into_any_element(),
            ),
        )
        .child(
            self.forward_cell_element(
                1.4,
                div()
                    .truncate()
                    .font_family(self.forward_mono_font())
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(if port.bind_addr.is_empty() {
                        "0.0.0.0".to_string()
                    } else {
                        port.bind_addr.clone()
                    })
                    .into_any_element(),
            ),
        )
        .child(
            self.forward_cell_element(
                1.4,
                div()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(match (port.process_name.clone(), port.pid) {
                        (Some(process), Some(pid)) => format!("{process} ({pid})"),
                        (Some(process), None) => process,
                        (None, _) => "—".to_string(),
                    })
                    .into_any_element(),
            ),
        )
        .child(
            div()
                .w(px(128.0))
                .pr(px(16.0))
                .flex()
                .justify_end()
                .child(if already_forwarded {
                    div()
                        .h(px(22.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(self.tokens.radii.sm))
                        .border_1()
                        .border_color(forwards_palette_alpha(TW_EMERALD_800, FORWARDS_TW_ALPHA_40))
                        .bg(forwards_palette_alpha(TW_EMERALD_900, FORWARDS_TW_ALPHA_30))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(forwards_palette_color(TW_EMERALD_400))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            12.0,
                            forwards_palette_color(TW_EMERALD_400),
                        ))
                        .child(self.render_forward_ui_text(
                            self.i18n.t("forwards.detection.alreadyForwarded"),
                        ))
                        .into_any_element()
                } else {
                    self.render_forward_button(
                        self.i18n.t("forwards.detection.forward"),
                        Some(LucideIcon::Play),
                        ForwardButtonVariant::Ghost,
                        true,
                        has_background,
                        cx.listener(move |this, _event, _window, cx| {
                            this.create_local_forward_for_detected_port(
                                tab_id,
                                node_id.clone(),
                                forward_port.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .h(px(24.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .into_any_element()
                }),
        )
        .into_any_element()
    }

    fn forward_row_base(
        &self,
        height: f32,
        bg: gpui::Rgba,
        corners: ForwardRowCorners,
    ) -> gpui::Div {
        div()
            .h(px(height))
            .w_full()
            .flex()
            .items_center()
            .bg(bg)
            .when(matches!(corners, ForwardRowCorners::Top), |row| {
                row.rounded_t(px(self.tokens.radii.lg))
            })
            .when(matches!(corners, ForwardRowCorners::Bottom), |row| {
                row.rounded_b(px(self.tokens.radii.lg))
            })
    }

    fn forward_cell(&self, flex: f32, text: String) -> AnyElement {
        self.forward_cell_element(
            flex,
            self.render_forward_ui_text(text)
                .truncate()
                .into_any_element(),
        )
    }

    fn forward_mono_cell(&self, flex: f32, text: String) -> AnyElement {
        self.forward_cell_element(
            flex,
            div()
                .truncate()
                .font_family(self.forward_mono_font())
                .child(text)
                .into_any_element(),
        )
    }

    fn forward_cell_element(&self, flex: f32, child: AnyElement) -> AnyElement {
        div()
            .flex_grow()
            .flex_basis(px(0.0))
            .min_w(px(0.0))
            .px_4()
            .when(flex > 1.0, |cell| cell.flex_grow())
            .child(child)
            .into_any_element()
    }

}
