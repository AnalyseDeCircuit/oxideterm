// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub const WSL_GRAPHICS_UNAVAILABLE: &str =
    "WSL Graphics is only available on Windows with the wsl-graphics feature enabled";

#[derive(Debug, thiserror::Error)]
pub enum WslGraphicsError {
    #[error(
        "Xtigervnc is not installed in WSL distro '{0}'. Install with: sudo apt install tigervnc-standalone-server"
    )]
    NoVncServer(String),
    #[error(
        "No supported desktop environment found in WSL distro '{0}'. Install Xfce with: sudo apt install xfce4"
    )]
    NoDesktop(String),
    #[error("No D-Bus launcher found in WSL distro '{0}'. Install with: sudo apt install dbus-x11")]
    NoDbus(String),
    #[error("VNC server did not become ready in time")]
    VncStartTimeout,
    #[error("WSL is not available or no distributions installed")]
    WslNotAvailable,
    #[error("Graphics session not found: {0}")]
    SessionNotFound(String),
    #[error("{0}")]
    SessionLimit(String),
    #[error("{0}")]
    InvalidAppArgv(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{WSL_GRAPHICS_UNAVAILABLE}")]
    UnsupportedPlatform,
}
