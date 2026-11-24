use super::*;
use crate::form_entity::ConnectionSelectAnchorStore;

/// The desktop flow retains ownership of its draft; a portable mount needs only form state.
#[derive(Clone)]
pub(in crate::views) enum FormOwner {
    #[cfg(not(target_family = "wasm"))]
    Desktop(Entity<ConnectionFlowEntity>),
    Portable(Entity<ConnectionFormState>, ConnectionSelectAnchorStore),
}
impl FormOwner {
    pub(in crate::views) fn state<'a>(&self, cx: &'a App) -> &'a ConnectionFormState {
        match self {
            #[cfg(not(target_family = "wasm"))]
            Self::Desktop(flow) => &flow.read(cx).form,
            Self::Portable(state, _) => state.read(cx),
        }
    }
    pub(in crate::views) fn update<R>(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut ConnectionFormState) -> R,
    ) -> R {
        self.update_with(cx, |state, _| change(state))
    }
    pub(in crate::views) fn update_with<R>(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut ConnectionFormState, &mut App) -> R,
    ) -> R {
        match self {
            #[cfg(not(target_family = "wasm"))]
            Self::Desktop(flow) => flow.update(cx, |flow, cx| {
                let r = change(&mut flow.form, cx);
                cx.notify();
                r
            }),
            Self::Portable(state, _) => state.update(cx, |state, cx| {
                let r = change(state, cx);
                cx.notify();
                r
            }),
        }
    }
    pub(in crate::views) fn anchors(&self, _cx: &App) -> ConnectionSelectAnchorStore {
        match self {
            #[cfg(not(target_family = "wasm"))]
            Self::Desktop(flow) => flow.read(_cx).select_anchor_store(),
            Self::Portable(_, anchors) => anchors.clone(),
        }
    }
}
