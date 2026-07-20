// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationProxyError {
    #[error("application proxy is unavailable: {0}")]
    Unavailable(String),
    #[error("invalid application proxy configuration: {0}")]
    InvalidConfiguration(String),
    #[error("failed to configure application proxy: {0}")]
    Client(#[from] reqwest::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationProxyProtocol {
    HttpConnect,
    HttpsConnect,
    Socks5,
}

#[derive(Eq, PartialEq)]
pub enum ApplicationProxyAuth {
    None,
    Password {
        username: String,
        password: Zeroizing<String>,
    },
}

impl fmt::Debug for ApplicationProxyAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::Password { username, .. } => formatter
                .debug_struct("Password")
                .field("username", username)
                .field("password", &"[redacted secret]")
                .finish(),
        }
    }
}

#[derive(Eq, PartialEq)]
pub struct CustomApplicationProxy {
    pub protocol: ApplicationProxyProtocol,
    pub host: String,
    pub port: u16,
    pub auth: ApplicationProxyAuth,
    pub remote_dns: bool,
    pub no_proxy: String,
}

impl fmt::Debug for CustomApplicationProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomApplicationProxy")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("auth", &self.auth)
            .field("remote_dns", &self.remote_dns)
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub enum ApplicationProxyPolicy {
    Direct,
    #[default]
    System,
    Custom(CustomApplicationProxy),
    Unavailable {
        reason: String,
    },
}
