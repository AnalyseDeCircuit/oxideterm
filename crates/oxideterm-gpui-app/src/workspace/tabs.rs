use super::*;
use crate::workspace::forwards::ForwardingWorkerResult;

// Tauri keeps terminal tab actions in a fixed right-side toolbar outside the
// scroll container. Keep these shared geometry constants in one place so the
// renderer and scroll math cannot drift apart.
const TABBAR_LEGACY_ACTION_PADDING_X: f32 = 8.0;
const TABBAR_LEGACY_ACTION_GAP: f32 = 4.0;
const TABBAR_LEGACY_ACTION_BUTTON_SIZE: f32 = 24.0;
const TABBAR_LEGACY_ACTION_BORDER_WIDTH: f32 = 1.0;
const TABBAR_LEGACY_BROADCAST_BADGE_HEIGHT: f32 = 20.0;
const TABBAR_LEGACY_BROADCAST_BADGE_PADDING_X: f32 = 6.0;
const TABBAR_LEGACY_BROADCAST_BADGE_GAP: f32 = 4.0;
const TABBAR_LEGACY_BROADCAST_ICON_SIZE: f32 = 12.0;
const TABBAR_LEGACY_BROADCAST_FONT_SIZE: f32 = 11.0;
const TABBAR_LEGACY_PANE_BADGE_MIN_WIDTH: f32 = 20.0;

include!("tabs/create.rs");
include!("tabs/state.rs");
include!("tabs/nodes.rs");
include!("tabs/nodes_reconnect_helpers.rs");
include!("tabs/navigation.rs");
include!("tabs/render.rs");
include!("tabs/helpers.rs");
