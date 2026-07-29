use super::*;

// Keep tab responsibilities in real modules while preserving WorkspaceApp's API.
mod create;
mod detach;
mod entity;
mod helpers;
mod navigation;
mod nodes;
mod nodes_reconnect_helpers;
mod render;
mod state;

pub(in crate::workspace) use entity::{
    TabCloseConfirmKeyAction, TabCloseProcessProbe, TabDetachTransition, TabMountCleanupPlan,
    TabMountCloseReason, TabMountId, TabRemovalTransition, TerminalLocation,
    WorkspaceTabHostEntity, WorkspaceTabHostEvent,
};

// The main tab strip keeps a thin visual thumb while exposing a larger drag target.
const TABBAR_SCROLLBAR_HEIGHT: f32 = 3.0;
const TABBAR_SCROLLBAR_DRAG_HEIGHT: f32 = 10.0;
const TABBAR_SCROLLBAR_HORIZONTAL_INSET: f32 = 8.0;
const TABBAR_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 32.0;
const TABBAR_SCROLLBAR_MAX_THUMB_WIDTH: f32 = 160.0;
const TABBAR_SCROLLBAR_RADIUS: f32 = 2.0;
const TABBAR_SCROLLBAR_ALPHA: u32 = 0x40;
const TABBAR_SCROLLBAR_HOVER_ALPHA: u32 = 0x99;

#[derive(Clone, Copy)]
struct TabbarScrollbarGeometry {
    viewport_left: f32,
    track_width: f32,
    thumb_width: f32,
    thumb_left: f32,
    max_scroll: f32,
}

// Keep the geometry calculation independent from GPUI events so edge cases remain testable.
fn calculate_tabbar_scrollbar_geometry(
    viewport_left: f32,
    viewport_width: f32,
    max_scroll: f32,
    scroll_x: f32,
) -> Option<TabbarScrollbarGeometry> {
    let track_width = (viewport_width - TABBAR_SCROLLBAR_HORIZONTAL_INSET * 2.0).max(0.0);
    if viewport_width <= 1.0 || max_scroll <= 1.0 || track_width <= 1.0 {
        return None;
    }

    let content_width = viewport_width + max_scroll;
    let min_thumb_width = TABBAR_SCROLLBAR_MIN_THUMB_WIDTH.min(track_width);
    let max_thumb_width = TABBAR_SCROLLBAR_MAX_THUMB_WIDTH
        .max(min_thumb_width)
        .min(track_width);
    let thumb_width = (viewport_width / content_width * track_width)
        .max(min_thumb_width)
        .min(max_thumb_width);
    let thumb_travel = track_width - thumb_width;
    if thumb_travel <= 1.0 {
        return None;
    }

    let thumb_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET
        + scroll_x.clamp(0.0, max_scroll) / max_scroll * thumb_travel;
    Some(TabbarScrollbarGeometry {
        viewport_left,
        track_width,
        thumb_width,
        thumb_left,
        max_scroll,
    })
}

// Convert a clamped thumb position back into the tab strip's logical scroll offset.
fn tabbar_scroll_x_for_thumb_left(thumb_left: f32, geometry: TabbarScrollbarGeometry) -> f32 {
    let track_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET;
    let thumb_travel = geometry.track_width - geometry.thumb_width;
    let clamped_left = thumb_left.clamp(track_left, track_left + thumb_travel);
    (clamped_left - track_left) / thumb_travel * geometry.max_scroll
}
