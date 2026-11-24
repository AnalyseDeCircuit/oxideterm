//! Forwarding views and per-tab interaction state. SSH transports and listeners are hosted separately.

mod components;
pub mod data;
mod forms;
mod helpers;
mod model;
mod request;
mod sampling;
mod style;
mod surface;
mod view;

pub use model::{ForwardInput, ForwardsInputKey, ForwardsPageState, PortDetectionViewState};
pub use request::{
    ForwardingQuickAction, ForwardingRuntimeOperation, ForwardingRuntimeSnapshot, ForwardsFrame,
    ForwardsImeFrame, ForwardsRequest, ForwardsViewBindings,
};
pub use sampling::{FORWARDS_PORT_SCAN_INTERVAL, ForwardsSampler, ForwardsSamplingDue};
pub use view::ForwardsViews;

use style::*;

use data::{DetectedPort, ForwardRule, ForwardStats, ForwardStatus, ForwardType, NodeId};
use gpui::{
    AnyElement, App, ClipboardItem, Context, MouseButton, MouseDownEvent, SharedString, Window,
    div, px, rgb,
};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant as UiButtonVariant,
        IconButtonOptions, ToolbarButtonOptions,
    },
    confirm_dialog,
    icons::LucideIcon,
    modal::rounded_shell_child_radius,
    surface::{
        color_for_background, color_for_background_or_alpha, color_with_alpha,
        tauri_glass_surface_shadow,
    },
    text_input::TextInputView,
    virtual_list::{
        TauriVirtualListSpec, sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_workspace::TabId;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    time::Duration,
};
use view::ForwardsView;
