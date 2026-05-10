impl IdeSurface {
    fn request_open_folder_picker(&mut self, cx: &mut Context<Self>) {
        self.sync_all_editors(cx);
        if self.workspace.has_dirty_buffers() {
            self.folder_switch_confirm_open = true;
            cx.notify();
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let initial_path = self.root_path.clone().unwrap_or_else(|| "/".to_string());
        self.open_remote_folder_picker_for_node(node_id, initial_path, cx);
    }

    fn load_folder_picker_current(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let path = self.folder_picker.current_path.clone();
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn load_folder_picker_path(
        &mut self,
        node_id: String,
        path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let path = normalize_remote_path(&path.into());
        self.folder_picker.open = true;
        self.folder_picker.node_id = Some(node_id.clone());
        self.folder_picker.current_path = path.clone();
        self.folder_picker.path_input = path.clone();
        self.folder_picker.loading = true;
        self.folder_picker.error = None;
        self.folder_picker.selected_folder = None;
        self.folder_picker.generation = self.folder_picker.generation.wrapping_add(1);
        let generation = self.folder_picker.generation;
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let path_for_task = path.clone();
            let result = await_ide_backend(backend_runtime.spawn(async move {
                let location = IdeLocation::remote(node_id, path_for_task);
                fs.list_dir(&location).await.map(folder_picker_dirs)
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                // The Tauri dialog resets async state on every path change. The
                // generation guard gives GPUI the same observable behavior when
                // an older SFTP list returns after a newer navigation request.
                if this.folder_picker.generation != generation {
                    return;
                }
                this.folder_picker.loading = false;
                match result {
                    Ok(folders) => {
                        this.folder_picker.error = None;
                        this.folder_picker.current_path = path;
                        this.folder_picker.path_input = this.folder_picker.current_path.clone();
                        this.folder_picker.folders = folders;
                        this.folder_picker.selected_folder = None;
                    }
                    Err(error) => this.folder_picker.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn enter_folder_picker_folder(&mut self, folder_name: &str, cx: &mut Context<Self>) {
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let path = join_remote_child(&self.folder_picker.current_path, folder_name);
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn go_folder_picker_parent(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.current_path == "/" || self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let parent = parent_remote_path(&self.folder_picker.current_path);
        self.load_folder_picker_path(node_id, parent, cx);
    }

    fn go_folder_picker_home(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        self.load_folder_picker_path(node_id, "/", cx);
    }

    fn submit_folder_picker_path(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let path = self.folder_picker.path_input.trim().to_string();
        if path.is_empty() {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn selected_folder_picker_path(&self) -> String {
        match self.folder_picker.selected_folder.as_deref() {
            Some(name) => join_remote_child(&self.folder_picker.current_path, name),
            None => self.folder_picker.current_path.clone(),
        }
    }

    fn confirm_folder_picker(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let final_path = self.selected_folder_picker_path();
        self.folder_picker.open = false;
        self.folder_picker.path_input_focused = false;
        self.open_remote_project(node_id, final_path, cx);
    }

    fn close_folder_picker(&mut self, cx: &mut Context<Self>) {
        self.folder_picker.open = false;
        self.folder_picker.path_input_focused = false;
        cx.notify();
    }

    fn handle_folder_picker_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.folder_picker.open {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_folder_picker(cx),
            "enter" => self.submit_folder_picker_path(cx),
            "backspace" if self.folder_picker.path_input_focused => {
                self.folder_picker.path_input.pop();
                cx.notify();
            }
            _ if self.folder_picker.path_input_focused => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !text.is_empty()
                    && !text.chars().any(char::is_control)
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                {
                    self.folder_picker.path_input.push_str(text);
                    cx.notify();
                }
            }
            _ => {}
        }
        cx.stop_propagation();
    }

}
