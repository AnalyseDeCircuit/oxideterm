use crate::app::{TerminalInputHandler, TerminalPane};
use gpui::{App, Bounds, Context, Entity, Pixels, Window};
pub(crate) use oxideterm_gpui_terminal_view::{CommandMarkAnnotation, element::*};
pub(crate) type TerminalElement = oxideterm_gpui_terminal_view::TerminalElement<TerminalPane>;
pub(crate) type TerminalElementInput =
    oxideterm_gpui_terminal_view::TerminalElementInput<TerminalPane>;
mod command_marks;
pub(crate) use command_marks::*;
mod input {
    pub(crate) use oxideterm_gpui_terminal_view::input::*;

    #[cfg(test)]
    pub(crate) use super::default_key_sequence::oxideterm_key_escape_sequence;
}

#[cfg(test)]
use oxideterm_terminal::TermMode;
#[cfg(test)]
#[path = "../../oxideterm-gpui-terminal-view/tests/support/default_key_sequence.rs"]
mod default_key_sequence;
mod links;
use oxideterm_gpui_terminal_view::selection;

pub(crate) use input::*;
pub(crate) use links::*;
pub(crate) use selection::*;

#[cfg(test)]
mod tests;

impl oxideterm_gpui_terminal_view::TerminalCanvasHost for TerminalPane {
    fn viewport_changed(&mut self, bounds: Bounds<Pixels>, scale: f32, cx: &mut Context<Self>) {
        self.apply_viewport_bounds(bounds, scale, cx);
    }

    fn register_input(
        view: Entity<Self>,
        focus: &gpui::FocusHandle,
        content_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            focus,
            TerminalInputHandler {
                view,
                content_bounds,
            },
            cx,
        );
    }
}
