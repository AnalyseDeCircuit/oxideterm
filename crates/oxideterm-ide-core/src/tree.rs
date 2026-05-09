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
        self.expanded.insert(location.stable_key());
    }

    pub fn collapse(&mut self, location: &IdeLocation) {
        self.expanded.remove(&location.stable_key());
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
        self.directories.insert(
            directory.stable_key(),
            DirectoryState {
                location: directory,
                children,
            },
        );
    }

    pub fn children(&self, directory: &IdeLocation) -> Option<&[FileTreeEntry]> {
        self.directories
            .get(&directory.stable_key())
            .map(|directory| directory.children.as_slice())
    }

    pub fn clear(&mut self) {
        self.expanded.clear();
        self.selected = None;
        self.directories.clear();
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

        tree.expand(&root);
        tree.set_selected(Some(child.location.clone()));
        tree.set_children(root.clone(), vec![child.clone()]);

        assert!(tree.is_expanded(&root));
        assert_eq!(tree.selected(), Some(&child.location));
        assert_eq!(tree.children(&root), Some([child].as_slice()));
    }
}
