// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Byte-oriented support for tmux control mode.
//!
//! The protocol layer owns no process, terminal, or network transport. This
//! keeps a `tmux -CC` takeover on the PTY or SSH channel that already owns the
//! shell, while allowing local and remote sessions to share the same parser.

mod ids;
mod layout;
mod notification;
mod output;
mod protocol;
mod stream;

pub use ids::{PaneId, SessionId, WindowId};
pub use layout::{Layout, LayoutCell, LayoutError, LayoutKind, SplitDirection};
pub use notification::Notification;
pub use output::decode_output;
pub use protocol::{CommandGuard, ControlEvent, ControlParser};
pub use stream::{
    CONTROL_MODE_ENTER, CONTROL_MODE_EXIT, ControlStream, ControlStreamError, ControlStreamMode,
    StreamEvent,
};
