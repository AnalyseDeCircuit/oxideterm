use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    env,
    path::PathBuf,
    sync::Arc,
    thread::JoinHandle,
};

use alacritty_terminal::{
    event::{Event as AlacEvent, EventListener, Notify, OnResize, WindowSize},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point},
    sync::FairMutex,
    term::{
        Config, Term,
        cell::{Cell, Flags},
    },
    tty::{self, Shell},
};
use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, unbounded};
use oxideterm_terminal_encoding::{EncodingHint, TerminalInputEncoder};
use oxideterm_terminal_graphics::{
    DEFAULT_STORAGE_LIMIT_MB, GraphicsCursor, TerminalGraphicsEvent, TerminalImagePlacement,
};

mod backpressure;
mod color;
mod data;
mod local_graphics_event_loop;
mod local_shell;
mod process;
mod process_lifecycle;
mod search;
mod session;
mod shell_integration;

pub use alacritty_terminal::term::TermMode;
pub use data::{
    GraphicsOptions, TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape,
    TerminalImageAnimationState, TerminalImageData, TerminalImageFrame, TerminalImageId,
    TerminalImageProtocol, TerminalImageSnapshot, TerminalRow, TerminalSearchMatch,
    TerminalSearchRange, TerminalSnapshot,
};
pub use local_shell::{LocalPtyConfig, ShellInfo, default_shell, scan_shells};
pub use oxideterm_terminal_encoding::{
    EncodingMismatchDetector, TERMINAL_ENCODINGS, TerminalEncoding,
    TerminalInputEncoder as RawTerminalInputEncoder, TerminalOutputDecoder,
};
pub use oxideterm_trzsz::{TrzszTransferDirection, TrzszTransferPolicy, TrzszTransferSelection};
pub use process::{TerminalLifecycle, TerminalProcessInfo};
pub use session::{
    SerialError, SerialErrorCode, SerialFlowControl, SerialParity, SerialPortInfo,
    SerialSessionConfig, SshPtySession, SshSessionConfig, TelnetSessionConfig, TerminalDrainBudget,
    TerminalDrainReport, TerminalMagicKind, TerminalOutputProcessor, TerminalResize,
    TerminalSession, TerminalSessionBackend, TerminalSessionKind, TerminalSessionStatus,
    serial_list_ports,
};
pub use shell_integration::{
    ShellIntegrationEvent, ShellIntegrationEventKind, ShellIntegrationLifecycleState,
    ShellIntegrationSource, ShellIntegrationStatus, TerminalCommandMark,
    TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence, TerminalCommandMarkDetectionSource,
    TerminalCommandMarkEvent,
};

use color::{
    OXIDETERM_DARK_THEME, attrs_from_flags, color_for_alacritty_request_with_override,
    style_colors_for_cell,
};
use local_graphics_event_loop::{
    LocalGraphicsEventLoop, LocalGraphicsMsg, LocalGraphicsNotifier, LocalPtyReadReport,
};
use local_shell::shell_args_for_profile;
use process::{ProcessState, TerminalSignal, signal_process_group};
#[cfg(windows)]
use process_lifecycle::WindowsTerminalJob;
#[cfg(not(windows))]
use process_lifecycle::cleanup_local_pty_process_tree;
use search::{append_grid_line_text, search_logical_line_matches, viewport_row_for_grid_line};

// Local PTY pieces stay included in this module so crate-private terminal
// state and the public `oxideterm_terminal` API remain unchanged while the
// previous monolithic lib.rs is split by responsibility.
include!("local/events.rs");
include!("local/graphics_state.rs");
include!("local/env.rs");
include!("local/pty.rs");
include!("local/controls.rs");
#[cfg(test)]
include!("local/tests.rs");
