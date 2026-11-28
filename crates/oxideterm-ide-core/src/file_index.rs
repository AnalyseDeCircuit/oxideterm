// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, collections::VecDeque, path::Path};

use crate::{FileKind, FileTreeEntry, IdeLocation};

const MAX_FILES: usize = 20_000;
const MAX_DIRECTORIES: usize = 2_048;

struct IndexedFile {
    location: IdeLocation,
    relative_lowercase: String,
}

/// One quick-open session shares a directory walk across all of its queries.
pub struct ProjectFileIndex {
    root: IdeLocation,
    pending: VecDeque<IdeLocation>,
    queued_directories: usize,
    files: Vec<IndexedFile>,
    truncated: bool,
}

impl ProjectFileIndex {
    pub fn new(root: IdeLocation) -> Self {
        Self {
            pending: VecDeque::from([root.clone()]),
            root,
            queued_directories: 1,
            files: Vec::new(),
            truncated: false,
        }
    }

    pub fn next_directory(&mut self) -> Option<IdeLocation> {
        self.pending.pop_front()
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub fn add_entries(&mut self, entries: Vec<FileTreeEntry>) {
        for entry in entries {
            match entry.kind {
                FileKind::Directory => {
                    if entry.is_symlink
                        || matches!(
                            entry.name.as_str(),
                            "." | ".." | ".git" | "node_modules" | "target"
                        )
                    {
                        continue;
                    }
                    if self.queued_directories < MAX_DIRECTORIES {
                        self.queued_directories += 1;
                        self.pending.push_back(entry.location);
                    } else {
                        self.truncated = true;
                    }
                }
                FileKind::File | FileKind::Symlink => {
                    if self.files.len() == MAX_FILES {
                        self.truncated = true;
                        self.pending.clear();
                        break;
                    }
                    if let Some(relative) = relative_path(&self.root, &entry.location) {
                        let relative_lowercase = relative.to_lowercase();
                        self.files.push(IndexedFile {
                            location: entry.location,
                            relative_lowercase,
                        });
                    }
                }
                FileKind::Other => {}
            }
        }
    }

    /// A plain query searches basenames. A slash opts into relative-path matching.
    pub fn matches(&self, query: &str, limit: usize) -> Vec<IdeLocation> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let by_path = query.contains('/');
        let mut hits = Vec::new();
        for (index, file) in self.files.iter().enumerate() {
            let text = if by_path {
                file.relative_lowercase.as_str()
            } else {
                file.relative_lowercase
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
            };
            if let Some(score) = match_score(text, &query) {
                hits.push((
                    score,
                    text.len(),
                    &file.relative_lowercase,
                    path(&file.location),
                    index,
                ));
            }
        }
        if hits.len() > limit {
            hits.select_nth_unstable(limit);
            hits.truncate(limit);
        }
        hits.sort_unstable();
        hits.into_iter()
            .map(|(_, _, _, _, index)| self.files[index].location.clone())
            .collect()
    }
}

fn path(location: &IdeLocation) -> &Path {
    match location {
        IdeLocation::Local { path } => path,
        IdeLocation::Remote { path, .. } => Path::new(path),
    }
}

fn relative_path<'a>(root: &IdeLocation, location: &'a IdeLocation) -> Option<Cow<'a, str>> {
    match (root, location) {
        (
            IdeLocation::Remote {
                node_id,
                path: root,
            },
            IdeLocation::Remote {
                node_id: node,
                path,
            },
        ) if node_id == node => path
            .strip_prefix(root.trim_end_matches('/'))?
            .strip_prefix('/')
            .map(Cow::Borrowed),
        (IdeLocation::Local { path: root }, IdeLocation::Local { path }) => {
            let relative = path.strip_prefix(root).ok()?.to_string_lossy();
            if std::path::MAIN_SEPARATOR == '/' {
                Some(relative)
            } else {
                Some(Cow::Owned(relative.replace(std::path::MAIN_SEPARATOR, "/")))
            }
        }
        _ => None,
    }
}

fn match_score(text: &str, query: &str) -> Option<(u8, usize, usize)> {
    if text == query {
        return Some((0, 0, 0));
    }
    if let Some(start) = text.find(query) {
        return Some((if start == 0 { 1 } else { 2 }, start, 0));
    }
    let mut characters = text.char_indices();
    let mut first = None;
    let mut last = 0;
    for query_char in query.chars() {
        let (index, _) = characters.find(|(_, ch)| *ch == query_char)?;
        first.get_or_insert(index);
        last = index;
    }
    let first = first?;
    Some((3, last - first, first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SavedFileVersion;

    #[test]
    fn quick_open_finds_nested_names_and_disambiguates_paths() {
        let location = |path: &str| IdeLocation::remote("project", format!("/repo/{path}"));
        let entry = |path: &str, kind: FileKind, is_symlink| FileTreeEntry {
            location: location(path),
            name: path.rsplit('/').next().unwrap().to_owned(),
            kind,
            is_symlink,
            version: SavedFileVersion::unknown(),
        };
        let mut index = ProjectFileIndex::new(IdeLocation::remote("project", "/repo/"));
        assert_eq!(index.next_directory(), Some(location("")));
        index.add_entries(vec![
            entry("src", FileKind::Directory, false),
            entry(".git", FileKind::Directory, false),
            entry("loop", FileKind::Directory, true),
            entry("README", FileKind::File, false),
            entry("ReadMe.txt", FileKind::File, false),
            entry("old_readme", FileKind::File, false),
            entry("road_map_extras", FileKind::File, false),
        ]);
        assert_eq!(index.next_directory(), Some(location("src")));
        index.add_entries(vec![
            entry("src/README", FileKind::File, false),
            entry("src/工具.py", FileKind::File, false),
        ]);
        assert_eq!(index.next_directory(), None);
        assert_eq!(
            index.matches("readme", 3),
            [
                location("README"),
                location("src/README"),
                location("ReadMe.txt")
            ]
        );
        assert_eq!(
            index.matches("RME", 10),
            [
                location("README"),
                location("src/README"),
                location("ReadMe.txt"),
                location("old_readme"),
                location("road_map_extras")
            ]
        );
        assert_eq!(index.matches("SRC/工具", 10), [location("src/工具.py")]);
        assert!(index.matches("src", 10).is_empty());
    }
}
