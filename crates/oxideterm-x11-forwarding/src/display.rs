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
    fn parses_local_unix_displays() {
        assert_eq!(
            X11Display::parse(":0").unwrap(),
            X11Display {
                transport: X11DisplayTransport::Unix,
                display: 0,
                screen: 0,
            }
        );
        assert_eq!(X11Display::parse("unix:2.1").unwrap().screen, 1);
        assert_eq!(
            X11Display::parse("localhost/unix:3").unwrap().transport,
            X11DisplayTransport::Unix
        );
        assert_eq!(
            X11Display::parse("/private/tmp/com.apple.launchd.abcd/org.xquartz:0")
                .unwrap()
                .transport,
            X11DisplayTransport::UnixSocket {
                path: "/private/tmp/com.apple.launchd.abcd/org.xquartz".to_string()
            }
        );
    }

    #[test]
    fn parses_tcp_displays() {
        assert_eq!(
            X11Display::parse("localhost:10.0").unwrap(),
            X11Display {
                transport: X11DisplayTransport::Tcp {
                    host: "localhost".to_string()
                },
                display: 10,
                screen: 0,
            }
        );
        assert_eq!(
            X11Display::parse("[::1]:4").unwrap().transport,
            X11DisplayTransport::Tcp {
                host: "::1".to_string()
            }
        );
        assert_eq!(
            X11Display::parse("tcp/[::1]:4").unwrap().transport,
            X11DisplayTransport::Tcp {
                host: "::1".to_string()
            }
        );
    }

    #[test]
    fn rejects_invalid_displays() {
        assert!(matches!(
            X11Display::parse(""),
            Err(X11ForwardingError::EmptyDisplay)
        ));
        assert!(matches!(
            X11Display::parse("localhost"),
            Err(X11ForwardingError::InvalidDisplay(_))
        ));
        assert!(matches!(
            X11Display::parse(":abc"),
            Err(X11ForwardingError::InvalidDisplay(_))
        ));
    }

    #[test]
    fn builds_local_endpoints_from_display_transport() {
        assert_eq!(
            X11Display::parse(":0").unwrap().local_endpoint().unwrap(),
            X11LocalEndpoint::UnixSocket {
                path: "/tmp/.X11-unix/X0".to_string()
            }
        );
        assert_eq!(
            X11Display::parse("localhost:10")
                .unwrap()
                .local_endpoint()
                .unwrap(),
            X11LocalEndpoint::Tcp {
                host: "localhost".to_string(),
                port: 6010
            }
        );
        assert_eq!(
            X11Display::parse("/private/tmp/com.apple.launchd.abcd/org.xquartz:0")
                .unwrap()
                .local_endpoint()
                .unwrap(),
            X11LocalEndpoint::UnixSocket {
                path: "/private/tmp/com.apple.launchd.abcd/org.xquartz:0".to_string()
            }
        );
    }

    #[test]
    fn tcp_port_rejects_unrepresentable_display_numbers() {
        assert!(matches!(
            X11Display::parse("localhost:60000")
                .unwrap()
                .local_endpoint(),
            Err(X11ForwardingError::DisplayPortOutOfRange(60000))
        ));
    }
}
