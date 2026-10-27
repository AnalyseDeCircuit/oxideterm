use super::file_list::sftp_sorted_filtered_indices;
use super::*;

/// One flattened row of the sidebar remote tree.
#[derive(Clone, Debug)]
pub(in crate::workspace::sftp) struct SftpTreeRow {
    pub(in crate::workspace::sftp) entry: SftpFileEntry,
    pub(in crate::workspace::sftp) depth: usize,
    pub(in crate::workspace::sftp) expanded: bool,
    pub(in crate::workspace::sftp) loading: bool,
}

impl SftpWorkspaceEntity {
    pub(in crate::workspace::sftp) fn flatten_sftp_tree_rows(&self) -> Vec<SftpTreeRow> {
        let mut rows = Vec::new();
        self.push_sftp_tree_rows(&self.remote_files, 0, &mut rows);
        rows
    }

    /// The directory represented by the visible tree focus. A selected file
    /// resolves to its containing directory; stale selections are ignored so
    /// navigation and collapse cannot leave the sync target outside the tree.
    pub(in crate::workspace::sftp) fn tree_sync_directory_path(&self) -> Option<String> {
        let selected = self.tree_selected.as_deref()?;
        let row = self
            .flatten_sftp_tree_rows()
            .into_iter()
            .find(|row| row.entry.path == selected)?;
        if row.entry.file_type == SftpFileType::Directory {
            Some(row.entry.path)
        } else {
            Some(parent_path(&row.entry.path, true))
        }
    }

    fn push_sftp_tree_rows(
        &self,
        files: &[SftpFileEntry],
        depth: usize,
        rows: &mut Vec<SftpTreeRow>,
    ) {
        let indices = sftp_sorted_filtered_indices(
            files,
            "",
            self.remote_sort_field,
            self.remote_sort_direction,
        );
        for index in indices {
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
                self.push_sftp_tree_rows(children, depth + 1, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn file(name: &str, path: &str, file_type: SftpFileType) -> SftpFileEntry {
        SftpFileEntry {
            name: name.to_string(),
            path: path.to_string(),
            file_type,
            size_known: true,
            size: 1,
            modified: Some(0),
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    fn listing(cwd: &str, files: Vec<SftpFileEntry>) -> RemoteSftpListing {
        RemoteSftpListing {
            cwd: cwd.to_string(),
            files,
        }
    }

    fn prepare_entity(cx: &mut TestAppContext) -> gpui::Entity<SftpWorkspaceEntity> {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            sftp.current_surface_id = Some(SftpSurfaceId::Sidebar);
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("node")));
            sftp.view_generation = 1;
            sftp.remote_path = "/root".to_string();
            sftp.remote_path_input = "/root".to_string();
        });
        entity
    }

    #[gpui::test]
    fn flatten_orders_directories_then_children_by_depth(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.remote_files = vec![
                file("b.txt", "/root/b.txt", SftpFileType::File),
                file("dir2", "/root/dir2", SftpFileType::Directory),
                file("dir1", "/root/dir1", SftpFileType::Directory),
            ];
            sftp.tree_expanded.insert("/root/dir1".to_string());
            sftp.tree_children.insert(
                "/root/dir1".to_string(),
                vec![file(
                    "child.txt",
                    "/root/dir1/child.txt",
                    SftpFileType::File,
                )],
            );
        });
        let rows = entity.read_with(cx, |sftp, _| sftp.flatten_sftp_tree_rows());
        let names = rows
            .iter()
            .map(|row| row.entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["dir1", "child.txt", "dir2", "b.txt"]);
        assert_eq!(rows[1].depth, 1);
        assert!(rows[0].expanded);
        assert!(!rows[2].expanded);
    }

    #[gpui::test]
    fn sync_target_follows_the_selected_tree_entry(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.remote_files = vec![
                file("dir", "/root/dir", SftpFileType::Directory),
                file("note.txt", "/root/note.txt", SftpFileType::File),
            ];
            assert_eq!(sftp.tree_sync_directory_path(), None);
            sftp.tree_selected = Some("/root/dir".to_string());
            assert_eq!(
                sftp.tree_sync_directory_path().as_deref(),
                Some("/root/dir")
            );
            sftp.tree_selected = Some("/root/note.txt".to_string());
            assert_eq!(sftp.tree_sync_directory_path().as_deref(), Some("/root"));
            sftp.tree_selected = Some("/outside".to_string());
            assert_eq!(sftp.tree_sync_directory_path(), None);
        });
    }

    #[gpui::test]
    fn tree_context_menu_targets_the_absolute_row_path(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, cx| {
            sftp.remote_selected.insert("unrelated".to_string());
            let row = file("dir", "/root/dir", SftpFileType::Directory);
            sftp.open_tree_context_menu(row, 10.0, 20.0, cx);
            assert_eq!(sftp.tree_selected.as_deref(), Some("/root/dir"));
            assert!(sftp.remote_selected.contains("unrelated"));
            let menu = sftp.context_menu.as_ref().expect("tree menu");
            assert!(menu.tree);
            assert_eq!(
                menu.file.as_ref().map(|file| file.path.as_str()),
                Some("/root/dir")
            );
        });
    }

    #[gpui::test]
    fn flatten_marks_loading_directories(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.remote_files = vec![
                file("dir", "/root/dir", SftpFileType::Directory),
                file("queued", "/root/queued", SftpFileType::Directory),
            ];
            sftp.tree_expanded.insert("/root/dir".to_string());
            sftp.tree_expanded.insert("/root/queued".to_string());
            sftp.tree_loading.insert("/root/dir".to_string());
            sftp.tree_pending.push_back("/root/queued".to_string());
        });
        let rows = entity.read_with(cx, |sftp, _| sftp.flatten_sftp_tree_rows());
        assert!(rows[0].loading);
        assert!(rows[0].expanded);
        assert!(rows[1].loading, "queued directories also need a spinner");
    }

    #[gpui::test]
    fn stale_tree_result_is_dropped(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.view_generation = 2;
            sftp.tree_loading.insert("/root/dir".to_string());
        });
        let mut effects = VecDeque::new();
        let result = SftpWorkerResult::TreeChildren {
            surface_id: SftpSurfaceId::Sidebar,
            remote_id: SftpRemoteId::Node(NodeId::new("node")),
            view_generation: 1,
            tree_generation: 0,
            path: "/root/dir".to_string(),
            result: Ok(listing("/root/dir", Vec::new())),
        };
        let changed = entity.update(cx, |sftp, cx| {
            sftp.reduce_worker_result(result, &mut effects, cx)
        });
        assert!(!changed);
        assert!(effects.is_empty());
        assert!(
            entity.read_with(cx, |sftp, _| sftp.tree_children.is_empty()),
            "stale result must not populate the cache"
        );
    }

    #[gpui::test]
    fn tree_reset_invalidates_inflight_children_result(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.tree_expanded.insert("/root/dir".to_string());
            sftp.tree_loading.insert("/root/dir".to_string());
            sftp.clear_sftp_tree_state();
        });
        let mut effects = VecDeque::new();
        let result = SftpWorkerResult::TreeChildren {
            surface_id: SftpSurfaceId::Sidebar,
            remote_id: SftpRemoteId::Node(NodeId::new("node")),
            view_generation: 1,
            tree_generation: 0,
            path: "/root/dir".to_string(),
            result: Ok(listing("/root/dir", Vec::new())),
        };
        let changed = entity.update(cx, |sftp, cx| {
            sftp.reduce_worker_result(result, &mut effects, cx)
        });
        assert!(!changed);
        assert!(effects.is_empty());
        assert!(
            entity.read_with(cx, |sftp, _| {
                sftp.tree_children.is_empty() && sftp.tree_loading.is_empty()
            }),
            "a reset must drop results from the previous tree epoch"
        );
    }

    #[gpui::test]
    fn failed_tree_result_advances_the_pending_queue(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.tree_loading.insert("/root/first".to_string());
            sftp.tree_pending.push_back("/root/second".to_string());
        });
        let mut effects = VecDeque::new();
        let result = SftpWorkerResult::TreeChildren {
            surface_id: SftpSurfaceId::Sidebar,
            remote_id: SftpRemoteId::Node(NodeId::new("node")),
            view_generation: 1,
            tree_generation: 0,
            path: "/root/first".to_string(),
            result: Err("permission denied".to_string()),
        };
        let changed = entity.update(cx, |sftp, cx| {
            sftp.reduce_worker_result(result, &mut effects, cx)
        });
        assert!(changed);
        let state = entity.read_with(cx, |sftp, _| {
            (
                sftp.tree_loading.clone(),
                sftp.tree_pending.clone(),
                sftp.tree_expanded.clone(),
            )
        });
        assert_eq!(
            state.0.len(),
            1,
            "the next queued directory must start loading"
        );
        assert!(state.0.contains("/root/second"));
        assert!(state.1.is_empty());
        assert!(!state.2.contains("/root/first"));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            SftpWorkspaceEffect::StartTreeChildrenLoad { path, .. } if path == "/root/second"
        )));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            SftpWorkspaceEffect::TreeChildrenLoadFailed { path, .. } if path == "/root/first"
        )));
    }
}
