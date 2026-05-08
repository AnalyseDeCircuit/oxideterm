use std::{cell::Cell, collections::VecDeque, sync::Arc, time::Instant};

use alacritty_terminal::{
    event::Event as AlacEvent,
    grid::{Dimensions, Scroll},
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
    LocalEventListener, LocalPtyConfig, LocalPtySession, TermMode, TerminalEvent,
    TerminalGraphicsState, TerminalLifecycle, TerminalProcessInfo, TerminalSearchMatch,
    TerminalSize, TerminalSnapshot, append_grid_line_text, backpressure::MagicScanWindow,
    focus_report_sequence, graphics_cursor_from_term, search_logical_line_matches,
    snapshot_from_term,
};

// Session backends are kept in this module scope so the TerminalSession
// facade, local PTY adapter, and SSH PTY owner keep their existing API and
// private access while avoiding another thousand-line implementation file.
include!("session/types.rs");
include!("session/facade.rs");
include!("session/local_backend.rs");
include!("session/ssh_config.rs");
include!("session/ssh_pty.rs");
