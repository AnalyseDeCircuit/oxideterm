// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::filesystem::{
    FileStat, FileSystemCapabilities, IdeFileData, IdeFileError, IdeFileErrorKind, IdeFileSystem,
    WriteMode,
};
use crate::model::{
    DirtyCloseDecision, FileKind, FileTreeEntry, IdeLocation, OpenFileOutcome, ReloadError,
    RestoreSkipReason, RestoreSnapshotResult, SavedFileVersion,
};
use crate::workspace::{IdeWorkspace, WorkspaceError};

struct MemoryFs {
    data: IdeFileData,
    fail_write: bool,
    atomic_write: bool,
}

impl MemoryFs {
    fn new(text: &str, version: SavedFileVersion) -> Self {
        Self {
            data: IdeFileData {
                text: text.into(),
                version,
            },
            fail_write: false,
            atomic_write: true,
        }
    }

    fn failing_write(mut self) -> Self {
        self.fail_write = true;
        self
    }
}

impl IdeFileSystem for MemoryFs {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: self.atomic_write,
            directory_listing: true,
            conflict_detection: true,
        }
    }

    fn read_file(&self, _location: &IdeLocation) -> Result<IdeFileData, IdeFileError> {
        Ok(self.data.clone())
    }

    fn stat(&self, _location: &IdeLocation) -> Result<FileStat, IdeFileError> {
        Ok(FileStat {
            version: self.data.version.clone(),
            is_read_only: false,
        })
    }

    fn list_dir(&self, _location: &IdeLocation) -> Result<Vec<FileTreeEntry>, IdeFileError> {
        Ok(Vec::new())
    }

    fn write_file(
        &self,
        _location: &IdeLocation,
        _text: &str,
        _expected_version: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError> {
        assert_eq!(mode, WriteMode::AtomicReplace);
        if self.fail_write {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Disconnected,
                "connection lost",
            ));
        }
        Ok(SavedFileVersion {
            size_bytes: Some(7),
            modified_millis: Some(100),
            etag: Some("saved".into()),
        })
    }
}

fn local_file(name: &str) -> IdeLocation {
    IdeLocation::local(format!("/tmp/oxideterm/{name}"))
}

#[test]
fn open_file_reuses_existing_tab_for_same_location() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");

    let first = workspace
        .open_file(
            local_file("main.rs"),
            "fn main() {}",
            SavedFileVersion::unknown(),
        )
        .unwrap();
    let second = workspace
        .open_file(
            local_file("main.rs"),
            "ignored",
            SavedFileVersion::unknown(),
        )
        .unwrap();

    let OpenFileOutcome::Opened(tab_id) = first else {
        panic!("first open should allocate a tab");
    };
    assert_eq!(second, OpenFileOutcome::Reused(tab_id));
    assert_eq!(workspace.tabs().len(), 1);
    assert_eq!(workspace.active_tab(), Some(tab_id));
}

#[test]
fn edits_mark_dirty_and_save_clears_dirty() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("README.md"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };

    workspace.replace_buffer_text(tab_id, "new").unwrap();
    assert!(workspace.buffer(tab_id).unwrap().is_dirty());

    workspace
        .mark_saved(
            tab_id,
            SavedFileVersion {
                size_bytes: Some(3),
                modified_millis: Some(10),
                etag: Some("v2".to_string()),
            },
        )
        .unwrap();
    assert!(!workspace.buffer(tab_id).unwrap().is_dirty());
}

#[test]
fn replacing_buffer_with_identical_text_keeps_dirty_state() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("clean.txt"), "same", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };

    workspace.replace_buffer_text(tab_id, "same").unwrap();

    assert!(!workspace.buffer(tab_id).unwrap().is_dirty());
}

#[test]
fn save_tab_with_clears_dirty_only_after_success() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("save.rs"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "changed").unwrap();

    let failed = MemoryFs::new("unused", SavedFileVersion::unknown()).failing_write();
    assert!(workspace.save_tab_with(&failed, tab_id).is_err());
    assert!(workspace.buffer(tab_id).unwrap().is_dirty());

    let saved = workspace
        .save_tab_with(
            &MemoryFs::new("unused", SavedFileVersion::unknown()),
            tab_id,
        )
        .unwrap();
    assert_eq!(saved.etag.as_deref(), Some("saved"));
    assert!(!workspace.buffer(tab_id).unwrap().is_dirty());
}

#[test]
fn stale_save_completion_preserves_newer_dirty_text() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("race.rs"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace
        .replace_buffer_text(tab_id, "saved request")
        .unwrap();
    let save_text = workspace.buffer(tab_id).unwrap().text.clone();
    let save_revision = workspace.buffer(tab_id).unwrap().revision;

    workspace
        .replace_buffer_text(tab_id, "newer local edit")
        .unwrap();
    let clean = workspace
        .complete_save_at_revision(
            tab_id,
            save_text,
            save_revision,
            SavedFileVersion {
                size_bytes: Some(13),
                modified_millis: Some(200),
                etag: Some("saved-request".into()),
            },
        )
        .unwrap();

    let buffer = workspace.buffer(tab_id).unwrap();
    assert!(!clean);
    assert_eq!(buffer.text, "newer local edit");
    assert_eq!(buffer.saved_text, "saved request");
    assert_eq!(buffer.version.etag.as_deref(), Some("saved-request"));
    assert!(buffer.is_dirty());
}

#[test]
fn close_after_save_keeps_tab_open_when_newer_edit_arrives() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(
            local_file("close-race.rs"),
            "old",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "save me").unwrap();
    let save_text = workspace.buffer(tab_id).unwrap().text.clone();
    let save_revision = workspace.buffer(tab_id).unwrap().revision;
    let request = workspace.request_close_tab(tab_id).unwrap().unwrap();

    workspace.replace_buffer_text(tab_id, "keep me").unwrap();
    let closed = workspace
        .complete_dirty_close_after_save_at_revision(
            request.id,
            save_text,
            save_revision,
            SavedFileVersion {
                size_bytes: Some(7),
                modified_millis: Some(300),
                etag: Some("saved-before-close".into()),
            },
        )
        .unwrap();

    let buffer = workspace.buffer(tab_id).unwrap();
    assert!(!closed);
    assert!(workspace.pending_close().is_none());
    assert_eq!(buffer.text, "keep me");
    assert_eq!(buffer.saved_text, "save me");
    assert!(buffer.is_dirty());
}

#[test]
fn reload_tab_with_refuses_dirty_buffers() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("reload.rs"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "dirty").unwrap();

    let result = workspace.reload_tab_with(
        &MemoryFs::new("remote", SavedFileVersion::unknown()),
        tab_id,
    );

    assert_eq!(result, Err(ReloadError::DirtyBuffer));
    assert_eq!(workspace.buffer(tab_id).unwrap().text, "dirty");
}

#[test]
fn reload_tab_with_replaces_clean_buffer() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let version = SavedFileVersion {
        size_bytes: Some(6),
        modified_millis: Some(42),
        etag: Some("remote".into()),
    };
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("reload.rs"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };

    workspace
        .reload_tab_with(&MemoryFs::new("remote", version.clone()), tab_id)
        .unwrap();

    let buffer = workspace.buffer(tab_id).unwrap();
    assert_eq!(buffer.text, "remote");
    assert_eq!(buffer.version, version);
    assert!(!buffer.is_dirty());
}

#[test]
fn dirty_close_requires_confirmation_and_cancel_keeps_tab() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("dirty.txt"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "new").unwrap();

    let request = workspace.request_close_tab(tab_id).unwrap().unwrap();
    assert_eq!(request.tab_id, tab_id);
    assert!(workspace.pending_close().is_some());

    workspace
        .resolve_dirty_close(request.id, DirtyCloseDecision::Cancel)
        .unwrap();
    assert_eq!(workspace.tabs().len(), 1);
    assert!(workspace.pending_close().is_none());
}

#[test]
fn dirty_close_discard_removes_tab() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("dirty.txt"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "new").unwrap();
    let request = workspace.request_close_tab(tab_id).unwrap().unwrap();

    workspace
        .resolve_dirty_close(request.id, DirtyCloseDecision::Discard)
        .unwrap();
    assert!(workspace.tabs().is_empty());
    assert!(workspace.buffer(tab_id).is_none());
}

#[test]
fn close_all_tabs_stops_on_first_dirty_tab() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(local_file("dirty.txt"), "old", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "new").unwrap();

    let request = workspace.request_close_all_tabs().unwrap().unwrap();

    assert_eq!(request.tab_id, tab_id);
    assert_eq!(workspace.tabs().len(), 1);
}

#[test]
fn delete_path_closes_clean_affected_tabs_like_tauri_store() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::remote("node-a", "/repo"), "repo");
    let OpenFileOutcome::Opened(root_file) = workspace
        .open_file(
            IdeLocation::remote("node-a", "/repo/src/main.rs"),
            "main",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(other) = workspace
        .open_file(
            IdeLocation::remote("node-a", "/repo/README.md"),
            "readme",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };

    let closed = workspace
        .close_clean_tabs_under(&IdeLocation::remote("node-a", "/repo/src"))
        .unwrap();

    assert_eq!(closed, vec![root_file]);
    assert!(workspace.buffer(root_file).is_none());
    assert!(workspace.buffer(other).is_some());
}

#[test]
fn delete_path_refuses_dirty_affected_tabs_like_tauri_store() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::remote("node-a", "/repo"), "repo");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(
            IdeLocation::remote("node-a", "/repo/src/main.rs"),
            "main",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "dirty").unwrap();

    let result = workspace.close_clean_tabs_under(&IdeLocation::remote("node-a", "/repo/src"));

    assert!(matches!(result, Err(WorkspaceError::DirtyTabs(names)) if names == "main.rs"));
    assert!(workspace.buffer(tab_id).is_some());
}

#[test]
fn rename_path_retargets_open_tabs_without_clearing_dirty_text() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::remote("node-a", "/repo"), "repo");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(
            IdeLocation::remote("node-a", "/repo/src/main.rs"),
            "main",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace.replace_buffer_text(tab_id, "dirty").unwrap();

    let renamed = workspace
        .rename_tabs_under(
            &IdeLocation::remote("node-a", "/repo/src"),
            &IdeLocation::remote("node-a", "/repo/app"),
        )
        .unwrap();

    assert_eq!(renamed, vec![tab_id]);
    let tab = workspace
        .tabs()
        .iter()
        .find(|tab| tab.id == tab_id)
        .unwrap();
    assert_eq!(tab.title, "main.rs");
    assert_eq!(
        tab.location,
        IdeLocation::remote("node-a", "/repo/app/main.rs")
    );
    let buffer = workspace.buffer(tab_id).unwrap();
    assert_eq!(buffer.location, tab.location);
    assert_eq!(buffer.text, "dirty");
    assert!(buffer.is_dirty());
}

#[test]
fn file_tree_state_is_included_in_snapshot_restore() {
    let mut source = IdeWorkspace::new();
    let root = IdeLocation::remote("node-a", "/home/demo");
    let child = FileTreeEntry {
        location: IdeLocation::remote("node-a", "/home/demo/src"),
        kind: FileKind::Directory,
        name: "src".into(),
        version: SavedFileVersion::unknown(),
    };
    source.open_project(root.clone(), "demo");
    source.set_tree_expanded(&root, true).unwrap();
    source
        .set_tree_children(root.clone(), vec![child.clone()])
        .unwrap();
    source
        .select_tree_entry(Some(child.location.clone()))
        .unwrap();

    let snapshot = source.snapshot().unwrap();
    let mut restored = IdeWorkspace::new();
    assert_eq!(
        restored.restore_snapshot(snapshot),
        RestoreSnapshotResult::Restored { tab_count: 0 }
    );

    assert!(restored.file_tree().is_expanded(&root));
    assert_eq!(restored.file_tree().selected(), Some(&child.location));
    assert_eq!(
        restored.file_tree().children(&root),
        Some([child].as_slice())
    );
}

#[test]
fn snapshot_restore_preserves_dirty_buffers_and_active_tab() {
    let mut source = IdeWorkspace::new();
    source.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
    let OpenFileOutcome::Opened(first) = source
        .open_file(
            IdeLocation::remote("node-a", "/home/demo/a.rs"),
            "saved",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(second) = source
        .open_file(
            IdeLocation::remote("node-a", "/home/demo/b.rs"),
            "b",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    source.replace_buffer_text(first, "dirty").unwrap();

    let snapshot = source.snapshot().unwrap();
    let mut restored = IdeWorkspace::new();
    assert_eq!(
        restored.restore_snapshot(snapshot),
        RestoreSnapshotResult::Restored { tab_count: 2 }
    );
    assert_eq!(restored.active_tab(), Some(second));
    assert_eq!(restored.buffer(first).unwrap().text, "dirty");
    assert!(restored.buffer(first).unwrap().is_dirty());
}

#[test]
fn reconnect_restore_restores_open_file_without_clearing_dirty_buffer() {
    let mut source = IdeWorkspace::new();
    let location = IdeLocation::remote("node-a", "/home/demo/src/main.rs");
    source.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
    let OpenFileOutcome::Opened(tab_id) = source
        .open_file(location.clone(), "saved", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    source
        .replace_buffer_text(tab_id, "dirty local edit")
        .unwrap();

    let snapshot = source.snapshot().unwrap();
    let mut restored = IdeWorkspace::new();
    assert_eq!(
        restored.restore_snapshot(snapshot),
        RestoreSnapshotResult::Restored { tab_count: 1 }
    );

    let buffer = restored.buffer(tab_id).unwrap();
    assert_eq!(buffer.location, location);
    assert_eq!(buffer.text, "dirty local edit");
    assert!(buffer.is_dirty());
}

#[test]
fn stale_reload_result_cannot_overwrite_newer_dirty_buffer() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
    let OpenFileOutcome::Opened(tab_id) = workspace
        .open_file(
            IdeLocation::remote("node-a", "/home/demo/src/main.rs"),
            "saved",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };
    workspace
        .replace_buffer_text(tab_id, "newer dirty local edit")
        .unwrap();

    let stale_remote = MemoryFs::new("stale remote read", SavedFileVersion::unknown());
    assert_eq!(
        workspace.reload_tab_with(&stale_remote, tab_id),
        Err(ReloadError::DirtyBuffer)
    );
    assert_eq!(
        workspace.buffer(tab_id).unwrap().text,
        "newer dirty local edit"
    );
    assert!(workspace.buffer(tab_id).unwrap().is_dirty());
}

#[test]
fn restore_skips_after_user_closed_project() {
    let mut source = IdeWorkspace::new();
    source.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
    source
        .open_file(
            IdeLocation::remote("node-a", "/home/demo/a.rs"),
            "a",
            SavedFileVersion::unknown(),
        )
        .unwrap();
    let snapshot = source.snapshot().unwrap();

    let mut target = IdeWorkspace::new();
    target.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
    target.close_project();

    assert_eq!(
        target.restore_snapshot(snapshot),
        RestoreSnapshotResult::Skipped(RestoreSkipReason::ProjectWasClosedByUser)
    );
}

#[test]
fn restore_skips_when_current_project_has_dirty_edits() {
    let mut source = IdeWorkspace::new();
    source.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    source
        .open_file(local_file("a.rs"), "a", SavedFileVersion::unknown())
        .unwrap();
    let snapshot = source.snapshot().unwrap();

    let mut target = IdeWorkspace::new();
    target.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = target
        .open_file(local_file("b.rs"), "b", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    target.replace_buffer_text(tab_id, "dirty").unwrap();

    assert_eq!(
        target.restore_snapshot(snapshot),
        RestoreSnapshotResult::Skipped(RestoreSkipReason::ExistingDirtyBuffers)
    );
}

#[test]
fn tab_pin_state_round_trips_through_snapshot() {
    let mut source = IdeWorkspace::new();
    source.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(tab_id) = source
        .open_file(
            local_file("pinned.rs"),
            "fn main() {}",
            SavedFileVersion::unknown(),
        )
        .unwrap()
    else {
        panic!("file should open");
    };

    assert!(source.toggle_tab_pin(tab_id).unwrap());
    let snapshot = source.snapshot().unwrap();
    assert!(
        snapshot
            .tabs
            .iter()
            .any(|tab| tab.id == tab_id && tab.is_pinned)
    );

    let mut restored = IdeWorkspace::new();
    assert_eq!(
        restored.restore_snapshot(snapshot),
        RestoreSnapshotResult::Restored { tab_count: 1 }
    );
    assert!(
        restored
            .tabs()
            .iter()
            .any(|tab| tab.id == tab_id && tab.is_pinned)
    );
}

#[test]
fn reorders_tabs_before_target() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(first) = workspace
        .open_file(local_file("a.rs"), "a", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(second) = workspace
        .open_file(local_file("b.rs"), "b", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(third) = workspace
        .open_file(local_file("c.rs"), "c", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };

    workspace.move_tab_before(third, first).unwrap();

    let order = workspace
        .tabs()
        .iter()
        .map(|tab| tab.id)
        .collect::<Vec<_>>();
    assert_eq!(order, vec![third, first, second]);
}

#[test]
fn reorders_tabs_to_dnd_target_index() {
    let mut workspace = IdeWorkspace::new();
    workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
    let OpenFileOutcome::Opened(first) = workspace
        .open_file(local_file("a.rs"), "a", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(second) = workspace
        .open_file(local_file("b.rs"), "b", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };
    let OpenFileOutcome::Opened(third) = workspace
        .open_file(local_file("c.rs"), "c", SavedFileVersion::unknown())
        .unwrap()
    else {
        panic!("file should open");
    };

    workspace.move_tab_to_index(first, 2).unwrap();

    let order = workspace
        .tabs()
        .iter()
        .map(|tab| tab.id)
        .collect::<Vec<_>>();
    assert_eq!(order, vec![second, third, first]);
}
