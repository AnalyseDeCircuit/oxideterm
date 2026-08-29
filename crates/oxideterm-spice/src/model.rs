// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, path::PathBuf};

use oxide_spice_helper_protocol::{
    HelperConnectOptions, HelperEndpoint, HelperEvent, HelperSasl, HelperSecret,
    HelperTransportSecurity,
};
use oxideterm_remote_desktop::{
    RemoteDesktopFrameDeliverySlot, RemoteDesktopHelperEvent, RemoteDesktopWorkerId,
};
use zeroize::Zeroizing;

/// A SPICE Ticket or SASL password whose allocation is cleared when ownership ends.
pub struct SpiceSecret(Zeroizing<String>);

impl SpiceSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    fn into_helper_secret(mut self) -> HelperSecret {
        // Move the String allocation across the process boundary without creating a plaintext copy.
        HelperSecret::new(std::mem::take(&mut *self.0))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn duplicate_for_reauthentication(&self) -> Self {
        // A session keeps one zeroizing owner so reconnect can start a fresh helper process.
        Self(Zeroizing::new(self.0.to_string()))
    }
}

impl From<String> for SpiceSecret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<Zeroizing<String>> for SpiceSecret {
    fn from(value: Zeroizing<String>) -> Self {
        Self(value)
    }
}

impl fmt::Debug for SpiceSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted SPICE secret]")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpiceEndpoint {
    Tcp {
        host: String,
        port: u16,
    },
    #[cfg(unix)]
    Unix {
        path: PathBuf,
    },
}

pub enum SpiceTransportSecurity {
    Plain,
    Tls {
        server_name: String,
        root_certificates_der: Vec<Vec<u8>>,
    },
}

pub enum SpiceSasl {
    Gssapi {
        hostname: String,
        service: String,
    },
    Password {
        hostname: String,
        service: String,
        authentication_id: String,
        authorization_id: Option<String>,
        password: SpiceSecret,
        allow_gssapi: bool,
    },
}

pub struct SpiceConnectOptions {
    pub endpoint: SpiceEndpoint,
    pub ticket: SpiceSecret,
    pub transport_security: SpiceTransportSecurity,
    pub sasl: Option<SpiceSasl>,
}

impl SpiceConnectOptions {
    pub fn plain_tcp(host: impl Into<String>, port: u16, ticket: SpiceSecret) -> Self {
        Self {
            endpoint: SpiceEndpoint::Tcp {
                host: host.into(),
                port,
            },
            ticket,
            transport_security: SpiceTransportSecurity::Plain,
            sasl: None,
        }
    }

    pub(crate) fn into_helper_options(self) -> HelperConnectOptions {
        let endpoint = match self.endpoint {
            SpiceEndpoint::Tcp { host, port } => HelperEndpoint::Tcp { host, port },
            #[cfg(unix)]
            SpiceEndpoint::Unix { path } => HelperEndpoint::Unix { path },
        };
        let transport_security = match self.transport_security {
            SpiceTransportSecurity::Plain => HelperTransportSecurity::Plain,
            SpiceTransportSecurity::Tls {
                server_name,
                root_certificates_der,
            } => HelperTransportSecurity::Tls {
                server_name,
                root_certificates_der,
            },
        };
        let sasl = self.sasl.map(|sasl| match sasl {
            SpiceSasl::Gssapi { hostname, service } => HelperSasl::Gssapi { hostname, service },
            SpiceSasl::Password {
                hostname,
                service,
                authentication_id,
                authorization_id,
                password,
                allow_gssapi,
            } => HelperSasl::Password {
                hostname,
                service,
                authentication_id,
                authorization_id,
                password: password.into_helper_secret(),
                allow_gssapi,
            },
        });
        HelperConnectOptions {
            endpoint,
            ticket: self.ticket.into_helper_secret(),
            transport_security,
            sasl,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpiceHelperCommand {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
}

pub struct SpiceWorkerConfig {
    pub worker_id: RemoteDesktopWorkerId,
    pub helper: SpiceHelperCommand,
    pub connect: SpiceConnectOptions,
    pub frame_slot: RemoteDesktopFrameDeliverySlot,
    pub audio_playback: bool,
    pub audio_capture: bool,
}

pub enum SpiceWorkerDelivery {
    FrameReady {
        worker_id: RemoteDesktopWorkerId,
    },
    FrameRecoveryRequired {
        worker_id: RemoteDesktopWorkerId,
    },
    RemoteDesktopEvent {
        worker_id: RemoteDesktopWorkerId,
        event: RemoteDesktopHelperEvent,
    },
    Event {
        worker_id: RemoteDesktopWorkerId,
        event: HelperEvent,
    },
    TransportFailed {
        worker_id: RemoteDesktopWorkerId,
        message: String,
    },
    Terminated {
        worker_id: RemoteDesktopWorkerId,
        exit_code: Option<i32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = SpiceSecret::new("spice-ticket-marker");

        let output = format!("{secret:?}");

        assert!(output.contains("redacted"));
        assert!(!output.contains("spice-ticket-marker"));
    }
}
