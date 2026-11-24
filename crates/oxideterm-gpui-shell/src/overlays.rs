//! A window's portal paint order and its blocking input order share the same priorities.
pub mod transients;
use gpui::{AnyElement, App, FocusHandle, Window};
pub use oxideterm_gpui_ui::modal::{staged_overlay_root, staged_overlay_scrim};
use oxideterm_gpui_ui::motion::ExitPhase;

#[derive(Default)]
pub struct OverlayStack {
    priority: u16,
    layers: Vec<(u16, AnyElement)>,
}
impl OverlayStack {
    pub fn at(&mut self, priority: u16) {
        self.priority = priority;
    }
    pub fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.layers
            .extend(elements.into_iter().map(|el| (self.priority, el)));
    }
    pub fn ordered(mut self) -> impl Iterator<Item = AnyElement> {
        self.layers.sort_by_key(|layer| layer.0);
        self.layers.into_iter().map(|(_, el)| el)
    }
}
/// Nonblocking portals are painted but are never candidates for keyboard ownership.
#[derive(Clone, Copy)]
pub struct ModalLayer<K> {
    pub owner: K,
    pub priority: u16,
    pub phase: ExitPhase,
    pub editor: bool,
}
pub fn top_blocking<K>(layers: impl IntoIterator<Item = ModalLayer<K>>) -> Option<ModalLayer<K>> {
    layers.into_iter().max_by_key(|layer| layer.priority)
}
#[derive(Clone, Copy)]
pub struct ModalKeyRoute<K> {
    pub dispatch_owner: Option<K>,
    consume: bool,
}
impl<K> ModalKeyRoute<K> {
    pub fn consumes_key(&self) -> bool {
        self.consume
    }
}
impl<K> ModalLayer<K> {
    pub fn key_route(self, key: &str) -> ModalKeyRoute<K> {
        let visible = self.phase == ExitPhase::Visible;
        let yield_editor = visible && self.editor && key != "escape";
        ModalKeyRoute {
            dispatch_owner: (visible && !yield_editor).then_some(self.owner),
            consume: !yield_editor,
        }
    }
}
/// Remember the invoker until the last exiting frame. Never displace focus deliberately
/// transferred to another view (for example, by executing a palette command).
#[derive(Default)]
pub struct PortalFocus {
    return_to: Option<FocusHandle>,
}
impl PortalFocus {
    pub fn enter(&mut self, focus: &FocusHandle, window: &mut Window, cx: &mut App) {
        if self.return_to.is_none() {
            self.return_to = window.focused(cx);
        }
        window.focus(focus, cx);
    }
    pub fn restore(&mut self, focus: &FocusHandle, window: &mut Window, cx: &mut App) {
        if let Some(previous) = self.return_to.take()
            && focus.is_focused(window)
        {
            window.focus(&previous, cx);
        }
    }
    pub fn forget(&mut self) {
        self.return_to = None;
    }
}
