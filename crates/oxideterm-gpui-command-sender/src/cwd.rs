use oxideterm_environment::{
    CurrentDirectoryEntry, CurrentDirectoryEntryKind, current_directory_cd_command,
    current_directory_parent, current_directory_path_is_explicit,
};

pub const MAX_ENTRIES: usize = 160;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Parent,
    Directory,
    File,
    TypedPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub name: String,
    pub path: String,
}

pub fn chip_label(path: &str) -> String {
    let path = path.trim();
    if path.is_empty()
        || path == "/"
        || path == "~"
        || path.ends_with(":\\")
        || path.ends_with(":/")
    {
        return path.to_string();
    }
    let separator = if path.contains('\\') { '\\' } else { '/' };
    let mut segments = path
        .split(separator)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return path.to_string();
    }
    let tail = segments.split_off(segments.len().saturating_sub(2));
    let label = tail.join(&separator.to_string());
    if path.starts_with("~/") && tail.len() == 1 {
        format!("~/{label}")
    } else {
        label
    }
}

pub fn choices(path: &str, entries: &[CurrentDirectoryEntry], input: &str) -> Vec<Entry> {
    let query = input.trim().to_ascii_lowercase();
    let mut rows = Vec::new();

    if let Some(parent) = current_directory_parent(path) {
        rows.push(Entry {
            kind: Kind::Parent,
            name: "..".to_string(),
            path: parent,
        });
    }
    rows.extend(
        entries
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.name().to_ascii_lowercase().contains(&query)
                    || entry.path().to_ascii_lowercase().contains(&query)
            })
            .map(|entry| Entry {
                kind: match entry.kind() {
                    CurrentDirectoryEntryKind::Directory => Kind::Directory,
                    CurrentDirectoryEntryKind::File => Kind::File,
                },
                name: entry.name().to_string(),
                path: entry.path().to_string(),
            }),
    );
    if let Some(path) = typed_path(input, entries) {
        rows.push(Entry {
            kind: Kind::TypedPath,
            name: path.clone(),
            path,
        });
    }
    rows
}

fn typed_path(input: &str, entries: &[CurrentDirectoryEntry]) -> Option<String> {
    let query = input.trim();
    if !current_directory_path_is_explicit(query) || entries.iter().any(|e| e.path() == query) {
        return None;
    }
    current_directory_cd_command(query).map(|_| query.to_owned())
}
