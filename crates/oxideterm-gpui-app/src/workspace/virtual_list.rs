use gpui::{ListAlignment, ListState};
#[cfg(test)]
use gpui::{ScrollStrategy, px};
pub(crate) use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualListSpec, VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
    sync_virtual_list_state_by_signatures, tauri_virtual_list, tauri_virtual_list_state,
};

pub(crate) use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualScrollAlign, scroll_tauri_virtual_list_to_index, tauri_virtual_uniform_list,
};

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
    use oxideterm_gpui_ui::virtual_list::browser_drag_edge_scroll_step;

    #[test]
    fn nearest_virtual_scroll_strategy_matches_browser_edges() {
        assert_eq!(
            oxideterm_gpui_ui::virtual_list::nearest_virtual_scroll_strategy(
                px(100.0),
                px(200.0),
                px(32.0),
                2
            ),
            Some(ScrollStrategy::Top)
        );
        assert_eq!(
            oxideterm_gpui_ui::virtual_list::nearest_virtual_scroll_strategy(
                px(100.0),
                px(200.0),
                px(32.0),
                9
            ),
            Some(ScrollStrategy::Bottom)
        );
        assert_eq!(
            oxideterm_gpui_ui::virtual_list::nearest_virtual_scroll_strategy(
                px(100.0),
                px(200.0),
                px(32.0),
                4
            ),
            None
        );
    }

    #[test]
    fn browser_drag_edge_scroll_step_handles_idle_direction_and_clamping() {
        let cases = [
            (150.0, None),
            (250.0, None),
            (0.0, Some(-26.0)),
            (400.0, Some(26.0)),
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
