impl WorkspaceApp {
    fn open_sftp_context_menu(
        &mut self,
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        x: f32,
        y: f32,
    ) {
        self.sftp_view.active_pane = pane;
        if let Some(file) = file.as_ref() {
            let selected = match pane {
                SftpPane::Local => &mut self.sftp_view.local_selected,
                SftpPane::Remote => &mut self.sftp_view.remote_selected,
            };
            if !selected.contains(&file.name) {
                selected.clear();
                selected.insert(file.name.clone());
                match pane {
                    SftpPane::Local => self.sftp_view.local_last_selected = Some(file.name.clone()),
                    SftpPane::Remote => {
                        self.sftp_view.remote_last_selected = Some(file.name.clone())
                    }
                }
            }
        }
        self.sftp_view.context_menu = Some(SftpContextMenu { pane, file, x, y });
    }

    fn open_sftp_rename_dialog(&mut self, pane: SftpPane, old_name: String) {
        self.sftp_view.dialog_value = old_name.clone();
        self.sftp_view.dialog = Some(SftpDialog::Rename { pane, old_name });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn open_sftp_new_folder_dialog(&mut self, pane: SftpPane) {
        self.sftp_view.dialog_value.clear();
        self.sftp_view.dialog = Some(SftpDialog::NewFolder { pane });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn queue_sftp_transfers(&mut self, pane: SftpPane, direction: SftpTransferDirection) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let selected = match pane {
            SftpPane::Local => self.sftp_view.local_selected.clone(),
            SftpPane::Remote => self.sftp_view.remote_selected.clone(),
        };
        if selected.is_empty() {
            return;
        }
        let source_files = match pane {
            SftpPane::Local => self.sftp_view.local_files.clone(),
            SftpPane::Remote => self.sftp_view.remote_files.clone(),
        };
        let pending_transfers = selected
            .into_iter()
            .filter_map(|name| {
                source_files
                    .iter()
                    .find(|file| file.name == name)
                    .cloned()
                    .map(|source| SftpPendingTransfer {
                        name,
                        direction,
                        source,
                    })
            })
            .collect::<Vec<_>>();
        if pending_transfers.is_empty() {
            return;
        }

        let target_files = self.sftp_target_files_for_direction(direction);
        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let conflicts = sftp_transfer_conflicts(&pending_transfers, &target_files);
        if !conflicts.is_empty() && conflict_action == oxideterm_settings::ConflictAction::Ask {
            self.sftp_view.conflict_state = Some(SftpConflictState {
                conflicts,
                current_index: 0,
                pending_transfers,
                resolved_actions: HashMap::new(),
                apply_to_all: false,
            });
            self.sftp_view.dialog = Some(SftpDialog::Conflict);
            self.sftp_view.context_menu = None;
            self.clear_sftp_selection(pane);
            return;
        }

        let resolved_actions = conflicts
            .into_iter()
            .map(|conflict| {
                (
                    conflict.file_name,
                    sftp_conflict_resolution_from_settings(conflict_action),
                )
            })
            .collect::<HashMap<_, _>>();
        self.execute_sftp_pending_transfers(node_id, pending_transfers, resolved_actions);
        self.clear_sftp_selection(pane);
    }

    fn sftp_target_files_for_direction(&self, direction: SftpTransferDirection) -> Vec<SftpFileEntry> {
        match direction {
            SftpTransferDirection::Upload => self.sftp_view.remote_files.clone(),
            SftpTransferDirection::Download => self.sftp_view.local_files.clone(),
        }
    }

    fn execute_sftp_pending_transfers(
        &mut self,
        node_id: NodeId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
    ) {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return;
        };
        let target_files = self.sftp_target_files_for_direction(direction);
        for transfer in pending_transfers {
            let resolution = resolved_actions.get(&transfer.name).copied();
            if resolution == Some(SftpConflictResolution::Skip) {
                continue;
            }
            if resolution == Some(SftpConflictResolution::SkipOlder)
                && sftp_source_not_newer_than_target(&transfer, &target_files)
            {
                continue;
            }
            let target_name = if resolution == Some(SftpConflictResolution::Rename) {
                unique_sftp_conflict_name(&transfer.name, &target_files)
            } else {
                transfer.name.clone()
            };
            self.queue_sftp_pending_transfer(node_id.clone(), transfer, target_name);
        }
    }

    fn queue_sftp_pending_transfer(
        &mut self,
        node_id: NodeId,
        transfer: SftpPendingTransfer,
        target_name: String,
    ) {
        let direction = transfer.direction;
        let is_directory = transfer.source.file_type == SftpFileType::Directory;
        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;
        let transfer_id = id.to_string();
        let size = transfer.source.size.max(1);
        let local_path = match direction {
            SftpTransferDirection::Upload => transfer.source.path.clone(),
            SftpTransferDirection::Download => join_local_path(&self.sftp_view.local_path, &target_name),
        };
        let remote_path = match direction {
            SftpTransferDirection::Upload => join_sftp_path(&self.sftp_view.remote_path, &target_name),
            SftpTransferDirection::Download => transfer.source.path.clone(),
        };
        self.sftp_view.transfers.push(SftpTransferItem {
            id,
            transfer_id: transfer_id.clone(),
            name: if is_directory {
                format!("{target_name}/")
            } else {
                target_name
            },
            local_path: local_path.clone(),
            remote_path: remote_path.clone(),
            direction,
            size,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        });
        self.spawn_sftp_transfer_task(
            id,
            transfer_id,
            node_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            None,
        );
    }

    fn toggle_sftp_conflict_apply_all(&mut self) {
        if let Some(conflict) = self.sftp_view.conflict_state.as_mut() {
            conflict.apply_to_all = !conflict.apply_to_all;
        }
    }

    fn resolve_sftp_transfer_conflict(&mut self, resolution: SftpConflictResolution) {
        let Some(mut conflict_state) = self.sftp_view.conflict_state.clone() else {
            return;
        };
        let Some(tab_id) = self.active_tab_id else {
            self.cancel_sftp_transfer_conflicts();
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            self.cancel_sftp_transfer_conflicts();
            return;
        };
        if conflict_state.conflicts.is_empty() {
            self.cancel_sftp_transfer_conflicts();
            return;
        }

        let current_index = conflict_state.current_index;
        if conflict_state.apply_to_all {
            for conflict in conflict_state.conflicts.iter().skip(current_index) {
                conflict_state
                    .resolved_actions
                    .insert(conflict.file_name.clone(), resolution);
            }
            self.sftp_view.conflict_state = None;
            self.sftp_view.dialog = None;
            self.execute_sftp_pending_transfers(
                node_id,
                conflict_state.pending_transfers,
                conflict_state.resolved_actions,
            );
            return;
        }

        if let Some(conflict) = conflict_state.conflicts.get(current_index) {
            conflict_state
                .resolved_actions
                .insert(conflict.file_name.clone(), resolution);
        }

        if current_index + 1 < conflict_state.conflicts.len() {
            conflict_state.current_index += 1;
            conflict_state.apply_to_all = false;
            self.sftp_view.conflict_state = Some(conflict_state);
            self.sftp_view.dialog = Some(SftpDialog::Conflict);
        } else {
            self.sftp_view.conflict_state = None;
            self.sftp_view.dialog = None;
            self.execute_sftp_pending_transfers(
                node_id,
                conflict_state.pending_transfers,
                conflict_state.resolved_actions,
            );
        }
    }

    fn cancel_sftp_transfer_conflicts(&mut self) {
        self.sftp_view.conflict_state = None;
        self.close_sftp_dialog();
    }
}
