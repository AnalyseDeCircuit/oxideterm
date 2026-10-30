// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! BLE discovery UI for the new-connection form: the transport's form branch
//! (adapter picker, scan table, in-place GATT inspection) plus the scan,
//! inspect and selection actions behind it.

use super::*;
use gpui::InteractiveElement;

impl WorkspaceApp {
    pub(super) fn render_ble_form_branch(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The window text system measures shaped text so tooltip predicates
        // can compare content width against the real cell widths below.
        let text_system = window.text_system();
        let theme = self.tokens.ui;
        let Some((
            ble_profile_name,
            notes,
            group,
            adapters,
            adapters_loading,
            adapter_index,
            devices,
            scanning,
            scan_started,
            selected_device_id,
            inspecting,
            device_details,
            inspect_error,
            selected_service_uuid,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.ble.profile_name.clone(),
                form.notes.clone(),
                form.group.clone(),
                form.ble.adapters.clone(),
                form.ble.adapters_loading,
                form.ble.selected_adapter_index,
                form.ble.devices.clone(),
                form.ble.scanning,
                form.ble.scan_started,
                form.ble.selected_device_id.clone(),
                form.ble.inspecting,
                form.ble.device_details.clone(),
                form.ble.inspect_error.clone(),
                form.ble.selected_service_uuid,
            )
        })
        else {
            return div().into_any_element();
        };
        let device_list_focus = self.connection_form_state(cx).ble_device_list_focus.clone();

        let adapter_value = adapters
            .get(adapter_index)
            .map(|adapter| adapter.display_label())
            .unwrap_or_default();
        let adapter_control = if adapters_loading {
            self.render_connection_hint(self.i18n.t("modals.new_connection.ble_loading_adapters"))
                .into_any_element()
        } else if adapters.is_empty() {
            self.render_connection_hint(self.i18n.t("modals.new_connection.ble_no_adapters"))
                .into_any_element()
        } else {
            self.render_new_connection_select_control(
                NewConnectionSelect::BleAdapter,
                adapter_value,
                false,
                false,
                cx,
            )
        };
        // Scanning needs a live adapter; the button only guards against empty
        // hardware. While a scan is running the button stays enabled and acts
        // as a manual stop instead of locking the user out.
        let scan_disabled = adapters_loading || adapters.is_empty();
        let scan_label = if scanning {
            self.i18n.t("modals.new_connection.ble_stop")
        } else {
            self.i18n.t("modals.new_connection.ble_scan")
        };
        let scan_icon = if scanning {
            Self::render_lucide_icon(LucideIcon::Square, 14.0, rgb(self.tokens.ui.text))
        } else {
            Self::render_lucide_icon(LucideIcon::RefreshCw, 14.0, rgb(self.tokens.ui.text))
        };
        let adapter_available = !adapters_loading && !adapters.is_empty();

        // The results frame exists only once a scan has run or produced rows;
        // an always-present empty frame reads as a broken panel. Its contents
        // stay mutually exclusive so one region owns every scan state.
        let device_list: Option<AnyElement> = if adapter_available
            && (scanning || scan_started || !devices.is_empty())
        {
            let content: AnyElement = if scanning {
                self.render_connection_hint(self.i18n.t("modals.new_connection.ble_scanning"))
                    .into_any_element()
            } else if devices.is_empty() {
                self.render_connection_hint(self.i18n.t("modals.new_connection.ble_no_devices"))
                    .into_any_element()
            } else {
                // Column widths are fixed so mono addresses and pills align
                // across rows; only the device name flexes.
                let colors = oxideterm_gpui_ui::TauriTableColors {
                    header_border: rgb(self.tokens.ui.border),
                    header_bg: rgb(self.tokens.ui.bg_sunken),
                    row_border: rgb(self.tokens.ui.border),
                    row_hover_bg: rgb(self.tokens.ui.bg_hover),
                    row_selected_bg: rgb(self.tokens.ui.bg_active),
                };
                let metrics = oxideterm_gpui_ui::TauriTableMetrics::from_tokens(&self.tokens);
                let header_cell = |flexible: bool, width: f32, label: &str| {
                    let cell = if flexible {
                        div().flex_1().min_w(px(100.0))
                    } else {
                        div().w(px(width)).flex_none()
                    };
                    cell.text_align(gpui::TextAlign::Center)
                        .truncate()
                        .child(label.to_string())
                        .into_any_element()
                };
                let header = oxideterm_gpui_ui::tauri_table_header(&self.tokens, colors, metrics)
                    .child(header_cell(
                        true,
                        0.0,
                        &self.i18n.t("modals.new_connection.ble_column_name"),
                    ))
                    .child(header_cell(
                        false,
                        122.0,
                        &self.i18n.t("modals.new_connection.ble_column_address"),
                    ))
                    .child(header_cell(
                        false,
                        62.0,
                        &self.i18n.t("modals.new_connection.ble_column_rssi"),
                    ))
                    .child(header_cell(
                        false,
                        88.0,
                        &self.i18n.t("modals.new_connection.ble_column_actions"),
                    ));
                let text_options =
                    |flexible: bool, width: f32| oxideterm_gpui_ui::TauriTableCellOptions {
                        width,
                        min_width: 100.0,
                        flexible,
                        padding_left: 0.0,
                        primary_text_size: self.tokens.metrics.ui_text_sm,
                        meta_text_size: self.tokens.metrics.ui_text_xs,
                        mono_font: Some(gpui::SharedString::from(self.tokens.metrics.font_family)),
                    };
                // The frame probe below reports the table's laid-out width with
                // a one-frame delay; before the first report the flexible name
                // column keeps its tooltip off, which is the safe default.
                let scan_table_width = self.connection_form_state(cx).ble_scan_table_width;
                let ui_font_family = oxideterm_gpui_ui::tauri_ui_font_family(
                    &self.settings_store.settings().appearance.ui_font_family,
                );
                let cell_font = |weight: gpui::FontWeight, family: gpui::SharedString| gpui::Font {
                    weight,
                    ..gpui::font(family)
                };
                let scan_name_width = scan_table_width.map(|table_width| {
                    // Border-box frame: 1px border per side plus the .p
                    // padding, then the row's .px padding and the fixed
                    // address, RSSI and actions columns. min_w(100) can only
                    // widen the real name cell, which makes tooltips
                    // over-eager but never missing.
                    table_width
                        - 2.0 * self.tokens.spacing.one
                        - 2.0
                        - 2.0 * metrics.padding_x
                        - 122.0
                        - 62.0
                        - 88.0
                });
                let mut body = div();
                for device in devices {
                    let selected = selected_device_id.as_ref() == Some(&device.id);
                    let label = device
                        .name
                        .clone()
                        .unwrap_or_else(|| device.address.clone());
                    // Only the name column can clip; the fixed columns fit
                    // their widths by construction.
                    let name_fits = scan_name_width.is_none_or(|available| {
                        oxideterm_gpui_ui::shaped_text_width(
                            &label,
                            cell_font(gpui::FontWeight::MEDIUM, ui_font_family.clone()),
                            self.tokens.metrics.ui_text_sm,
                            text_system,
                        ) <= available
                    });
                    let name_cell = oxideterm_gpui_ui::tauri_table_cell(
                        &self.tokens,
                        &text_options(true, 0.0),
                        oxideterm_gpui_ui::TauriTableCellStyle::Primary,
                        label.clone(),
                    )
                    .id(gpui::ElementId::Name(
                        format!("ble-scan-name-{}", device.address).into(),
                    ))
                    .when(!name_fits, |cell| {
                        let tokens = self.tokens.clone();
                        let tooltip_label = label.clone();
                        cell.tooltip(move |_window, cx| {
                            oxideterm_gpui_ui::tooltip::tooltip_view(
                                tokens.clone(),
                                tooltip_label.clone(),
                                None,
                                cx,
                            )
                        })
                    });
                    let mut row = oxideterm_gpui_ui::tauri_table_row(colors, metrics, selected)
                        .child(name_cell)
                        .child(oxideterm_gpui_ui::tauri_table_cell(
                            &self.tokens,
                            &text_options(false, 122.0),
                            oxideterm_gpui_ui::TauriTableCellStyle::MetaMono,
                            device.address.clone(),
                        ));
                    row = row.child(oxideterm_gpui_ui::tauri_table_cell(
                        &self.tokens,
                        &text_options(false, 62.0),
                        oxideterm_gpui_ui::TauriTableCellStyle::Meta,
                        device
                            .rssi
                            .map(|rssi| format!("{rssi} dBm"))
                            .unwrap_or_default(),
                    ));
                    let device_id = device.id.clone();
                    let detail_button = self.workspace_toolbar_action_button(
                        self.i18n.t("modals.new_connection.ble_inspect"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            // Selecting the row first anchors the expanding
                            // detail table to the clicked row; without it the
                            // table would render under the previously selected
                            // row. Inspect then clears stale details.
                            this.set_new_connection_ble_device(device_id.clone(), cx);
                            this.inspect_new_connection_ble_device(device_id.clone(), cx);
                            cx.stop_propagation();
                        }),
                    );
                    row = row.child(oxideterm_gpui_ui::tauri_table_checkbox_cell(
                        88.0,
                        detail_button,
                    ));
                    let device_id = device.id.clone();
                    row = row.on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.set_new_connection_ble_device(device_id.clone(), cx);
                            // A second click on the same row reads the GATT
                            // layout over a temporary link; single clicks
                            // keep plain selection semantics.
                            if event.click_count >= 2 {
                                this.inspect_new_connection_ble_device(device_id.clone(), cx);
                            }
                            // Clicking a row parks focus on the list so a
                            // following Enter can inspect the selection.
                            let list_focus =
                                this.connection_form_state(cx).ble_device_list_focus.clone();
                            window.focus(&list_focus, cx);
                            cx.stop_propagation();
                        }),
                    );
                    body = body.child(row);
                    // Expanding details in place keeps the inspected device
                    // visually anchored to its row; the table lives only on
                    // the selected row so other rows stay compact.
                    if selected {
                        // The detail wrapper (with padding) is appended only
                        // when there is actual content; otherwise every selected
                        // row would carry blank spacing below it.
                        let wrapper = |detail: AnyElement| {
                            div()
                                .w_full()
                                .pl(px(24.0))
                                .py(px(self.tokens.spacing.two))
                                .child(detail)
                        };
                        if inspecting {
                            body = body.child(wrapper(
                                self.render_connection_hint(
                                    self.i18n.t("modals.new_connection.ble_inspecting"),
                                )
                                .into_any_element(),
                            ));
                        } else if let Some(error) = &inspect_error {
                            body = body.child(wrapper(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.error))
                                    .child(error.clone())
                                    .into_any_element(),
                            ));
                        } else if let Some(details) = &device_details {
                            let detail_colors = oxideterm_gpui_ui::TauriTableColors {
                                header_border: rgb(self.tokens.ui.border),
                                header_bg: rgb(self.tokens.ui.bg_sunken),
                                row_border: rgb(self.tokens.ui.border),
                                row_hover_bg: rgb(self.tokens.ui.bg_hover),
                                row_selected_bg: rgb(self.tokens.ui.bg_active),
                            };
                            let detail_metrics =
                                oxideterm_gpui_ui::TauriTableMetrics::from_tokens(&self.tokens);
                            let detail_name_width = 140.0;
                            // Same border-box arithmetic as the scan table,
                            // minus the detail wrapper's 24px indent and the
                            // fixed name column.
                            let detail_props_width = scan_table_width.map(|table_width| {
                                table_width
                                    - 2.0 * self.tokens.spacing.one
                                    - 2.0
                                    - 24.0
                                    - 2.0 * detail_metrics.padding_x
                                    - detail_name_width
                            });
                            let detail_text = |flexible: bool, width: f32| {
                                oxideterm_gpui_ui::TauriTableCellOptions {
                                    width,
                                    min_width: 60.0,
                                    flexible,
                                    padding_left: 0.0,
                                    primary_text_size: self.tokens.metrics.ui_text_xs,
                                    meta_text_size: self.tokens.metrics.ui_text_xs,
                                    mono_font: Some(gpui::SharedString::from(
                                        self.tokens.metrics.font_family,
                                    )),
                                }
                            };
                            fn property_code(name: &str) -> Option<&'static str> {
                                match name {
                                    "read" => Some("R"),
                                    "write-without-response" => Some("WwR"),
                                    "write" => Some("W"),
                                    "notify" => Some("N"),
                                    _ => None,
                                }
                            }
                            fn short_uuid(uuid: &str) -> &str {
                                uuid.split('-').next().unwrap_or(uuid)
                            }
                            let mut detail_table = div().flex().flex_col().w_full();
                            for service in &details.services {
                                let service_selected = selected_service_uuid == Some(service.uuid);
                                let mut service_label =
                                    oxideterm_ble::service_display_name(&service.uuid);
                                if !service.primary {
                                    service_label.push_str(
                                        &self.i18n.t("modals.new_connection.ble_secondary_suffix"),
                                    );
                                }
                                let characteristics: String = service
                                    .characteristics
                                    .iter()
                                    .filter_map(|ch| {
                                        let props: Vec<&str> = ch
                                            .properties
                                            .iter()
                                            .filter_map(|p| property_code(p))
                                            .collect();
                                        if props.is_empty() {
                                            None
                                        } else {
                                            Some(format!(
                                                "{} [{}]",
                                                short_uuid(&ch.uuid.to_string()),
                                                props.join(" ")
                                            ))
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                let service_uuid_full = service.uuid.to_string();
                                let name_id = gpui::ElementId::Name(
                                    format!("ble-svc-name-{}", short_uuid(&service_uuid_full))
                                        .into(),
                                );
                                // The name column is fixed-width, so the
                                // shaped label decides whether the full UUID
                                // hint is needed.
                                let name_fits = oxideterm_gpui_ui::shaped_text_width(
                                    &service_label,
                                    cell_font(
                                        gpui::FontWeight::NORMAL,
                                        gpui::SharedString::from(self.tokens.metrics.font_family),
                                    ),
                                    self.tokens.metrics.ui_text_xs,
                                    text_system,
                                ) <= detail_name_width;
                                let name_cell = oxideterm_gpui_ui::tauri_table_cell(
                                    &self.tokens,
                                    &detail_text(false, detail_name_width),
                                    oxideterm_gpui_ui::TauriTableCellStyle::MetaMono,
                                    service_label.clone(),
                                )
                                .id(name_id)
                                .when(!name_fits, |cell| {
                                    let tokens = self.tokens.clone();
                                    let service_uuid_full = service_uuid_full.clone();
                                    cell.tooltip(move |_window, cx| {
                                        oxideterm_gpui_ui::tooltip::tooltip_view(
                                            tokens.clone(),
                                            service_uuid_full.clone(),
                                            None,
                                            cx,
                                        )
                                    })
                                });
                                let props_id = gpui::ElementId::Name(
                                    format!(
                                        "ble-svc-props-{}",
                                        short_uuid(&service.uuid.to_string())
                                    )
                                    .into(),
                                );
                                // The props column flexes; its available width
                                // comes from the probe's report of the frame
                                // below.
                                let props_fit = detail_props_width.is_none_or(|available| {
                                    oxideterm_gpui_ui::shaped_text_width(
                                        &characteristics,
                                        cell_font(gpui::FontWeight::NORMAL, ui_font_family.clone()),
                                        self.tokens.metrics.ui_text_xs,
                                        text_system,
                                    ) <= available
                                });
                                let props_cell = oxideterm_gpui_ui::tauri_table_cell(
                                    &self.tokens,
                                    &detail_text(true, 0.0),
                                    oxideterm_gpui_ui::TauriTableCellStyle::Meta,
                                    characteristics.clone(),
                                )
                                .id(props_id)
                                .when(!props_fit, |cell| {
                                    let tokens = self.tokens.clone();
                                    let characteristics_for_tooltip = characteristics.clone();
                                    cell.tooltip(move |_window, cx| {
                                        oxideterm_gpui_ui::tooltip::tooltip_view(
                                            tokens.clone(),
                                            characteristics_for_tooltip.clone(),
                                            None,
                                            cx,
                                        )
                                    })
                                });
                                let detail_row = oxideterm_gpui_ui::tauri_table_row(
                                    detail_colors,
                                    detail_metrics,
                                    service_selected,
                                )
                                .child(name_cell)
                                .child(props_cell);
                                let svc_uuid = service.uuid;
                                let detail_row = detail_row.on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.set_new_connection_ble_service(svc_uuid, cx);
                                        cx.stop_propagation();
                                    }),
                                );
                                detail_table = detail_table.child(detail_row);
                            }
                            body = body.child(wrapper(detail_table.into_any_element()));
                        }
                    }
                }
                // The header stays outside the scroll viewport so column
                // labels remain visible while the rows move.
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(
                        div()
                            .id("ble-device-rows")
                            .w_full()
                            .flex()
                            .flex_col()
                            .max_h(px(300.0))
                            .overflow_y_scroll()
                            .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation())
                            .child(body),
                    )
                    .into_any_element()
            };
            // The probe reports the frame's laid-out width every frame so the
            // flexible-column tooltip predicates above stay exact after
            // resizes, at the cost of a one-frame delay.
            let workspace = cx.entity();
            Some(
                oxideterm_gpui_ui::BoundsProbe::new(
                    div()
                        .id("ble-device-list")
                        .track_focus(&device_list_focus)
                        .on_key_down(cx.listener(
                            |this, event: &gpui::KeyDownEvent, _window, cx| {
                                if event.keystroke.key == "enter" {
                                    let selected = this
                                        .connection_form_state(cx)
                                        .form
                                        .as_ref()
                                        .and_then(|form| form.ble.selected_device_id.clone());
                                    if let Some(device_id) = selected {
                                        this.inspect_new_connection_ble_device(device_id, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .flex()
                        .flex_col()
                        .rounded(px(self.tokens.radii.sm))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .p(px(self.tokens.spacing.one))
                        .child(content),
                    move |bounds, _window, cx| {
                        let width = f32::from(bounds.size.width);
                        // The probe fires every frame; only real geometry
                        // changes may notify, or rendering would loop.
                        let _ = workspace.update(cx, |this, cx| {
                            let changed = this.connection_flow.update(cx, |flow, _flow_cx| {
                                if flow.form.ble_scan_table_width != Some(width) {
                                    flow.form.ble_scan_table_width = Some(width);
                                    true
                                } else {
                                    false
                                }
                            });
                            if changed {
                                cx.notify();
                            }
                        });
                    },
                )
                .into_any_element(),
            )
        } else {
            None
        };

        // The adapter picker and the scan action share one row so the control
        // keeps the width the button does not need. A static field label would
        // only repeat the section title above it.
        let adapter_row = div()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(div().flex_1().min_w(px(0.0)).child(adapter_control))
            .child(self.workspace_toolbar_action_button(
                scan_label,
                Some(scan_icon),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        disabled: scan_disabled,
                        ..ButtonOptions::default()
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, _window, cx| {
                    // The button toggles: an in-flight scan is stopped manually;
                    // otherwise a fresh discovery window is opened.
                    if this
                        .connection_form_state(cx)
                        .form
                        .as_ref()
                        .map(|form| form.ble.scanning)
                        .unwrap_or(false)
                    {
                        this.stop_new_connection_ble_scan(cx);
                    } else {
                        this.start_new_connection_ble_scan(cx);
                    }
                    cx.stop_propagation();
                }),
            ));

        // The adapter field and the results frame are one visual group, so the
        // body keeps the tighter intra-field rhythm the serial port field uses
        // instead of the wider section-level gap.
        let discovery_body = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(adapter_row)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.two))
                    .when_some(device_list, |column, list| column.child(list))
                    .child(self.render_connection_hint(
                        self.i18n.t("modals.new_connection.ble_scan_devices_hint"),
                    )),
            )
            .into_any_element();

        let basic = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                self.render_connection_field(
                    self.i18n.t("modals.new_connection.ble_profile_name"),
                    &ble_profile_name,
                    self.i18n
                        .t("modals.new_connection.ble_profile_name_placeholder"),
                    NewConnectionField::BleProfileName,
                    false,
                    cx,
                ),
            )
            .child(self.render_connection_notes_fields(&notes, cx))
            .child(self.render_connection_group_select(self.i18n.t("ssh.form.group"), &group, cx))
            .into_any_element();

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_form_section(ConnectionFormSection::Basic, basic, cx))
            .child(self.render_connection_form_section(
                ConnectionFormSection::BleDiscovery,
                discovery_body,
                cx,
            ))
            // Byte-stream transport options mirror the serial control bar so the
            // initial session state matches what the control bar would toggle.
            .child(self.render_connection_form_section(
                ConnectionFormSection::BleTransportOptions,
                self.render_ble_transport_options(cx),
                cx,
            ))
            // Terminal behavior is shared with the telnet form: both call the
            // same renderer so encoding, key sequences, highlighting, and
            // session logging stay in sync instead of diverging.
            .child(self.render_connection_form_section(
                ConnectionFormSection::Terminal,
                self.render_connection_terminal_options(cx),
                cx,
            ))
            .child(self.render_connection_form_section(
                ConnectionFormSection::Appearance,
                self.render_connection_appearance_options(cx),
                cx,
            ))
            .into_any_element()
    }

    /// Renders the BLE transport options: the ATT MTU cap and the write
    /// characteristic mode. Byte-stream display/send choices live in the
    /// shared terminal behavior section instead.
    fn render_ble_transport_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((mtu_draft, write_without_response)) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| (form.ble.mtu.clone(), form.ble.write_without_response))
        else {
            return div().into_any_element();
        };
        // Saved preferences remain editable without a live scan. Once a device
        // is inspected, restrict the choice to its advertised characteristics.
        let write_without_response_available = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| {
                if form.ble.device_details.is_none() && form.ble.profile_id.is_some() {
                    return Some(true);
                }
                let details = form.ble.device_details.as_ref()?;
                let service_uuid = form.ble.selected_service_uuid?;
                Some(
                    details
                        .services
                        .iter()
                        .find(|service| service.uuid == service_uuid)
                        .is_some_and(|service| {
                            service.characteristics.iter().any(|characteristic| {
                                characteristic
                                    .properties
                                    .contains(&"write-without-response")
                            })
                        }),
                )
            })
            .unwrap_or(false);
        let mtu_invalid = !mtu_draft.trim().is_empty()
            && mtu_draft
                .trim()
                .parse::<u16>()
                .map(|value| {
                    !(oxideterm_connections::BLE_ATT_MTU_MIN
                        ..=oxideterm_connections::BLE_ATT_MTU_MAX)
                        .contains(&value)
                })
                .unwrap_or(true);
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                // Same label + hover hint pattern as the SSH
                                // connect timeout field.
                                self.render_connection_label_with_help(
                                    "new-connection-ble-mtu-help",
                                    "new-connection-ble-mtu",
                                    "modals.new_connection.ble_mtu",
                                    "modals.new_connection.ble_mtu_hint",
                                    cx,
                                ),
                                self.render_connection_input(
                                    &mtu_draft,
                                    self.i18n.t("modals.new_connection.ble_mtu_placeholder"),
                                    NewConnectionField::BleMtu,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(self.render_connection_checkbox_with_help(
                                "new-connection-ble-wwr-help",
                                "new-connection-ble-wwr",
                                "modals.new_connection.ble_write_without_response",
                                "modals.new_connection.ble_write_without_response_hint",
                                // Reflects the unchecked state whenever the
                                // selected service has no matching
                                // characteristic, instead of only graying out
                                // a stale check.
                                write_without_response_available && write_without_response,
                                !write_without_response_available,
                                |form| {
                                    form.ble.write_without_response =
                                        !form.ble.write_without_response
                                },
                                cx,
                            )),
                    ),
            )
            .when(mtu_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.ble_mtu_invalid"),
                    self.tokens.ui.error,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn refresh_ble_adapters(&mut self, cx: &mut Context<Self>) {
        // Bumping the generation invalidates any in-flight adapter listing or
        // scan so their results cannot reach a form that moved on.
        let Some((generation, form_id)) = self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.ble.scan_generation += 1;
                form.ble.adapters_loading = true;
                form.ble.scanning = false;
                form.ble.scan_started = false;
                form.ble.inspecting = false;
                form.ble.devices.clear();
                form.ble.selected_device_id = None;
                form.ble.device_details = None;
                form.error = None;
                Some((form.ble.scan_generation, form.ble.instance_id))
            } else {
                None
            }
        }) else {
            return;
        };
        cx.notify();

        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            // Adapter enumeration goes through platform Bluetooth stacks and
            // must stay off the GPUI thread.
            let result = runtime
                .spawn(oxideterm_ble::ble_list_adapters())
                .await
                .map_err(|error| error.to_string())
                .and_then(|inner| inner.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.update_connection_form_state(cx, |state| {
                    let Some(form) = state.form.as_mut() else {
                        return;
                    };
                    if form.ble.scan_generation != generation || form.ble.instance_id != form_id {
                        return;
                    }
                    form.ble.adapters_loading = false;
                    match result {
                        Ok(adapters) => {
                            form.ble.adapters = adapters;
                            form.ble.selected_adapter_index = 0;
                        }
                        Err(message) => {
                            form.error = Some(format!(
                                "{}: {message}",
                                this.i18n
                                    .t("modals.new_connection.ble_load_adapters_failed")
                            ));
                        }
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn stop_new_connection_ble_scan(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                // Bumping the generation makes the in-flight scan's write-back
                // a no-op; dropping the guard closes the discovery window so
                // the radio is released immediately.
                form.ble.scan_generation += 1;
                form.ble.scanning = false;
                form.ble.scan_started = false;
                form.ble.adapter_guard = None;
                form.ble.selected_device_id = None;
                form.ble.device_details = None;
                form.ble.inspect_error = None;
                form.ble.selected_service_uuid = None;
                form.error = None;
            }
        });
        cx.notify();
    }

    pub(super) fn start_new_connection_ble_scan(&mut self, cx: &mut Context<Self>) {
        let Some((generation, adapter_index, form_id)) =
            self.update_connection_form_state(cx, |state| {
                state.form.as_mut().map(|form| {
                    form.ble.scan_generation += 1;
                    form.ble.scanning = true;
                    form.ble.scan_started = false;
                    form.ble.devices.clear();
                    form.ble.selected_device_id = None;
                    form.ble.device_details = None;
                    form.ble.inspect_error = None;
                    form.ble.selected_service_uuid = None;
                    form.error = None;
                    (
                        form.ble.scan_generation,
                        form.ble.selected_adapter_index,
                        form.ble.instance_id,
                    )
                })
            })
        else {
            return;
        };
        cx.notify();

        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            // Radio scanning blocks for the whole discovery window and must
            // stay off the GPUI thread. A fresh guard opens the adapter and
            // owns the discovery window; it replaces any previous guard so
            // the old window closes before a new one opens.
            let result = runtime
                .spawn(async move {
                    let guard = oxideterm_ble::BleAdapterGuard::open(adapter_index).await?;
                    let devices = guard.scan(oxideterm_ble::BLE_SCAN_DURATION).await?;
                    Ok::<_, oxideterm_ble::BleError>((devices, guard))
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|inner| {
                    inner.map_err(|error: oxideterm_ble::BleError| error.to_string())
                });
            let _ = weak.update(cx, |this, cx| {
                this.update_connection_form_state(cx, |state| {
                    let Some(form) = state.form.as_mut() else {
                        return;
                    };
                    if form.ble.scan_generation != generation || form.ble.instance_id != form_id {
                        // A newer scan superseded this one; drop the guard so
                        // its discovery window closes instead of leaking.
                        if let Ok((_, guard)) = result.as_ref() {
                            // We cannot await here; dropping the guard closes
                            // the underlying adapter session, which releases
                            // the discovery window on the BlueZ side.
                            let _ = guard;
                        }
                        return;
                    }
                    form.ble.scanning = false;
                    form.ble.scan_started = true;
                    match result {
                        Ok((devices, guard)) => {
                            form.ble.adapter_guard = Some(guard);
                            form.ble.devices = devices.clone();
                            form.ble.selected_device_id = devices
                                .iter()
                                .find(|device| device.name.is_some())
                                .map(|device| device.id.clone());
                        }
                        Err(message) => {
                            form.ble.adapter_guard = None;
                            form.error = Some(format!(
                                "{}: {message}",
                                this.i18n.t("modals.new_connection.ble_scan_failed")
                            ));
                        }
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn inspect_new_connection_ble_device(
        &mut self,
        device_id: oxideterm_ble::BlePeripheralId,
        cx: &mut Context<Self>,
    ) {
        let Some((generation, adapter, form_id)) = self.update_connection_form_state(cx, |state| {
            state.form.as_mut().and_then(|form| {
                // Inspection reuses the adapter that produced the scan results
                // so the device object (Linux) or cache entry (macOS/Windows)
                // is still addressable without re-scanning. Resolve the guard
                // before mutating state: a missing guard (scan never run or
                // released by a transport switch) must not leave the
                // inspecting flag stuck at true.
                let adapter = form
                    .ble
                    .adapter_guard
                    .as_ref()
                    .map(|guard| guard.adapter().clone())?;
                // The shared generation token also drops an in-flight
                // inspection whenever a rescan or adapter switch starts.
                form.ble.scan_generation += 1;
                form.ble.inspecting = true;
                form.ble.device_details = None;
                form.ble.inspect_error = None;
                form.ble.selected_service_uuid = None;
                form.error = None;
                Some((form.ble.scan_generation, adapter, form.ble.instance_id))
            })
        }) else {
            return;
        };
        cx.notify();

        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            // The inspection link connects, reads the GATT layout, and
            // disconnects again; keep it off the GPUI thread.
            let inspected_device = device_id.clone();
            let result = runtime
                .spawn(async move { oxideterm_ble::ble_inspect_device(&adapter, device_id).await })
                .await
                .map_err(|error| error.to_string())
                .and_then(|inner| inner.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.update_connection_form_state(cx, |state| {
                    let Some(form) = state.form.as_mut() else {
                        return;
                    };
                    if form.ble.scan_generation != generation
                        || form.ble.instance_id != form_id
                        || form.ble.selected_device_id.as_ref() != Some(&inspected_device)
                    {
                        return;
                    }
                    form.ble.inspecting = false;
                    match result {
                        Ok(details) => form.ble.device_details = Some(details),
                        Err(message) => {
                            form.ble.inspect_error = Some(format!(
                                "{}: {message}",
                                this.i18n.t("modals.new_connection.ble_inspect_failed")
                            ));
                        }
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn set_new_connection_ble_adapter(
        &mut self,
        adapter_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.ble.selected_adapter_index = adapter_index;
                // Results from the previous adapter no longer describe the
                // selection; the orphaned scan write-back is dropped by the
                // generation bump, and its discovery window closes with the
                // guard.
                form.ble.scan_generation += 1;
                form.ble.scanning = false;
                form.ble.scan_started = false;
                form.ble.inspecting = false;
                form.ble.devices.clear();
                form.ble.adapter_guard = None;
                form.ble.selected_device_id = None;
                form.ble.device_details = None;
                form.ble.inspect_error = None;
                form.ble.selected_service_uuid = None;
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_ble_device(
        &mut self,
        device_id: oxideterm_ble::BlePeripheralId,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.ble.scan_generation += 1;
                form.ble.inspecting = false;
                form.ble.selected_device_id = Some(device_id);
                // Details and the selected service belong to the previously
                // inspected device and would mislead once the selection moves.
                form.ble.device_details = None;
                form.ble.inspect_error = None;
                form.ble.selected_service_uuid = None;
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        cx.notify();
    }

    /// Selects a service inside the expanded BLE device details table.
    ///
    /// Only services that expose both notify and write capabilities can back a
    /// terminal, but the selection itself is unconditional so the open button
    /// can derive its enabled state from `ble.selected_service_uuid`.
    pub(super) fn set_new_connection_ble_service(
        &mut self,
        service_uuid: uuid::Uuid,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.ble.selected_service_uuid = Some(service_uuid);
                form.ble.inspect_error = None;
                form.error = None;
            }
        });
        cx.notify();
    }
}
