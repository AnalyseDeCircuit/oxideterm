// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslDistro {
    pub name: String,
    pub is_default: bool,
    pub is_running: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslgStatus {
    pub available: bool,
    pub wayland: bool,
    pub x11: bool,
    pub wslg_version: Option<String>,
    pub has_openbox: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphicsSessionMode {
    Desktop,
    App {
        argv: Vec<String>,
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslGraphicsSession {
    pub id: String,
    pub ws_port: u16,
    pub ws_token: String,
    pub distro: String,
    pub desktop_name: String,
    pub mode: GraphicsSessionMode,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DesktopCandidate {
    pub probe_cmd: &'static str,
    pub launch_cmd: &'static str,
    pub extra_env: &'static str,
    pub display_name: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PrerequisiteResult {
    pub vnc_cmd: String,
    pub desktop: DesktopCandidate,
    pub dbus_cmd: String,
}

const DESKTOP_CANDIDATES: &[DesktopCandidate] = &[
    DesktopCandidate {
        probe_cmd: "xfce4-session",
        launch_cmd: "xfce4-session",
        extra_env: "",
        display_name: "Xfce",
    },
    DesktopCandidate {
        probe_cmd: "gnome-session",
        launch_cmd: "gnome-session --session=gnome-xorg",
        extra_env: "export XDG_SESSION_TYPE=x11\nexport GDK_BACKEND=x11",
        display_name: "GNOME",
    },
    DesktopCandidate {
        probe_cmd: "startplasma-x11",
        launch_cmd: "startplasma-x11",
        extra_env: "export QT_QPA_PLATFORM=xcb\nexport DESKTOP_SESSION=plasma\nexport KWIN_COMPOSE=N",
        display_name: "KDE Plasma",
    },
    DesktopCandidate {
        probe_cmd: "mate-session",
        launch_cmd: "mate-session",
        extra_env: "",
        display_name: "MATE",
    },
    DesktopCandidate {
        probe_cmd: "startlxde",
        launch_cmd: "startlxde",
        extra_env: "",
        display_name: "LXDE",
    },
    DesktopCandidate {
        probe_cmd: "cinnamon-session",
        launch_cmd: "cinnamon-session",
        extra_env: "",
        display_name: "Cinnamon",
    },
    DesktopCandidate {
        probe_cmd: "openbox-session",
        launch_cmd: "openbox-session",
        extra_env: "",
        display_name: "Openbox",
    },
    DesktopCandidate {
        probe_cmd: "fluxbox",
        launch_cmd: "fluxbox",
        extra_env: "",
        display_name: "Fluxbox",
    },
    DesktopCandidate {
        probe_cmd: "icewm-session",
        launch_cmd: "icewm-session",
        extra_env: "",
        display_name: "IceWM",
    },
];

pub fn desktop_candidates() -> &'static [DesktopCandidate] {
    DESKTOP_CANDIDATES
}
