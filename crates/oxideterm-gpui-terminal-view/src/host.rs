use gpui::{App, Bounds, Context, Entity, FocusHandle, Pixels, Window};

/// The canvas owns grid geometry; the host decides what a resize or IME edit does.
pub trait TerminalCanvasHost: Sized + 'static {
    fn viewport_changed(&mut self, bounds: Bounds<Pixels>, scale: f32, cx: &mut Context<Self>);
    fn register_input(
        view: Entity<Self>,
        focus: &FocusHandle,
        content: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    );
}

pub struct TerminalElementInput<H> {
    pub view: Entity<H>,
    pub focus_handle: FocusHandle,
    pub last_viewport_bounds: Option<Bounds<Pixels>>,
    pub last_viewport_scale_factor_bits: Option<u32>,
}
impl<H> Clone for TerminalElementInput<H> {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            focus_handle: self.focus_handle.clone(),
            last_viewport_bounds: self.last_viewport_bounds,
            last_viewport_scale_factor_bits: self.last_viewport_scale_factor_bits,
        }
    }
}

impl<H: TerminalCanvasHost> TerminalElementInput<H> {
    pub(crate) fn sync_viewport(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let scale = window.scale_factor();
        if self.last_viewport_bounds == Some(bounds)
            && self.last_viewport_scale_factor_bits == Some(scale.to_bits())
        {
            return;
        }
        // A stable paint must not enqueue another entity update.
        let view = self.view.clone();
        window.on_next_frame(move |_, cx| {
            view.update(cx, |host, cx| host.viewport_changed(bounds, scale, cx))
        });
    }
    pub(crate) fn register_input(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        H::register_input(self.view.clone(), &self.focus_handle, bounds, window, cx);
    }
}
