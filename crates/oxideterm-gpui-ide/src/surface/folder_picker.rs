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
        set_folder_picker_path_input(&mut self.folder_picker, path.clone());
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
                        let current_path = this.folder_picker.current_path.clone();
                        set_folder_picker_path_input(
                            &mut this.folder_picker,
                            current_path,
                        );
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
        self.folder_picker.path_marked_text = None;
        self.folder_picker.path_selection_drag_anchor = None;
        self.open_remote_project(node_id, final_path, cx);
    }

    fn close_folder_picker(&mut self, cx: &mut Context<Self>) {
        let close_transient_surface =
            self.root_path.is_none() && matches!(self.load_state, IdeLoadState::Empty);
        self.folder_picker.open = false;
        self.folder_picker.path_input_focused = false;
        self.folder_picker.path_marked_text = None;
        self.folder_picker.path_selection_drag_anchor = None;
        if close_transient_surface {
            // Folder-picker-only IDE tabs are created before a project exists.
            // Canceling the picker should discard that temporary tab instead of
            // leaving an empty IDE surface that blocks the next open attempt.
            cx.emit(IdeSurfaceEvent::TransientFolderPickerCancelled);
        }
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
        let key = event.keystroke.key.as_str();
        let composing = self.folder_picker.path_marked_text.is_some();
        if (composing && matches!(key, "enter" | "space" | " "))
            || (self.folder_picker.path_input_focused && key_uses_platform_text_input(event))
        {
            // Printable text and IME composition must reach EntityInputHandler.
            return;
        }
        let uses_text_modifier = event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control;
        match key {
            "escape" => self.close_folder_picker(cx),
            "enter" => self.submit_folder_picker_path(cx),
            "a" if self.folder_picker.path_input_focused && uses_text_modifier => {
                let end = self.folder_picker.path_input.encode_utf16().count();
                self.folder_picker.path_marked_text = None;
                self.folder_picker.path_selection_range = Some(0..end);
                self.folder_picker.path_selection_reversed = false;
                cx.notify();
            }
            "c" if self.folder_picker.path_input_focused && uses_text_modifier => {
                if let Some(selected) = folder_picker_selected_text(&self.folder_picker) {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                }
            }
            "x" if self.folder_picker.path_input_focused && uses_text_modifier => {
                if let Some(selected) = folder_picker_selected_text(&self.folder_picker) {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                    replace_folder_picker_selection(&mut self.folder_picker, "");
                    cx.notify();
                }
            }
            "v" if self.folder_picker.path_input_focused && uses_text_modifier => {
                let clipboard_text = cx.read_from_clipboard().and_then(|item| item.text());
                if let Some(text) = clipboard_text {
                    replace_folder_picker_selection(&mut self.folder_picker, &text);
                    cx.notify();
                }
            }
            "backspace" if self.folder_picker.path_input_focused => {
                delete_folder_picker_backward(&mut self.folder_picker);
                cx.notify();
            }
            "delete" if self.folder_picker.path_input_focused => {
                delete_folder_picker_forward(&mut self.folder_picker);
                cx.notify();
            }
            "left" | "arrowleft" if self.folder_picker.path_input_focused => {
                move_folder_picker_caret(
                    &mut self.folder_picker,
                    false,
                    event.keystroke.modifiers.shift,
                );
                cx.notify();
            }
            "right" | "arrowright" if self.folder_picker.path_input_focused => {
                move_folder_picker_caret(
                    &mut self.folder_picker,
                    true,
                    event.keystroke.modifiers.shift,
                );
                cx.notify();
            }
            "home" if self.folder_picker.path_input_focused => {
                move_folder_picker_caret_to(
                    &mut self.folder_picker,
                    0,
                    event.keystroke.modifiers.shift,
                );
                cx.notify();
            }
            "end" if self.folder_picker.path_input_focused => {
                let end = self.folder_picker.path_input.encode_utf16().count();
                move_folder_picker_caret_to(
                    &mut self.folder_picker,
                    end,
                    event.keystroke.modifiers.shift,
                );
                cx.notify();
            }
            _ => {}
        }
        cx.stop_propagation();
    }

    fn update_folder_picker_path_anchor(
        &mut self,
        anchor: TextInputAnchor,
        _cx: &mut Context<Self>,
    ) {
        if self.folder_picker.path_input_anchor.as_ref() != Some(&anchor) {
            // Geometry-only updates support pointer hit-testing without
            // scheduling another render from the layout pass.
            self.folder_picker.path_input_anchor = Some(anchor);
        }
    }

    fn begin_folder_picker_path_selection(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.folder_picker.path_input_focused = true;
        let Some(index) = self.folder_picker_path_index_for_position(event.position, window) else {
            return;
        };
        self.folder_picker.path_marked_text = None;
        if event.click_count >= 2 {
            let end = self.folder_picker.path_input.encode_utf16().count();
            self.folder_picker.path_selection_range = Some(0..end);
            self.folder_picker.path_selection_reversed = false;
            self.folder_picker.path_selection_drag_anchor = None;
        } else {
            let anchor = if event.modifiers.shift {
                folder_picker_selection_anchor(&self.folder_picker)
            } else {
                index
            };
            set_folder_picker_selection_from_anchor(&mut self.folder_picker, anchor, index);
            self.folder_picker.path_selection_drag_anchor = Some(anchor);
        }
        cx.notify();
    }

    fn update_folder_picker_path_selection_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() || !self.folder_picker.open {
            return;
        }
        let Some(anchor) = self.folder_picker.path_selection_drag_anchor else {
            return;
        };
        let Some(index) = self.folder_picker_path_index_for_position(event.position, window) else {
            return;
        };
        let previous_range = self.folder_picker.path_selection_range.clone();
        let previous_reversed = self.folder_picker.path_selection_reversed;
        set_folder_picker_selection_from_anchor(&mut self.folder_picker, anchor, index);
        if previous_range != self.folder_picker.path_selection_range
            || previous_reversed != self.folder_picker.path_selection_reversed
        {
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn finish_folder_picker_path_selection_drag(&mut self) {
        self.folder_picker.path_selection_drag_anchor = None;
    }

    fn folder_picker_path_index_for_position(
        &self,
        position: Point<Pixels>,
        window: &mut Window,
    ) -> Option<usize> {
        let bounds = self.folder_picker.path_input_anchor?.bounds;
        let left = bounds.left() + px(self.tokens.metrics.ui_control_padding_x);
        let right = bounds.right() - px(self.tokens.metrics.ui_control_padding_x);
        let relative_x = (position.x - left).clamp(px(0.0), (right - left).max(px(0.0)));
        let value = &self.folder_picker.path_input;
        if value.is_empty() {
            return Some(0);
        }
        let display = SharedString::from(value.clone());
        let run = TextRun {
            len: display.len(),
            font: font(SharedString::from(
                self.tokens.metrics.markdown_code_font_family,
            )),
            color: rgb(self.tokens.ui.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(
            display,
            px(self.tokens.metrics.ui_text_sm),
            &[run],
            None,
        );
        let mut byte_index = shaped
            .closest_index_for_x(relative_x.clamp(px(0.0), shaped.width))
            .min(value.len());
        while !value.is_char_boundary(byte_index) {
            byte_index = byte_index.saturating_sub(1);
        }
        Some(value[..byte_index].encode_utf16().count())
    }

    fn folder_picker_path_bounds_for_range(
        &self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Bounds<Pixels> {
        let value = self.folder_picker_path_text_with_marked();
        let byte_index = utf16_offset_to_byte(&value, range_utf16.start);
        let prefix = SharedString::from(value[..byte_index].to_string());
        let run = TextRun {
            len: prefix.len(),
            font: font(SharedString::from(
                self.tokens.metrics.markdown_code_font_family,
            )),
            color: rgb(self.tokens.ui.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let prefix_width = window
            .text_system()
            .shape_line(
                prefix,
                px(self.tokens.metrics.ui_text_sm),
                &[run],
                None,
            )
            .width;
        // Candidate windows should follow the composing caret, not the field's
        // left edge, so CJK IME remains usable after moving within the path.
        Bounds {
            origin: gpui::point(
                element_bounds.left()
                    + px(self.tokens.metrics.ui_control_padding_x)
                    + prefix_width,
                element_bounds.bottom(),
            ),
            size: gpui::size(px(1.0), element_bounds.size.height),
        }
    }
}

fn set_folder_picker_path_input(state: &mut FolderPickerState, value: String) {
    let end = value.encode_utf16().count();
    state.path_input = value;
    state.path_selection_range = Some(end..end);
    state.path_selection_reversed = false;
    state.path_selection_drag_anchor = None;
    state.path_marked_text = None;
}

fn replace_folder_picker_selection(state: &mut FolderPickerState, replacement: &str) {
    let end = state.path_input.encode_utf16().count();
    let range = state.path_selection_range.clone().unwrap_or(end..end);
    replace_folder_picker_range(state, range, replacement);
}

fn replace_folder_picker_range(
    state: &mut FolderPickerState,
    range: Range<usize>,
    replacement: &str,
) {
    let start = utf16_offset_to_byte(&state.path_input, range.start);
    let end = utf16_offset_to_byte(&state.path_input, range.end);
    // The folder path is a single-line field even when clipboard text is not.
    let replacement = replacement.replace(['\r', '\n'], "");
    state.path_input.replace_range(start..end, &replacement);
    state.path_marked_text = None;
    let caret = range.start + replacement.encode_utf16().count();
    state.path_selection_range = Some(caret..caret);
    state.path_selection_reversed = false;
    state.path_selection_drag_anchor = None;
}

fn folder_picker_selected_text(state: &FolderPickerState) -> Option<String> {
    let selection = state.path_selection_range.clone()?;
    (selection.start < selection.end).then(|| {
        let start = utf16_offset_to_byte(&state.path_input, selection.start);
        let end = utf16_offset_to_byte(&state.path_input, selection.end);
        state.path_input[start..end].to_string()
    })
}

fn folder_picker_path_text_with_marked(state: &FolderPickerState) -> String {
    let Some(marked) = state.path_marked_text.as_ref() else {
        return state.path_input.clone();
    };
    let mut text = state.path_input.clone();
    let start = utf16_offset_to_byte(&text, marked.replacement_range.start);
    let end = utf16_offset_to_byte(&text, marked.replacement_range.end);
    text.replace_range(start..end, &marked.text);
    text
}

fn delete_folder_picker_backward(state: &mut FolderPickerState) {
    let end = state.path_input.encode_utf16().count();
    let selection = state.path_selection_range.clone().unwrap_or(end..end);
    if selection.start < selection.end {
        replace_folder_picker_range(state, selection, "");
        return;
    }
    let byte = utf16_offset_to_byte(&state.path_input, selection.start);
    let previous = state.path_input[..byte]
        .graphemes(true)
        .next_back()
        .map_or(selection.start, |grapheme| {
            selection.start.saturating_sub(grapheme.encode_utf16().count())
        });
    replace_folder_picker_range(state, previous..selection.start, "");
}

fn delete_folder_picker_forward(state: &mut FolderPickerState) {
    let end = state.path_input.encode_utf16().count();
    let selection = state.path_selection_range.clone().unwrap_or(end..end);
    if selection.start < selection.end {
        replace_folder_picker_range(state, selection, "");
        return;
    }
    let byte = utf16_offset_to_byte(&state.path_input, selection.end);
    let next = state.path_input[byte..]
        .graphemes(true)
        .next()
        .map_or(selection.end, |grapheme| {
            selection.end + grapheme.encode_utf16().count()
        });
    replace_folder_picker_range(state, selection.end..next, "");
}

fn move_folder_picker_caret(state: &mut FolderPickerState, forward: bool, extend: bool) {
    let end = state.path_input.encode_utf16().count();
    let selection = state.path_selection_range.clone().unwrap_or(end..end);
    let focus = folder_picker_selection_focus(state);
    let next = if !extend && selection.start < selection.end {
        if forward {
            selection.end
        } else {
            selection.start
        }
    } else if forward {
        let byte = utf16_offset_to_byte(&state.path_input, focus);
        state.path_input[byte..]
            .graphemes(true)
            .next()
            .map_or(focus, |grapheme| focus + grapheme.encode_utf16().count())
    } else {
        let byte = utf16_offset_to_byte(&state.path_input, focus);
        state.path_input[..byte]
            .graphemes(true)
            .next_back()
            .map_or(focus, |grapheme| {
                focus.saturating_sub(grapheme.encode_utf16().count())
            })
    };
    move_folder_picker_caret_to(state, next, extend);
}

fn move_folder_picker_caret_to(state: &mut FolderPickerState, target: usize, extend: bool) {
    let end = state.path_input.encode_utf16().count();
    let target = target.min(end);
    state.path_marked_text = None;
    state.path_selection_drag_anchor = None;
    if extend {
        let anchor = folder_picker_selection_anchor(state);
        set_folder_picker_selection_from_anchor(state, anchor, target);
    } else {
        state.path_selection_range = Some(target..target);
        state.path_selection_reversed = false;
    }
}

fn folder_picker_selection_anchor(state: &FolderPickerState) -> usize {
    let end = state.path_input.encode_utf16().count();
    let selection = state.path_selection_range.clone().unwrap_or(end..end);
    if state.path_selection_reversed {
        selection.end
    } else {
        selection.start
    }
}

fn folder_picker_selection_focus(state: &FolderPickerState) -> usize {
    let end = state.path_input.encode_utf16().count();
    let selection = state.path_selection_range.clone().unwrap_or(end..end);
    if state.path_selection_reversed {
        selection.start
    } else {
        selection.end
    }
}

fn set_folder_picker_selection_from_anchor(
    state: &mut FolderPickerState,
    anchor: usize,
    focus: usize,
) {
    if focus < anchor {
        state.path_selection_range = Some(focus..anchor);
        state.path_selection_reversed = true;
    } else {
        state.path_selection_range = Some(anchor..focus);
        state.path_selection_reversed = false;
    }
}

#[cfg(test)]
mod folder_picker_input_tests {
    use super::*;

    fn path_input(value: &str, selection: Range<usize>) -> FolderPickerState {
        FolderPickerState {
            path_input: value.to_string(),
            path_selection_range: Some(selection),
            ..FolderPickerState::default()
        }
    }

    #[test]
    fn replacement_uses_utf16_selection_boundaries() {
        let mut state = path_input("/目录/🚀/file", 4..6);

        replace_folder_picker_selection(&mut state, "项目");

        assert_eq!(state.path_input, "/目录/项目/file");
        assert_eq!(state.path_selection_range, Some(6..6));
    }

    #[test]
    fn backward_delete_removes_one_unicode_grapheme() {
        let mut state = path_input("/目录/🚀", 6..6);

        delete_folder_picker_backward(&mut state);

        assert_eq!(state.path_input, "/目录/");
        assert_eq!(state.path_selection_range, Some(4..4));
    }

    #[test]
    fn shift_navigation_preserves_anchor_and_direction() {
        let mut state = path_input("/a🚀b", 5..5);

        move_folder_picker_caret(&mut state, false, true);
        move_folder_picker_caret(&mut state, false, true);
        move_folder_picker_caret(&mut state, false, true);

        assert_eq!(state.path_selection_range, Some(1..5));
        assert!(state.path_selection_reversed);
        assert_eq!(
            folder_picker_selected_text(&state),
            Some("a🚀b".to_string())
        );
    }

    #[test]
    fn navigation_treats_joined_emoji_as_one_grapheme() {
        let value = "/👨‍👩‍👧/file";
        let end_of_emoji = "/👨‍👩‍👧".encode_utf16().count();
        let mut state = path_input(value, end_of_emoji..end_of_emoji);

        move_folder_picker_caret(&mut state, false, false);

        assert_eq!(state.path_selection_range, Some(1..1));
    }

    #[test]
    fn pasted_path_remains_single_line() {
        let mut state = path_input("/root/", 6..6);

        replace_folder_picker_selection(&mut state, "folder\r\nchild");

        assert_eq!(state.path_input, "/root/folderchild");
    }

    #[test]
    fn marked_text_replaces_selection_without_committing_the_path() {
        let mut state = path_input("/目录/old", 4..7);
        state.path_marked_text = Some(IdeMarkedText {
            replacement_range: 4..7,
            text: "项目".to_string(),
        });

        assert_eq!(folder_picker_path_text_with_marked(&state), "/目录/项目");
        assert_eq!(state.path_input, "/目录/old");
    }
}
