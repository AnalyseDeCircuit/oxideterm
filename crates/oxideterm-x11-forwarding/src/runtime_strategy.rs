// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11RuntimePlatform {
    Unix,
    MacOs,
    Windows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11RuntimeSupport {
    Full,
    Partial,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11RuntimeStrategy {
    pub platform: X11RuntimePlatform,
    pub support: X11RuntimeSupport,
    pub supports_unix_socket_endpoint: bool,
    pub supports_tcp_endpoint: bool,
    pub can_read_xauthority_file: bool,
    pub can_run_remote_xauth_update: bool,
    pub note: &'static str,
}

impl X11RuntimeStrategy {
    pub fn for_current_platform() -> Self {
        Self::for_platform(current_platform())
    }

    pub fn for_platform(platform: X11RuntimePlatform) -> Self {
        match platform {
            X11RuntimePlatform::Unix => Self {
                platform,
                support: X11RuntimeSupport::Full,
                supports_unix_socket_endpoint: true,
                supports_tcp_endpoint: true,
                can_read_xauthority_file: true,
                can_run_remote_xauth_update: true,
                note: "Unix runtime can use local Unix/TCP X11 endpoints and remote xauth.",
            },
            X11RuntimePlatform::MacOs => Self {
                platform,
                support: X11RuntimeSupport::Partial,
                supports_unix_socket_endpoint: true,
                supports_tcp_endpoint: true,
                can_read_xauthority_file: true,
                can_run_remote_xauth_update: true,
                note: "macOS runtime depends on XQuartz DISPLAY/XAUTHORITY being available.",
            },
            X11RuntimePlatform::Windows => Self {
                platform,
                support: X11RuntimeSupport::Partial,
                supports_unix_socket_endpoint: false,
                supports_tcp_endpoint: true,
                can_read_xauthority_file: true,
                can_run_remote_xauth_update: true,
                note: "Windows runtime requires a TCP X server endpoint; Unix socket DISPLAY values are unsupported.",
            },
        }
    }
}

#[cfg(target_os = "macos")]
fn current_platform() -> X11RuntimePlatform {
    X11RuntimePlatform::MacOs
}

#[cfg(all(unix, not(target_os = "macos")))]
fn current_platform() -> X11RuntimePlatform {
    X11RuntimePlatform::Unix
}

#[cfg(windows)]
fn current_platform() -> X11RuntimePlatform {
    X11RuntimePlatform::Windows
}

#[cfg(not(any(unix, windows)))]
fn current_platform() -> X11RuntimePlatform {
    X11RuntimePlatform::Unix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_strategy_rejects_unix_socket_endpoint() {
        let strategy = X11RuntimeStrategy::for_platform(X11RuntimePlatform::Windows);

        assert_eq!(strategy.support, X11RuntimeSupport::Partial);
        assert!(!strategy.supports_unix_socket_endpoint);
        assert!(strategy.supports_tcp_endpoint);
    }

    #[test]
    fn macos_strategy_is_partial_but_uses_xquartz_paths() {
        let strategy = X11RuntimeStrategy::for_platform(X11RuntimePlatform::MacOs);

        assert_eq!(strategy.support, X11RuntimeSupport::Partial);
        assert!(strategy.supports_unix_socket_endpoint);
        assert!(strategy.can_read_xauthority_file);
    }

    #[test]
    fn unix_strategy_is_full_runtime_baseline() {
        let strategy = X11RuntimeStrategy::for_platform(X11RuntimePlatform::Unix);

        assert_eq!(strategy.support, X11RuntimeSupport::Full);
        assert!(strategy.supports_unix_socket_endpoint);
        assert!(strategy.can_run_remote_xauth_update);
    }
}
