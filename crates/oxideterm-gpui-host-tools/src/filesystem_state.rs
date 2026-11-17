use std::{cell::RefCell, ops::Range};

use gpui::{ListAlignment, ListState, px};
use oxideterm_connection_monitor::{
    FilesystemFilter, ResourceFilesystemEntry, ResourceFilesystemSnapshot,
    filesystem_row_signature, visible_filesystem_rows,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualListSpec, VirtualListSignatureCache, expandable_row_signature,
    sync_tauri_variable_list_state_by_signatures, tauri_virtual_list_state,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemSnapshotRequest {
    pub connection_id: String,
    pub notify: bool,
    pub failure_fallback: String,
}

/// UI state shared by all mounts of this surface. The host executes requests
/// and delivers parsed snapshots; the panel has no transport or timer.
pub struct FilesystemPanel {
    query: String,
    filter: FilesystemFilter,
    expanded_index: Option<usize>,
    running: Option<FilesystemSnapshotRequest>,
    snapshot: Option<(String, ResourceFilesystemSnapshot)>,
    list_state: ListState,
    list_cache: RefCell<VirtualListSignatureCache>,
}

pub(super) fn filesystem_list_spec() -> TauriVirtualListSpec {
    TauriVirtualListSpec::new(px(64.0), 8)
}

impl Default for FilesystemPanel {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: FilesystemFilter::All,
            expanded_index: None,
            running: None,
            snapshot: None,
            list_state: tauri_virtual_list_state(0, ListAlignment::Top, filesystem_list_spec()),
            list_cache: RefCell::default(),
        }
    }
}

impl FilesystemPanel {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn replace_query(&mut self, replacement_range: Option<Range<usize>>, text: &str) {
        replace_utf16(&mut self.query, replacement_range, text);
        self.expanded_index = None;
    }

    pub(super) fn filter(&self) -> FilesystemFilter {
        self.filter
    }

    pub fn select_filter(&mut self, filter: FilesystemFilter) -> bool {
        if self.filter == filter {
            return false;
        }
        self.filter = filter;
        self.expanded_index = None;
        true
    }

    pub(super) fn expanded_index(&self) -> Option<usize> {
        self.expanded_index
    }

    pub fn toggle_expanded(&mut self, index: usize) {
        self.expanded_index = (self.expanded_index != Some(index)).then_some(index);
    }

    pub fn refresh_in_flight(&self) -> bool {
        self.running.is_some()
    }

    pub fn begin_refresh(&mut self, request: FilesystemSnapshotRequest) {
        debug_assert!(self.running.is_none());
        self.running = Some(request);
    }

    /// A result for a different request must not clear the busy state.
    pub fn take_refresh(&mut self, request: &FilesystemSnapshotRequest) -> bool {
        if self.running.as_ref() != Some(request) {
            return false;
        }
        self.running = None;
        true
    }

    pub fn store_snapshot(
        &mut self,
        request: FilesystemSnapshotRequest,
        snapshot: ResourceFilesystemSnapshot,
    ) {
        self.snapshot = Some((request.connection_id, snapshot));
    }

    pub fn snapshot_for(&self, connection_id: &str) -> Option<&ResourceFilesystemSnapshot> {
        self.snapshot
            .as_ref()
            .filter(|(source, _)| source == connection_id)
            .map(|(_, snapshot)| snapshot)
    }

    pub(super) fn visible_rows(&self, connection_id: &str) -> Vec<ResourceFilesystemEntry> {
        self.snapshot_for(connection_id)
            .map(|snapshot| visible_filesystem_rows(&snapshot.entries, &self.query, self.filter))
            .unwrap_or_default()
    }

    pub(super) fn list_state(
        &self,
        rows: &[ResourceFilesystemEntry],
        connection_id: &str,
    ) -> ListState {
        let signatures = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                expandable_row_signature(
                    filesystem_row_signature(row),
                    self.expanded_index == Some(index),
                )
            })
            .collect::<Vec<_>>();
        // Expansion remeasures one row without resetting the user's scroll position.
        let identity = format!(
            "host-filesystems:{connection_id}:{}:{}",
            self.query, self.filter as u8
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.list_state,
            &mut self.list_cache.borrow_mut(),
            &identity,
            &signatures,
            filesystem_list_spec(),
        );
        self.list_state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connection_monitor::ResourceFilesystemStatus;

    #[test]
    fn filesystem_search_replaces_a_utf16_selection_and_closes_the_expanded_row() {
        let mut panel = FilesystemPanel::default();
        panel.replace_query(None, "a💾旧");
        panel.toggle_expanded(2);
        panel.replace_query(Some(1..3), "新");
        assert_eq!(panel.query(), "a新旧");
        assert_eq!(panel.expanded_index(), None);
    }

    #[test]
    fn filesystem_refresh_does_not_relabel_a_previous_hosts_snapshot() {
        let mut panel = FilesystemPanel::default();
        let request = |connection_id: &str| FilesystemSnapshotRequest {
            connection_id: connection_id.into(),
            notify: true,
            failure_fallback: "Refresh failed".into(),
        };
        let previous = request("a");
        panel.begin_refresh(previous.clone());
        assert!(panel.take_refresh(&previous));
        panel.store_snapshot(
            previous.clone(),
            ResourceFilesystemSnapshot {
                status: ResourceFilesystemStatus::Unavailable,
                entries: vec![],
            },
        );

        let next = request("b");
        panel.begin_refresh(next.clone());
        assert!(panel.snapshot_for("b").is_none());
        assert!(!panel.take_refresh(&previous));
        assert!(panel.refresh_in_flight());
        assert!(panel.take_refresh(&next));
        panel.store_snapshot(
            next,
            ResourceFilesystemSnapshot {
                status: ResourceFilesystemStatus::Error {
                    message: "Mount listing failed".into(),
                },
                entries: vec![],
            },
        );
        assert!(panel.snapshot_for("a").is_none());
        assert_eq!(
            panel.snapshot_for("b").unwrap().status,
            ResourceFilesystemStatus::Error {
                message: "Mount listing failed".into()
            }
        );
    }
}
