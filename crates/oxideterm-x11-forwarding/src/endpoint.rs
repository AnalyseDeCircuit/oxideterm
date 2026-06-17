// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum X11LocalEndpoint {
    UnixSocket { path: String },
    Tcp { host: String, port: u16 },
}

impl X11LocalEndpoint {
    pub fn unix_socket_for_display(display: u16) -> Self {
        Self::UnixSocket {
            path: format!("/tmp/.X11-unix/X{display}"),
        }
    }

    pub fn is_loopback_tcp(&self) -> bool {
        match self {
            Self::Tcp { host, .. } => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
            Self::UnixSocket { .. } => false,
        }
    }
}
