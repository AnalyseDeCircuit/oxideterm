use crate::input::TerminalBlinkMode;
use gpui::Context;
use oxideterm_settings::{
    CursorStyle, MAX_TERMINAL_PADDING, TERMINAL_SCROLLBACK_MAX, TERMINAL_SCROLLBACK_MIN,
    TerminalSettings,
};
use oxideterm_terminal_emulator::TerminalCursorShape;

use super::TerminalView;

#[derive(Clone, Copy)]
pub(super) struct ViewOptions {
    pub cursor: Option<TerminalCursorShape>,
    pub blink: TerminalBlinkMode,
    pub inset_x: f32,
    pub inset_y: f32,
    pub bidi: bool,
    pub smart_copy: bool,
    pub protected_paste: bool,
    pub middle_paste: bool,
    pub right_paste: bool,
    pub respect_mouse_tracking: bool,
    pub osc52_copy: bool,
    pub copy_on_select: bool,
    pub selection_requires_shift: bool,
    pub link_modifier: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            cursor: None,
            blink: TerminalBlinkMode::TerminalControlled,
            inset_x: 0.,
            inset_y: 0.,
            bidi: true,
            smart_copy: true,
            protected_paste: false,
            middle_paste: false,
            right_paste: false,
            respect_mouse_tracking: true,
            osc52_copy: true,
            copy_on_select: false,
            selection_requires_shift: false,
            link_modifier: true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct TerminalViewPreferences {
    options: ViewOptions,
    backspace: oxideterm_settings::TerminalBackspaceSequence,
    delete: oxideterm_settings::TerminalDeleteSequence,
    history: usize,
}

impl From<&TerminalSettings> for TerminalViewPreferences {
    fn from(p: &TerminalSettings) -> Self {
        let options = ViewOptions {
            cursor: Some(match p.cursor_style {
                CursorStyle::Block => TerminalCursorShape::Block,
                CursorStyle::Bar => TerminalCursorShape::Bar,
                CursorStyle::Underline => TerminalCursorShape::Underline,
            }),
            blink: if p.cursor_blink {
                TerminalBlinkMode::On
            } else {
                TerminalBlinkMode::Off
            },
            inset_x: p.padding_horizontal.clamp(0, MAX_TERMINAL_PADDING) as f32,
            inset_y: p.padding_vertical.clamp(0, MAX_TERMINAL_PADDING) as f32,
            bidi: p.unicode.bidi_enabled,
            smart_copy: p.smart_copy,
            protected_paste: p.paste_protection,
            middle_paste: p.middle_click_paste,
            right_paste: p.right_click_paste,
            respect_mouse_tracking: p.right_click_paste_respects_mouse_tracking,
            osc52_copy: p.osc52_clipboard,
            copy_on_select: p.copy_on_select,
            selection_requires_shift: p.selection_requires_shift,
            link_modifier: p.open_links_with_modifier,
        };
        Self {
            options,
            backspace: p.backspace_sequence,
            delete: p.delete_sequence,
            history: p
                .scrollback
                .clamp(TERMINAL_SCROLLBACK_MIN, TERMINAL_SCROLLBACK_MAX)
                as usize,
        }
    }
}

impl TerminalView {
    pub fn apply_preferences(&mut self, p: TerminalViewPreferences, cx: &mut Context<Self>) {
        self.options = p.options;
        self.set_backspace_sequence(p.backspace);
        self.set_delete_sequence(p.delete);
        if self.emulator.get_mut().set_scrollback_limit(p.history) {
            self.publish_snapshot(true, cx);
            self.refresh_search(cx);
        } else {
            self.update_blink(cx);
            cx.notify();
        }
    }
}
