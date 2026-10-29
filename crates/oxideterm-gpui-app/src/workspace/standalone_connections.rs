use super::*;
use oxideterm_connections::SavedUpstreamProxyPolicy;
mod persistence;
use crate::workspace::new_connection::{
    BleOpenRequest, BleStandaloneTarget, BleTraceTarget, MoshConnectionOptions,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopProviderManifest, RemoteDesktopSecret,
};

pub(super) type StandaloneConnectionId = String;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) enum StandaloneConnectionKind {
    Mosh,
    Telnet,
    Serial,
    Ble,
    Rdp,
    Vnc,
}

/// Device identity carried by a BLE session record.
///
/// A BLE peripheral handle is assigned by the OS at scan time and cannot be
/// restored, so the record keeps the discovery clues the resolver matches
/// against instead of the runtime id.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct BleConnectionClues {
    pub adapter_mac: Option<String>,
    pub device_mac: Option<String>,
    pub device_name: Option<String>,
    pub service_uuid: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum StandaloneConnectionSurface {
    Terminal(TerminalSessionId),
    RemoteDesktop(TabId),
}

pub(super) enum StandaloneConnectionLaunch {
    Serial {
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
    },
    SavedSerial {
        profile_id: String,
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
    },
    Telnet {
        config: TelnetSessionConfig,
        upstream_proxy: SavedUpstreamProxyPolicy,
        terminal_options: ConnectionTerminalOptions,
    },
    SavedTelnet {
        profile_id: String,
        config: TelnetSessionConfig,
        upstream_proxy: SavedUpstreamProxyPolicy,
        terminal_options: ConnectionTerminalOptions,
    },
    Ble {
        clues: BleConnectionClues,
        terminal_options: ConnectionTerminalOptions,
    },
    SavedBle {
        profile_id: String,
        clues: BleConnectionClues,
        terminal_options: ConnectionTerminalOptions,
    },
    MoshPreflight {
        // The pending record owns zeroizing SSH authentication until retry or removal.
        config: SshConfig,
        options: MoshConnectionOptions,
    },
    SavedMosh {
        profile_id: String,
    },
    RemoteDesktop {
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        // Ephemeral credentials remain zeroizing and are dropped with the runtime record.
        password: Option<RemoteDesktopSecret>,
        ssh_gateway_connection_id: Option<String>,
    },
    SavedRemoteDesktop {
        profile_id: String,
    },
    RestoredMosh {
        profile: oxideterm_connections::MoshProfile,
    },
    RestoredRemoteDesktop {
        profile: oxideterm_connections::RemoteDesktopProfile,
    },
}

pub(super) struct StandaloneConnectionRecord {
    pending_ble: Option<super::new_connection::BlePendingOpen>,
    pub(super) id: StandaloneConnectionId,
    attempt_id: String,
    pub(super) kind: StandaloneConnectionKind,
    pub(super) title: String,
    pub(super) launch: StandaloneConnectionLaunch,
    pub(super) surface: Option<StandaloneConnectionSurface>,
    pub(super) readiness: ActiveSessionReadiness,
}

enum StandaloneReconnectPlan {
    Serial {
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        saved_profile_id: Option<String>,
    },
    Telnet {
        config: TelnetSessionConfig,
        upstream_proxy: SavedUpstreamProxyPolicy,
        terminal_options: ConnectionTerminalOptions,
        saved_profile_id: Option<String>,
    },
    Ble {
        clues: BleConnectionClues,
        terminal_options: ConnectionTerminalOptions,
        saved_profile_id: Option<String>,
    },
    MoshPreflight {
        config: SshConfig,
        options: MoshConnectionOptions,
    },
    SavedMosh {
        profile_id: String,
    },
    RemoteDesktop {
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        ssh_gateway_connection_id: Option<String>,
    },
    SavedRemoteDesktop {
        profile_id: String,
    },
}

#[derive(Default)]
pub(super) struct StandaloneConnectionRegistry {
    records: Vec<StandaloneConnectionRecord>,
    snapshot_path: Option<PathBuf>,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn disconnect_standalone_connection(
        &mut self,
        connection_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let surface = self
            .standalone_connections
            .record(connection_id)
            .and_then(|record| record.surface);
        if let Some(surface) = surface {
            // Explicit disconnect tears down the current surface but retains its logical record.
            self.close_standalone_connection_surface(surface, window, cx);
        } else {
            // Pending asynchronous launches observe this state before mounting a surface.
            self.standalone_connections.mark_disconnected(connection_id);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn reconnect_standalone_connection(
        &mut self,
        connection_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_restored_standalone_form(connection_id, window, cx) {
            return;
        }
        let Some((title, surface, plan)) = self.standalone_reconnect_plan(connection_id) else {
            return;
        };
        if let Some(surface) = surface {
            // A reconnect always receives a new pane or desktop tab identity.
            self.close_standalone_connection_surface(surface, window, cx);
        }
        let Some(connection_attempt_id) =
            self.standalone_connections.begin_reconnect(connection_id)
        else {
            return;
        };

        let result = match plan {
            StandaloneReconnectPlan::Serial {
                config,
                terminal_options,
                saved_profile_id,
            } => self
                .create_serial_terminal_tab_for_connection(
                    config,
                    terminal_options,
                    title,
                    Some(connection_attempt_id.clone()),
                    window,
                    cx,
                )
                .map(|session_id| {
                    if let Some(profile_id) = saved_profile_id {
                        self.register_terminal_saved_connection(
                            session_id,
                            oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                            profile_id,
                            cx,
                        );
                    }
                }),
            StandaloneReconnectPlan::Telnet {
                config,
                upstream_proxy,
                terminal_options,
                saved_profile_id,
            } => self
                .create_telnet_terminal_tab_for_connection(
                    config,
                    upstream_proxy,
                    None,
                    terminal_options,
                    title,
                    Some(connection_attempt_id.clone()),
                    window,
                    cx,
                )
                .map(|session_id| {
                    if let Some(profile_id) = saved_profile_id {
                        self.telnet_terminal_profile_ids
                            .insert(session_id, profile_id.clone());
                        self.register_terminal_saved_connection(
                            session_id,
                            oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                            profile_id,
                            cx,
                        );
                    }
                }),
            StandaloneReconnectPlan::Ble {
                clues,
                terminal_options,
                saved_profile_id,
            } => {
                // BLE never mounts a terminal directly: the resolver must scan
                // and match the device again, and it reports through the same
                // trace card as a saved-profile open.
                let saved_profile = saved_profile_id.as_deref().and_then(|id| {
                    self.connection_store
                        .ble_profiles()
                        .iter()
                        .find(|p| p.id == id)
                });
                let runtime_options = saved_profile
                    .map(|profile| {
                        crate::workspace::new_connection::terminal_serial_runtime_options_from_ble_profile(profile)
                    })
                    .unwrap_or_default();
                let mtu = saved_profile.and_then(|profile| profile.mtu);
                self.start_ble_open_flow(
                    BleOpenRequest {
                        target: BleTraceTarget {
                            node_id: connection_id.to_string(),
                            label: title.clone(),
                            endpoint: clues
                                .device_name
                                .clone()
                                .or_else(|| clues.device_mac.clone()),
                        },
                        adapter_guard: None,
                        selected_device: None,
                        adapter_mac: clues.adapter_mac,
                        device_mac: clues.device_mac,
                        device_name: clues.device_name,
                        service_uuid: clues.service_uuid,
                        title: format!("BLE {title}"),
                        terminal_options,
                        runtime_options,
                        mtu,
                        // Saved profiles carry the user's preference; unsaved
                        // clues fall back to the lower-latency default.
                        write_without_response: saved_profile
                            .map(|profile| profile.write_without_response)
                            .unwrap_or(true),
                        profile_id: saved_profile_id,
                        form_id: None,
                        standalone: BleStandaloneTarget::Reconnect {
                            attempt_id: connection_attempt_id.clone(),
                        },
                    },
                    cx,
                );
                Ok(())
            }
            StandaloneReconnectPlan::MoshPreflight {
                config,
                mut options,
            } => {
                options.runtime_connection_attempt_id = Some(connection_attempt_id.clone());
                self.start_ssh_preflight(config, title, SshConnectionIntent::Mosh(options), cx);
                Ok(())
            }
            StandaloneReconnectPlan::SavedMosh { profile_id } => {
                self.open_saved_mosh_profile_for_connection(
                    &profile_id,
                    Some(connection_attempt_id.clone()),
                    window,
                    cx,
                );
                Ok(())
            }
            StandaloneReconnectPlan::RemoteDesktop {
                profile,
                provider,
                password,
                ssh_gateway_connection_id,
            } => {
                if ssh_gateway_connection_id.is_some() {
                    self.open_remote_desktop_connection_for_connection(
                        profile,
                        password,
                        ssh_gateway_connection_id,
                        Some(connection_attempt_id.clone()),
                        window,
                        cx,
                    );
                } else {
                    self.open_remote_desktop_tab_for_connection(
                        profile,
                        provider,
                        title,
                        password,
                        None,
                        None,
                        Some(connection_attempt_id.clone()),
                        window,
                        cx,
                    );
                }
                Ok(())
            }
            StandaloneReconnectPlan::SavedRemoteDesktop { profile_id } => {
                self.open_saved_remote_desktop_profile_for_connection(
                    &profile_id,
                    Some(connection_attempt_id.clone()),
                    window,
                    cx,
                );
                Ok(())
            }
        };
        if result.is_err() {
            self.standalone_connections
                .mark_attempt_error(&connection_attempt_id);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn remove_standalone_connection(
        &mut self,
        connection_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let surface = self
            .standalone_connections
            .record(connection_id)
            .and_then(|record| record.surface);
        if let Some(surface) = surface {
            self.close_standalone_connection_surface(surface, window, cx);
        }
        self.expanded_standalone_connections.remove(connection_id);
        self.standalone_connections.remove(connection_id);
        cx.notify();
    }

    fn close_standalone_connection_surface(
        &mut self,
        surface: StandaloneConnectionSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match surface {
            StandaloneConnectionSurface::Terminal(session_id) => {
                self.close_terminal_session(session_id, window, cx);
            }
            StandaloneConnectionSurface::RemoteDesktop(tab_id) => {
                self.request_close_tab_by_id(tab_id, window, cx);
            }
        }
    }

    fn standalone_reconnect_plan(
        &self,
        connection_id: &str,
    ) -> Option<(
        String,
        Option<StandaloneConnectionSurface>,
        StandaloneReconnectPlan,
    )> {
        let record = self.standalone_connections.record(connection_id)?;
        let plan = match &record.launch {
            StandaloneConnectionLaunch::RestoredMosh { .. }
            | StandaloneConnectionLaunch::RestoredRemoteDesktop { .. } => return None,
            StandaloneConnectionLaunch::Serial {
                config,
                terminal_options,
            } => StandaloneReconnectPlan::Serial {
                config: config.clone(),
                terminal_options: terminal_options.clone(),
                saved_profile_id: None,
            },
            StandaloneConnectionLaunch::SavedSerial {
                profile_id,
                config,
                terminal_options,
            } => {
                let current_profile = self
                    .connection_store
                    .serial_profiles()
                    .iter()
                    .find(|profile| profile.id == *profile_id);
                StandaloneReconnectPlan::Serial {
                    config: current_profile.map_or_else(
                        || config.clone(),
                        |profile| SerialSessionConfig {
                            port_path: profile.port_path.clone(),
                            baud_rate: profile.baud_rate,
                            data_bits: profile.data_bits,
                            stop_bits: profile.stop_bits,
                            parity: new_connection::terminal_serial_parity_from_profile(
                                &profile.parity,
                            ),
                            flow_control: new_connection::terminal_serial_flow_from_profile(
                                &profile.flow_control,
                            ),
                            runtime_options:
                                new_connection::terminal_serial_runtime_options_from_profile(
                                    profile,
                                ),
                        },
                    ),
                    terminal_options: current_profile.map_or_else(
                        || terminal_options.clone(),
                        |profile| profile.terminal.clone(),
                    ),
                    saved_profile_id: Some(profile_id.clone()),
                }
            }
            StandaloneConnectionLaunch::Telnet {
                config,
                upstream_proxy,
                terminal_options,
            } => StandaloneReconnectPlan::Telnet {
                config: config.clone(),
                upstream_proxy: upstream_proxy.clone(),
                terminal_options: terminal_options.clone(),
                saved_profile_id: None,
            },
            StandaloneConnectionLaunch::SavedTelnet {
                profile_id,
                config,
                upstream_proxy,
                terminal_options,
            } => {
                let current_profile = self
                    .connection_store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.id == *profile_id);
                StandaloneReconnectPlan::Telnet {
                    upstream_proxy: current_profile.map_or_else(
                        || upstream_proxy.clone(),
                        |profile| profile.upstream_proxy.clone(),
                    ),
                    config: current_profile.map_or_else(
                        || config.clone(),
                        |profile| TelnetSessionConfig {
                            line_endings: new_connection::terminal_telnet_line_endings_from_profile(
                                profile,
                            ),
                            host: profile.host.clone(),
                            port: profile.port,
                        },
                    ),
                    terminal_options: current_profile.map_or_else(
                        || terminal_options.clone(),
                        |profile| profile.terminal.clone(),
                    ),
                    saved_profile_id: Some(profile_id.clone()),
                }
            }
            StandaloneConnectionLaunch::Ble {
                clues,
                terminal_options,
            } => StandaloneReconnectPlan::Ble {
                clues: clues.clone(),
                terminal_options: terminal_options.clone(),
                saved_profile_id: None,
            },
            StandaloneConnectionLaunch::SavedBle {
                profile_id,
                clues,
                terminal_options,
            } => {
                // A saved profile stays authoritative so edits (renamed device,
                // changed service) apply on the next reconnect.
                let current_profile = self
                    .connection_store
                    .ble_profiles()
                    .iter()
                    .find(|profile| profile.id == *profile_id);
                StandaloneReconnectPlan::Ble {
                    clues: current_profile.map_or_else(
                        || clues.clone(),
                        |profile| BleConnectionClues {
                            adapter_mac: profile.adapter_mac.clone(),
                            device_mac: profile.device_mac.clone(),
                            device_name: profile.device_name.clone(),
                            service_uuid: profile.service_uuid.clone(),
                        },
                    ),
                    terminal_options: current_profile.map_or_else(
                        || terminal_options.clone(),
                        |profile| profile.terminal.clone(),
                    ),
                    saved_profile_id: Some(profile_id.clone()),
                }
            }
            StandaloneConnectionLaunch::MoshPreflight { config, options } => {
                StandaloneReconnectPlan::MoshPreflight {
                    config: config.clone(),
                    options: options.clone(),
                }
            }
            StandaloneConnectionLaunch::SavedMosh { profile_id } => {
                StandaloneReconnectPlan::SavedMosh {
                    profile_id: profile_id.clone(),
                }
            }
            StandaloneConnectionLaunch::RemoteDesktop {
                profile,
                provider,
                password,
                ssh_gateway_connection_id,
            } => StandaloneReconnectPlan::RemoteDesktop {
                profile: profile.clone(),
                provider: provider.clone(),
                password: password
                    .as_ref()
                    .map(RemoteDesktopSecret::duplicate_for_reauthentication),
                ssh_gateway_connection_id: ssh_gateway_connection_id.clone(),
            },
            StandaloneConnectionLaunch::SavedRemoteDesktop { profile_id } => {
                StandaloneReconnectPlan::SavedRemoteDesktop {
                    profile_id: profile_id.clone(),
                }
            }
        };
        Some((record.title.clone(), record.surface, plan))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial_launch() -> StandaloneConnectionLaunch {
        StandaloneConnectionLaunch::Serial {
            config: SerialSessionConfig {
                port_path: "/dev/tty.test".to_string(),
                baud_rate: 115_200,
                data_bits: 8,
                stop_bits: 1,
                parity: oxideterm_terminal::SerialParity::None,
                flow_control: oxideterm_terminal::SerialFlowControl::None,
                runtime_options: Default::default(),
            },
            terminal_options: ConnectionTerminalOptions::default(),
        }
    }

    #[test]
    fn reconnect_rejects_stale_and_cancelled_attempts_before_binding_a_fresh_surface() {
        let first_surface = StandaloneConnectionSurface::Terminal(TerminalSessionId(41));
        let next_surface = StandaloneConnectionSurface::Terminal(TerminalSessionId(73));
        let mut registry = StandaloneConnectionRegistry::default();
        let connection_id = registry.insert(
            StandaloneConnectionKind::Serial,
            "Test serial".to_string(),
            serial_launch(),
            first_surface,
        );

        registry.release_surface(first_surface);

        let disconnected = registry
            .record(&connection_id)
            .expect("record must survive");
        assert_eq!(disconnected.surface, None);
        assert_eq!(disconnected.readiness, ActiveSessionReadiness::Disconnected);

        let next_attempt_id = registry.begin_reconnect(&connection_id).unwrap();
        registry.mark_attempt_error(&connection_id);
        assert!(registry.is_connecting_attempt(&next_attempt_id));
        assert_eq!(
            registry.record(&connection_id).unwrap().readiness,
            ActiveSessionReadiness::Connecting
        );

        registry.mark_disconnected(&connection_id);
        registry.mark_attempt_error(&next_attempt_id);
        let cancelled = registry.record(&connection_id).unwrap();
        assert_eq!(cancelled.readiness, ActiveSessionReadiness::Disconnected);
        assert!(!registry.is_connecting_attempt(&next_attempt_id));

        let fresh_attempt_id = registry.begin_reconnect(&connection_id).unwrap();
        assert!(!registry.bind_surface_for_attempt(&next_attempt_id, next_surface));
        assert!(registry.bind_surface_for_attempt(&fresh_attempt_id, next_surface));
        let rebound = registry.record(&connection_id).expect("record must remain");
        assert_eq!(rebound.surface, Some(next_surface));
        assert_ne!(rebound.surface, Some(first_surface));
    }

    #[test]
    fn ble_pending_open_is_cancelled_on_disconnect_retry_and_remove() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut registry = StandaloneConnectionRegistry::default();
        let id = registry.insert(
            StandaloneConnectionKind::Serial,
            "Pending".into(),
            serial_launch(),
            StandaloneConnectionSurface::Terminal(TerminalSessionId(41)),
        );
        for action in ["disconnect", "retry", "remove"] {
            let attempt = registry.begin_reconnect(&id).unwrap();
            let cancellation = tokio_util::sync::CancellationToken::new();
            registry.set_ble_pending(
                &attempt,
                super::super::new_connection::BlePendingOpen {
                    cancellation: cancellation.clone(),
                    _worker: runtime.spawn(std::future::pending()),
                },
            );
            match action {
                "disconnect" => registry.mark_disconnected(&id),
                "retry" => {
                    registry.begin_reconnect(&id).unwrap();
                }
                "remove" => {
                    registry.remove(&id);
                }
                _ => unreachable!(),
            }
            assert!(cancellation.is_cancelled(), "{action}");
            assert!(!registry.is_connecting_attempt(&attempt), "{action}");
            assert!(
                !registry.bind_surface_for_attempt(
                    &attempt,
                    StandaloneConnectionSurface::Terminal(TerminalSessionId(73))
                ),
                "{action}"
            );
        }
    }
}

impl StandaloneConnectionRegistry {
    pub(super) fn records(&self) -> &[StandaloneConnectionRecord] {
        &self.records
    }

    pub(super) fn record(&self, id: &str) -> Option<&StandaloneConnectionRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    pub(super) fn record_mut(&mut self, id: &str) -> Option<&mut StandaloneConnectionRecord> {
        self.records.iter_mut().find(|record| record.id == id)
    }

    pub(super) fn set_telnet_line_endings(
        &mut self,
        session_id: TerminalSessionId,
        input: Option<oxideterm_terminal::SerialLineEnding>,
        output: Option<oxideterm_terminal::SerialLineEnding>,
    ) {
        // Only this surface's launch defaults change; sibling live sessions retain theirs.
        let Some(record) = self.records.iter_mut().find(|record| {
            record.surface == Some(StandaloneConnectionSurface::Terminal(session_id))
        }) else {
            return;
        };
        match &mut record.launch {
            StandaloneConnectionLaunch::Telnet { config, .. }
            | StandaloneConnectionLaunch::SavedTelnet { config, .. } => {
                if let Some(value) = input {
                    config.line_endings.input = value;
                }
                if let Some(value) = output {
                    config.line_endings.output = value;
                }
            }
            _ => return,
        }
        self.persist();
    }

    pub(super) fn connection_id_for_surface(
        &self,
        surface: StandaloneConnectionSurface,
    ) -> Option<StandaloneConnectionId> {
        self.records
            .iter()
            .find(|record| record.surface == Some(surface))
            .map(|record| record.id.clone())
    }

    pub(super) fn insert(
        &mut self,
        kind: StandaloneConnectionKind,
        title: String,
        launch: StandaloneConnectionLaunch,
        surface: StandaloneConnectionSurface,
    ) -> StandaloneConnectionId {
        let id = uuid::Uuid::new_v4().to_string();
        self.records.push(StandaloneConnectionRecord {
            pending_ble: None,
            id: id.clone(),
            attempt_id: id.clone(),
            kind,
            title,
            launch,
            surface: Some(surface),
            readiness: ActiveSessionReadiness::Ready,
        });
        self.persist();
        id
    }

    pub(super) fn insert_pending(
        &mut self,
        kind: StandaloneConnectionKind,
        title: String,
        launch: StandaloneConnectionLaunch,
    ) -> StandaloneConnectionId {
        let id = uuid::Uuid::new_v4().to_string();
        self.records.push(StandaloneConnectionRecord {
            pending_ble: None,
            id: id.clone(),
            attempt_id: id.clone(),
            kind,
            title,
            launch,
            surface: None,
            readiness: ActiveSessionReadiness::Connecting,
        });
        self.persist();
        id
    }

    pub(super) fn bind_surface_for_attempt(
        &mut self,
        attempt_id: &str,
        surface: StandaloneConnectionSurface,
    ) -> bool {
        if !self.is_connecting_attempt(attempt_id) {
            return false;
        }
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt_id)
        else {
            return false;
        };
        record.surface = Some(surface);
        record.pending_ble = None;
        record.readiness = ActiveSessionReadiness::Connecting;
        true
    }

    pub(super) fn release_surface(&mut self, surface: StandaloneConnectionSurface) {
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.surface == Some(surface))
        else {
            return;
        };
        // The logical connection remains available for a fresh surface after teardown.
        record.surface = None;
        record.readiness = ActiveSessionReadiness::Disconnected;
    }

    pub(super) fn mark_attempt_error(&mut self, attempt_id: &str) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt_id)
        {
            if record.readiness != ActiveSessionReadiness::Connecting {
                return;
            }
            record.surface = None;
            record.readiness = ActiveSessionReadiness::Error;
            record.pending_ble = None;
        }
    }

    pub(super) fn mark_disconnected(&mut self, id: &str) {
        if let Some(record) = self.record_mut(id) {
            record.pending_ble = None;
            record.surface = None;
            record.readiness = ActiveSessionReadiness::Disconnected;
        }
    }

    pub(super) fn is_connecting_attempt(&self, attempt_id: &str) -> bool {
        self.records.iter().any(|record| {
            record.attempt_id == attempt_id
                && record.surface.is_none()
                && record.readiness == ActiveSessionReadiness::Connecting
        })
    }

    pub(super) fn set_ble_pending(
        &mut self,
        attempt_id: &str,
        task: super::new_connection::BlePendingOpen,
    ) {
        if !self.is_connecting_attempt(attempt_id) {
            return;
        }
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt_id)
        {
            record.pending_ble = Some(task);
        }
    }

    pub(super) fn cancel_attempt(&mut self, attempt_id: &str) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt_id)
        {
            record.pending_ble = None;
            record.readiness = ActiveSessionReadiness::Disconnected;
        }
    }

    pub(super) fn begin_reconnect(&mut self, id: &str) -> Option<String> {
        let record = self.record_mut(id)?;
        record.pending_ble = None;
        let attempt_id = uuid::Uuid::new_v4().to_string();
        record.attempt_id = attempt_id.clone();
        record.surface = None;
        record.readiness = ActiveSessionReadiness::Connecting;
        Some(attempt_id)
    }

    pub(super) fn remove_attempt(&mut self, attempt_id: &str) {
        if let Some(index) = self
            .records
            .iter()
            .position(|record| record.attempt_id == attempt_id)
        {
            self.records.remove(index);
            self.persist();
        }
    }

    pub(super) fn remove(&mut self, id: &str) -> Option<StandaloneConnectionRecord> {
        let index = self.records.iter().position(|record| record.id == id)?;
        let mut record = self.records.remove(index);
        record.pending_ble = None;
        self.persist();
        Some(record)
    }

    pub(super) fn replace_with_saved_profile(
        &mut self,
        surface: StandaloneConnectionSurface,
        profile_id: String,
        store: &ConnectionStore,
    ) {
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.surface == Some(surface))
        else {
            return;
        };
        match record.kind {
            StandaloneConnectionKind::Serial => {
                if let StandaloneConnectionLaunch::Serial {
                    config,
                    terminal_options,
                } = &record.launch
                {
                    record.launch = StandaloneConnectionLaunch::SavedSerial {
                        profile_id,
                        config: config.clone(),
                        terminal_options: terminal_options.clone(),
                    };
                }
            }
            StandaloneConnectionKind::Telnet => {
                let Some(profile) = store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.id == profile_id)
                else {
                    return;
                };
                if let StandaloneConnectionLaunch::Telnet { config, .. } = &record.launch {
                    record.launch = StandaloneConnectionLaunch::SavedTelnet {
                        profile_id,
                        config: config.clone(),
                        // Saving transfers retry credentials to the protected store.
                        upstream_proxy: profile.upstream_proxy.clone(),
                        terminal_options: profile.terminal.clone(),
                    };
                }
            }
            StandaloneConnectionKind::Ble => {
                // Saving an ad-hoc BLE session keeps the discovered identity as
                // a fallback for when the profile is deleted or unreadable.
                if let StandaloneConnectionLaunch::Ble {
                    clues,
                    terminal_options,
                } = &record.launch
                {
                    record.launch = StandaloneConnectionLaunch::SavedBle {
                        profile_id,
                        clues: clues.clone(),
                        terminal_options: terminal_options.clone(),
                    };
                }
            }
            StandaloneConnectionKind::Mosh => {
                // Reacquire protected authentication data for each future connection attempt.
                record.launch = StandaloneConnectionLaunch::SavedMosh { profile_id };
            }
            StandaloneConnectionKind::Rdp | StandaloneConnectionKind::Vnc => {
                record.launch = StandaloneConnectionLaunch::SavedRemoteDesktop { profile_id };
            }
        }
        self.persist();
    }
}
