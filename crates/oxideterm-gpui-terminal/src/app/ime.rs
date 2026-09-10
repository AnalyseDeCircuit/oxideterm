use std::ops::Range;

use gpui::{App, Bounds, Context, Entity, InputHandler, Pixels, UTF16Selection, Window, point, px};

use super::TerminalPane;
use crate::terminal_view::ime_cursor_bounds_for_snapshot;

impl TerminalPane {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range_for_ime(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // Alternate-screen programs still accept composed text. Returning None
        // prevents the platform from querying cursor bounds and leaves its
        // candidate window at the previous terminal's position.
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn unmark_text_for_ime(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_marked_text(cx);
    }

    fn replace_text_in_range_for_ime(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_text(text, cx);
    }

    fn replace_and_mark_text_in_range_for_ime(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_marked_text(new_text, cx);
    }

    fn bounds_for_range_for_ime(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let mut bounds = ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics)?;
        bounds.origin += element_bounds.origin
            + point(
                px(range_utf16.start as f32 * self.metrics.cell_width_f32()),
                px(0.0),
            );
        Some(bounds)
    }

    fn character_index_for_point_for_ime(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

pub(crate) struct TerminalInputHandler {
    pub(crate) view: Entity<TerminalPane>,
    pub(crate) content_bounds: Bounds<Pixels>,
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.view.update(cx, |view, cx| {
            view.selected_text_range_for_ime(ignore_disabled_input, window, cx)
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.view.update(cx, |view, _cx| view.marked_text_range())
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.view.update(cx, |view, cx| {
            view.text_for_range(range_utf16, adjusted_range, window, cx)
        })
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, cx| {
            view.replace_text_in_range_for_ime(replacement_range, text, window, cx);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range_for_ime(
                range_utf16,
                new_text,
                new_selected_range,
                window,
                cx,
            );
        });
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.unmark_text_for_ime(window, cx);
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.view.update(cx, |view, cx| {
            view.bounds_for_range_for_ime(range_utf16, self.content_bounds, window, cx)
        })
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.view.update(cx, |view, cx| {
            view.character_index_for_point_for_ime(point, window, cx)
        })
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminalUiPreferences;
    use gpui::{AppContext, PlatformInputHandler, TestAppContext, size};

    #[gpui::test]
    fn ime_candidate_position_follows_alternate_screen_after_switching_panes(
        cx: &mut TestAppContext,
    ) {
        let (shell, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                24,
                TerminalUiPreferences::default(),
                window,
                cx,
            )
            .expect("shell terminal")
        });
        let tui = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    80,
                    24,
                    TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("TUI terminal")
            })
        });
        for (pane, output, row, col, origin) in [
            (
                &shell,
                b"\x1b[3;6H".as_slice(),
                2,
                5,
                point(px(10.0), px(20.0)),
            ),
            (
                &tui,
                b"\x1b[?1049h\x1b[?25l\x1b[13;21H".as_slice(),
                12,
                20,
                point(px(310.0), px(40.0)),
            ),
            (
                &shell,
                b"\x1b[6;4H".as_slice(),
                5,
                3,
                point(px(10.0), px(20.0)),
            ),
            (
                &tui,
                b"\x1b[8;10H".as_slice(),
                7,
                9,
                point(px(310.0), px(40.0)),
            ),
            (
                &tui,
                b"\x1b[?1049l\x1b[4;8H".as_slice(),
                3,
                7,
                point(px(310.0), px(40.0)),
            ),
        ] {
            pane.update(cx, |pane, _cx| {
                let mut terminal = pane.terminal.lock();
                terminal.feed_recording_output(output);
                pane.snapshot = terminal.snapshot();
            });
            cx.update(|window, cx| {
                let focus_handle = pane.read(cx).focus_handle.clone();
                window.focus(&focus_handle, cx);
                let metrics = &pane.read(cx).metrics;
                let expected = origin
                    + point(
                        px(col as f32 * metrics.cell_width_f32()),
                        px(row as f32 * metrics.line_height_f32()),
                    );
                let mut handler = TerminalInputHandler {
                    view: pane.clone(),
                    content_bounds: Bounds::new(origin, size(px(640.0), px(480.0))),
                };
                for composing in [false, true] {
                    if composing {
                        handler.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
                    }
                    // Windows queries with false at composition start; GPUI's
                    // coordinate invalidation queries with true between frames.
                    for ignore_disabled_input in [false, true] {
                        let selection = handler
                            .selected_text_range(ignore_disabled_input, window, cx)
                            .expect("an active terminal must expose an IME insertion point");
                        let bounds = if ignore_disabled_input {
                            PlatformInputHandler::compute_ime_candidate_bounds(
                                handler.marked_text_range(window, cx),
                                &selection,
                                |range| handler.bounds_for_range(range, window, cx),
                            )
                        } else {
                            handler.bounds_for_range(selection.range, window, cx)
                        }
                        .expect("current terminal cursor bounds");
                        assert_eq!(
                            bounds.origin, expected,
                            "row={row}, col={col}, composing={composing}"
                        );
                    }
                }
                handler.unmark_text(window, cx);
            });
        }
    }
}
