// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

use crate::{PaneId, SessionId, WindowId};

/// A notification emitted by tmux outside a command response block.
///
/// Text fields remain raw bytes because tmux pane output is not guaranteed to
/// be UTF-8. `Other` retains notifications added by future tmux releases.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Notification<'a> {
    Output {
        pane: PaneId,
        bytes: Cow<'a, [u8]>,
    },
    ExtendedOutput {
        pane: PaneId,
        milliseconds_behind: u64,
        bytes: Cow<'a, [u8]>,
    },
    Pause(PaneId),
    Continue(PaneId),
    PaneModeChanged(PaneId),
    LayoutChanged {
        window: WindowId,
        layout: &'a [u8],
        visible_layout: Option<&'a [u8]>,
        flags: Option<&'a [u8]>,
    },
    WindowAdded(WindowId),
    WindowClosed(WindowId),
    WindowRenamed {
        window: WindowId,
        name: &'a [u8],
    },
    WindowPaneChanged {
        window: WindowId,
        pane: PaneId,
    },
    UnlinkedWindowAdded(WindowId),
    UnlinkedWindowClosed(WindowId),
    UnlinkedWindowRenamed {
        window: WindowId,
        name: &'a [u8],
    },
    SessionChanged {
        session: SessionId,
        name: &'a [u8],
    },
    SessionRenamed {
        session: SessionId,
        name: &'a [u8],
    },
    SessionWindowChanged {
        session: SessionId,
        window: WindowId,
    },
    SessionsChanged,
    ClientDetached {
        client: &'a [u8],
    },
    ClientSessionChanged {
        client: &'a [u8],
        session: SessionId,
        name: &'a [u8],
    },
    PasteBufferChanged {
        name: &'a [u8],
    },
    PasteBufferDeleted {
        name: &'a [u8],
    },
    SubscriptionChanged {
        name: &'a [u8],
        session: SessionId,
        window: Option<WindowId>,
        window_index: Option<u64>,
        pane: Option<PaneId>,
        value: &'a [u8],
    },
    ConfigError {
        message: &'a [u8],
    },
    Message {
        message: &'a [u8],
    },
    Exit {
        reason: Option<&'a [u8]>,
    },
    Other {
        name: &'a [u8],
        arguments: &'a [u8],
    },
}
