fn render_tree_row_virtual(
    row: TreeRenderRow,
    selected_locations: &HashSet<IdeLocation>,
    loading_paths: &HashSet<String>,
    tokens: &ThemeTokens,
    entity: Entity<IdeSurface>,
) -> AnyElement {
    let entry = row.entry;
    let selected = selected_locations.contains(&entry.location);
    let is_dir = matches!(entry.kind, FileKind::Directory);
    let path_key = entry.location.stable_key();
    let loading = loading_paths.contains(&path_key);
    let icon = if is_dir {
        file_icons::folder_icon(&entry.name, row.expanded)
    } else {
        file_icons::file_icon(&entry.name)
    }
    .with_symlink(matches!(entry.kind, FileKind::Symlink));
    let row_bg = if selected {
        rgba((tokens.ui.accent << 8) | IDE_TREE_SELECTED_ALPHA)
    } else {
        rgba(0x00000000)
    };
    let left_entry = entry.clone();
    let right_entry = entry.clone();
    let context_menu_entity = entity.clone();

    div()
        .h(px(IDE_ROW_HEIGHT))
        .w_full()
        .px_1()
        .flex()
        .items_center()
        .gap_1()
        .cursor_pointer()
        .bg(row_bg)
        .text_size(px(tokens.metrics.ui_text_xs))
        .hover(|style| style.bg(rgba((tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA)))
        .on_mouse_down(MouseButton::Left, {
            move |event: &MouseDownEvent, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.tree_context_menu = None;
                    this.select_tree_row(left_entry.clone(), event.modifiers, cx);
                    cx.stop_propagation();
                });
            }
        })
        .on_mouse_down(MouseButton::Right, {
            move |event: &MouseDownEvent, window, cx| {
                let _ = context_menu_entity.update(cx, |this, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.open_tree_context_menu(
                        right_entry.location.clone(),
                        matches!(right_entry.kind, FileKind::Directory),
                        right_entry.name.clone(),
                        event.position,
                        cx,
                    );
                    cx.stop_propagation();
                });
            }
        })
        .child(div().w(px((row.depth as f32) * IDE_TREE_INDENT_STEP)))
        .child(if is_dir {
            tree_chevron_icon(
                tokens,
                tree_motion_id("chevron", &path_key),
                14.0,
                tokens.ui.text_secondary,
                row.expanded,
            )
        } else {
            div().w(px(14.0)).into_any_element()
        })
        .child(if loading {
            tree_spinner_icon(
                tokens,
                tree_motion_id("spinner", &path_key),
                IDE_ICON_SIZE,
                tokens.ui.accent,
            )
        } else {
            icon.render(
                if is_dir {
                    IDE_ICON_SIZE
                } else {
                    IDE_FILE_ICON_SIZE
                },
                tokens,
            )
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(rgb(if selected {
                    tokens.ui.accent
                } else if is_dir {
                    tokens.ui.text
                } else {
                    tokens.ui.text_muted
                }))
                .child(entry.name),
        )
        .into_any_element()
}

#[cfg(test)]
mod tree_selection_tests {
    use super::*;
    use gpui::TestAppContext;

    struct TreeTestView {
        surface: Entity<IdeSurface>,
        _observation: gpui::Subscription,
    }

    impl Render for TreeTestView {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.surface.update(cx, |surface, cx| {
                surface.render_tree_rows(IdeLocation::remote("selection-test", "/repo"), cx)
            })
        }
    }

    #[gpui::test]
    fn modifier_clicks_select_tree_rows_without_opening_files(cx: &mut TestAppContext) {
        let root = IdeLocation::remote("selection-test", "/repo");
        let entries = ["a.txt", "b.txt", "c.txt"].map(|name| FileTreeEntry {
            location: IdeLocation::remote("selection-test", format!("/repo/{name}")),
            name: name.into(),
            kind: FileKind::File,
            version: SavedFileVersion::unknown(),
        });
        let surface = cx.new(|cx| {
            let router =
                oxideterm_ssh::NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::default());
            let backend = Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            );
            let mut surface = IdeSurface::new(
                NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled),
                oxideterm_theme::default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend,
                cx,
            );
            surface.workspace.open_project(root.clone(), "repo");
            surface
                .workspace
                .set_tree_children(root, entries.to_vec())
                .unwrap();
            surface.load_state = IdeLoadState::Ready;
            surface
        });
        let (_, cx) = cx.add_window_view(|_, cx| TreeTestView {
            surface: surface.clone(),
            _observation: cx.observe(&surface, |_, _, cx| cx.notify()),
        });
        cx.simulate_resize(gpui::size(px(320.0), px(200.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let point = |row| gpui::point(px(70.0), px(IDE_ROW_HEIGHT * (row as f32 + 0.5)));
        let command = gpui::Modifiers {
            platform: true,
            ..Default::default()
        };
        for row in [0, 2] {
            cx.simulate_click(point(row), command);
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        surface.read_with(cx, |surface, _| {
            assert_eq!(
                surface.workspace.file_tree().selection(),
                &[entries[0].location.clone(), entries[2].location.clone()]
            );
            assert!(
                surface.workspace.tabs().is_empty(),
                "modifier clicks must not open documents"
            );
        });
        cx.simulate_click(point(2), command);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.read_with(cx, |surface, _| {
            assert_eq!(
                surface.workspace.file_tree().selection(),
                &[entries[0].location.clone()]
            );
        });
        // Start a stable range at the first row, then grow and shrink it.
        surface.update(cx, |surface, _| {
            surface
                .workspace
                .select_tree_entry(Some(entries[0].location.clone()))
                .unwrap();
        });
        let shift = gpui::Modifiers {
            shift: true,
            ..Default::default()
        };
        for (row, expected) in [(2, entries.to_vec()), (1, entries[..2].to_vec())] {
            cx.simulate_click(point(row), shift);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            surface.read_with(cx, |surface, _| {
                assert_eq!(
                    surface.workspace.file_tree().selection(),
                    expected
                        .iter()
                        .map(|entry| entry.location.clone())
                        .collect::<Vec<_>>()
                );
            });
        }
        surface.update(cx, |surface, cx| {
            let selected = surface.workspace.file_tree().selection().to_vec();
            surface.open_tree_context_menu(
                entries[0].location.clone(),
                false,
                entries[0].name.clone(),
                point(0),
                cx,
            );
            assert_eq!(surface.workspace.file_tree().selection(), selected);
            surface.request_copy_tree_item(
                entries[0].location.clone(),
                entries[0].name.clone(),
                false,
                cx,
            );
            assert_eq!(
                surface.tree_clipboard.as_ref().unwrap().entries,
                entries[..2]
            );
            let opened = surface
                .workspace
                .open_file(
                    entries[1].location.clone(),
                    "saved",
                    SavedFileVersion::unknown(),
                )
                .unwrap();
            let oxideterm_ide_core::OpenFileOutcome::Opened(tab) = opened else {
                panic!("new document")
            };
            surface
                .workspace
                .replace_buffer_text(tab, "unsaved")
                .unwrap();
            surface.request_delete_tree_item(
                entries[0].location.clone(),
                entries[0].name.clone(),
                false,
                cx,
            );
            let confirm = surface.delete_confirm.as_ref().unwrap();
            assert_eq!(confirm.entries, entries[..2]);
            assert_eq!(confirm.unsaved_tab_count, 1);
            surface.confirm_delete_tree_item(cx);
            assert_eq!(
                surface.workspace.buffer(tab).unwrap().text.as_ref(),
                "unsaved"
            );
            let directory = FileTreeEntry {
                location: IdeLocation::remote("selection-test", "/repo/folder"),
                name: "folder".into(),
                kind: FileKind::Directory,
                version: SavedFileVersion::unknown(),
            };
            let child = FileTreeEntry {
                location: IdeLocation::remote("selection-test", "/repo/folder/child.txt"),
                name: "child.txt".into(),
                kind: FileKind::File,
                version: SavedFileVersion::unknown(),
            };
            surface
                .workspace
                .set_tree_children(
                    IdeLocation::remote("selection-test", "/repo"),
                    vec![directory.clone()],
                )
                .unwrap();
            surface
                .workspace
                .set_tree_children(directory.location.clone(), vec![child.clone()])
                .unwrap();
            surface
                .workspace
                .select_tree_entry(Some(directory.location.clone()))
                .unwrap();
            surface
                .workspace
                .select_tree_entries(child.location.clone(), &[], true, false)
                .unwrap();
            assert_eq!(
                surface.tree_operation_entries(child.location, child.name, false),
                vec![directory]
            );
        });
    }
}
