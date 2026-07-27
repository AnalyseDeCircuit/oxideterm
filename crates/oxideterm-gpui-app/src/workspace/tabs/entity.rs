// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

/// Owns workspace-wide tab, pane, and terminal identity allocation.
pub(in crate::workspace) struct WorkspaceTabHostEntity {
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
}

impl WorkspaceTabHostEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
        }
    }

    pub(in crate::workspace) fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub(in crate::workspace) fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    pub(in crate::workspace) fn alloc_session_id(&mut self) -> TerminalSessionId {
        let id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_workspace_ids_are_monotonic_and_independent() {
        let mut tab_host = WorkspaceTabHostEntity::new();

        assert_eq!(tab_host.alloc_tab_id(), TabId(1));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(1));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(1));
        assert_eq!(tab_host.alloc_tab_id(), TabId(2));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(2));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(2));
    }
}
