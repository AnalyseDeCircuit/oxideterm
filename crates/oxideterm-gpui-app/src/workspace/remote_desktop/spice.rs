// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, VecDeque};

use super::*;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_spice::{
    SpiceAgentFeatures, SpiceAgentStateKind, SpiceAudioDataMode, SpiceChannelCapabilities,
    SpiceFileTransferFailure, SpiceFileTransferState, SpiceGraphicsDevice, SpiceMouseMode,
    SpiceNativeBackendStatus, SpicePlaybackStateKind, SpicePortStateKind, SpiceRecordStateKind,
    SpiceTopologyMonitor, SpiceUsbDeviceIdentity,
};
use zeroize::Zeroize as _;

const SPICE_PORT_BUFFER_BYTES: usize = 1024 * 1024;
const SPICE_PORT_INPUT_MAX_BYTES: usize = 64 * 1024;
const SPICE_AUDIO_VOLUME_STEP: u16 = u16::MAX / 10;

#[derive(Default)]
pub(super) struct SpiceSessionRuntimeState {
    pub(super) tools_open: bool,
    pub(super) session_id: Option<u32>,
    pub(super) capabilities: Option<SpiceChannelCapabilities>,
    pub(super) server_name: Option<String>,
    pub(super) server_uuid: Option<[u8; 16]>,
    pub(super) mouse_mode: Option<SpiceMouseMode>,
    pub(super) keyboard_modifiers: u16,
    pub(super) topology: Vec<SpiceTopologyMonitor>,
    pub(super) maximum_monitors: Option<u16>,
    pub(super) playback: HashMap<u8, SpicePlaybackChannelState>,
    pub(super) record: HashMap<u8, SpiceRecordChannelState>,
    pub(super) agent_playback_volume: Option<SpiceAgentVolumeState>,
    pub(super) agent_record_volume: Option<SpiceAgentVolumeState>,
    pub(super) ports: HashMap<u8, SpicePortChannelState>,
    pub(super) selected_port: Option<u8>,
    pub(super) native_devices: Option<SpiceNativeDevicesState>,
    pub(super) agent: SpiceAgentRuntimeState,
    pub(super) file_transfers: HashMap<u64, SpiceFileTransferRuntimeState>,
}

#[derive(Default)]
pub(super) struct SpicePlaybackChannelState {
    pub(super) state: Option<SpicePlaybackStateKind>,
    pub(super) stream_generation: Option<u64>,
    pub(super) channels: Option<u32>,
    pub(super) sample_rate_hz: Option<u32>,
    pub(super) volumes: Vec<u16>,
    pub(super) muted: bool,
    pub(super) latency_ms: Option<u32>,
}

#[derive(Default)]
pub(super) struct SpiceRecordChannelState {
    pub(super) state: Option<SpiceRecordStateKind>,
    pub(super) stream_generation: Option<u64>,
    pub(super) mode: Option<SpiceAudioDataMode>,
    pub(super) channels: Option<u32>,
    pub(super) sample_rate_hz: Option<u32>,
    pub(super) volumes: Vec<u16>,
    pub(super) muted: bool,
}

#[derive(Clone)]
pub(super) struct SpiceAgentVolumeState {
    pub(super) muted: bool,
    pub(super) volumes: Vec<u16>,
}

#[derive(Default)]
pub(super) struct SpicePortChannelState {
    pub(super) state: Option<SpicePortStateKind>,
    pub(super) name: Option<String>,
    pub(super) opened: bool,
    pub(super) discontinuity: bool,
    pub(super) pending_data: VecDeque<Zeroizing<Vec<u8>>>,
    pending_bytes: usize,
    pub(super) input: Zeroizing<String>,
    pub(super) input_focused: bool,
}

#[derive(Clone)]
pub(in crate::workspace) struct SpiceNativeDevicesState {
    pub(in crate::workspace) usb_devices: Vec<SpiceUsbDeviceIdentity>,
    pub(in crate::workspace) usb_status: SpiceNativeBackendStatus,
    pub(in crate::workspace) smartcard_readers: Vec<String>,
    pub(in crate::workspace) smartcard_status: SpiceNativeBackendStatus,
}

#[derive(Default)]
pub(super) struct SpiceAgentRuntimeState {
    pub(super) state: Option<SpiceAgentStateKind>,
    pub(super) features: Option<SpiceAgentFeatures>,
    pub(super) graphics_devices: Vec<SpiceGraphicsDevice>,
}

pub(in crate::workspace) struct SpiceFileTransferRuntimeState {
    pub(in crate::workspace) state: SpiceFileTransferState,
    pub(in crate::workspace) accepted_bytes: u64,
    pub(in crate::workspace) failure: Option<SpiceFileTransferFailure>,
}

pub(super) struct SpiceActivitySummary {
    pub(super) usb_devices: usize,
    pub(super) usb_available: bool,
    pub(super) smartcard_readers: usize,
    pub(super) smartcard_available: bool,
    pub(super) active_transfers: usize,
    pub(super) failed_transfers: usize,
    pub(super) accepted_bytes: u64,
}

#[derive(Clone)]
pub(super) struct SpiceToolsSnapshot {
    pub(super) capabilities: Option<SpiceChannelCapabilities>,
    pub(super) native_devices: Option<SpiceNativeDevicesState>,
    pub(super) transfers: Vec<SpiceTransferSnapshot>,
    pub(super) ports: Vec<SpicePortSnapshot>,
    pub(super) selected_port: Option<SpicePortConsoleSnapshot>,
    pub(super) agent_playback_volume: Option<SpiceAgentVolumeState>,
    pub(super) agent_record_volume: Option<SpiceAgentVolumeState>,
}

#[derive(Clone)]
pub(super) struct SpiceTransferSnapshot {
    transfer_id: u64,
    state: SpiceFileTransferState,
    accepted_bytes: u64,
}

#[derive(Clone)]
pub(super) struct SpicePortSnapshot {
    channel_id: u8,
    name: Option<String>,
    opened: bool,
    pending_bytes: usize,
    discontinuity: bool,
}

#[derive(Clone)]
pub(super) struct SpicePortConsoleSnapshot {
    channel_id: u8,
    name: Option<String>,
    opened: bool,
    output: Zeroizing<String>,
    input: Zeroizing<String>,
    input_focused: bool,
}

#[derive(Clone, Copy)]
enum SpiceAgentVolumeChange {
    Decrease,
    Increase,
    ToggleMute,
}

impl SpiceSessionRuntimeState {
    pub(super) fn tools_snapshot(&self) -> SpiceToolsSnapshot {
        let selected_port = self.selected_port.and_then(|channel_id| {
            let port = self.ports.get(&channel_id)?;
            let mut bytes = Zeroizing::new(Vec::with_capacity(port.pending_bytes));
            for chunk in &port.pending_data {
                bytes.extend_from_slice(chunk);
            }
            // Port payload stays in zeroizing buffers; display conversion is scoped to one render.
            let output = Zeroizing::new(String::from_utf8_lossy(&bytes).into_owned());
            Some(SpicePortConsoleSnapshot {
                channel_id,
                name: port.name.clone(),
                opened: port.opened,
                output,
                input: port.input.clone(),
                input_focused: port.input_focused,
            })
        });
        SpiceToolsSnapshot {
            capabilities: self.capabilities.clone(),
            native_devices: self.native_devices.clone(),
            transfers: self
                .file_transfers
                .iter()
                .map(|(transfer_id, transfer)| SpiceTransferSnapshot {
                    transfer_id: *transfer_id,
                    state: transfer.state,
                    accepted_bytes: transfer.accepted_bytes,
                })
                .collect(),
            ports: self
                .ports
                .iter()
                .map(|(channel_id, port)| SpicePortSnapshot {
                    channel_id: *channel_id,
                    name: port.name.clone(),
                    opened: port.opened,
                    pending_bytes: port.pending_bytes,
                    discontinuity: port.discontinuity,
                })
                .collect(),
            selected_port,
            agent_playback_volume: self.agent_playback_volume.clone(),
            agent_record_volume: self.agent_record_volume.clone(),
        }
    }

    pub(super) fn activity_summary(&self) -> SpiceActivitySummary {
        let (usb_devices, usb_available, smartcard_readers, smartcard_available) = self
            .native_devices
            .as_ref()
            .map(|devices| {
                (
                    devices.usb_devices.len(),
                    matches!(devices.usb_status, SpiceNativeBackendStatus::Available),
                    devices.smartcard_readers.len(),
                    matches!(
                        devices.smartcard_status,
                        SpiceNativeBackendStatus::Available
                    ),
                )
            })
            .unwrap_or((0, false, 0, false));
        let active_transfers = self
            .file_transfers
            .values()
            .filter(|transfer| {
                matches!(
                    transfer.state,
                    SpiceFileTransferState::WaitingForGuest
                        | SpiceFileTransferState::Sending
                        | SpiceFileTransferState::AwaitingCompletion
                )
            })
            .count();
        let failed_transfers = self
            .file_transfers
            .values()
            .filter(|transfer| transfer.failure.is_some())
            .count();
        let accepted_bytes = self
            .file_transfers
            .values()
            .map(|transfer| transfer.accepted_bytes)
            .sum();
        SpiceActivitySummary {
            usb_devices,
            usb_available,
            smartcard_readers,
            smartcard_available,
            active_transfers,
            failed_transfers,
            accepted_bytes,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn toggle_spice_tools(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        session_entity.update(cx, |session, cx| {
            if session.profile.protocol != RemoteDesktopProtocol::Spice {
                return;
            }
            session.spice.tools_open = !session.spice.tools_open;
            if session.spice.tools_open {
                session.send_spice_request(SpiceWorkerRequest::ListNativeDevices);
            }
            cx.notify();
        });
    }

    fn close_spice_tools(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                session.spice.tools_open = false;
                for port in session.spice.ports.values_mut() {
                    port.input_focused = false;
                }
                cx.notify();
            });
        }
    }

    fn refresh_spice_native_devices(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                session.send_spice_request(SpiceWorkerRequest::ListNativeDevices);
            });
        }
    }

    fn start_spice_usb_redirection(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        device: SpiceUsbDeviceIdentity,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                let supported = session
                    .spice
                    .capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.usbredir_channel_ids.contains(&channel_id)
                    });
                if supported {
                    session.send_spice_request(SpiceWorkerRequest::StartUsbRedirection {
                        channel_id,
                        device,
                    });
                }
            });
        }
    }

    fn start_spice_smartcard_redirection(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        display_name: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                let supported = session
                    .spice
                    .capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.smartcard_channel_ids.contains(&channel_id)
                    });
                if supported {
                    session.send_spice_request(SpiceWorkerRequest::StartSmartcardRedirection {
                        channel_id,
                        display_name,
                    });
                }
            });
        }
    }

    fn choose_spice_webdav_root(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        read_only: bool,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("remote_desktop.spice_webdav_select_folder"),
            )),
        });
        cx.spawn(async move |workspace, cx| {
            let root = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(root) = root else {
                return;
            };
            let _ = workspace.update(cx, |workspace, cx| {
                let Some(session_entity) = workspace.remote_desktop_session_entity(tab_id, cx)
                else {
                    return;
                };
                session_entity.update(cx, |session, _cx| {
                    let supported =
                        session
                            .spice
                            .capabilities
                            .as_ref()
                            .is_some_and(|capabilities| {
                                capabilities.webdav_channel_ids.contains(&channel_id)
                            });
                    if supported {
                        session.send_spice_request(SpiceWorkerRequest::StartWebDav {
                            channel_id,
                            root,
                            read_only,
                        });
                    }
                });
            });
        })
        .detach();
    }

    fn cancel_spice_file_transfer(
        &mut self,
        tab_id: TabId,
        transfer_id: u64,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                session.send_spice_request(SpiceWorkerRequest::FileTransferCancel { transfer_id });
            });
        }
    }

    fn send_spice_port_break(&mut self, tab_id: TabId, channel_id: u8, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                let supported = session
                    .spice
                    .capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.port_channel_ids.contains(&channel_id)
                    });
                if supported {
                    session.send_spice_request(SpiceWorkerRequest::PortBreak { channel_id });
                }
            });
        }
    }

    fn select_spice_port(&mut self, tab_id: TabId, channel_id: u8, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                if session.spice.ports.contains_key(&channel_id) {
                    session.spice.selected_port = Some(channel_id);
                    for port in session.spice.ports.values_mut() {
                        port.input_focused = false;
                    }
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn focus_spice_port_input(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                for (port_channel_id, port) in &mut session.spice.ports {
                    port.input_focused = *port_channel_id == channel_id && port.opened;
                }
                cx.notify();
            });
        }
        self.show_active_input_caret(cx);
    }

    pub(in crate::workspace) fn spice_focused_port(&self, tab_id: TabId, cx: &App) -> Option<u8> {
        let session = self.remote_desktop_session_entity(tab_id, cx)?;
        let session = session.read(cx);
        if !session.spice.tools_open {
            return None;
        }
        session
            .spice
            .ports
            .iter()
            .find_map(|(channel_id, port)| port.input_focused.then_some(*channel_id))
    }

    pub(in crate::workspace) fn spice_port_input<'a>(
        &self,
        tab_id: TabId,
        channel_id: u8,
        cx: &'a App,
    ) -> Option<&'a str> {
        let session = self.remote_desktop_session_entity(tab_id, cx)?;
        let session = session.read(cx);
        let port = session.spice.ports.get(&channel_id)?;
        (session.spice.tools_open && port.input_focused).then_some(port.input.as_str())
    }

    pub(in crate::workspace) fn replace_spice_port_input(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return false;
        };
        session_entity.update(cx, |session, cx| {
            let Some(port) = session.spice.ports.get_mut(&channel_id) else {
                return false;
            };
            if !session.spice.tools_open || !port.input_focused || !port.opened {
                return false;
            }
            if port.input.len().saturating_add(text.len()) > SPICE_PORT_INPUT_MAX_BYTES {
                return false;
            }
            replace_utf16(&mut port.input, replacement_range, text);
            cx.notify();
            true
        })
    }

    fn send_spice_port_input(
        &mut self,
        tab_id: TabId,
        channel_id: u8,
        append_line_feed: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        session_entity.update(cx, |session, cx| {
            let Some(port) = session.spice.ports.get_mut(&channel_id) else {
                return;
            };
            if !port.opened || (port.input.is_empty() && !append_line_feed) {
                return;
            }
            // Move the draft allocation directly into the helper request and leave no UI copy.
            let mut data = std::mem::take(&mut *port.input).into_bytes();
            if append_line_feed {
                data.push(b'\n');
            }
            session.send_spice_request(SpiceWorkerRequest::PortWrite { channel_id, data });
            cx.notify();
        });
    }

    fn clear_spice_port_output(&mut self, tab_id: TabId, channel_id: u8, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                let Some(port) = session.spice.ports.get_mut(&channel_id) else {
                    return;
                };
                port.pending_data.clear();
                port.pending_bytes = 0;
                port.discontinuity = false;
                cx.notify();
            });
        }
    }

    fn change_spice_agent_volume(
        &mut self,
        tab_id: TabId,
        is_playback: bool,
        change: SpiceAgentVolumeChange,
        cx: &mut Context<Self>,
    ) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        session_entity.update(cx, |session, cx| {
            let volume = if is_playback {
                session.spice.agent_playback_volume.as_ref()
            } else {
                session.spice.agent_record_volume.as_ref()
            };
            let Some(volume) = volume else {
                return;
            };
            let mut volumes = volume.volumes.clone();
            let muted = match change {
                SpiceAgentVolumeChange::Decrease => {
                    for value in &mut volumes {
                        *value = value.saturating_sub(SPICE_AUDIO_VOLUME_STEP);
                    }
                    volume.muted
                }
                SpiceAgentVolumeChange::Increase => {
                    for value in &mut volumes {
                        *value = value.saturating_add(SPICE_AUDIO_VOLUME_STEP);
                    }
                    volume.muted
                }
                SpiceAgentVolumeChange::ToggleMute => !volume.muted,
            };
            session.send_spice_request(SpiceWorkerRequest::SyncAgentAudioVolume {
                is_playback,
                muted,
                volumes,
            });
            cx.notify();
        });
    }

    pub(super) fn render_spice_tools(
        &self,
        tab_id: TabId,
        snapshot: SpiceToolsSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let usb_channels = snapshot
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.usbredir_channel_ids.as_slice())
            .unwrap_or_default();
        let smartcard_channels = snapshot
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.smartcard_channel_ids.as_slice())
            .unwrap_or_default();
        let webdav_channel = snapshot
            .capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.webdav_channel_ids.first().copied());
        let mut transfers = snapshot.transfers;
        transfers.sort_by_key(|transfer| transfer.transfer_id);
        let mut ports = snapshot.ports;
        ports.sort_by_key(|port| port.channel_id);
        let selected_port = snapshot.selected_port;
        let agent_playback_volume = snapshot.agent_playback_volume;
        let agent_record_volume = snapshot.agent_record_volume;

        let (usb_status, usb_devices, smartcard_status, smartcard_readers) = snapshot
            .native_devices
            .map(|devices| {
                (
                    devices.usb_status,
                    devices.usb_devices,
                    devices.smartcard_status,
                    devices.smartcard_readers,
                )
            })
            .unwrap_or_else(|| {
                (
                    SpiceNativeBackendStatus::Unavailable {
                        reason: self.i18n.t("remote_desktop.spice_devices_not_loaded"),
                    },
                    Vec::new(),
                    SpiceNativeBackendStatus::Unavailable {
                        reason: self.i18n.t("remote_desktop.spice_devices_not_loaded"),
                    },
                    Vec::new(),
                )
            });

        let usb_rows = usb_devices
            .into_iter()
            .enumerate()
            .map(|(index, device)| {
                let channel_id = usb_channels.get(index).copied();
                let label = self
                    .i18n
                    .t("remote_desktop.spice_usb_device")
                    .replace("{{vendor}}", &format!("{:04x}", device.vendor_id))
                    .replace("{{product}}", &format!("{:04x}", device.product_id))
                    .replace("{{bus}}", &device.bus_number.to_string())
                    .replace("{{address}}", &device.device_address.to_string());
                self.render_spice_device_row(
                    label,
                    self.i18n.t("remote_desktop.spice_redirect"),
                    channel_id,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(channel_id) = channel_id {
                            this.start_spice_usb_redirection(tab_id, channel_id, device, cx);
                        }
                    }),
                )
            })
            .collect::<Vec<_>>();
        let smartcard_rows = smartcard_readers
            .into_iter()
            .enumerate()
            .map(|(index, display_name)| {
                let channel_id = smartcard_channels.get(index).copied();
                let action_name = display_name.clone();
                self.render_spice_device_row(
                    display_name,
                    self.i18n.t("remote_desktop.spice_redirect"),
                    channel_id,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(channel_id) = channel_id {
                            this.start_spice_smartcard_redirection(
                                tab_id,
                                channel_id,
                                action_name.clone(),
                                cx,
                            );
                        }
                    }),
                )
            })
            .collect::<Vec<_>>();

        dialog_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .w(px(620.0))
                .max_h(px(680.0))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(modal_header(
                    &self.tokens,
                    self.i18n.t("remote_desktop.spice_tools"),
                    self.i18n.t("remote_desktop.spice_tools_description"),
                ))
                .child(
                    modal_body(&self.tokens)
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scrollbar()
                        .gap(px(14.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .child(self.render_spice_section_title(
                                    self.i18n.t("remote_desktop.spice_native_devices"),
                                ))
                                .child(div().flex_1())
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t("remote_desktop.spice_refresh_devices"),
                                    Some(Self::render_lucide_icon(
                                        LucideIcon::RefreshCw,
                                        12.0,
                                        rgb(theme.text_muted),
                                    )),
                                    compact_spice_tool_button(false),
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.refresh_spice_native_devices(tab_id, cx);
                                    }),
                                )),
                        )
                        .child(self.render_spice_device_section(
                            self.i18n.t("remote_desktop.spice_usb_devices"),
                            usb_status,
                            usb_rows,
                        ))
                        .child(self.render_spice_device_section(
                            self.i18n.t("remote_desktop.spice_smartcard_readers"),
                            smartcard_status,
                            smartcard_rows,
                        ))
                        .when(
                            agent_playback_volume.is_some() || agent_record_volume.is_some(),
                            |body| {
                                body.child(self.render_spice_audio_volume_section(
                                    tab_id,
                                    agent_playback_volume,
                                    agent_record_volume,
                                    cx,
                                ))
                            },
                        )
                        .when(!transfers.is_empty(), |body| {
                            body.child(self.render_spice_transfer_section(tab_id, transfers, cx))
                        })
                        .when(!ports.is_empty(), |body| {
                            body.child(self.render_spice_port_section(tab_id, ports, cx))
                                .when_some(selected_port, |body, port| {
                                    body.child(self.render_spice_port_console(tab_id, port, cx))
                                })
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(self.render_spice_section_title(
                                    self.i18n.t("remote_desktop.spice_webdav"),
                                ))
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(
                                            self.i18n.t("remote_desktop.spice_webdav_description"),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(8.0))
                                        .child(self.workspace_toolbar_action_button(
                                            self.i18n.t("remote_desktop.spice_share_folder"),
                                            Some(Self::render_lucide_icon(
                                                LucideIcon::FolderOpen,
                                                12.0,
                                                rgb(theme.text_muted),
                                            )),
                                            compact_spice_tool_button(webdav_channel.is_none()),
                                            cx.listener(move |this, _event, _window, cx| {
                                                if let Some(channel_id) = webdav_channel {
                                                    this.choose_spice_webdav_root(
                                                        tab_id, channel_id, false, cx,
                                                    );
                                                }
                                            }),
                                        ))
                                        .child(
                                            self.workspace_toolbar_action_button(
                                                self.i18n.t(
                                                    "remote_desktop.spice_share_folder_read_only",
                                                ),
                                                None,
                                                compact_spice_tool_button(webdav_channel.is_none()),
                                                cx.listener(move |this, _event, _window, cx| {
                                                    if let Some(channel_id) = webdav_channel {
                                                        this.choose_spice_webdav_root(
                                                            tab_id, channel_id, true, cx,
                                                        );
                                                    }
                                                }),
                                            ),
                                        ),
                                ),
                        ),
                )
                .child(
                    modal_footer(&self.tokens).child(self.workspace_toolbar_action_button(
                        self.i18n.t("window_controls.close"),
                        None,
                        compact_spice_tool_button(false),
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_spice_tools(tab_id, cx);
                        }),
                    )),
                ),
        )
    }

    fn render_spice_section_title(&self, label: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(self.tokens.ui.text))
            .child(label)
            .into_any_element()
    }

    fn render_spice_device_section(
        &self,
        title: String,
        status: SpiceNativeBackendStatus,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let status_message = match status {
            SpiceNativeBackendStatus::Available if rows.is_empty() => {
                Some(self.i18n.t("remote_desktop.spice_no_devices"))
            }
            SpiceNativeBackendStatus::Available => None,
            SpiceNativeBackendStatus::Unavailable { reason } => Some(reason),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_spice_section_title(title))
            .when_some(status_message, |section, message| {
                section.child(
                    div()
                        .p(px(10.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgb(theme.bg_hover))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(message),
                )
            })
            .children(rows)
            .into_any_element()
    }

    fn render_spice_device_row(
        &self,
        label: String,
        action_label: String,
        channel_id: Option<u8>,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(36.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .child(self.workspace_toolbar_action_button(
                action_label,
                None,
                compact_spice_tool_button(channel_id.is_none()),
                listener,
            ))
            .into_any_element()
    }

    fn render_spice_transfer_section(
        &self,
        tab_id: TabId,
        transfers: Vec<SpiceTransferSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = transfers.into_iter().map(|transfer| {
            let terminal = matches!(
                transfer.state,
                SpiceFileTransferState::Completed
                    | SpiceFileTransferState::Cancelled
                    | SpiceFileTransferState::Failed
                    | SpiceFileTransferState::AgentDisconnected
            );
            let label = self
                .i18n
                .t("remote_desktop.spice_transfer_status")
                .replace("{{id}}", &transfer.transfer_id.to_string())
                .replace("{{bytes}}", &transfer.accepted_bytes.to_string());
            let transfer_id = transfer.transfer_id;
            self.render_spice_device_row(
                label,
                self.i18n.t("remote_desktop.spice_cancel_transfer"),
                (!terminal).then_some(0),
                cx.listener(move |this, _event, _window, cx| {
                    this.cancel_spice_file_transfer(tab_id, transfer_id, cx);
                }),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                self.render_spice_section_title(self.i18n.t("remote_desktop.spice_file_transfers")),
            )
            .children(rows)
            .into_any_element()
    }

    fn render_spice_audio_volume_section(
        &self,
        tab_id: TabId,
        playback: Option<SpiceAgentVolumeState>,
        record: Option<SpiceAgentVolumeState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                self.render_spice_section_title(self.i18n.t("remote_desktop.spice_audio_volume")),
            )
            .when_some(playback, |section, volume| {
                section.child(self.render_spice_audio_volume_row(
                    tab_id,
                    true,
                    self.i18n.t("remote_desktop.spice_audio_playback_volume"),
                    volume,
                    cx,
                ))
            })
            .when_some(record, |section, volume| {
                section.child(self.render_spice_audio_volume_row(
                    tab_id,
                    false,
                    self.i18n.t("remote_desktop.spice_audio_record_volume"),
                    volume,
                    cx,
                ))
            })
            .into_any_element()
    }

    fn render_spice_audio_volume_row(
        &self,
        tab_id: TabId,
        is_playback: bool,
        label: String,
        volume: SpiceAgentVolumeState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let percentage = if volume.volumes.is_empty() {
            0
        } else {
            let total = volume
                .volumes
                .iter()
                .map(|value| u64::from(*value))
                .sum::<u64>();
            total
                .saturating_mul(100)
                .checked_div(volume.volumes.len() as u64 * u64::from(u16::MAX))
                .unwrap_or(0)
        };
        let has_channels = !volume.volumes.is_empty();
        div()
            .min_h(px(36.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(format!("{label} · {percentage}%")),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.spice_volume_down"),
                None,
                compact_spice_tool_button(!has_channels),
                cx.listener(move |this, _event, _window, cx| {
                    this.change_spice_agent_volume(
                        tab_id,
                        is_playback,
                        SpiceAgentVolumeChange::Decrease,
                        cx,
                    );
                }),
            ))
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.spice_volume_up"),
                None,
                compact_spice_tool_button(!has_channels),
                cx.listener(move |this, _event, _window, cx| {
                    this.change_spice_agent_volume(
                        tab_id,
                        is_playback,
                        SpiceAgentVolumeChange::Increase,
                        cx,
                    );
                }),
            ))
            .child(self.workspace_toolbar_action_button(
                if volume.muted {
                    self.i18n.t("remote_desktop.spice_unmute")
                } else {
                    self.i18n.t("remote_desktop.spice_mute")
                },
                None,
                compact_spice_tool_button(!has_channels),
                cx.listener(move |this, _event, _window, cx| {
                    this.change_spice_agent_volume(
                        tab_id,
                        is_playback,
                        SpiceAgentVolumeChange::ToggleMute,
                        cx,
                    );
                }),
            ))
            .into_any_element()
    }

    fn render_spice_port_section(
        &self,
        tab_id: TabId,
        ports: Vec<SpicePortSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = ports.into_iter().map(|port| {
            let label = self
                .i18n
                .t("remote_desktop.spice_port_status")
                .replace(
                    "{{name}}",
                    port.name
                        .as_deref()
                        .unwrap_or_else(|| if port.opened { "#" } else { "×" }),
                )
                .replace("{{id}}", &port.channel_id.to_string())
                .replace("{{bytes}}", &port.pending_bytes.to_string())
                .replace("{{break}}", if port.discontinuity { "!" } else { "" });
            let channel_id = port.channel_id;
            self.render_spice_device_row(
                label,
                self.i18n.t("remote_desktop.spice_open_console"),
                Some(channel_id),
                cx.listener(move |this, _event, _window, cx| {
                    this.select_spice_port(tab_id, channel_id, cx);
                }),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_spice_section_title(self.i18n.t("remote_desktop.spice_ports")))
            .children(rows)
            .into_any_element()
    }

    fn render_spice_port_console(
        &self,
        tab_id: TabId,
        port: SpicePortConsoleSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let channel_id = port.channel_id;
        let target = WorkspaceImeTarget::SpicePort {
            tab_id: tab_id.0,
            channel_id,
        };
        let input = self.text_input_with_workspace_ime(
            target,
            text_input(
                &self.tokens,
                TextInputView {
                    value: &port.input,
                    placeholder: self.i18n.t("remote_desktop.spice_port_input_placeholder"),
                    focused: port.input_focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .w_full(),
            move |this, cx| this.focus_spice_port_input(tab_id, channel_id, cx),
            cx,
        );
        let title = port
            .name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| format!("#{}", port.channel_id));
        let output = if port.output.is_empty() {
            self.i18n.t("remote_desktop.spice_port_output_empty")
        } else {
            port.output.to_string()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                self.render_spice_section_title(
                    self.i18n
                        .t("remote_desktop.spice_port_console")
                        .replace("{{name}}", &title),
                ),
            )
            .child(
                div()
                    .max_h(px(180.0))
                    .overflow_y_scrollbar()
                    .p(px(10.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
                    .child(output),
            )
            .child(input)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.spice_port_send"),
                        None,
                        compact_spice_tool_button(!port.opened || port.input.is_empty()),
                        cx.listener(move |this, _event, _window, cx| {
                            this.send_spice_port_input(tab_id, channel_id, false, cx);
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.spice_port_send_line"),
                        None,
                        compact_spice_tool_button(!port.opened),
                        cx.listener(move |this, _event, _window, cx| {
                            this.send_spice_port_input(tab_id, channel_id, true, cx);
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.spice_send_break"),
                        None,
                        compact_spice_tool_button(!port.opened),
                        cx.listener(move |this, _event, _window, cx| {
                            this.send_spice_port_break(tab_id, channel_id, cx);
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.spice_port_clear_output"),
                        None,
                        compact_spice_tool_button(port.output.is_empty()),
                        cx.listener(move |this, _event, _window, cx| {
                            this.clear_spice_port_output(tab_id, channel_id, cx);
                        }),
                    )),
            )
            .into_any_element()
    }
}

fn compact_spice_tool_button(disabled: bool) -> ToolbarButtonOptions {
    ToolbarButtonOptions {
        button: ButtonOptions {
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Sm,
            radius: ButtonRadius::Md,
            disabled,
        },
        height: Some(24.0),
        padding_x: Some(8.0),
        font_size: Some(12.0),
        ..ToolbarButtonOptions::default()
    }
}

impl RemoteDesktopSessionEntity {
    pub(super) fn send_spice_request(&self, request: SpiceWorkerRequest) {
        if self.profile.protocol == RemoteDesktopProtocol::Spice
            && let Some(worker) = self.worker.as_ref()
        {
            worker.send_spice(request);
        }
    }

    pub(super) fn apply_spice_event(&mut self, event: SpiceHelperEvent, _cx: &mut Context<Self>) {
        match event {
            SpiceHelperEvent::Connected {
                session_id,
                capabilities,
            } => {
                self.spice.session_id = Some(session_id);
                self.spice.capabilities = Some(capabilities);
            }
            SpiceHelperEvent::ServerIdentity { name, uuid } => {
                self.spice.server_name = name;
                self.spice.server_uuid = uuid;
            }
            SpiceHelperEvent::MouseMode { mode } => self.spice.mouse_mode = Some(mode),
            SpiceHelperEvent::KeyboardModifiers { bits } => {
                self.spice.keyboard_modifiers = bits;
            }
            SpiceHelperEvent::Topology {
                maximum_allowed,
                monitors,
                ..
            } => {
                self.spice.maximum_monitors = Some(maximum_allowed);
                self.spice.topology = monitors;
            }
            SpiceHelperEvent::PlaybackState {
                channel_id,
                stream_generation,
                state,
                channels,
                sample_rate_hz,
                ..
            } => {
                let playback = self.spice.playback.entry(channel_id).or_default();
                playback.state = Some(state);
                playback.stream_generation = stream_generation;
                playback.channels = channels;
                playback.sample_rate_hz = sample_rate_hz;
                if matches!(
                    state,
                    SpicePlaybackStateKind::Stopped | SpicePlaybackStateKind::Closed
                ) {
                    playback.stream_generation = None;
                }
            }
            SpiceHelperEvent::PlaybackSettings {
                channel_id,
                volumes,
                muted,
                latency_ms,
            } => {
                let playback = self.spice.playback.entry(channel_id).or_default();
                playback.volumes = volumes;
                playback.muted = muted;
                playback.latency_ms = latency_ms;
            }
            SpiceHelperEvent::RecordState {
                channel_id,
                stream_generation,
                state,
                mode,
                channels,
                sample_rate_hz,
                ..
            } => {
                let record = self.spice.record.entry(channel_id).or_default();
                record.state = Some(state);
                record.stream_generation = Some(stream_generation);
                record.mode = mode;
                record.channels = channels;
                record.sample_rate_hz = sample_rate_hz;
            }
            SpiceHelperEvent::RecordSettings {
                channel_id,
                volumes,
                muted,
            } => {
                let record = self.spice.record.entry(channel_id).or_default();
                record.volumes = volumes;
                record.muted = muted;
            }
            SpiceHelperEvent::PortState {
                channel_id,
                state,
                name,
                opened,
                ..
            } => {
                let port = self.spice.ports.entry(channel_id).or_default();
                port.state = Some(state);
                port.name = name;
                port.opened = opened;
                if state == SpicePortStateKind::Closed {
                    port.pending_data.clear();
                    port.pending_bytes = 0;
                    port.input.zeroize();
                    port.input_focused = false;
                }
            }
            SpiceHelperEvent::PortData {
                channel_id,
                discontinuity,
                data,
            } => {
                let port = self.spice.ports.entry(channel_id).or_default();
                port.discontinuity |= discontinuity;
                while port.pending_bytes.saturating_add(data.len()) > SPICE_PORT_BUFFER_BYTES {
                    let Some(dropped) = port.pending_data.pop_front() else {
                        break;
                    };
                    port.pending_bytes = port.pending_bytes.saturating_sub(dropped.len());
                    port.discontinuity = true;
                }
                port.pending_bytes = port.pending_bytes.saturating_add(data.len());
                port.pending_data.push_back(Zeroizing::new(data));
            }
            SpiceHelperEvent::PortBreak { channel_id } => {
                self.spice
                    .ports
                    .entry(channel_id)
                    .or_default()
                    .discontinuity = true;
            }
            SpiceHelperEvent::NativeDevices {
                usb_devices,
                usb_status,
                smartcard_readers,
                smartcard_status,
            } => {
                self.spice.native_devices = Some(SpiceNativeDevicesState {
                    usb_devices,
                    usb_status,
                    smartcard_readers,
                    smartcard_status,
                });
            }
            SpiceHelperEvent::AgentState {
                state, features, ..
            } => {
                self.spice.agent.state = Some(state);
                self.spice.agent.features = features;
            }
            SpiceHelperEvent::AgentGraphicsDevices { displays, .. } => {
                self.spice.agent.graphics_devices = displays;
            }
            SpiceHelperEvent::AgentGraphicsDevicesReset => {
                self.spice.agent.graphics_devices.clear();
            }
            SpiceHelperEvent::FileTransferState {
                transfer_id,
                state,
                accepted_bytes,
                failure,
            } => {
                self.spice.file_transfers.insert(
                    transfer_id,
                    SpiceFileTransferRuntimeState {
                        state,
                        accepted_bytes,
                        failure,
                    },
                );
            }
            SpiceHelperEvent::AgentAudioVolume {
                is_playback,
                muted,
                volumes,
                ..
            } => {
                let volume = SpiceAgentVolumeState { muted, volumes };
                if is_playback {
                    self.spice.agent_playback_volume = Some(volume);
                } else {
                    self.spice.agent_record_volume = Some(volume);
                }
            }
            SpiceHelperEvent::AgentAudioVolumeReset => {
                self.spice.agent_playback_volume = None;
                self.spice.agent_record_volume = None;
            }
            SpiceHelperEvent::ClipboardOffer { .. }
            | SpiceHelperEvent::ClipboardRequest { .. }
            | SpiceHelperEvent::ClipboardData { .. }
            | SpiceHelperEvent::Status { .. }
            | SpiceHelperEvent::Error { .. }
            | SpiceHelperEvent::PlaybackData { .. }
            | SpiceHelperEvent::Frame { .. }
            | SpiceHelperEvent::Cursor { .. }
            | SpiceHelperEvent::HelloAck { .. } => {}
        }
    }
}
