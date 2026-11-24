use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use crate::metrics::terminal_row_timestamp_identity;
use oxideterm_terminal_emulator::{TerminalCell, TerminalRow, TerminalSnapshot};

#[derive(Clone, Debug)]
pub struct TerminalRowTimestamp {
    pub label: String,
    pub signature: u64,
    pub source_signature: u64,
}

/// Visual metadata follows the emulator row, independent of viewport coordinates.
#[derive(Clone, Default)]
pub struct TerminalRowTimestampStore {
    pub entries: HashMap<u64, TerminalRowTimestamp>,
}

impl TerminalRowTimestampStore {
    pub fn get(&self, identity: u64) -> Option<&TerminalRowTimestamp> {
        self.entries.get(&identity)
    }

    #[cfg(test)]
    pub fn insert_label(&mut self, identity: u64, label: impl Into<String>) {
        self.entries.insert(
            identity,
            TerminalRowTimestamp {
                label: label.into(),
                signature: 0,
                source_signature: 0,
            },
        );
    }
}

pub fn record_timestampable_snapshot_rows(
    store: &mut TerminalRowTimestampStore,
    snapshot: &TerminalSnapshot,
    label: &str,
) {
    for row in &snapshot.lines {
        let key = terminal_row_timestamp_identity(row);
        if key == 0 {
            continue;
        }
        // The snapshot signature is a cheap invalidation key. Cursor-only changes
        // still fall through to the content signature comparison below.
        if store
            .entries
            .get(&key)
            .is_some_and(|timestamp| timestamp.source_signature == row.signature)
        {
            continue;
        }

        if terminal_row_has_timestamp_content(row) {
            let timestamp_signature = terminal_row_timestamp_signature(row);
            if let Some(timestamp) = store.entries.get_mut(&key) {
                if timestamp.signature != timestamp_signature {
                    timestamp.label = label.to_string();
                    timestamp.signature = timestamp_signature;
                }
                timestamp.source_signature = row.signature;
            } else {
                store.entries.insert(
                    key,
                    TerminalRowTimestamp {
                        label: label.to_string(),
                        signature: timestamp_signature,
                        source_signature: row.signature,
                    },
                );
            }
        } else {
            // Blank viewport rows are recycled later. Removing their metadata
            // prevents new output from inheriting a stale line-modification time.
            store.entries.remove(&key);
        }
    }
}

pub fn trim_row_timestamp_history(
    row_timestamps: &mut HashMap<u64, TerminalRowTimestamp>,
    retained_rows: usize,
) {
    // IDs are process-wide; other terminals can make them arbitrarily sparse.
    // Batch pruning bounds memory without scanning the cache on every frame.
    if row_timestamps.len() <= retained_rows.saturating_add(1024) {
        return;
    }
    let mut identities: Vec<_> = row_timestamps.keys().copied().collect();
    let remove_count = identities.len() - retained_rows;
    identities.select_nth_unstable(remove_count);
    let min_identity = identities[remove_count];
    row_timestamps.retain(|identity, _| *identity >= min_identity);
}

fn terminal_row_timestamp_signature(row: &TerminalRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.wrapped.hash(&mut hasher);
    // Ignore unstyled trailing padding so a grid resize does not restamp output.
    // Styled blanks and links remain part of the content signature.
    let content_end = row.cells.iter().rposition(terminal_cell_has_content);
    let Some(content_end) = content_end else {
        return hasher.finish();
    };
    for cell in &row.cells[..=content_end] {
        cell.ch.hash(&mut hasher);
        cell.zerowidth().hash(&mut hasher);
        cell.wide.hash(&mut hasher);
        cell.fg.hash(&mut hasher);
        cell.bg.hash(&mut hasher);
        cell.attrs.hash(&mut hasher);
        cell.hyperlink().hash(&mut hasher);
    }
    hasher.finish()
}

fn terminal_row_has_timestamp_content(row: &TerminalRow) -> bool {
    row.cells.iter().any(terminal_cell_has_content)
}

/// Explicit styling and links distinguish meaningful blanks from grid padding.
fn terminal_cell_has_content(cell: &TerminalCell) -> bool {
    !cell.ch.is_whitespace()
        || !cell.zerowidth().is_empty()
        || cell.style_origin.foreground_explicit()
        || cell.style_origin.background_explicit()
        || cell.attrs != Default::default()
        || cell.hyperlink().is_some()
}
