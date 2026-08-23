// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use crate::{Notification, PaneId, SessionId, WindowId, decode_output, ids::parse_decimal};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandGuard {
    pub timestamp: u64,
    pub number: u64,
    pub flags: u64,
}

impl CommandGuard {
    pub fn is_control_command(self) -> bool {
        self.flags != 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ControlEvent<'a> {
    Begin(CommandGuard),
    End(CommandGuard),
    Error(CommandGuard),
    CommandOutput(&'a [u8]),
    Notification(Notification<'a>),
    Text(&'a [u8]),
}

#[derive(Debug, Default)]
pub struct ControlParser {
    open_command: Option<CommandGuard>,
}

impl ControlParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.open_command = None;
    }

    pub fn open_command(&self) -> Option<CommandGuard> {
        self.open_command
    }

    pub fn parse_line<'a>(&mut self, line: &'a [u8]) -> ControlEvent<'a> {
        if let Some(open) = self.open_command {
            if let Some((kind, guard)) = parse_guard(line)
                && guard.number == open.number
                && !matches!(kind, GuardKind::Begin)
            {
                self.open_command = None;
                return match kind {
                    GuardKind::End => ControlEvent::End(guard),
                    GuardKind::Error => ControlEvent::Error(guard),
                    GuardKind::Begin => unreachable!("begin guards do not close command blocks"),
                };
            }

            // Only the matching terminator has structure inside a response
            // block; a line beginning with '%' may be ordinary command output.
            return ControlEvent::CommandOutput(line);
        }

        if let Some((kind, guard)) = parse_guard(line) {
            return match kind {
                GuardKind::Begin => {
                    self.open_command = Some(guard);
                    ControlEvent::Begin(guard)
                }
                GuardKind::End => ControlEvent::End(guard),
                GuardKind::Error => ControlEvent::Error(guard),
            };
        }

        parse_notification(line)
            .map(ControlEvent::Notification)
            .unwrap_or(ControlEvent::Text(line))
    }
}

#[derive(Clone, Copy)]
enum GuardKind {
    Begin,
    End,
    Error,
}

fn parse_guard(line: &[u8]) -> Option<(GuardKind, CommandGuard)> {
    let mut fields = line.split(|byte| *byte == b' ');
    let kind = match fields.next()? {
        b"%begin" => GuardKind::Begin,
        b"%end" => GuardKind::End,
        b"%error" => GuardKind::Error,
        _ => return None,
    };
    let guard = CommandGuard {
        timestamp: parse_decimal(fields.next()?)?,
        number: parse_decimal(fields.next()?)?,
        flags: parse_decimal(fields.next()?)?,
    };
    fields.next().is_none().then_some((kind, guard))
}

fn parse_notification(line: &[u8]) -> Option<Notification<'_>> {
    let rest = line.strip_prefix(b"%")?;
    let (name, arguments) = split_once(rest, b' ');
    let fallback = || Notification::Other { name, arguments };

    Some(match name {
        b"output" => parse_output(arguments).unwrap_or_else(fallback),
        b"extended-output" => parse_extended_output(arguments).unwrap_or_else(fallback),
        b"pause" => parse_pane(arguments)
            .map(Notification::Pause)
            .unwrap_or_else(fallback),
        b"continue" => parse_pane(arguments)
            .map(Notification::Continue)
            .unwrap_or_else(fallback),
        b"pane-mode-changed" => parse_pane(arguments)
            .map(Notification::PaneModeChanged)
            .unwrap_or_else(fallback),
        b"layout-change" => parse_layout_changed(arguments).unwrap_or_else(fallback),
        b"window-add" => parse_window(arguments)
            .map(Notification::WindowAdded)
            .unwrap_or_else(fallback),
        b"window-close" => parse_window(arguments)
            .map(Notification::WindowClosed)
            .unwrap_or_else(fallback),
        b"window-renamed" => parse_window_name(arguments, false).unwrap_or_else(fallback),
        b"window-pane-changed" => parse_window_pane(arguments).unwrap_or_else(fallback),
        b"unlinked-window-add" => parse_window(arguments)
            .map(Notification::UnlinkedWindowAdded)
            .unwrap_or_else(fallback),
        b"unlinked-window-close" => parse_window(arguments)
            .map(Notification::UnlinkedWindowClosed)
            .unwrap_or_else(fallback),
        b"unlinked-window-renamed" => parse_window_name(arguments, true).unwrap_or_else(fallback),
        b"session-changed" => parse_session_name(arguments, false).unwrap_or_else(fallback),
        b"session-renamed" => parse_session_name(arguments, true).unwrap_or_else(fallback),
        b"session-window-changed" => parse_session_window(arguments).unwrap_or_else(fallback),
        b"sessions-changed" if arguments.is_empty() => Notification::SessionsChanged,
        b"client-detached" => Notification::ClientDetached { client: arguments },
        b"client-session-changed" => parse_client_session(arguments).unwrap_or_else(fallback),
        b"paste-buffer-changed" => Notification::PasteBufferChanged { name: arguments },
        b"paste-buffer-deleted" => Notification::PasteBufferDeleted { name: arguments },
        b"subscription-changed" => parse_subscription(arguments).unwrap_or_else(fallback),
        b"config-error" => Notification::ConfigError { message: arguments },
        b"message" => Notification::Message { message: arguments },
        b"exit" => Notification::Exit {
            reason: (!arguments.is_empty()).then_some(arguments),
        },
        _ => fallback(),
    })
}

fn parse_output(arguments: &[u8]) -> Option<Notification<'_>> {
    let (pane, bytes) = split_once(arguments, b' ');
    Some(Notification::Output {
        pane: PaneId::parse(pane)?,
        bytes: decode_output(bytes),
    })
}

fn parse_extended_output(arguments: &[u8]) -> Option<Notification<'_>> {
    let (pane, remaining) = split_once(arguments, b' ');
    let (milliseconds_behind, extension_fields) = split_once(remaining, b' ');
    Some(Notification::ExtendedOutput {
        pane: PaneId::parse(pane)?,
        milliseconds_behind: parse_decimal(milliseconds_behind)?,
        // tmux reserves fields between the age and the standalone colon.
        // Ignore them so a newer server cannot leak metadata into pane output.
        bytes: decode_output(payload_after_colon(extension_fields)?),
    })
}

fn parse_pane(arguments: &[u8]) -> Option<PaneId> {
    PaneId::parse(first_field(arguments))
}

fn parse_window(arguments: &[u8]) -> Option<WindowId> {
    WindowId::parse(first_field(arguments))
}

fn parse_layout_changed(arguments: &[u8]) -> Option<Notification<'_>> {
    let (window, remaining) = split_once(arguments, b' ');
    let (layout, remaining) = split_once(remaining, b' ');
    if layout.is_empty() {
        return None;
    }
    let (visible_layout, flags) = split_once(remaining, b' ');
    Some(Notification::LayoutChanged {
        window: WindowId::parse(window)?,
        layout,
        visible_layout: (!visible_layout.is_empty()).then_some(visible_layout),
        flags: (!flags.is_empty()).then_some(flags),
    })
}

fn parse_window_name(arguments: &[u8], unlinked: bool) -> Option<Notification<'_>> {
    let (window, name) = split_once(arguments, b' ');
    let window = WindowId::parse(window)?;
    Some(if unlinked {
        Notification::UnlinkedWindowRenamed { window, name }
    } else {
        Notification::WindowRenamed { window, name }
    })
}

fn parse_window_pane(arguments: &[u8]) -> Option<Notification<'_>> {
    let (window, pane) = split_once(arguments, b' ');
    Some(Notification::WindowPaneChanged {
        window: WindowId::parse(window)?,
        pane: PaneId::parse(first_field(pane))?,
    })
}

fn parse_session_name(arguments: &[u8], renamed: bool) -> Option<Notification<'_>> {
    let (session, name) = split_once(arguments, b' ');
    let session = SessionId::parse(session)?;
    Some(if renamed {
        Notification::SessionRenamed { session, name }
    } else {
        Notification::SessionChanged { session, name }
    })
}

fn parse_session_window(arguments: &[u8]) -> Option<Notification<'_>> {
    let (session, window) = split_once(arguments, b' ');
    Some(Notification::SessionWindowChanged {
        session: SessionId::parse(session)?,
        window: WindowId::parse(first_field(window))?,
    })
}

fn parse_client_session(arguments: &[u8]) -> Option<Notification<'_>> {
    let (client, remaining) = split_once(arguments, b' ');
    let (session, name) = split_once(remaining, b' ');
    Some(Notification::ClientSessionChanged {
        client,
        session: SessionId::parse(session)?,
        name,
    })
}

fn parse_subscription(arguments: &[u8]) -> Option<Notification<'_>> {
    let (name, remaining) = split_once(arguments, b' ');
    let (session, remaining) = split_once(remaining, b' ');
    let (window, remaining) = split_once(remaining, b' ');
    let (window_index, remaining) = split_once(remaining, b' ');
    let (pane, extension_fields) = split_once(remaining, b' ');
    Some(Notification::SubscriptionChanged {
        name,
        session: SessionId::parse(session)?,
        window: parse_optional(window, WindowId::parse)?,
        window_index: parse_optional(window_index, parse_decimal)?,
        pane: parse_optional(pane, PaneId::parse)?,
        // Fields added by future tmux versions sit before the standalone colon.
        value: payload_after_colon(extension_fields)?,
    })
}

fn payload_after_colon(fields: &[u8]) -> Option<&[u8]> {
    if fields == b":" {
        return Some(&[]);
    }
    if let Some(payload) = fields.strip_prefix(b": ") {
        return Some(payload);
    }
    if fields.ends_with(b" :") {
        return Some(&[]);
    }
    fields
        .windows(3)
        .position(|window| window == b" : ")
        .map(|delimiter| &fields[delimiter + 3..])
}

fn parse_optional<T>(field: &[u8], parse: impl FnOnce(&[u8]) -> Option<T>) -> Option<Option<T>> {
    if field == b"-" {
        Some(None)
    } else {
        parse(field).map(Some)
    }
}

fn first_field(bytes: &[u8]) -> &[u8] {
    split_once(bytes, b' ').0
}

fn split_once(bytes: &[u8], delimiter: u8) -> (&[u8], &[u8]) {
    bytes
        .iter()
        .position(|byte| *byte == delimiter)
        .map_or((bytes, &[]), |index| (&bytes[..index], &bytes[index + 1..]))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn response_block_keeps_percent_lines_as_command_output() {
        let mut parser = ControlParser::new();
        assert!(matches!(
            parser.parse_line(b"%begin 10 7 1"),
            ControlEvent::Begin(CommandGuard { number: 7, .. })
        ));
        assert_eq!(
            parser.parse_line(b"%output is command text"),
            ControlEvent::CommandOutput(b"%output is command text")
        );
        assert!(matches!(
            parser.parse_line(b"%end 10 7 1"),
            ControlEvent::End(CommandGuard { number: 7, .. })
        ));
    }

    #[test]
    fn output_is_decoded_without_utf8_conversion() {
        let mut parser = ControlParser::new();
        let mut line = b"%output %3 ".to_vec();
        line.extend_from_slice(&[0xff, b'\\', b'0', b'3', b'3']);
        match parser.parse_line(&line) {
            ControlEvent::Notification(Notification::Output { pane, bytes }) => {
                assert_eq!(pane, PaneId(3));
                assert_eq!(bytes.as_ref(), &[0xff, 0x1b]);
                assert!(matches!(bytes, Cow::Owned(_)));
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn reserved_fields_before_payload_are_ignored() {
        let mut parser = ControlParser::new();
        assert!(matches!(
            parser.parse_line(b"%extended-output %3 42 future field : text\\015"),
            ControlEvent::Notification(Notification::ExtendedOutput {
                pane: PaneId(3),
                milliseconds_behind: 42,
                bytes,
            }) if bytes.as_ref() == b"text\r"
        ));
        assert!(matches!(
            parser.parse_line(b"%subscription-changed watch $1 @2 3 %4 future : value"),
            ControlEvent::Notification(Notification::SubscriptionChanged {
                name: b"watch",
                session: SessionId(1),
                window: Some(WindowId(2)),
                window_index: Some(3),
                pane: Some(PaneId(4)),
                value: b"value",
            })
        ));
    }

    #[test]
    fn unknown_notifications_remain_structured_and_borrowed() {
        let mut parser = ControlParser::new();
        assert_eq!(
            parser.parse_line(b"%future-event opaque payload"),
            ControlEvent::Notification(Notification::Other {
                name: b"future-event",
                arguments: b"opaque payload",
            })
        );
    }
}
