use super::*;
use crate::workspace::new_connection::MoshConnectionOptions;
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopProviderManifest, RemoteDesktopSecret,
};

pub(super) type StandaloneConnectionId = String;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum StandaloneConnectionKind {
    Mosh,
    Telnet,
    Serial,
    Rdp,
    Vnc,
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
        terminal_options: ConnectionTerminalOptions,
    },
    SavedTelnet {
        profile_id: String,
        config: TelnetSessionConfig,
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
}

pub(super) struct StandaloneConnectionRecord {
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
                terminal_options,
                saved_profile_id,
            } => self
                .create_telnet_terminal_tab_for_connection(
                    config,
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
                terminal_options,
            } => StandaloneReconnectPlan::Telnet {
                config: config.clone(),
                terminal_options: terminal_options.clone(),
                saved_profile_id: None,
            },
            StandaloneConnectionLaunch::SavedTelnet {
                profile_id,
                config,
                terminal_options,
            } => {
                let current_profile = self
                    .connection_store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.id == *profile_id);
                StandaloneReconnectPlan::Telnet {
                    config: current_profile.map_or_else(
                        || config.clone(),
                        |profile| TelnetSessionConfig {
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
            },
            terminal_options: ConnectionTerminalOptions::default(),
        }
    }

    #[test]
    fn releasing_surface_keeps_connection_record_for_fresh_binding() {
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
        assert!(registry.bind_surface_for_attempt(&next_attempt_id, next_surface));
        let rebound = registry.record(&connection_id).expect("record must remain");
        assert_eq!(rebound.surface, Some(next_surface));
        assert_ne!(rebound.surface, Some(first_surface));
    }

    #[test]
    fn cancelled_pending_connection_rejects_late_failure_state() {
        let surface = StandaloneConnectionSurface::Terminal(TerminalSessionId(19));
        let mut registry = StandaloneConnectionRegistry::default();
        let connection_id = registry.insert(
            StandaloneConnectionKind::Serial,
            "Pending serial".to_string(),
            serial_launch(),
            surface,
        );
        registry.release_surface(surface);
        registry.record_mut(&connection_id).unwrap().readiness = ActiveSessionReadiness::Connecting;

        registry.mark_disconnected(&connection_id);
        registry.mark_attempt_error(&connection_id);

        let cancelled = registry.record(&connection_id).unwrap();
        assert_eq!(cancelled.readiness, ActiveSessionReadiness::Disconnected);
        assert!(!registry.is_connecting_attempt(&connection_id));
    }

    #[test]
    fn stale_attempt_cannot_complete_after_a_new_reconnect_starts() {
        let first_surface = StandaloneConnectionSurface::Terminal(TerminalSessionId(5));
        let mut registry = StandaloneConnectionRegistry::default();
        let connection_id = registry.insert(
            StandaloneConnectionKind::Serial,
            "Serial generation".to_string(),
            serial_launch(),
            first_surface,
        );
        let stale_attempt_id = connection_id.clone();
        registry.release_surface(first_surface);
        let current_attempt_id = registry.begin_reconnect(&connection_id).unwrap();

        registry.mark_attempt_error(&stale_attempt_id);

        assert!(registry.is_connecting_attempt(&current_attempt_id));
        assert_eq!(
            registry.record(&connection_id).unwrap().readiness,
            ActiveSessionReadiness::Connecting
        );
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
            id: id.clone(),
            attempt_id: id.clone(),
            kind,
            title,
            launch,
            surface: Some(surface),
            readiness: ActiveSessionReadiness::Ready,
        });
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
            id: id.clone(),
            attempt_id: id.clone(),
            kind,
            title,
            launch,
            surface: None,
            readiness: ActiveSessionReadiness::Connecting,
        });
        id
    }

    pub(super) fn bind_surface_for_attempt(
        &mut self,
        attempt_id: &str,
        surface: StandaloneConnectionSurface,
    ) -> bool {
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt_id)
        else {
            return false;
        };
        record.surface = Some(surface);
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
        }
    }

    pub(super) fn mark_disconnected(&mut self, id: &str) {
        if let Some(record) = self.record_mut(id) {
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

    pub(super) fn begin_reconnect(&mut self, id: &str) -> Option<String> {
        let record = self.record_mut(id)?;
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
        }
    }

    pub(super) fn remove(&mut self, id: &str) -> Option<StandaloneConnectionRecord> {
        let index = self.records.iter().position(|record| record.id == id)?;
        Some(self.records.remove(index))
    }

    pub(super) fn replace_with_saved_profile(
        &mut self,
        surface: StandaloneConnectionSurface,
        profile_id: String,
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
                if let StandaloneConnectionLaunch::Telnet {
                    config,
                    terminal_options,
                } = &record.launch
                {
                    record.launch = StandaloneConnectionLaunch::SavedTelnet {
                        profile_id,
                        config: config.clone(),
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
    }
}
