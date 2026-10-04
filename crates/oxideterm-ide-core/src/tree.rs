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
    selection: Vec<IdeLocation>,
    selection_anchor: Option<IdeLocation>,
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
        self.selection = location.iter().cloned().collect();
        self.selection_anchor = location.clone();
        self.selected = location;
    }

    pub fn selection(&self) -> &[IdeLocation] {
        &self.selection
    }

    pub fn select_entry(
        &mut self,
        location: IdeLocation,
        visible: &[IdeLocation],
        additive: bool,
        range: bool,
    ) {
        if range
            && let Some(anchor) = self.selection_anchor.as_ref()
            && let (Some(start), Some(end)) = (
                visible.iter().position(|entry| entry == anchor),
                visible.iter().position(|entry| entry == &location),
            )
        {
            if !additive {
                self.selection.clear();
            }
            for entry in &visible[start.min(end)..=start.max(end)] {
                if !self.selection.contains(entry) {
                    self.selection.push(entry.clone());
                }
            }
            self.selected = Some(location);
        } else if additive {
            if let Some(index) = self.selection.iter().position(|entry| entry == &location) {
                self.selection.remove(index);
                self.selected = self.selection.last().cloned();
            } else {
                self.selection.push(location.clone());
                self.selected = Some(location.clone());
            }
            self.selection_anchor = Some(location);
        } else {
            self.set_selected(Some(location));
        }
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
        if self.expanded.is_empty()
            && self.selection_anchor.is_none()
            && self.directories.is_empty()
        {
            return;
        }
        self.expanded.clear();
        self.selected = None;
        self.selection.clear();
        self.selection_anchor = None;
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
            // Multi-selection is transient; the saved primary row remains the restore contract.
            selection: snapshot.selected.iter().cloned().collect(),
            selection_anchor: snapshot.selected.clone(),
            selected: snapshot.selected,
            directories,
            revision: 1,
        }
    }
}
