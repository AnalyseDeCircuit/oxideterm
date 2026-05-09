// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{FileTreeEntry, IdeLocation};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTreeSnapshot {
    pub expanded: Vec<String>,
    pub selected: Option<IdeLocation>,
    pub directories: Vec<FileTreeDirectorySnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTreeDirectorySnapshot {
    pub location: IdeLocation,
    pub children: Vec<FileTreeEntry>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileTreeState {
    expanded: HashSet<String>,
    selected: Option<IdeLocation>,
    directories: HashMap<String, DirectoryState>,
    // Structural revision for GPUI virtualization caches. Selection is not
    // included because rows resolve selected state live during rendering.
    revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectoryState {
    location: IdeLocation,
    children: Vec<FileTreeEntry>,
}

impl FileTreeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expand(&mut self, location: &IdeLocation) {
        if self.expanded.insert(location.stable_key()) {
            self.bump_revision();
        }
    }

    pub fn collapse(&mut self, location: &IdeLocation) {
        if self.expanded.remove(&location.stable_key()) {
            self.bump_revision();
        }
    }

    pub fn is_expanded(&self, location: &IdeLocation) -> bool {
        self.expanded.contains(&location.stable_key())
    }

    pub fn set_selected(&mut self, location: Option<IdeLocation>) {
        self.selected = location;
    }

    pub fn selected(&self) -> Option<&IdeLocation> {
        self.selected.as_ref()
    }

    pub fn set_children(&mut self, directory: IdeLocation, children: Vec<FileTreeEntry>) {
        let key = directory.stable_key();
        let next = DirectoryState {
            location: directory,
            children,
        };
        if self.directories.get(&key) != Some(&next) {
            self.directories.insert(key, next);
            self.bump_revision();
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    pub fn children(&self, directory: &IdeLocation) -> Option<&[FileTreeEntry]> {
        self.directories
            .get(&directory.stable_key())
            .map(|directory| directory.children.as_slice())
    }

    pub fn clear(&mut self) {
        if self.expanded.is_empty() && self.selected.is_none() && self.directories.is_empty() {
            return;
        }
        self.expanded.clear();
        self.selected = None;
        self.directories.clear();
        self.bump_revision();
    }

    pub fn snapshot(&self) -> FileTreeSnapshot {
        let mut expanded = self.expanded.iter().cloned().collect::<Vec<_>>();
        expanded.sort();
        let mut directories = self
            .directories
            .values()
            .map(|directory| FileTreeDirectorySnapshot {
                location: directory.location.clone(),
                children: directory.children.clone(),
            })
            .collect::<Vec<_>>();
        directories
            .sort_by(|left, right| left.location.stable_key().cmp(&right.location.stable_key()));
        FileTreeSnapshot {
            expanded,
            selected: self.selected.clone(),
            directories,
        }
    }

    pub fn restore(snapshot: FileTreeSnapshot) -> Self {
        let directories = snapshot
            .directories
            .into_iter()
            .map(|directory| {
                (
                    directory.location.stable_key(),
                    DirectoryState {
                        location: directory.location,
                        children: directory.children,
                    },
                )
            })
            .collect();
        Self {
            expanded: snapshot.expanded.into_iter().collect(),
            selected: snapshot.selected,
            directories,
            revision: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{FileKind, SavedFileVersion};

    use super::*;

    #[test]
    fn tracks_expansion_selection_and_children() {
        let root = IdeLocation::local("/tmp/oxideterm");
        let child = FileTreeEntry {
            location: IdeLocation::local("/tmp/oxideterm/main.rs"),
            kind: FileKind::File,
            name: "main.rs".into(),
            version: SavedFileVersion::unknown(),
        };
        let mut tree = FileTreeState::new();

        let initial_revision = tree.revision();
        tree.expand(&root);
        tree.set_selected(Some(child.location.clone()));
        tree.set_children(root.clone(), vec![child.clone()]);

        assert!(tree.is_expanded(&root));
        assert_eq!(tree.selected(), Some(&child.location));
        assert_eq!(tree.children(&root), Some([child].as_slice()));
        assert!(tree.revision() > initial_revision);
    }
}
