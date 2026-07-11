fn render_tree_row_virtual(
    row: TreeRenderRow,
    selected_location: Option<&IdeLocation>,
    loading_paths: &HashSet<String>,
    tokens: &ThemeTokens,
    entity: Entity<IdeSurface>,
) -> AnyElement {
    let entry = row.entry;
    let selected = selected_location == Some(&entry.location);
    let is_dir = matches!(entry.kind, FileKind::Directory);
    let path_key = entry.location.stable_key();
    let loading = loading_paths.contains(&path_key);
    let icon = if is_dir {
        file_icons::folder_icon(row.expanded, entry.name == ".git", tokens)
    } else {
        file_icons::file_icon(&entry.name, tokens)
    };
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
            move |_event: &MouseDownEvent, _window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.tree_context_menu = None;
                    this.open_tree_entry(left_entry.clone(), cx);
                    cx.stop_propagation();
                });
            }
        })
        .on_mouse_down(MouseButton::Right, {
            move |event: &MouseDownEvent, _window, cx| {
                let _ = context_menu_entity.update(cx, |this, cx| {
                    let _ = this
                        .workspace
                        .select_tree_entry(Some(right_entry.location.clone()));
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
        } else if is_dir {
            tree_svg_icon(icon.path, IDE_ICON_SIZE, icon.color)
        } else {
            tree_svg_icon(icon.path, IDE_FILE_ICON_SIZE, icon.color)
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
