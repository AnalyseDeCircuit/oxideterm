use std::borrow::Cow;

use oxideterm_settings::{TerminalBackspaceSequence, TerminalDeleteSequence};

use super::{KittyKeyEventType, TermMode, configurable_key_escape_sequence};

pub(crate) fn oxideterm_key_escape_sequence(
    keystroke: &gpui::Keystroke,
    mode: &TermMode,
    option_as_meta: bool,
    event_type: KittyKeyEventType,
) -> Option<Cow<'static, str>> {
    configurable_key_escape_sequence(
        keystroke,
        mode,
        option_as_meta,
        TerminalBackspaceSequence::default(),
        TerminalDeleteSequence::default(),
        event_type,
    )
}
