// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{X11ForwardingError, X11Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11RemoteDisplayAllocator {
    pub bind_host: String,
    pub start_display: u16,
    pub max_displays: u16,
    pub screen: u16,
}

impl X11RemoteDisplayAllocator {
    pub fn localhost() -> Self {
        Self {
            bind_host: "localhost".to_string(),
            start_display: 10,
            max_displays: 64,
            screen: 0,
        }
    }

    pub fn candidates(&self) -> impl Iterator<Item = u16> + '_ {
        self.start_display..self.start_display.saturating_add(self.max_displays)
    }

    pub fn display_value(&self, display: u16) -> String {
        format!("{}:{display}.{}", self.bind_host, self.screen)
    }

    pub fn probe_command(&self) -> String {
        let first = self.start_display;
        let last = self
            .start_display
            .saturating_add(self.max_displays.saturating_sub(1));
        // Keep the remote probe POSIX-sh compatible: ss/lsof/netstat are
        // optional, and the socket-path check catches sshd-created X11 sockets.
        format!(
            "d={first}; while [ \"$d\" -le {last} ]; do p=$((6000+d)); \
if [ ! -S /tmp/.X11-unix/X$d ] && \
{{ ! command -v ss >/dev/null 2>&1 || ! ss -H -ltn 2>/dev/null | grep -Eq \"[.:]$p[[:space:]]\"; }} && \
{{ ! command -v lsof >/dev/null 2>&1 || ! lsof -nP -iTCP:$p -sTCP:LISTEN >/dev/null 2>&1; }} && \
{{ ! command -v netstat >/dev/null 2>&1 || ! netstat -an 2>/dev/null | grep -Eq \"[.:]$p[[:space:]].*LISTEN\"; }}; then \
echo $d; exit 0; fi; d=$((d+1)); done; exit 1"
        )
    }

    pub fn parse_probe_output(&self, output: &str) -> X11Result<u16> {
        let value = output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or(X11ForwardingError::RemoteDisplayUnavailable)?;
        let display = value
            .parse::<u16>()
            .map_err(|_| X11ForwardingError::InvalidDisplay("remote display probe".to_string()))?;
        if self.candidates().any(|candidate| candidate == display) {
            Ok(display)
        } else {
            Err(X11ForwardingError::RemoteDisplayUnavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_scans_default_openssh_style_range() {
        let allocator = X11RemoteDisplayAllocator::localhost();

        assert_eq!(
            allocator.candidates().take(3).collect::<Vec<_>>(),
            vec![10, 11, 12]
        );
        assert_eq!(allocator.display_value(10), "localhost:10.0");
        assert_eq!(allocator.parse_probe_output("12\n").unwrap(), 12);
    }

    #[test]
    fn allocator_rejects_out_of_range_probe_result() {
        let allocator = X11RemoteDisplayAllocator {
            start_display: 10,
            max_displays: 2,
            ..X11RemoteDisplayAllocator::localhost()
        };

        assert_eq!(
            allocator.parse_probe_output("20").unwrap_err(),
            X11ForwardingError::RemoteDisplayUnavailable
        );
    }

    #[test]
    fn allocator_probe_command_checks_multiple_backends() {
        let command = X11RemoteDisplayAllocator::localhost().probe_command();

        assert!(command.contains("ss -H -ltn"));
        assert!(command.contains("lsof -nP"));
        assert!(command.contains("netstat -an"));
        assert!(command.contains("/tmp/.X11-unix/X$d"));
        assert!(!command.contains("seq "));
    }
}
