use crate::{ShellContent, host::FrameSource};
use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, MouseButton, Window, div,
    prelude::*, px,
};

pub struct WorkspaceShell {
    focus: FocusHandle,
    frame: Box<FrameSource>,
}

impl WorkspaceShell {
    pub fn new(
        focus: FocusHandle,
        frame: impl Fn(&mut Window, &mut App) -> ShellContent + 'static,
    ) -> Self {
        Self {
            focus,
            frame: Box::new(frame),
        }
    }
}

impl Focusable for WorkspaceShell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slots = match (self.frame)(window, cx) {
            ShellContent::Exclusive(screen) => return screen,
            ShellContent::Frame(slots) => slots,
        };
        let env = slots.environment;
        let chrome = |slot: Option<AnyElement>| {
            slot.filter(|_| !env.zen_mode)
                .map(|child| lock_chrome(child, env.chrome_locked))
        };
        let center = div()
            .flex_1()
            .flex()
            .flex_col()
            .min_w(px(env.tokens.metrics.min_main_width))
            .overflow_hidden()
            .children(slots.tab_bar)
            .child(
                div()
                    .flex_1()
                    .relative()
                    .overflow_hidden()
                    .child(div().absolute().inset_0().child(slots.center)),
            );
        slots
            .root
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .children(slots.background)
            .children(slots.title_row.filter(|_| !env.zen_mode))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .children(chrome(slots.left_rail))
                    .children(chrome(slots.left_panel.drawer))
                    .child(center)
                    .children(chrome(slots.right_panel.drawer))
                    .children(chrome(slots.right_rail)),
            )
            .children(slots.left_panel.handle.filter(|_| !env.zen_mode))
            .children(slots.right_panel.handle.filter(|_| !env.zen_mode))
            .children(slots.overlays.ordered())
            .into_any_element()
    }
}
fn lock_chrome(element: AnyElement, locked: bool) -> AnyElement {
    if !locked {
        return element;
    }
    div()
        .relative()
        .h_full()
        .flex_none()
        .child(div().h_full().opacity(0.42).child(element))
        .child(
            div()
                .absolute()
                .inset_0()
                .cursor(CursorStyle::OperationNotAllowed)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                .on_scroll(|_, _, cx| cx.stop_propagation()),
        )
        .into_any_element()
}
