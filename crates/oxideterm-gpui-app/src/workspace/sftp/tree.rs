use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn sftp_tree_mode(&self, cx: &App) -> bool {
        self.sftp_view().read(cx).tree_mode(
            self.settings_store.settings().sftp.sidebar_view_mode
                == oxideterm_settings::SftpSidebarViewMode::Tree,
        )
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
    fn flatten_stops_ancestor_cycles_without_hiding_sibling_aliases(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _| {
            let first = file("first", "/root/first", SftpFileType::Directory);
            let second = file("second", "/root/second", SftpFileType::Directory);
            let shared = file("shared", "/target/shared", SftpFileType::Directory);
            sftp.remote_files = vec![first.clone(), second];
            sftp.tree_expanded.extend([
                "/root/first".into(),
                "/root/second".into(),
                "/target/shared".into(),
            ]);
            // A canonical listing for a self-link includes the link itself.
            sftp.tree_children
                .insert("/root/first".into(), vec![first.clone(), shared.clone()]);
            sftp.tree_children
                .insert("/root/second".into(), vec![shared]);
            // A link through a shared directory can also point back to an ancestor.
            sftp.tree_children
                .insert("/target/shared".into(), vec![first]);
            let rows = sftp.flatten_sftp_tree_rows();
            assert_eq!(
                rows.iter()
                    .map(|row| (row.entry.path.as_str(), row.depth))
                    .collect::<Vec<_>>(),
                [
                    ("/root/first", 0),
                    ("/target/shared", 1),
                    ("/root/second", 0),
                    ("/target/shared", 1),
                    ("/root/first", 2),
                ]
            );
        });
    }

    #[gpui::test]
    fn sync_target_follows_the_selected_tree_entry(cx: &mut TestAppContext) {
        let entity = prepare_entity(cx);
        entity.update(cx, |sftp, _cx| {
            sftp.remote_files = vec![file("dir", "/root/dir", SftpFileType::Directory)];
            sftp.tree_expanded.insert("/root/dir".into());
            sftp.tree_children.insert(
                "/root/dir".into(),
                vec![file("note.txt", "/root/dir/note.txt", SftpFileType::File)],
            );
            assert_eq!(sftp.terminal_sync_path(true), "/root");
            sftp.tree_selected = Some("/root/dir".into());
            assert_eq!(sftp.terminal_sync_path(true), "/root/dir");
            sftp.tree_selected = Some("/root/dir/note.txt".into());
            assert_eq!(sftp.terminal_sync_path(true), "/root/dir");
            sftp.tree_selected = Some("/outside".into());
            assert_eq!(sftp.terminal_sync_path(true), "/root");
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
            assert!(sftp.remote_selected.is_empty());
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
