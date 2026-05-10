use std::{cell::Cell, collections::VecDeque, sync::Arc, time::Instant};

use alacritty_terminal::{
    event::{Event as AlacEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::Line,
    sync::FairMutex,
    term::{Config, Term},
    vte::ansi::Processor,
};
use anyhow::Result;
use crossbeam_channel::{Receiver, unbounded};
use oxideterm_ssh::{
    ConnectionConsumer, SshConfig, SshConnectionHandle, SshConnectionRegistry, SshPromptHandler,
    SshPtyHandle, SshTransportClient, SshTransportCommand,
};
use oxideterm_terminal_encoding::{
    EncodingMismatchDetector, TerminalEncoding, TerminalInputEncoder, TerminalOutputDecoder,
};
use oxideterm_terminal_graphics::{GraphicsIngress, GraphicsOptions};
use oxideterm_trzsz::{TrzszConsumer, TrzszConsumerEvent, TrzszTransfer, TrzszTransferPolicy};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::error::TryRecvError;

pub use crate::backpressure::{TerminalDrainBudget, TerminalDrainReport, TerminalMagicKind};

use crate::{
    LocalEventListener, LocalPtyConfig, LocalPtySession, TermMode, TerminalCommandMark,
    TerminalEvent, TerminalGraphicsState, TerminalLifecycle, TerminalProcessInfo,
    TerminalSearchMatch, TerminalSize, TerminalSnapshot, append_grid_line_text,
    backpressure::MagicScanWindow, focus_report_sequence, graphics_cursor_from_term,
    search_logical_line_matches, shell_integration::TerminalShellIntegration, snapshot_from_term,
};

const MAX_COMMAND_OUTPUT_LINES: usize = 400;
const MAX_COMMAND_OUTPUT_CHARS: usize = 24_000;

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

// Session backends are kept in this module scope so the TerminalSession
// facade, local PTY adapter, and SSH PTY owner keep their existing API and
// private access while avoiding another thousand-line implementation file.
include!("session/types.rs");
include!("session/facade.rs");
include!("session/playback.rs");
include!("session/local_backend.rs");
include!("session/ssh_config.rs");
include!("session/ssh_pty.rs");
