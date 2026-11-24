mod element;
mod highlight;
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
mod semantic;

pub(crate) use element::*;
pub(crate) use input::*;
pub(crate) use links::*;
pub(crate) use selection::*;

#[cfg(test)]
mod tests;
