use std::{
    cell::Cell,
    collections::VecDeque,
    io::{Read, Write},
    sync::Arc,
    time::Instant,
};

use alacritty_terminal::{
    event::{Event as AlacEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::Line,
    sync::FairMutex,
    term::{Config, Term},
    vte::ansi::{self, Handler, Processor},
};
use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, unbounded};
use oxideterm_modem_transfer::{ModemConsumer, ModemConsumerEvent, ModemTransfer};
use oxideterm_ssh::{
    ConnectionConsumer, ManagedKeyResolver, SshConfig, SshConnectionHandle, SshConnectionRegistry,
    SshPromptHandler, SshPtyHandle, SshTransportClient, SshTransportCommand,
};
use oxideterm_terminal_encoding::{
    EncodingMismatchDetector, TerminalEncoding, TerminalInputEncoder, TerminalOutputDecoder,
};
use oxideterm_terminal_graphics::{GraphicsIngress, GraphicsOptions, TerminalGraphicsSegment};
use oxideterm_trzsz::{TrzszConsumer, TrzszConsumerEvent, TrzszTransfer, TrzszTransferPolicy};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    runtime::Runtime,
};

pub use crate::backpressure::{TerminalDrainBudget, TerminalDrainReport, TerminalMagicKind};

use crate::{
    LocalEventListener, LocalPtyConfig, LocalPtySession, TermMode, TerminalCommandMark,
    TerminalEvent, TerminalGraphicsState, TerminalLifecycle, TerminalModemTransferRequest,
    TerminalProcessInfo, TerminalSearchMatch, TerminalSize, TerminalSnapshot,
    append_grid_line_text, backpressure::MagicScanWindow, focus_report_sequence,
    graphics_cursor_from_term, search_logical_line_matches,
    shell_integration::TerminalShellIntegration, snapshot_from_term,
    snapshot_from_term_with_display_offset,
};

const MAX_COMMAND_OUTPUT_LINES: usize = 400;
const MAX_COMMAND_OUTPUT_CHARS: usize = 24_000;
const MAX_AI_TERMINAL_BUFFER_LINES: usize = 500;

// AI terminal tools mirror the Tauri registry getter: expose the most recent
// physical buffer rows, including scrollback, rather than only the viewport.
fn terminal_buffer_text_from_term<T: EventListener>(term: &Term<T>, max_cols: usize) -> String {
    let grid = term.grid();
    let top_line = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
    let bottom_line = term.screen_lines() as i32;
    let line_count = (bottom_line - top_line).max(0) as usize;
    let kept_lines = line_count.min(MAX_AI_TERMINAL_BUFFER_LINES);
    let start_line = bottom_line - kept_lines as i32;
    let mut lines = Vec::with_capacity(kept_lines);

    for line in start_line..bottom_line {
        let row = &grid[Line(line)];
        let mut text = String::new();
        let mut cell_map = Vec::new();
        append_grid_line_text(row[..].iter(), line, max_cols, &mut text, &mut cell_map);
        lines.push(text.trim_end().to_string());
    }

    lines.join("\n")
}

fn command_output_text_from_term<T: EventListener>(
    term: &Term<T>,
    mark: &TerminalCommandMark,
) -> String {
    let start = mark.command_line.saturating_add(1);
    let end = mark.end_line.unwrap_or_else(|| {
        let scrollback = term.total_lines().saturating_sub(term.screen_lines());
        let cursor_line = term.renderable_content().cursor.point.line.0.max(0) as usize;
        scrollback.saturating_add(cursor_line)
    });
    if start > end {
        return String::new();
    }

    let mut text = String::new();
    for absolute_line in start..=end {
        if absolute_line - start >= MAX_COMMAND_OUTPUT_LINES
            || text.len() >= MAX_COMMAND_OUTPUT_CHARS
        {
            break;
        }
        if absolute_line > start {
            text.push('\n');
        }
        let remaining = MAX_COMMAND_OUTPUT_CHARS.saturating_sub(text.len());
        if remaining == 0 {
            break;
        }
        let line = crate::shell_integration::line_text(term, absolute_line);
        if line.len() > remaining {
            let mut end = 0;
            for (index, ch) in line.char_indices() {
                let next = index + ch.len_utf8();
                if next > remaining {
                    break;
                }
                end = next;
            }
            text.push_str(&line[..end]);
            break;
        }
        text.push_str(&line);
    }
    text
}

pub(crate) fn clear_terminal_buffer<T: EventListener>(term: &mut Term<T>) {
    // Tauri clear_buffer clears the host-owned scroll buffer rather than
    // sending input to the shell. Native keeps the same boundary by mutating
    // the emulator state directly: first blank the viewport, then discard saved
    // scrollback so plugins cannot recover stale pre-clear output.
    Handler::clear_screen(term, ansi::ClearMode::All);
    Handler::clear_screen(term, ansi::ClearMode::Saved);
}

fn apply_terminal_output_processor<'a>(
    processor: &Option<TerminalOutputProcessor>,
    bytes: &'a [u8],
) -> std::borrow::Cow<'a, [u8]> {
    let Some(processor) = processor else {
        return std::borrow::Cow::Borrowed(bytes);
    };
    std::borrow::Cow::Owned(processor(bytes))
}

// Session backends are kept in this module scope so the TerminalSession
// facade, local PTY adapter, and SSH PTY owner keep their existing API and
// private access while avoiding another thousand-line implementation file.
include!("session/types.rs");
include!("session/facade.rs");
include!("session/playback.rs");
include!("session/local_backend.rs");
include!("session/ssh_config.rs");
include!("session/ssh_pty.rs");
include!("session/telnet.rs");
include!("session/serial.rs");
include!("session/raw_tcp.rs");
include!("session/raw_udp.rs");

#[cfg(test)]
mod tests {
    use alacritty_terminal::{
        event::VoidListener,
        vte::ansi::{Processor, StdSyncHandler},
    };

    use super::*;

    #[test]
    fn ai_terminal_buffer_text_includes_scrollback_rows() {
        let size = TerminalSize {
            cols: 12,
            rows: 3,
            cell_width: 8,
            cell_height: 17,
        };
        let mut config = Config::default();
        config.scrolling_history = 16;
        let mut term = Term::new(config, &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();

        // More rows than the viewport proves AI tools receive buffer context,
        // not just the currently visible screen.
        for index in 0..6 {
            let line = format!("line-{index}\r\n");
            parser.advance(&mut term, line.as_bytes());
        }

        let buffer = terminal_buffer_text_from_term(&term, size.cols);

        assert!(buffer.lines().count() > size.rows);
        assert!(buffer.contains("line-0"));
    }

    #[test]
    fn ai_terminal_buffer_text_keeps_tauri_line_limit() {
        let size = TerminalSize {
            cols: 16,
            rows: 5,
            cell_width: 8,
            cell_height: 17,
        };
        let mut config = Config::default();
        config.scrolling_history = 600;
        let mut term = Term::new(config, &size, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();

        // Tauri's registry getter caps AI terminal context to the last 500
        // physical buffer rows to avoid copying unbounded scrollback.
        for index in 0..520 {
            let line = format!("row-{index:03}\r\n");
            parser.advance(&mut term, line.as_bytes());
        }

        let buffer = terminal_buffer_text_from_term(&term, size.cols);

        assert_eq!(buffer.split('\n').count(), MAX_AI_TERMINAL_BUFFER_LINES);
        assert!(!buffer.contains("row-000"));
        assert!(buffer.contains("row-519"));
    }

    #[test]
    fn ssh_output_events_are_emitted_only_when_enabled() {
        let mut session = SshPtySession::new(
            SshSessionConfig::new("127.0.0.1", 9, "nobody"),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            1000,
        );

        // TerminalEvent::Output duplicates decoded display bytes for recording,
        // so SSH keeps it disabled on the normal render path.
        session.feed_utf8_terminal_output(b"not recorded");
        assert!(
            session
                .take_events()
                .into_iter()
                .all(|event| !matches!(event, TerminalEvent::Output(_)))
        );

        TerminalSessionBackend::set_output_events_enabled(&mut session, true);
        session.feed_utf8_terminal_output(b"recorded");

        assert!(
            session
                .take_events()
                .into_iter()
                .any(|event| matches!(event, TerminalEvent::Output(bytes) if bytes == b"recorded"))
        );
    }
}
