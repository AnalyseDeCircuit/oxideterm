//! Browser Web Bluetooth discovery and transport form branch for the portable
//! connection studio.

use super::*;
use crate::form_model::SerialLineEnding;
use oxideterm_gpui_ui::button::button_with;

impl ConnectionFormView {
    pub(super) fn render_web_ble_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some((
            profile_name,
            scanning,
            device_name,
            device_id,
            services,
            selected_service_uuid,
            inspect_error,
            input_line_ending,
            local_echo,
            write_without_response,
            mtu,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.ble.profile_name.clone(),
                form.ble.scanning,
                form.ble.web_device_name.clone(),
                form.ble.web_device_id.clone(),
                form.ble.web_services.clone(),
                form.ble.selected_service_uuid,
                form.ble.inspect_error.clone(),
                form.ble.input_line_ending,
                form.ble.local_echo,
                form.ble.write_without_response,
                form.ble.mtu.clone(),
            )
        })
        else {
            return div().into_any_element();
        };

        let scan_label = if scanning {
            self.i18n.t("modals.new_connection.ble_scanning")
        } else {
            self.i18n.t("modals.new_connection.ble_scan")
        };

        let scan_button = button_with(
            &self.tokens,
            scan_label,
            ButtonOptions {
                variant: ButtonVariant::Default,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: scanning,
                ..ButtonOptions::default()
            },
        )
        .id("web-ble-scan-button")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.request(FormAction::RequestBleScan, Some(window), cx);
                cx.stop_propagation();
            }),
        );

        let mut discovery_card = div()
            .w_full()
            .p_3()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgb(theme.bg_sunken))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Self::render_lucide_icon(
                                LucideIcon::Bluetooth,
                                15.0,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(device_name.unwrap_or_else(|| {
                                        self.i18n.t("modals.new_connection.ble_web_no_selection")
                                    })),
                            ),
                    )
                    .child(scan_button),
            );

        if let Some(id) = device_id {
            discovery_card = discovery_card.child(
                div()
                    .font_family("Lilex")
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(theme.text_muted))
                    .child(id),
            );
        }

        if let Some(err) = inspect_error {
            discovery_card = discovery_card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.error))
                    .child(err),
            );
        }

        if !services.is_empty() {
            let mut service_list = div()
                .w_full()
                .rounded(px(self.tokens.radii.sm))
                .border_1()
                .border_color(rgba((theme.border << 8) | 0x66))
                .overflow_hidden()
                .flex()
                .flex_col();
            for (index, svc) in services.into_iter().enumerate() {
                let selected = selected_service_uuid == Some(svc.uuid);
                let uuid = svc.uuid;
                let writable = svc.has_write || svc.has_write_without_response;
                let flags = match (writable, svc.has_notify) {
                    (true, true) => "TX + RX",
                    (true, false) => "TX",
                    (false, true) => "RX",
                    (false, false) => "GATT",
                };
                service_list = service_list.child(
                    div()
                        .id(("web-ble-service-row", index))
                        .w_full()
                        .px_3()
                        .py_2()
                        .when(index > 0, |r| {
                            r.border_t_1()
                                .border_color(rgba((theme.border << 8) | 0x45))
                        })
                        .when(selected, |r| r.bg(rgba((theme.accent << 8) | 0x26)))
                        .hover(move |r| r.bg(rgb(theme.bg_hover)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .cursor_pointer()
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .font_weight(if selected {
                                            gpui::FontWeight::SEMIBOLD
                                        } else {
                                            gpui::FontWeight::MEDIUM
                                        })
                                        .text_color(rgb(theme.text))
                                        .child(svc.label),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .font_family("Lilex")
                                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(uuid.to_string()),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(self.tokens.radii.xs))
                                .border_1()
                                .border_color(rgba(
                                    (if writable && svc.has_notify {
                                        theme.success
                                    } else {
                                        theme.border
                                    } << 8)
                                        | 0x80,
                                ))
                                .font_family("Lilex")
                                .text_size(px(self.tokens.metrics.ui_text_2xs))
                                .text_color(rgb(if writable && svc.has_notify {
                                    theme.success
                                } else {
                                    theme.text_muted
                                }))
                                .child(flags),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.ble.selected_service_uuid = Some(uuid);
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
            }
            discovery_card = discovery_card.child(service_list);
        }

        let line_ending_buttons = div().flex().items_center().gap_1().children(
            [
                (SerialLineEnding::CrLf, "CRLF"),
                (SerialLineEnding::Lf, "LF"),
                (SerialLineEnding::Cr, "CR"),
            ]
            .into_iter()
            .map(|(ending, label)| {
                let active = input_line_ending == ending;
                div()
                    .id(SharedString::from(format!("web-ble-eol-{label}")))
                    .px_2()
                    .py_1()
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x26)
                    } else {
                        rgb(theme.bg_sunken)
                    })
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                    .cursor_pointer()
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ble.input_line_ending = ending;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            }),
        );

        let transport_card = div()
            .w_full()
            .p_3()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgb(theme.bg_sunken))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("terminal.serial_control.line_ending")),
                    )
                    .child(line_ending_buttons),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("terminal.serial_control.local_echo")),
                    )
                    .child(
                        switch_with(
                            &self.tokens,
                            "web-ble-local-echo",
                            String::new(),
                            local_echo,
                            SwitchOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.ble.local_echo = !form.ble.local_echo;
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text))
                            .child(
                                self.i18n
                                    .t("modals.new_connection.ble_write_without_response"),
                            ),
                    )
                    .child(
                        switch_with(
                            &self.tokens,
                            "web-ble-write-no-resp",
                            String::new(),
                            write_without_response,
                            SwitchOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.ble.write_without_response =
                                            !form.ble.write_without_response;
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(self.render_connection_field(
                self.i18n.t("modals.new_connection.ble_mtu"),
                &mtu,
                "247".into(),
                NewConnectionField::BleMtu,
                false,
                cx,
            ));

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.render_connection_field(
                self.i18n.t("modals.new_connection.ble_profile_name"),
                &profile_name,
                self.i18n.t("modals.new_connection.transport_ble"),
                NewConnectionField::BleProfileName,
                false,
                cx,
            ))
            .child(discovery_card)
            .child(transport_card)
            .into_any_element()
    }
}
