// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::{X11ForwardingError, X11LocalEndpoint, X11Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum X11DisplayTransport {
    Unix,
    UnixSocket { path: String },
    Tcp { host: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X11Display {
    pub transport: X11DisplayTransport,
    pub display: u16,
    pub screen: u16,
}

impl X11Display {
    pub fn parse(input: &str) -> X11Result<Self> {
        let value = input.trim();
        if value.is_empty() {
            return Err(X11ForwardingError::EmptyDisplay);
        }

        let Some((prefix, suffix)) = value.rsplit_once(':') else {
            return Err(X11ForwardingError::InvalidDisplay(
                "missing display separator ':'".to_string(),
            ));
        };

        let (display, screen) = parse_display_and_screen(suffix)?;
        let transport = parse_transport(prefix)?;

        Ok(Self {
            transport,
            display,
            screen,
        })
    }

    pub fn tcp_port(&self) -> X11Result<u16> {
        6000u16
            .checked_add(self.display)
            .ok_or(X11ForwardingError::DisplayPortOutOfRange(self.display))
    }

    pub fn local_endpoint(&self) -> X11Result<X11LocalEndpoint> {
        match &self.transport {
            X11DisplayTransport::Unix => {
                Ok(X11LocalEndpoint::unix_socket_for_display(self.display))
            }
            X11DisplayTransport::UnixSocket { path } => Ok(X11LocalEndpoint::UnixSocket {
                path: format!("{path}:{}", self.display),
            }),
            X11DisplayTransport::Tcp { host } => Ok(X11LocalEndpoint::Tcp {
                host: host.clone(),
                port: self.tcp_port()?,
            }),
        }
    }

    pub fn remote_display_value(&self, remote_display: u16) -> String {
        format!("localhost:{remote_display}.{}", self.screen)
    }

    pub fn xauth_query_display(&self) -> String {
        match &self.transport {
            X11DisplayTransport::Unix => format!(":{}", self.display),
            X11DisplayTransport::UnixSocket { path } => format!("{path}:{}", self.display),
            X11DisplayTransport::Tcp { host } => format!("{host}:{}", self.display),
        }
    }
}

fn parse_display_and_screen(value: &str) -> X11Result<(u16, u16)> {
    let (display, screen) = match value.split_once('.') {
        Some((display, screen)) => (display, screen),
        None => (value, "0"),
    };

    let display = parse_u16_component(display, "display number")?;
    let screen = parse_u16_component(screen, "screen number")?;
    Ok((display, screen))
}

fn parse_transport(prefix: &str) -> X11Result<X11DisplayTransport> {
    if prefix.is_empty() || prefix == "unix" || prefix == "unix/" || prefix.ends_with("/unix") {
        return Ok(X11DisplayTransport::Unix);
    }

    if prefix.starts_with('/') {
        return Ok(X11DisplayTransport::UnixSocket {
            path: prefix.to_string(),
        });
    }

    if let Some((left, right)) = prefix.split_once('/') {
        if left == "unix" && right.is_empty() {
            return Ok(X11DisplayTransport::Unix);
        }
        if is_tcp_protocol(left) {
            return Ok(X11DisplayTransport::Tcp {
                host: normalize_host(right)?,
            });
        }
        if is_tcp_protocol(right) {
            return Ok(X11DisplayTransport::Tcp {
                host: normalize_host(left)?,
            });
        }
        if right == "unix" {
            return Ok(X11DisplayTransport::Unix);
        }
    }

    Ok(X11DisplayTransport::Tcp {
        host: normalize_host(prefix)?,
    })
}

fn normalize_host(host: &str) -> X11Result<String> {
    let host = host.trim();
    if host.is_empty() {
        return Err(X11ForwardingError::InvalidDisplay(
            "TCP display host must not be empty".to_string(),
        ));
    }
    Ok(host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host)
        .to_string())
}

fn parse_u16_component(value: &str, label: &str) -> X11Result<u16> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(X11ForwardingError::InvalidDisplay(format!(
            "{label} must be a non-negative integer"
        )));
    }
    value.parse::<u16>().map_err(|_| {
        X11ForwardingError::InvalidDisplay(format!("{label} is too large for X11 forwarding"))
    })
}

fn is_tcp_protocol(value: &str) -> bool {
    matches!(value, "tcp" | "inet" | "inet6")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_addresses_resolve_transport_screen_and_local_endpoint() {
        for (input, transport, display, screen, endpoint) in [
            (
                ":0",
                X11DisplayTransport::Unix,
                0,
                0,
                X11LocalEndpoint::UnixSocket {
                    path: "/tmp/.X11-unix/X0".into(),
                },
            ),
            (
                "unix:2.1",
                X11DisplayTransport::Unix,
                2,
                1,
                X11LocalEndpoint::UnixSocket {
                    path: "/tmp/.X11-unix/X2".into(),
                },
            ),
            (
                "localhost/unix:3",
                X11DisplayTransport::Unix,
                3,
                0,
                X11LocalEndpoint::UnixSocket {
                    path: "/tmp/.X11-unix/X3".into(),
                },
            ),
            (
                "/private/tmp/com.apple.launchd.abcd/org.xquartz:0",
                X11DisplayTransport::UnixSocket {
                    path: "/private/tmp/com.apple.launchd.abcd/org.xquartz".into(),
                },
                0,
                0,
                X11LocalEndpoint::UnixSocket {
                    path: "/private/tmp/com.apple.launchd.abcd/org.xquartz:0".into(),
                },
            ),
            (
                "localhost:10.0",
                X11DisplayTransport::Tcp {
                    host: "localhost".into(),
                },
                10,
                0,
                X11LocalEndpoint::Tcp {
                    host: "localhost".into(),
                    port: 6010,
                },
            ),
            (
                "[::1]:4",
                X11DisplayTransport::Tcp { host: "::1".into() },
                4,
                0,
                X11LocalEndpoint::Tcp {
                    host: "::1".into(),
                    port: 6004,
                },
            ),
            (
                "tcp/[::1]:4",
                X11DisplayTransport::Tcp { host: "::1".into() },
                4,
                0,
                X11LocalEndpoint::Tcp {
                    host: "::1".into(),
                    port: 6004,
                },
            ),
        ] {
            let parsed = X11Display::parse(input).unwrap();
            assert_eq!(
                parsed,
                X11Display {
                    transport,
                    display,
                    screen
                },
                "{input}"
            );
            assert_eq!(parsed.local_endpoint().unwrap(), endpoint, "{input}");
        }
    }

    #[test]
    fn invalid_display_addresses_cannot_resolve_a_local_endpoint() {
        for input in ["", "localhost", ":abc", "localhost:60000"] {
            let error = X11Display::parse(input)
                .and_then(|display| display.local_endpoint())
                .unwrap_err();
            match input {
                "" => assert!(matches!(error, X11ForwardingError::EmptyDisplay)),
                "localhost:60000" => assert!(matches!(
                    error,
                    X11ForwardingError::DisplayPortOutOfRange(60000)
                )),
                _ => assert!(
                    matches!(error, X11ForwardingError::InvalidDisplay(_)),
                    "{input}"
                ),
            }
        }
    }
}
