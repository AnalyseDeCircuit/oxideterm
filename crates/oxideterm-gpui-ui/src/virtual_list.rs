use gpui::{App, IntoElement, List, ListAlignment, ListState, Pixels, Styled, Window, list};

#[derive(Clone, Copy, Debug)]
pub struct TauriVirtualListSpec {
    pub row_height: Pixels,
    pub overscan: usize,
}

impl TauriVirtualListSpec {
    pub fn new(row_height: Pixels, overscan: usize) -> Self {
        Self {
            row_height,
            overscan,
        }
    }

    pub fn overdraw(self) -> Pixels {
        self.row_height * self.overscan as f32
    }
}

pub fn tauri_virtual_list_state(
    item_count: usize,
    alignment: ListAlignment,
    spec: TauriVirtualListSpec,
) -> ListState {
    // TanStack useVirtualizer takes row estimate and overscan at construction.
    // GPUI stores the equivalent overdraw in ListState, so route every native
    // virtual-list state through the same spec instead of passing raw pixels.
    ListState::new(item_count, alignment, spec.overdraw())
}

#[derive(Default)]
pub struct VirtualListSignatureCache {
    identity: Option<String>,
    signatures: Vec<u64>,
}

pub fn tauri_virtual_list<R>(
    state: ListState,
    spec: TauriVirtualListSpec,
    render_item: impl 'static + FnMut(usize, &mut Window, &mut App) -> R,
) -> List
where
    R: IntoElement,
{
    // Variable-height browser lists still share the same virtual-list policy:
    // feature code owns identity/signature sync, and the shared helper owns the
    // GPUI list shell so scroll surfaces do not drift per page.
    let _overdraw = spec.overdraw();
    let mut render_item = render_item;
    list(state, move |index, window, cx| {
        render_item(index, window, cx).into_any_element()
    })
    .size_full()
}

pub fn sync_virtual_list_state_by_signatures(
    state: &mut ListState,
    cache: &mut VirtualListSignatureCache,
    identity: &str,
    signatures: &[u64],
    alignment: ListAlignment,
    overdraw: Pixels,
) -> bool {
    // React keeps virtual rows stable by key; this mirrors that behavior for GPUI
    // ListState without making every virtualized surface reimplement splice logic.
    // The return value lets a feature restore behavior such as tail following
    // after the underlying ListState has been replaced.
    let identity_changed = cache.identity.as_deref() != Some(identity);
    if identity_changed || state.item_count() != cache.signatures.len() {
        *state = ListState::new(signatures.len(), alignment, overdraw);
        cache.identity = Some(identity.to_string());
        cache.signatures = signatures.to_vec();
        return true;
    }

    let old_len = cache.signatures.len();
    let new_len = signatures.len();
    let shared_len = old_len.min(new_len);
    for (index, signature) in signatures.iter().take(shared_len).enumerate() {
        if cache.signatures.get(index) != Some(signature) {
            // A streamed or newly loaded body is still the same row. Splicing it
            // resets the reader's offset inside that row to zero.
            state.remeasure_items(index..index + 1);
        }
    }
    if old_len < new_len {
        state.splice(old_len..old_len, new_len - old_len);
    } else if old_len > new_len {
        state.splice(new_len..old_len, 0);
    }
    cache.signatures = signatures.to_vec();
    false
}

/// Folds a height-changing row state into its signature.
///
/// Expansion belongs to one row, not to the data set: keeping it in the list
/// identity resets the list and throws away the offset the user scrolled to, so
/// callers encode it here and let the signature sync remeasure that row only.
pub fn expandable_row_signature(signature: u64, expanded: bool) -> u64 {
    signature ^ u64::from(expanded)
}

/// Returns whether the list was reset, which discards the user's scroll offset.
pub fn sync_tauri_variable_list_state_by_signatures(
    state: &ListState,
    cache: &mut VirtualListSignatureCache,
    identity: &str,
    signatures: &[u64],
    spec: TauriVirtualListSpec,
) -> bool {
    // GPUI's variable-height list keeps its state by item index. Mirroring
    // React keys requires explicit splice/reset calls when filtered rows move
    // or change; keep that bookkeeping centralized for sidebar/dialog lists.
    // The overdraw lives in ListState::new, but accepting the same spec here
    // keeps row-height/overscan visible at every variable-list sync call site.
    let _overdraw = spec.overdraw();
    let identity_changed = cache.identity.as_deref() != Some(identity);
    if identity_changed || state.item_count() != cache.signatures.len() {
        state.reset(signatures.len());
        cache.identity = Some(identity.to_string());
        cache.signatures = signatures.to_vec();
        return true;
    }

    let old_len = cache.signatures.len();
    let new_len = signatures.len();
    let shared_len = old_len.min(new_len);
    for (index, signature) in signatures.iter().take(shared_len).enumerate() {
        if cache.signatures.get(index) != Some(signature) {
            // A changed card still owns the user's offset within that same row.
            state.remeasure_items(index..index + 1);
        }
    }
    if old_len < new_len {
        state.splice(old_len..old_len, new_len - old_len);
    } else if old_len > new_len {
        state.splice(new_len..old_len, 0);
    }
    cache.signatures = signatures.to_vec();
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn expanding_one_row_keeps_the_scroll_offset() {
        let state = ListState::new(0, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache::default();
        let spec = TauriVirtualListSpec::new(px(32.0), 8);
        let rows = [11u64, 22, 33];
        let collapsed = rows
            .iter()
            .map(|signature| expandable_row_signature(*signature, false))
            .collect::<Vec<_>>();
        let expanded = rows
            .iter()
            .enumerate()
            .map(|(index, signature)| expandable_row_signature(*signature, index == 1))
            .collect::<Vec<_>>();

        assert!(sync_tauri_variable_list_state_by_signatures(
            &state,
            &mut cache,
            "host-packages:conn-1:",
            &collapsed,
            spec,
        ));
        // Expansion must remeasure its row instead of resetting the list, which
        // would throw away the offset the user scrolled to.
        assert!(!sync_tauri_variable_list_state_by_signatures(
            &state,
            &mut cache,
            "host-packages:conn-1:",
            &expanded,
            spec,
        ));
    }

    #[test]
    fn signature_sync_resets_for_new_identity_and_preserves_same_identity_follow_mode() {
        let mut state = ListState::new(0, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache::default();

        let list_was_reset = sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "conversation-a",
            &[1, 2, 3],
            ListAlignment::Top,
            px(32.0),
        );

        assert!(list_was_reset);
        assert_eq!(state.item_count(), 3);
        assert_eq!(cache.identity.as_deref(), Some("conversation-a"));
        assert_eq!(cache.signatures, vec![1, 2, 3]);

        let list_was_reset = sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "conversation-b",
            &[9],
            ListAlignment::Top,
            px(32.0),
        );

        assert!(list_was_reset);
        assert_eq!(state.item_count(), 1);
        assert_eq!(cache.identity.as_deref(), Some("conversation-b"));
        assert_eq!(cache.signatures, vec![9]);
        state.set_follow_mode(gpui::FollowMode::Tail);

        let list_was_reset = sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "conversation-b",
            &[2],
            ListAlignment::Top,
            px(32.0),
        );

        assert!(!list_was_reset);
        assert!(
            state.is_following_tail(),
            "stream updates must preserve the user's active tail-follow mode"
        );
    }

    #[test]
    fn message_updates_preserve_scroll_offset() {
        let mut state = ListState::new(0, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache::default();
        sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "chat",
            &[1, 2, 3],
            ListAlignment::Top,
            px(32.0),
        );
        state.scroll_to(gpui::ListOffset {
            item_ix: 1,
            offset_in_item: px(240.0),
        });
        sync_virtual_list_state_by_signatures(
            &mut state,
            &mut cache,
            "chat",
            &[1, 9, 3],
            ListAlignment::Top,
            px(32.0),
        );
        let top = state.logical_scroll_top();
        assert_eq!((top.item_ix, top.offset_in_item), (1, px(240.0)));
    }

    #[test]
    fn variable_settings_card_updates_preserve_scroll_offset() {
        let state = ListState::new(0, ListAlignment::Top, px(0.0));
        let mut cache = VirtualListSignatureCache::default();
        let spec = TauriVirtualListSpec::new(px(400.0), 2);
        sync_tauri_variable_list_state_by_signatures(
            &state,
            &mut cache,
            "appearance",
            &[1, 2, 3],
            spec,
        );
        state.scroll_to(gpui::ListOffset {
            item_ix: 1,
            offset_in_item: px(720.0),
        });
        sync_tauri_variable_list_state_by_signatures(
            &state,
            &mut cache,
            "appearance",
            &[1, 9, 3],
            spec,
        );
        let top = state.logical_scroll_top();
        assert_eq!((top.item_ix, top.offset_in_item), (1, px(720.0)));
    }
}
