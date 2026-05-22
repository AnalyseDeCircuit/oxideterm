use std::ops::Range;

use gpui::{
    App, ElementId, IntoElement, List, ListAlignment, ListState, Pixels, Point, ScrollStrategy,
    Styled, UniformList, UniformListScrollHandle, Window, list, px, uniform_list,
};

const BROWSER_DRAG_AUTOSCROLL_EDGE_PX: f32 = 48.0;
const BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX: f32 = 26.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TauriVirtualScrollAlign {
    Nearest,
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TauriVirtualListSpec {
    pub row_height: Pixels,
    pub overscan: usize,
}

impl TauriVirtualListSpec {
    pub(crate) fn new(row_height: Pixels, overscan: usize) -> Self {
        Self {
            row_height,
            overscan,
        }
    }

    pub(crate) fn overdraw(self) -> Pixels {
        self.row_height * self.overscan as f32
    }
}

#[derive(Default)]
pub(super) struct VirtualListSignatureCache {
    identity: Option<String>,
    signatures: Vec<u64>,
}

pub(crate) fn tracked_uniform_list<R>(
    id: impl Into<ElementId>,
    item_count: usize,
    scroll_handle: UniformListScrollHandle,
    render_items: impl 'static + Fn(Range<usize>, &mut Window, &mut App) -> Vec<R>,
) -> UniformList
where
    R: IntoElement,
{
    // Tauri list surfaces rely on the browser scroll container as the single owner.
    // Native GPUI call sites should get the same tracked scroll wiring through one helper.
    uniform_list(id, item_count, render_items)
        .track_scroll(scroll_handle)
        .size_full()
}

pub(crate) fn tauri_virtual_uniform_list<R>(
    id: impl Into<ElementId>,
    item_count: usize,
    scroll_handle: UniformListScrollHandle,
    spec: TauriVirtualListSpec,
    render_items: impl 'static + Fn(Range<usize>, &mut Window, &mut App) -> Vec<R>,
) -> UniformList
where
    R: IntoElement,
{
    // Mirrors TanStack useVirtualizer({ estimateSize, overscan }) from Tauri.
    // GPUI uniform_list measures actual rows, but every migrated list should keep
    // the Tauri estimate/overscan beside the call site instead of hiding it in CSS.
    let _estimated_row_height = spec.row_height;
    let _overdraw = spec.overdraw();
    tracked_uniform_list(id, item_count, scroll_handle, render_items)
}

pub(crate) fn tauri_virtual_list<R>(
    state: ListState,
    render_item: impl 'static + FnMut(usize, &mut Window, &mut App) -> R,
) -> List
where
    R: IntoElement,
{
    // Variable-height browser lists still share the same virtual-list policy:
    // feature code owns identity/signature sync, and the shared helper owns the
    // GPUI list shell so scroll surfaces do not drift per page.
    let mut render_item = render_item;
    list(state, move |index, window, cx| {
        render_item(index, window, cx).into_any_element()
    })
    .size_full()
}

pub(crate) fn scroll_tauri_virtual_list_to_index(
    handle: &UniformListScrollHandle,
    index: usize,
    align: TauriVirtualScrollAlign,
) {
    match align {
        TauriVirtualScrollAlign::Nearest => {
            handle.scroll_to_item(index, ScrollStrategy::Center);
        }
        TauriVirtualScrollAlign::Start => {
            handle.scroll_to_item_strict(index, ScrollStrategy::Top);
        }
        TauriVirtualScrollAlign::Center => {
            handle.scroll_to_item_strict(index, ScrollStrategy::Center);
        }
        TauriVirtualScrollAlign::End => {
            handle.scroll_to_item_strict(index, ScrollStrategy::Bottom);
        }
    }
}

pub(crate) fn tauri_virtual_list_is_near_bottom(
    handle: &UniformListScrollHandle,
    threshold: Pixels,
) -> bool {
    // Browser scroll containers keep an event log "sticky" while the user is
    // within a small bottom threshold. GPUI's uniform list owns the same base
    // scroll handle internally, so expose the threshold test once for migrated
    // lists instead of reimplementing per sidebar or log view.
    let base_handle = handle.0.borrow().base_handle.clone();
    let max_offset = base_handle.max_offset();
    if max_offset.height <= px(0.0) {
        return true;
    }
    let remaining_to_bottom = max_offset.height + base_handle.offset().y;
    remaining_to_bottom <= threshold
}

pub(crate) fn uniform_list_edge_autoscroll(
    handle: &UniformListScrollHandle,
    position: Point<Pixels>,
) -> bool {
    // Browser drag interactions keep scrolling the owning list when the cursor
    // is held near an edge. GPUI exposes the backing ScrollHandle through the
    // uniform-list state, so keep the math centralized instead of duplicating
    // per file list or picker.
    let base_handle = handle.0.borrow().base_handle.clone();
    let bounds = base_handle.bounds();
    let max_offset = base_handle.max_offset();
    if max_offset.height <= px(0.0)
        || bounds.size.height <= px(1.0)
        || bounds.size.width <= px(1.0)
        || position.x < bounds.left()
        || position.x > bounds.right()
    {
        return false;
    }

    let Some(step) = browser_drag_edge_scroll_step(bounds.top(), bounds.bottom(), position.y)
    else {
        return false;
    };
    let offset = base_handle.offset();
    let next_y = (offset.y - px(step)).clamp(-max_offset.height, px(0.0));
    if next_y == offset.y {
        return false;
    }
    base_handle.set_offset(Point::new(offset.x, next_y));
    true
}

pub(super) fn sync_virtual_list_state_by_signatures(
    state: &mut ListState,
    cache: &mut VirtualListSignatureCache,
    identity: &str,
    signatures: &[u64],
    alignment: ListAlignment,
    overdraw: Pixels,
) {
    // React keeps virtual rows stable by key; this mirrors that behavior for GPUI
    // ListState without making every virtualized surface reimplement splice logic.
    let identity_changed = cache.identity.as_deref() != Some(identity);
    if identity_changed || state.item_count() != cache.signatures.len() {
        *state = ListState::new(signatures.len(), alignment, overdraw);
        cache.identity = Some(identity.to_string());
        cache.signatures = signatures.to_vec();
        return;
    }

    let old_len = cache.signatures.len();
    let new_len = signatures.len();
    let shared_len = old_len.min(new_len);
    for (index, signature) in signatures.iter().take(shared_len).enumerate() {
        if cache.signatures.get(index) != Some(signature) {
            state.splice(index..index + 1, 1);
        }
    }
    if old_len < new_len {
        state.splice(old_len..old_len, new_len - old_len);
    } else if old_len > new_len {
        state.splice(new_len..old_len, 0);
    }
    cache.signatures = signatures.to_vec();
}

fn browser_drag_edge_scroll_step(top: Pixels, bottom: Pixels, y: Pixels) -> Option<f32> {
    let edge = BROWSER_DRAG_AUTOSCROLL_EDGE_PX;
    let top = f32::from(top);
    let bottom = f32::from(bottom);
    let y = f32::from(y);
    let step = if y < top + edge {
        -((top + edge - y) / edge).clamp(0.0, 1.0) * BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX
    } else if y > bottom - edge {
        ((y - (bottom - edge)) / edge).clamp(0.0, 1.0) * BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX
    } else {
        0.0
    };
    (step.abs() >= 1.0).then_some(step)
}

#[allow(dead_code)]
pub(super) fn sync_tauri_virtual_list_state_by_signatures(
    state: &mut ListState,
    cache: &mut VirtualListSignatureCache,
    identity: &str,
    signatures: &[u64],
    alignment: ListAlignment,
    spec: TauriVirtualListSpec,
) {
    sync_virtual_list_state_by_signatures(
        state,
        cache,
        identity,
        signatures,
        alignment,
        spec.overdraw(),
    );
}

pub(super) fn sync_tauri_variable_list_state_by_signatures(
    state: &ListState,
    cache: &mut VirtualListSignatureCache,
    identity: &str,
    signatures: &[u64],
) {
    // GPUI's variable-height list keeps its state by item index. Mirroring
    // React keys requires explicit splice/reset calls when filtered rows move
    // or change; keep that bookkeeping centralized for sidebar/dialog lists.
    let identity_changed = cache.identity.as_deref() != Some(identity);
    if identity_changed || state.item_count() != cache.signatures.len() {
        state.reset(signatures.len());
        cache.identity = Some(identity.to_string());
        cache.signatures = signatures.to_vec();
        return;
    }

    let old_len = cache.signatures.len();
    let new_len = signatures.len();
    let shared_len = old_len.min(new_len);
    for (index, signature) in signatures.iter().take(shared_len).enumerate() {
        if cache.signatures.get(index) != Some(signature) {
            state.splice(index..index + 1, 1);
        }
    }
    if old_len < new_len {
        state.splice(old_len..old_len, new_len - old_len);
    } else if old_len > new_len {
        state.splice(new_len..old_len, 0);
    }
    cache.signatures = signatures.to_vec();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ListAlignment, px};

    #[test]
    fn signature_sync_initializes_list_state_for_identity() {
        let mut state = ListState::new(0, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache::default();

        sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "conversation-a",
            &[1, 2, 3],
            ListAlignment::Top,
            px(32.0),
        );

        assert_eq!(state.item_count(), 3);
        assert_eq!(cache.identity.as_deref(), Some("conversation-a"));
        assert_eq!(cache.signatures, vec![1, 2, 3]);
    }

    #[test]
    fn signature_sync_rebuilds_when_identity_changes() {
        let mut state = ListState::new(3, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache {
            identity: Some("conversation-a".to_string()),
            signatures: vec![1, 2, 3],
        };

        sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "conversation-b",
            &[9],
            ListAlignment::Top,
            px(32.0),
        );

        assert_eq!(state.item_count(), 1);
        assert_eq!(cache.identity.as_deref(), Some("conversation-b"));
        assert_eq!(cache.signatures, vec![9]);
    }

    #[test]
    fn tauri_virtual_spec_maps_overscan_to_gpui_overdraw() {
        let spec = TauriVirtualListSpec::new(px(38.0), 16);
        assert_eq!(spec.overdraw(), px(608.0));
    }

    #[test]
    fn browser_drag_edge_scroll_step_is_idle_away_from_edges() {
        assert_eq!(
            browser_drag_edge_scroll_step(px(100.0), px(300.0), px(150.0)),
            None
        );
        assert_eq!(
            browser_drag_edge_scroll_step(px(100.0), px(300.0), px(250.0)),
            None
        );
    }

    #[test]
    fn browser_drag_edge_scroll_step_tracks_edge_direction() {
        let upward_step = browser_drag_edge_scroll_step(px(100.0), px(300.0), px(120.0)).unwrap();
        let downward_step = browser_drag_edge_scroll_step(px(100.0), px(300.0), px(280.0)).unwrap();

        assert!(upward_step < 0.0);
        assert!(downward_step > 0.0);
        assert!(
            upward_step.abs() <= BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX,
            "edge autoscroll must clamp to the shared browser-like max step"
        );
        assert!(
            downward_step.abs() <= BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX,
            "edge autoscroll must clamp to the shared browser-like max step"
        );
    }

    #[test]
    fn browser_drag_edge_scroll_step_clamps_past_edges() {
        assert_eq!(
            browser_drag_edge_scroll_step(px(100.0), px(300.0), px(0.0)),
            Some(-BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX)
        );
        assert_eq!(
            browser_drag_edge_scroll_step(px(100.0), px(300.0), px(400.0)),
            Some(BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX)
        );
    }
}
