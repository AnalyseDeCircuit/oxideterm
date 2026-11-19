use std::collections::{HashMap, HashSet, VecDeque};

use oxideterm_sftp::{
    SftpFileEntry, SftpFileType,
    browser_files::{SftpSortDirection, SftpSortField, sftp_sorted_filtered_indices},
};

/// One flattened row of the sidebar remote tree.
#[derive(Clone, Debug)]
pub struct SftpTreeRow {
    pub entry: SftpFileEntry,
    pub depth: usize,
    pub expanded: bool,
    pub loading: bool,
}

pub struct SftpTreeData<'a> {
    pub root: &'a [SftpFileEntry],
    pub tree_expanded: &'a HashSet<String>,
    pub tree_loading: &'a HashSet<String>,
    pub tree_pending: &'a VecDeque<String>,
    pub tree_children: &'a HashMap<String, Vec<SftpFileEntry>>,
    pub remote_sort_field: SftpSortField,
    pub remote_sort_direction: SftpSortDirection,
}

impl SftpTreeData<'_> {
    pub fn rows(&self) -> Vec<SftpTreeRow> {
        let mut rows = Vec::new();
        self.push_sftp_tree_rows(self.root, 0, &mut HashSet::new(), &mut rows);
        rows
    }

    fn push_sftp_tree_rows(
        &self,
        files: &[SftpFileEntry],
        depth: usize,
        ancestors: &mut HashSet<String>,
        rows: &mut Vec<SftpTreeRow>,
    ) {
        let indices = sftp_sorted_filtered_indices(
            files,
            "",
            self.remote_sort_field,
            self.remote_sort_direction,
        );
        for index in indices {
            // SFTP canonicalizes directory listings. A link back to an ancestor
            // can return the same cached path; only prune that ancestry chain,
            // so sibling links to a shared directory remain independently visible.
            if ancestors.contains(&files[index].path) {
                continue;
            }
            let entry = files[index].clone();
            let is_directory = entry.file_type == SftpFileType::Directory;
            let expanded = is_directory && self.tree_expanded.contains(&entry.path);
            let loading = is_directory
                && (self.tree_loading.contains(&entry.path)
                    || self.tree_pending.contains(&entry.path));
            let path = entry.path.clone();
            rows.push(SftpTreeRow {
                entry,
                depth,
                expanded,
                loading,
            });
            if expanded && let Some(children) = self.tree_children.get(&path) {
                ancestors.insert(path.clone());
                self.push_sftp_tree_rows(children, depth + 1, ancestors, rows);
                ancestors.remove(&path);
            }
        }
    }
}
