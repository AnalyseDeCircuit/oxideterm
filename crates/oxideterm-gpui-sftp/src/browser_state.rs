// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl SftpWorkspaceEntity {
    pub(crate) fn cancel_path_edit(&mut self, pane: SftpPane, cx: &mut Context<Self>) {
        // Cancelling a path edit restores the committed path and releases only
        // the SFTP-owned input focus; workspace IME coordination stays at root.
        match pane {
            SftpPane::Local => {
                self.local_path_completion.dismiss();
                self.local_path_input.clone_from(&self.local_path);
                self.editing_local_path = false;
                if self.focused_input == Some(SftpInput::LocalPath) {
                    self.set_input_focus(None, cx);
                }
            }
            SftpPane::Remote => {
                self.remote_path_completion.dismiss();
                self.remote_path_input.clone_from(&self.remote_path);
                self.editing_remote_path = false;
                if self.focused_input == Some(SftpInput::RemotePath) {
                    self.set_input_focus(None, cx);
                }
            }
        }
    }

    pub fn start_path_edit(&mut self, pane: SftpPane, cx: &mut Context<Self>) {
        self.active_pane = pane;
        match pane {
            SftpPane::Local => {
                self.local_path_completion.dismiss();
                self.editing_local_path = true;
                self.local_path_input.clone_from(&self.local_path);
                self.set_input_focus(Some(SftpInput::LocalPath), cx);
            }
            SftpPane::Remote => {
                self.remote_path_completion.dismiss();
                self.editing_remote_path = true;
                self.remote_path_input.clone_from(&self.remote_path);
                self.set_input_focus(Some(SftpInput::RemotePath), cx);
            }
        }
    }

    pub fn apply_local_path(&mut self, path: String) -> Result<usize, String> {
        self.local_path_completion.dismiss();
        // Preserve the horizontal breadcrumb position when navigating through a long path.
        self.local_path = path.clone();
        self.local_path_input.clone_from(&path);
        self.editing_local_path = false;
        let listing = self.local_host.list(&path);
        let outcome = listing.as_ref().map(Vec::len).map_err(ToString::to_string);
        self.local_files = local_files_or_error(&path, listing);
        self.local_selected.clear();
        self.local_last_selected = None;
        self.focused_input = None;
        self.clear_context_menu_immediately();
        outcome
    }

    pub fn apply_pair_primary_path(&mut self, path: String) {
        self.completion_selection.remove(&SftpPane::Local);
        self.local_path_completion.dismiss();
        self.local_path = normalize_remote_path(&path);
        self.local_path_input.clone_from(&self.local_path);
        self.editing_local_path = false;
        self.local_selected.clear();
        self.local_last_selected = None;
        self.clear_context_menu_immediately();
        self.pair_primary_loading = true;
    }

    pub fn apply_remote_path(&mut self, path: String) {
        self.remote_path_completion.dismiss();
        self.completion_selection.remove(&SftpPane::Remote);
        // Preserve the horizontal breadcrumb position when navigating through a long path.
        self.remote_path = path.clone();
        self.remote_path_input = path;
        self.editing_remote_path = false;
        self.remote_selected.clear();
        self.remote_last_selected = None;
        self.clear_context_menu_immediately();
    }

    pub fn toggle_sort(&mut self, pane: SftpPane, field: SftpSortField) {
        let (sort_field, sort_direction) = match pane {
            SftpPane::Local => (&mut self.local_sort_field, &mut self.local_sort_direction),
            SftpPane::Remote => (&mut self.remote_sort_field, &mut self.remote_sort_direction),
        };
        if *sort_field == field {
            *sort_direction = match *sort_direction {
                SftpSortDirection::Asc => SftpSortDirection::Desc,
                SftpSortDirection::Desc => SftpSortDirection::Asc,
            };
        } else {
            *sort_field = field;
            *sort_direction = SftpSortDirection::Asc;
        }
    }

    pub(crate) fn ordered_file_names(&self, pane: SftpPane) -> Vec<String> {
        let (files, filter, field, direction) = match pane {
            SftpPane::Local => (
                &self.local_files,
                &self.local_filter,
                self.local_sort_field,
                self.local_sort_direction,
            ),
            SftpPane::Remote => (
                &self.remote_files,
                &self.remote_filter,
                self.remote_sort_field,
                self.remote_sort_direction,
            ),
        };
        sorted_sftp_files(files, filter, field, direction)
            .into_iter()
            .map(|file| file.name)
            .collect()
    }

    pub fn selected_names(&self, pane: SftpPane) -> Vec<String> {
        let selected = match pane {
            SftpPane::Local => &self.local_selected,
            SftpPane::Remote => &self.remote_selected,
        };
        self.ordered_file_names(pane)
            .into_iter()
            .filter(|name| selected.contains(name))
            .collect()
    }

    pub fn single_selected_file(&self, pane: SftpPane) -> Option<SftpFileEntry> {
        let selected = self.selected_names(pane);
        if selected.len() != 1 {
            return None;
        }
        let name = selected.first()?;
        let files = match pane {
            SftpPane::Local => &self.local_files,
            SftpPane::Remote => &self.remote_files,
        };
        files.iter().find(|file| &file.name == name).cloned()
    }

    pub fn clear_selection(&mut self, pane: SftpPane) -> bool {
        match pane {
            SftpPane::Local => {
                let changed = !self.local_selected.is_empty() || self.local_last_selected.is_some();
                self.local_selected.clear();
                self.local_last_selected = None;
                changed
            }
            SftpPane::Remote => {
                let tree_changed = self.tree_selected.take().is_some();
                let changed = tree_changed
                    || !self.remote_selected.is_empty()
                    || self.remote_last_selected.is_some();
                self.remote_selected.clear();
                self.remote_last_selected = None;
                changed
            }
        }
    }

    pub fn select_all_files(&mut self, pane: SftpPane) {
        let names = self.ordered_file_names(pane);
        match pane {
            SftpPane::Local => {
                self.local_selected = names.iter().cloned().collect();
                self.local_last_selected = names.last().cloned();
            }
            SftpPane::Remote => {
                self.remote_selected = names.iter().cloned().collect();
                self.remote_last_selected = names.last().cloned();
            }
        }
    }

    pub fn select_file(&mut self, pane: SftpPane, name: String, modifiers: gpui::Modifiers) {
        self.active_pane = pane;
        if pane == SftpPane::Remote {
            // A filtered flat selection replaces the previously focused tree row.
            self.tree_selected = None;
        }
        self.clear_context_menu_immediately();
        let range_names = self.ordered_file_names(pane);
        let (selected, last_selected) = match pane {
            SftpPane::Local => (&mut self.local_selected, &mut self.local_last_selected),
            SftpPane::Remote => (&mut self.remote_selected, &mut self.remote_last_selected),
        };
        if modifiers.shift
            && let Some(last) = last_selected.as_ref()
            && let (Some(start), Some(end)) = (
                range_names.iter().position(|item| item == last),
                range_names.iter().position(|item| item == &name),
            )
        {
            selected.clear();
            let (min, max) = (start.min(end), start.max(end));
            selected.extend(range_names[min..=max].iter().cloned());
            *last_selected = Some(name);
            return;
        }
        if modifiers.platform || modifiers.control {
            if !selected.insert(name.clone()) {
                selected.remove(&name);
            }
        } else {
            selected.clear();
            selected.insert(name.clone());
        }
        *last_selected = Some(name);
    }

    pub fn move_selection(&mut self, pane: SftpPane, delta: isize) -> bool {
        let names = self.ordered_file_names(pane);
        if names.is_empty() {
            return false;
        }
        let current = self
            .selected_names(pane)
            .first()
            .and_then(|name| names.iter().position(|candidate| candidate == name))
            .unwrap_or(if delta > 0 { names.len() - 1 } else { 0 });
        let next = if delta > 0 {
            (current + 1) % names.len()
        } else if current == 0 {
            names.len() - 1
        } else {
            current - 1
        };
        let name = names[next].clone();
        let selected_names = self.selected_names(pane);
        let last_selected = match pane {
            SftpPane::Local => self.local_last_selected.as_ref(),
            SftpPane::Remote => self.remote_last_selected.as_ref(),
        };
        if selected_names.len() == 1
            && selected_names.first() == Some(&name)
            && last_selected == Some(&name)
        {
            return false;
        }
        match pane {
            SftpPane::Local => {
                self.local_selected.clear();
                self.local_selected.insert(name.clone());
                self.local_last_selected = Some(name);
                scroll_tauri_virtual_list_to_index(
                    &self.local_file_scroll,
                    next,
                    sftp_file_list_virtual_spec(self.file_row_height),
                    TauriVirtualScrollAlign::Nearest,
                );
            }
            SftpPane::Remote => {
                self.remote_selected.clear();
                self.remote_selected.insert(name.clone());
                self.remote_last_selected = Some(name);
                scroll_tauri_virtual_list_to_index(
                    &self.remote_file_scroll,
                    next,
                    sftp_file_list_virtual_spec(self.file_row_height),
                    TauriVirtualScrollAlign::Nearest,
                );
            }
        }
        true
    }

    pub(crate) fn finish_file_drag(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) -> Option<(SftpPane, SftpTransferDirection, Vec<String>)> {
        let drag = self.drag_state.take();
        let had_target = self.drag_over_pane.take().is_some();
        self.stop_drag_autoscroll();
        if had_target || drag.as_ref().is_some_and(|drag| drag.active) {
            cx.notify();
        }
        let drag = drag?;
        if !drag.active || drag.source_pane == pane {
            return None;
        }
        let direction = match drag.source_pane {
            SftpPane::Local => SftpTransferDirection::Upload,
            SftpPane::Remote => SftpTransferDirection::Download,
        };
        Some((drag.source_pane, direction, drag.names))
    }

    pub fn start_drag_candidate(&mut self, pane: SftpPane, x: f32, y: f32) {
        let names = self.selected_names(pane);
        if names.is_empty() {
            self.drag_state = None;
            self.stop_drag_autoscroll();
            return;
        }
        self.drag_state = Some(SftpDragState {
            source_pane: pane,
            names,
            start_x: x,
            start_y: y,
            active: false,
        });
        self.drag_over_pane = None;
        self.stop_drag_autoscroll();
    }

    pub(crate) fn update_drag_activation(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = self.drag_state.as_mut() else {
            return false;
        };
        let dx = x - drag.start_x;
        let dy = y - drag.start_y;
        if !drag.active && (dx * dx + dy * dy).sqrt() >= 5.0 {
            drag.active = true;
        }
        drag.active
    }

    pub fn update_drag(&mut self, pane: SftpPane, x: f32, y: f32) -> bool {
        let Some(was_active) = self.drag_state.as_ref().map(|drag| drag.active) else {
            return false;
        };
        if !self.update_drag_activation(x, y) {
            return false;
        }
        let active_changed = !was_active;
        let pane_changed = self.drag_over_pane != Some(pane);
        if pane_changed {
            self.drag_over_pane = Some(pane);
        }
        active_changed || pane_changed
    }

    pub fn update_drag_capture(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if self.update_drag_activation(f32::from(position.x), f32::from(position.y)) {
            self.drag_autoscroll_position = Some(position);
            let changed = self.apply_drag_autoscroll(position);
            self.schedule_drag_autoscroll(cx);
            changed
        } else {
            self.stop_drag_autoscroll();
            false
        }
    }

    pub(crate) fn schedule_drag_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.drag_autoscroll_scheduled {
            return;
        }
        self.drag_autoscroll_scheduled = true;
        cx.spawn(async move |entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.drag_autoscroll_scheduled = false;
                let Some(position) = sftp.drag_autoscroll_position else {
                    return;
                };
                if !sftp.drag_state.as_ref().is_some_and(|drag| drag.active) {
                    sftp.stop_drag_autoscroll();
                    return;
                }
                if sftp.apply_drag_autoscroll(position) {
                    cx.notify();
                }
                sftp.schedule_drag_autoscroll(cx);
            });
        })
        .detach();
    }

    pub(crate) fn apply_drag_autoscroll(&self, position: Point<Pixels>) -> bool {
        uniform_list_edge_autoscroll(&self.local_file_scroll, position)
            | uniform_list_edge_autoscroll(&self.remote_file_scroll, position)
    }

    pub fn stop_drag_autoscroll(&mut self) {
        self.drag_autoscroll_position = None;
        self.drag_autoscroll_scheduled = false;
    }

    pub fn cancel_drag_capture(&mut self) -> bool {
        let had_drag = self.drag_state.take().is_some();
        let had_target = self.drag_over_pane.take().is_some();
        self.stop_drag_autoscroll();
        had_drag || had_target
    }
}

impl SftpWorkspaceEntity {
    pub fn sftp_tree_data(&self) -> SftpTreeData<'_> {
        SftpTreeData {
            root: &self.remote_files,
            tree_expanded: &self.tree_expanded,
            tree_loading: &self.tree_loading,
            tree_pending: &self.tree_pending,
            tree_children: &self.tree_children,
            remote_sort_field: self.remote_sort_field,
            remote_sort_direction: self.remote_sort_direction,
        }
    }

    pub fn flatten_sftp_tree_rows(&self) -> Vec<SftpTreeRow> {
        self.sftp_tree_data().rows()
    }

    pub fn selected_tree_file(&self) -> Option<SftpFileEntry> {
        let selected = self.tree_selected.as_deref()?;
        self.flatten_sftp_tree_rows()
            .into_iter()
            .find(|row| row.entry.path == selected)
            .map(|row| row.entry)
    }

    pub fn select_tree_file(&mut self, path: String) {
        self.active_pane = SftpPane::Remote;
        self.focused_input = None;
        self.clear_context_menu_immediately();
        self.remote_selected.clear();
        self.remote_last_selected = None;
        self.tree_selected = Some(path);
    }

    pub fn move_tree_selection(&mut self, delta: isize) -> bool {
        let rows = self.flatten_sftp_tree_rows();
        if rows.is_empty() {
            return false;
        }
        let current = rows
            .iter()
            .position(|row| Some(&row.entry.path) == self.tree_selected.as_ref());
        let next = match current {
            Some(index) if delta > 0 => (index + 1) % rows.len(),
            Some(0) | None if delta < 0 => rows.len() - 1,
            Some(index) => index - 1,
            None => 0,
        };
        self.select_tree_file(rows[next].entry.path.clone());
        scroll_tauri_virtual_list_to_index(
            &self.remote_file_scroll,
            next,
            sftp_file_list_virtual_spec(self.file_row_height),
            TauriVirtualScrollAlign::Nearest,
        );
        true
    }

    /// The directory represented by the visible tree focus. A selected file
    /// resolves to its containing directory; stale selections are ignored so
    /// navigation and collapse cannot leave the sync target outside the tree.
    pub fn terminal_sync_path(&self, tree_enabled: bool) -> String {
        if self.tree_mode(tree_enabled)
            && let Some(path) = self.tree_sync_directory_path()
        {
            return path;
        }
        self.remote_path.trim().to_owned()
    }

    fn tree_sync_directory_path(&self) -> Option<String> {
        let file = self.selected_tree_file()?;
        if file.file_type == SftpFileType::Directory {
            Some(file.path)
        } else {
            Some(parent_path(&file.path, true))
        }
    }
}

impl SftpWorkspaceEntity {
    pub(crate) fn browser_list_data(&self, pane: SftpPane) -> SftpFileListData<'_> {
        let (files, filter, sort_field, sort_direction, selected, scroll, loading) = match pane {
            SftpPane::Local => (
                &self.local_files,
                &self.local_filter,
                self.local_sort_field,
                self.local_sort_direction,
                &self.local_selected,
                &self.local_file_scroll,
                self.pair_primary_loading,
            ),
            SftpPane::Remote => (
                &self.remote_files,
                &self.remote_filter,
                self.remote_sort_field,
                self.remote_sort_direction,
                &self.remote_selected,
                &self.remote_file_scroll,
                self.remote_loading,
            ),
        };
        SftpFileListData {
            files,
            filter,
            sort_field,
            sort_direction,
            selected,
            scroll,
            loading,
            compact: self.current_surface_id == Some(SftpSurfaceId::Sidebar),
            drag_over: self.drag_over_pane == Some(pane),
            tree: self.sftp_tree_data(),
            tree_selected: self.tree_selected.as_deref(),
        }
    }
}

pub struct SftpPaneRenderSnapshot {
    pub path: String,
    pub filter: String,
    pub sort_field: SftpSortField,
    pub sort_direction: SftpSortDirection,
    pub selected_count: usize,
    pub path_editing: bool,
    pub path_input: String,
}

pub struct SftpSurfaceRenderSnapshot {
    pub init_error: Option<SftpViewError>,
    pub pane_split_ratio: f32,
    pub active_pane: SftpPane,
    pub drag_over_pane: Option<SftpPane>,
    pub focused_input: Option<SftpInput>,
    pub dialog_open: bool,
    pub context_menu: Option<SftpContextMenu>,
    pub bookmark_menu: Option<SftpBookmarkMenu>,
    pub local: SftpPaneRenderSnapshot,
    pub remote: SftpPaneRenderSnapshot,
}

impl SftpWorkspaceEntity {
    pub fn surface_render_snapshot(&self) -> SftpSurfaceRenderSnapshot {
        SftpSurfaceRenderSnapshot {
            init_error: self.init_error.clone(),
            pane_split_ratio: self.pane_split_ratio,
            active_pane: self.active_pane,
            drag_over_pane: self.drag_over_pane,
            focused_input: self.focused_input,
            dialog_open: self.dialog.is_some(),
            context_menu: self.context_menu.clone(),
            bookmark_menu: self.bookmark_menu,
            local: SftpPaneRenderSnapshot {
                path: self.local_path.clone(),
                filter: self.local_filter.clone(),
                sort_field: self.local_sort_field,
                sort_direction: self.local_sort_direction,
                selected_count: self.local_selected.len(),
                path_editing: self.editing_local_path,
                path_input: self.local_path_input.clone(),
            },
            remote: SftpPaneRenderSnapshot {
                path: self.remote_path.clone(),
                filter: self.remote_filter.clone(),
                sort_field: self.remote_sort_field,
                sort_direction: self.remote_sort_direction,
                selected_count: self.remote_selected.len(),
                path_editing: self.editing_remote_path,
                path_input: self.remote_path_input.clone(),
            },
        }
    }

    pub fn schedule_context_menu_exit(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self.context_menu_exit_generation.take() else {
            return;
        };
        // The browser state owns the exit timer; a generation prevents an old
        // timer from clearing a menu that was opened again.
        cx.spawn(async move |entity, cx| {
            cx.background_executor().timer(delay).await;
            let _ = entity.update(cx, |sftp, cx| {
                if sftp.context_menu_presence.finish_exit(generation) {
                    sftp.context_menu = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
