use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, ParentElement, SharedString, Styled, Window,
    div, prelude::*, px, rgb,
};
use oxideterm_forwarding::{
    DetectedPort, ForwardEvent, ForwardRule, ForwardStats, ForwardStatus, ForwardType,
    ForwardUpdate, ForwardingManager, ForwardingRegistry, PortDetectionSnapshot,
};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{ButtonRadius, IconButtonOptions, icon_button},
    confirm_dialog,
    surface::{color_for_background, color_for_background_or_alpha, color_with_alpha},
    text_input::{TextInputView, text_input, text_input_anchor_probe},
    typography::tauri_cjk_ui_font_family as forwards_cjk_ui_font_family,
};
use oxideterm_ssh::NodeId;
use oxideterm_workspace::{Tab, TabId, TabKind, TabTitleSource};

use super::ime::WorkspaceImeTarget;
use super::*;

const FORWARDS_MAX_WIDTH: f32 = 896.0; // Tauri max-w-4xl
const FORWARDS_PAGE_PADDING: f32 = 16.0; // Tauri p-4
const FORWARDS_SECTION_GAP: f32 = 24.0; // Tauri space-y-6
const FORWARDS_TABLE_HEADER_H: f32 = 34.0; // Tauri px-4 py-2 text-sm
const FORWARDS_TABLE_ROW_H: f32 = 42.0;
const FORWARDS_TYPE_BADGE_H: f32 = 20.0;
const FORWARDS_PORT_SCAN_INTERVAL: Duration = Duration::from_secs(12);
const FORWARDS_STATS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const FORWARDS_BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] theme bg/panel/card 40%
const FORWARDS_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri [data-bg-active] bg-hover 50%
const FORWARDS_BG_ACTIVE_SUNKEN_ALPHA: u32 = 0x59; // Tauri [data-bg-active] bg-sunken 35%
const FORWARDS_BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri [data-bg-active] border 75%
const FORWARDS_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
const FORWARDS_TW_ALPHA_05: u32 = 0x0d; // Tauri /5
const FORWARDS_TW_ALPHA_30: u32 = 0x4d; // Tauri /30
const FORWARDS_TW_ALPHA_40: u32 = 0x66; // Tauri /40
const FORWARDS_TW_ALPHA_50: u32 = 0x80; // Tauri /50
const FORWARDS_ALPHA_TRANSPARENT: u32 = 0x00; // Tauri transparent root when tab background is active
const FORWARDS_DEFAULT_BIND_ADDRESS: &str = "localhost"; // Tauri create form default bindAddress
const FORWARDS_DEFAULT_TARGET_HOST: &str = "localhost"; // Tauri create form default targetHost
pub(crate) const FORWARDS_NODE_SESSION_PREFIX: &str = "node:";
// Tailwind palette literals used by the Tauri ForwardsView source.
const TW_BLACK: u32 = 0x000000;
const TW_BLUE_300: u32 = 0x93c5fd;
const TW_BLUE_400: u32 = 0x60a5fa;
const TW_BLUE_500: u32 = 0x3b82f6;
const TW_BLUE_900: u32 = 0x1e3a8a;
const TW_CYAN_500: u32 = 0x06b6d4;
const TW_EMERALD_400: u32 = 0x34d399;
const TW_EMERALD_800: u32 = 0x065f46;
const TW_EMERALD_900: u32 = 0x064e3b;
const TW_GREEN_400: u32 = 0x4ade80;
const TW_GREEN_500: u32 = 0x22c55e;
const TW_ORANGE_400: u32 = 0xfb923c;
const TW_ORANGE_500: u32 = 0xf97316;
const TW_PURPLE_400: u32 = 0xc084fc;
const TW_PURPLE_900: u32 = 0x581c87;
const TW_RED_400: u32 = 0xf87171;
const TW_RED_500: u32 = 0xef4444;
const TW_RED_900: u32 = 0x7f1d1d;
const TW_RED_950: u32 = 0x450a0a;
const TW_YELLOW_400: u32 = 0xfacc15;
const TW_YELLOW_900: u32 = 0x713f12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum ForwardInput {
    CreateBindAddress,
    CreateBindPort,
    CreateTargetHost,
    CreateTargetPort,
    EditBindAddress,
    EditBindPort,
    EditTargetHost,
    EditTargetPort,
}

impl ForwardInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::CreateBindAddress => 1,
            Self::CreateBindPort => 2,
            Self::CreateTargetHost => 3,
            Self::CreateTargetPort => 4,
            Self::EditBindAddress => 5,
            Self::EditBindPort => 6,
            Self::EditTargetHost => 7,
            Self::EditTargetPort => 8,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ForwardsViewState {
    show_new_form: bool,
    editing_forward: Option<ForwardRule>,
    pending_delete_forward: Option<ForwardRule>,
    copied_forward_id: Option<String>,
    forward_type: ForwardType,
    bind_address: String,
    bind_port: String,
    target_host: String,
    target_port: String,
    skip_health_check: bool,
    edit_bind_address: String,
    edit_bind_port: String,
    edit_target_host: String,
    edit_target_port: String,
    pub(super) focused_input: Option<ForwardInput>,
    pub(super) error: Option<String>,
    pending: bool,
    detected_ports: Vec<DetectedPort>,
    new_ports: Vec<DetectedPort>,
    has_scanned_ports: bool,
    port_scan_pending: bool,
    port_scan_error: Option<String>,
    last_port_scan_started: Option<Instant>,
    last_stats_refresh: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PortDetectionViewState {
    connection_id: Option<String>,
    detected_ports: Vec<DetectedPort>,
    new_ports: Vec<DetectedPort>,
    has_scanned_ports: bool,
    port_scan_pending: bool,
    port_scan_error: Option<String>,
    last_port_scan_started: Option<Instant>,
}

impl Default for ForwardsViewState {
    fn default() -> Self {
        Self {
            show_new_form: false,
            editing_forward: None,
            pending_delete_forward: None,
            copied_forward_id: None,
            forward_type: ForwardType::Local,
            bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            bind_port: String::new(),
            target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            target_port: String::new(),
            skip_health_check: false,
            edit_bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            edit_bind_port: String::new(),
            edit_target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            edit_target_port: String::new(),
            focused_input: None,
            error: None,
            pending: false,
            detected_ports: Vec::new(),
            new_ports: Vec::new(),
            has_scanned_ports: false,
            port_scan_pending: false,
            port_scan_error: None,
            last_port_scan_started: None,
            last_stats_refresh: None,
        }
    }
}

pub(super) enum ForwardingWorkerResult {
    Operation {
        tab_id: TabId,
        message_key: &'static str,
        binding: Option<(String, String, ConnectionConsumer)>,
        result: Result<(), String>,
    },
    Binding {
        binding: Option<(String, String, ConnectionConsumer)>,
    },
    PortScan {
        node_id: NodeId,
        connection_id: Option<String>,
        binding: Option<(String, String, ConnectionConsumer)>,
        result: Result<PortDetectionSnapshot, String>,
    },
}

include!("forwards/surface.rs");
include!("forwards/forms.rs");
include!("forwards/components.rs");
include!("forwards/actions.rs");
include!("forwards/helpers.rs");
