use std::collections::VecDeque;

use super::Metadata;
use crate::protocol::FileType;

/// Entries returned by the [`ReadDir`] iterator.
#[derive(Debug)]
pub struct DirEntry {
    file: String,
    metadata: Metadata,
}

impl DirEntry {
    /// Returns the file name for the file that this entry points at.
    pub fn file_name(&self) -> String {
        self.file.to_owned()
    }

    /// Returns the file type for the file that this entry points at.
    pub fn file_type(&self) -> FileType {
        self.metadata.file_type()
    }

    /// Returns the metadata for the file that this entry points at.
    pub fn metadata(&self) -> Metadata {
        self.metadata.to_owned()
    }
}

/// Iterator over the entries in a remote directory.
pub struct ReadDir {
    pub(crate) entries: VecDeque<(String, Metadata)>,
}

impl Iterator for ReadDir {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        match self.entries.pop_front() {
            None => None,
            Some(entry) if entry.0 == "." || entry.0 == ".." => self.next(),
            Some(entry) => Some(DirEntry {
                file: entry.0,
                metadata: entry.1,
            }),
        }
    }
}
