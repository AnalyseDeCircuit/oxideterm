use gpui::{
    AnchoredPositionMode, AnyElement, App, Context, Corner, CursorStyle, Div, Entity, FontWeight,
    MouseButton, MouseDownEvent, MouseMoveEvent, Render, Rgba, SharedString, Subscription, Window, anchored, deferred,
    div, point, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, normalize_sync_scope,
    operation::SyncDirection,
    progress::CloudSyncProgress,
    secret_keys,
    service::CloudSyncLocalSnapshot,
    state::{CloudSyncHistoryEntry, CloudSyncPersistedState},
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    StatusPillOptions, StatusTone, SurfaceKind, SurfaceOptions, SurfacePadding,
    button::ButtonVariant,
    modal::{overlay_content_boundary, popover_backdrop},
    select::{
        BrowserFocusOrigin, OverlayAnchor, SelectAnchorId, browser_focus_visible,
        select_anchor_probe, select_option_action, select_option_highlighted,
        select_panel_overlay_popup_with_max_height,
    },
    semantic_surface, status_pill, status_pill_element,
    text_input::{TextInputAnchorId, TextInputView, TextInputViewport, text_input_anchor_probe},
    virtual_list::tauri_virtual_list,
};
use oxideterm_i18n::I18n;
use oxideterm_settings_model::cloud_sync_form_input_value_ref;
use oxideterm_theme::ThemeTokens;

use crate::entity::{
    CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT, CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT,
    CloudSyncUiIntent, CloudSyncWorkspaceEntity, CloudSyncWorkspaceEvent,
};
use crate::*;

use crate::runtime::CloudSyncDiffRenderData;
use std::{
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

pub const CLOUD_SYNC_SWITCHER_ID: &str = "cloud-sync-tab-bar";
const CLOUD_SYNC_TW_ALPHA_10: u32 = 0x1a;
const CLOUD_SYNC_TW_ALPHA_40: u32 = 0x66;
const CLOUD_SYNC_TW_ALPHA_50: u32 = 0x80;
const CLOUD_SYNC_BG_ACTIVE_THEME_ALPHA: u32 = 0x66;
const CLOUD_SYNC_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60;
const CLOUD_SYNC_SECTION_DIFF_ITEM_MIN_WIDTH: f32 = 320.0;

#[derive(Clone, Copy)]
pub enum SelectableTextRole {
    PlainDocument,
    NonSelectable,
}
pub type CloudSyncTextRenderer =
    Rc<dyn Fn(SelectableTextRole, u64, String, u32, &mut App) -> AnyElement>;
pub type CloudSyncInputViewport =
    Rc<dyn Fn(SettingsInput) -> (TextInputAnchorId, TextInputViewport)>;

fn selectable_text_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

mod causal;
mod config;
mod history;
mod preview;
mod preview_diff;
mod select_state;
mod surface;

pub use config::cloud_sync_select_anchor_id;
/// Frame-scoped, read-only UI dependencies shared by Cloud Sync virtual rows.
pub struct CloudSyncListRenderProjection {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub selectable_text: CloudSyncTextRenderer,
    pub has_background: bool,
    pub input: CloudSyncInputRenderProjection,
    pub diff: CloudSyncDiffRenderData,
    pub mono_font_family: SharedString,
    pub tab_transition_active: bool,
}

/// Contains only frame-scoped text geometry; secret text is length-preserving masked.
#[derive(Clone, Default)]
pub struct CloudSyncInputRenderProjection {
    pub focused_input: Option<SettingsInput>,
    pub active_value: Option<String>,
    pub selected_range: Option<std::ops::Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<String>,
    pub caret_visible: bool,
}

/// Renders Cloud Sync virtual rows without retaining the workspace root.
#[derive(Clone)]
pub struct CloudSyncPageRenderer {
    pub cloud_sync: Entity<CloudSyncWorkspaceEntity>,
    pub render: std::rc::Rc<CloudSyncListRenderProjection>,
    pub input_anchors: CloudSyncInputViewport,
}

type CloudSyncFrame =
    Box<dyn Fn(&mut App) -> Option<(CloudSyncPageRenderer, Option<OverlayAnchor>)>>;
type DismissCloudSyncOverlays = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct CloudSyncPanelView {
    frame: CloudSyncFrame,
    dismiss_overlays: DismissCloudSyncOverlays,
    _sync_changed: Subscription,
}

impl CloudSyncPanelView {
    pub fn new(
        sync: &Entity<CloudSyncWorkspaceEntity>,
        frame: impl Fn(&mut App) -> Option<(CloudSyncPageRenderer, Option<OverlayAnchor>)> + 'static,
        dismiss_overlays: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let sync_changed = cx.observe(sync, |_, _, cx| cx.notify());
        Self {
            frame: Box::new(frame),
            dismiss_overlays: Rc::new(dismiss_overlays),
            _sync_changed: sync_changed,
        }
    }
}

impl Render for CloudSyncPanelView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((renderer, anchor)) = (self.frame)(cx) else {
            return div().into_any_element();
        };
        let dismiss = self.dismiss_overlays.clone();
        renderer.render_panel(anchor, move |window, cx| dismiss(window, cx), cx)
    }
}

impl std::ops::Deref for CloudSyncPageRenderer {
    type Target = CloudSyncListRenderProjection;

    fn deref(&self) -> &Self::Target {
        &self.render
    }
}

pub fn cloud_sync_tab_index(tab: CloudSyncTab) -> usize {
    match tab {
        CloudSyncTab::Overview => 0,
        CloudSyncTab::Configure => 1,
        CloudSyncTab::History => 2,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudSyncActionTone {
    Accent,
    Muted,
}

impl CloudSyncActionTone {
    fn color(self, tokens: &oxideterm_theme::ThemeTokens) -> u32 {
        match self {
            Self::Accent => tokens.ui.accent,
            Self::Muted => tokens.ui.text_muted,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudSyncTone {
    Accent,
    Success,
    Warning,
    Error,
    Muted,
}

impl CloudSyncTone {
    fn color(self, tokens: &oxideterm_theme::ThemeTokens) -> u32 {
        match self {
            Self::Accent => tokens.ui.accent,
            Self::Success => tokens.ui.success,
            Self::Warning => tokens.ui.warning,
            Self::Error => tokens.ui.error,
            Self::Muted => tokens.ui.text_muted,
        }
    }
}

fn cloud_sync_status_tone(tone: CloudSyncTone) -> StatusTone {
    match tone {
        CloudSyncTone::Accent => StatusTone::Accent,
        CloudSyncTone::Success => StatusTone::Success,
        CloudSyncTone::Warning => StatusTone::Warning,
        CloudSyncTone::Error => StatusTone::Error,
        CloudSyncTone::Muted => StatusTone::Neutral,
    }
}

fn health_tone(status: CloudSyncHealthStatus) -> CloudSyncTone {
    match status {
        CloudSyncHealthStatus::Pass => CloudSyncTone::Success,
        CloudSyncHealthStatus::Warning => CloudSyncTone::Warning,
        CloudSyncHealthStatus::Fail => CloudSyncTone::Error,
    }
}

fn local_diff_tone(status: CloudSyncLocalDiffStatus) -> CloudSyncTone {
    match status {
        CloudSyncLocalDiffStatus::Added => CloudSyncTone::Success,
        CloudSyncLocalDiffStatus::Modified => CloudSyncTone::Accent,
        CloudSyncLocalDiffStatus::Deleted => CloudSyncTone::Error,
        CloudSyncLocalDiffStatus::Unchanged | CloudSyncLocalDiffStatus::Excluded => {
            CloudSyncTone::Muted
        }
    }
}

fn remote_diff_tone(status: CloudSyncRemoteDiffStatus) -> CloudSyncTone {
    match status {
        CloudSyncRemoteDiffStatus::Creates => CloudSyncTone::Success,
        CloudSyncRemoteDiffStatus::Overwrites => CloudSyncTone::Warning,
        CloudSyncRemoteDiffStatus::RemovedByScope => CloudSyncTone::Error,
        CloudSyncRemoteDiffStatus::Unchanged | CloudSyncRemoteDiffStatus::Excluded => {
            CloudSyncTone::Muted
        }
        CloudSyncRemoteDiffStatus::Unknown => CloudSyncTone::Warning,
    }
}

fn cloud_sync_root_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        cloud_sync_theme_alpha(0x000000, 0x00)
    } else {
        rgb(color)
    }
}

// Tauri switches bg-theme-* surfaces to alpha-backed colors under
// data-bg-active; Cloud Sync mirrors the plugin manager's native helpers.
fn cloud_sync_theme_panel_bg(color: u32, has_background: bool) -> Rgba {
    cloud_sync_theme_card_bg(color, has_background)
}

fn cloud_sync_theme_card_bg(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background(
        color,
        has_background,
        CLOUD_SYNC_BG_ACTIVE_THEME_ALPHA,
    )
}

fn cloud_sync_theme_border_half(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background_or_alpha(
        color,
        has_background,
        CLOUD_SYNC_BG_ACTIVE_BORDER_HALF_ALPHA,
        CLOUD_SYNC_TW_ALPHA_50,
    )
}

fn cloud_sync_theme_alpha(color: u32, alpha: u32) -> Rgba {
    rgba((color << 8) | alpha)
}
