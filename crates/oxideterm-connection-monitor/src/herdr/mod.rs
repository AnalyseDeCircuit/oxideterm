// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Command builders and envelope parsers for the herdr CLI.
//!
//! The protocol layer owns no process, terminal, or network transport: the
//! caller runs these commands on the SSH channel that already owns the
//! connection. Herdr speaks newline-delimited JSON envelopes natively, so
//! parsing keys on the envelope shape rather than screen-scraped tables.
//! Exit codes carry no signal: error envelopes return exit code zero
//! (measured against herdr 0.9.4), so action success is read from the
//! envelope by [`herdr_action_outcome`].
//!
//! `crate::multiplexer` is the only consumer: the panel renders the
//! provider-neutral shapes it derives from these, never a herdr wire format,
//! so nothing here is part of the crate's public API.

mod command;
mod ids;
mod model;
mod parse;
// `crate::multiplexer` is the only consumer, so this list is exactly what the
// provider-neutral model reads; the shapes the protocol builds for itself stay
// inside the module.
pub use command::{
    HerdrActionKind, HerdrAttachTarget, build_herdr_action_command, build_herdr_attach_command,
    build_herdr_read_command, build_herdr_session_detail_command, build_herdr_snapshot_command,
};
pub use model::{HerdrSessionDetail, HerdrStatus};
pub use parse::{herdr_action_outcome, parse_herdr_session_detail, parse_herdr_snapshot};

/// Scrollback lines a pane read asks for; the panel shows a bounded window.
pub const HERDR_READ_LINES: usize = 500;
