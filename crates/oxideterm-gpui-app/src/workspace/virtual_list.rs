use std::ops::Range;

use gpui::{
    App, ElementId, IntoElement, ListAlignment, ListState, Pixels, ScrollStrategy, Styled,
    UniformList, UniformListScrollHandle, Window, uniform_list,
};

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
}
