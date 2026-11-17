use std::ops::Range;

use gpui::{
    App, ElementId, IntoElement, ListAlignment, ListState, Pixels, Point, ScrollStrategy, Styled,
    UniformList, UniformListScrollHandle, Window, px, uniform_list,
};
pub(crate) use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualListSpec, VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
    sync_virtual_list_state_by_signatures, tauri_virtual_list, tauri_virtual_list_state,
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
        .track_scroll(&scroll_handle)
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
    // GPUI measures the actual row height while the caller-owned spec supplies the
    // browser-equivalent overscan used to keep fast scrolling visually continuous.
    let _estimated_row_height = spec.row_height;
    tracked_uniform_list(id, item_count, scroll_handle, render_items).with_overscan(spec.overscan)
}

pub(crate) fn scroll_tauri_virtual_list_to_index(
    handle: &UniformListScrollHandle,
    index: usize,
    spec: TauriVirtualListSpec,
    align: TauriVirtualScrollAlign,
) {
    match align {
        TauriVirtualScrollAlign::Nearest => {
            let base_handle = handle.0.borrow().base_handle.clone();
            let bounds = base_handle.bounds();
            let offset = base_handle.offset();
            if let Some(strategy) = nearest_virtual_scroll_strategy(
                -offset.y,
                bounds.size.height,
                spec.row_height,
                index,
            ) {
                // Browser `block: nearest` moves the minimum edge needed to reveal
                // the row. GPUI only exposes Top/Bottom strategies, so choose the
                // edge from current geometry instead of centering every reveal.
                handle.scroll_to_item_strict(index, strategy);
            }
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

fn nearest_virtual_scroll_strategy(
    scroll_top: Pixels,
    viewport_height: Pixels,
    row_height: Pixels,
    index: usize,
) -> Option<ScrollStrategy> {
    if row_height <= px(0.0) || viewport_height <= px(0.0) {
        return Some(ScrollStrategy::Center);
    }

    let item_top = row_height * index as f32;
    let item_bottom = item_top + row_height;
    let viewport_bottom = scroll_top + viewport_height;
    if item_top < scroll_top {
        Some(ScrollStrategy::Top)
    } else if item_bottom > viewport_bottom {
        Some(ScrollStrategy::Bottom)
    } else {
        None
    }
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
    if max_offset.y <= px(0.0)
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
    let next_y = (offset.y - px(step)).clamp(-max_offset.y, px(0.0));
    if next_y == offset.y {
        return false;
    }
    base_handle.set_offset(Point::new(offset.x, next_y));
    true
}

pub(super) fn browser_drag_edge_scroll_step(
    start: Pixels,
    end: Pixels,
    pointer: Pixels,
) -> Option<f32> {
    let edge = BROWSER_DRAG_AUTOSCROLL_EDGE_PX;
    let start = f32::from(start);
    let end = f32::from(end);
    let pointer = f32::from(pointer);
    let step = if pointer < start + edge {
        -((start + edge - pointer) / edge).clamp(0.0, 1.0) * BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX
    } else if pointer > end - edge {
        ((pointer - (end - edge)) / edge).clamp(0.0, 1.0) * BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX
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
) -> bool {
    sync_virtual_list_state_by_signatures(
        state,
        cache,
        identity,
        signatures,
        alignment,
        spec.overdraw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_virtual_scroll_strategy_matches_browser_edges() {
        assert_eq!(
            nearest_virtual_scroll_strategy(px(100.0), px(200.0), px(32.0), 2),
            Some(ScrollStrategy::Top)
        );
        assert_eq!(
            nearest_virtual_scroll_strategy(px(100.0), px(200.0), px(32.0), 9),
            Some(ScrollStrategy::Bottom)
        );
        assert_eq!(
            nearest_virtual_scroll_strategy(px(100.0), px(200.0), px(32.0), 4),
            None
        );
    }

    #[test]
    fn browser_drag_edge_scroll_step_handles_idle_direction_and_clamping() {
        let cases = [
            (150.0, None),
            (250.0, None),
            (0.0, Some(-BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX)),
            (400.0, Some(BROWSER_DRAG_AUTOSCROLL_MAX_STEP_PX)),
        ];
        for (pointer_y, expected) in cases {
            assert_eq!(
                browser_drag_edge_scroll_step(px(100.0), px(300.0), px(pointer_y)),
                expected
            );
        }

        let upward_step = browser_drag_edge_scroll_step(px(100.0), px(300.0), px(120.0)).unwrap();
        let downward_step = browser_drag_edge_scroll_step(px(100.0), px(300.0), px(280.0)).unwrap();
        assert!(upward_step < 0.0);
        assert!(downward_step > 0.0);
    }
}
