use oxideterm_gpui_editor::{EditorKeybindings, EditorShortcut};
use oxideterm_gpui_markdown::{MarkdownDocument, MarkdownOptions, MarkdownVirtualListScrollHandle};
use oxideterm_gpui_ui::button::{IconButtonOptions, icon_button};

#[derive(Default)]
struct MarkdownPreviewState {
    editing: bool,
    source: Option<Arc<str>>,
    document: Option<MarkdownDocument>,
    selection: Option<MarkdownPreviewSelection>,
    source_version: Option<u64>,
    suspended_selection: Option<(usize, usize)>,
    scroll: MarkdownVirtualListScrollHandle,
    // The file tab owns parsing; closing it cancels delivery to the surface.
    parse_task: Option<Task<()>>,
}

impl MarkdownPreviewState {
    fn release_cache(&mut self) {
        self.parse_task = None;
        self.source = None;
        self.document = None;
        if let Some(selection) = self.selection.take() {
            self.suspended_selection = Some((selection.anchor, selection.head));
        }
        self.scroll.release_document_cache();
    }
}

impl IdeSurface {
    fn sync_tab_render_caches(&mut self, cx: &mut Context<Self>) {
        let active = self
            .workspace
            .active_tab()
            .filter(|_| self.mount.is_visible());
        let preview = self.is_markdown_preview();
        for (&tab, state) in &mut self.markdown_previews {
            if active != Some(tab) || state.editing {
                state.release_cache();
            }
        }
        for (&tab, editor) in &self.editors {
            let visible = active == Some(tab);
            editor.update(cx, |editor, cx| {
                editor.set_render_cache_active(visible && !preview, cx);
                if !visible {
                    editor.buffer().release_text_cache();
                }
            });
        }
    }

    fn active_markdown_tab(&self) -> Option<EditorTabId> {
        self.workspace
            .active_tab()
            .filter(|tab_id| self.is_markdown_tab(*tab_id))
    }

    fn is_markdown_tab(&self, tab_id: EditorTabId) -> bool {
        let Some(tab) = self.workspace.tabs().iter().find(|tab| tab.id == tab_id) else {
            return false;
        };
        let path = match &tab.location {
            IdeLocation::Local { path } => path.as_path(),
            IdeLocation::Remote { path, .. } => Path::new(path),
        };
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    }

    fn is_markdown_preview(&self) -> bool {
        self.active_markdown_tab().is_some_and(|tab_id| {
            !self
                .markdown_previews
                .get(&tab_id)
                .is_some_and(|state| state.editing)
        })
    }

    fn show_markdown_source(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        if self.is_markdown_tab(tab_id) {
            self.markdown_previews.entry(tab_id).or_default().editing = true;
            self.sync_tab_render_caches(cx);
        }
    }

    fn remove_editor(&mut self, tab_id: EditorTabId) {
        self.editors.remove(&tab_id);
        self.markdown_previews.remove(&tab_id);
    }

    fn toggle_markdown_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_markdown_selection();
        let Some(tab_id) = self.active_markdown_tab() else {
            return;
        };
        let Some(editor) = self.active_editor() else {
            return;
        };
        let editing = self.is_markdown_preview();
        self.sync_editor_to_workspace(tab_id, cx);
        self.markdown_previews.entry(tab_id).or_default().editing = editing;
        self.sync_tab_render_caches(cx);
        if editing {
            window.focus(&editor.focus_handle(cx), cx);
        } else {
            // The hidden editor must no longer receive text, IME, or command events.
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    fn markdown_options(&self) -> MarkdownOptions {
        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.background_surface_active = self.runtime_settings.background_active;
        options.html_details_label = self.labels.markdown_details.clone();
        options.mermaid_error_prefix = self.labels.markdown_mermaid_error.clone();
        options.mermaid_loading_label = self.labels.markdown_mermaid_loading.clone();
        // Remote paths must never resolve against the client filesystem.
        options.allowed_link_schemes = vec!["http", "https", "mailto"];
        options.allowed_image_schemes = vec!["http", "https", "data"];
        options
    }

    fn render_markdown_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        let preview = self.is_markdown_preview();
        let label = if preview {
            &self.labels.markdown_edit
        } else {
            &self.labels.markdown_preview
        }
        .clone();
        let tokens = self.tokens;
        div()
            .flex_none()
            .px_1()
            .child(
                icon_button(
                    &tokens,
                    self.icon(
                        if preview {
                            "lucide/pencil.svg"
                        } else {
                            "lucide/eye.svg"
                        },
                        IDE_TREE_TOOLBAR_ICON_SIZE,
                        tokens.ui.text_muted,
                    ),
                    IconButtonOptions::compact(IDE_TREE_TOOLBAR_BUTTON_SIZE),
                )
                .id("ide-markdown-toggle")
                .debug_selector(|| "ide-markdown-toggle".into())
                .tooltip(move |_, cx| {
                    oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.toggle_markdown_mode(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn prepare_markdown_preview(
        &mut self,
        tab_id: EditorTabId,
        source: Arc<str>,
        cx: &mut Context<Self>,
    ) {
        if !self.update_markdown_source(tab_id, &source, cx) {
            return;
        }
        let parse_source = source.clone();
        let parse = cx
            .background_executor()
            .spawn(async move { oxideterm_gpui_markdown::parser::parse(&parse_source) });
        let task = cx.spawn(async move |weak, cx| {
            let document = parse.await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .markdown_previews
                    .get(&tab_id)
                    .and_then(|state| state.source.as_ref())
                    .is_some_and(|current| Arc::ptr_eq(current, &source))
                {
                    this.install_markdown_document(tab_id, document);
                    cx.notify();
                }
            });
        });
        self.markdown_previews.get_mut(&tab_id).unwrap().parse_task = Some(task);
    }

    fn update_markdown_source(
        &mut self,
        tab_id: EditorTabId,
        source: &Arc<str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let version = self.editors[&tab_id].read(cx).buffer().version();
        let state = self.markdown_previews.entry(tab_id).or_default();
        if state
            .source
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, source))
        {
            return false;
        }
        if state.source_version != Some(version) {
            state.suspended_selection = None;
            state.scroll.invalidate_document_cache();
        }
        state.source_version = Some(version);
        state.source = Some(source.clone());
        state.document = None;
        state.selection = None;
        state.parse_task = None;
        true
    }

    fn ensure_markdown_selection(&mut self, tab: EditorTabId, cx: &mut Context<Self>) {
        let source = self.editors[&tab].read(cx).buffer().text_snapshot();
        self.update_markdown_source(tab, &source, cx);
        if self.markdown_previews[&tab].selection.is_some() {
            return;
        }
        // Copy/select-all must work even before the first resumed frame. Only
        // explicit clipboard commands synchronously rebuild a cold preview.
        let document = oxideterm_gpui_markdown::parser::parse(&source);
        self.install_markdown_document(tab, document);
    }

    fn install_markdown_document(&mut self, tab: EditorTabId, document: MarkdownDocument) {
        let mut selection = MarkdownPreviewSelection::new(&document, &self.markdown_options());
        let state = self.markdown_previews.get_mut(&tab).unwrap();
        state.parse_task = None;
        if let Some((anchor, head)) = state.suspended_selection.take() {
            selection.anchor = anchor;
            selection.head = head;
        }
        state.selection = Some(selection);
        state.document = Some(document);
    }

    fn render_markdown_preview(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tab_id = self.active_markdown_tab().expect("markdown tab selected");
        let source = self.editors[&tab_id].read(cx).buffer().text_snapshot();
        self.prepare_markdown_preview(tab_id, source, cx);
        let options = self.markdown_options();
        let state = &self.markdown_previews[&tab_id];
        if let Some(selection) = &state.selection {
            selection.layouts.borrow_mut().clear();
        }
        let Some(document) = &state.document else {
            return self.render_loading_file();
        };
        div()
            .debug_selector(|| "ide-markdown-preview".into())
            .size_full()
            .p_4()
            .child(
                oxideterm_gpui_markdown::render::render_document_virtual_selectable(
                    SharedString::from(format!("ide-markdown-preview-{}", tab_id.0)),
                    document,
                    &self.tokens,
                    &options,
                    &state.scroll,
                    None,
                    &mut |key, text, runs, links| {
                        self.render_markdown_fragment(tab_id, key, text, runs, links, cx)
                    },
                ),
            )
            .into_any_element()
    }

    fn handle_markdown_preview_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.is_markdown_preview() {
            return false;
        }
        let shortcut = if let Some(bindings) = cx.try_global::<EditorKeybindings>() {
            bindings.resolve(&event.keystroke)
        } else {
            let modifiers = event.keystroke.modifiers;
            let command = if cfg!(target_os = "macos") {
                modifiers.platform
            } else {
                modifiers.control
            };
            if command && !modifiers.shift && !modifiers.alt {
                match event.keystroke.key.to_ascii_lowercase().as_str() {
                    "s" => Some(EditorShortcut::Save),
                    "c" => Some(EditorShortcut::Copy),
                    "a" => Some(EditorShortcut::SelectAll),
                    "x" => Some(EditorShortcut::Cut),
                    "v" => Some(EditorShortcut::Paste),
                    _ => None,
                }
            } else {
                None
            }
        };
        match shortcut {
            Some(EditorShortcut::Copy) => self.copy_markdown_selection(cx),
            Some(EditorShortcut::SelectAll) => self.select_all_markdown(cx),
            Some(EditorShortcut::Cut | EditorShortcut::Paste) => {}
            _ => {
                if shortcut != Some(EditorShortcut::Save) {
                    return false;
                }
            }
        }
        if shortcut == Some(EditorShortcut::Save) {
            if let Some(tab_id) = self.workspace.active_tab() {
                self.sync_editor_to_workspace(tab_id, cx);
                self.save_tab(tab_id, cx);
            }
            cx.stop_propagation();
            return true;
        }
        cx.stop_propagation();
        true
    }
}

#[cfg(test)]
mod markdown_preview_tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_gpui_markdown::model::{Block, Inline};
    use oxideterm_ssh::{NodeRouter, SshConnectionRegistry};

    fn ready_surface(cx: &mut Context<IdeSurface>) -> IdeSurface {
        let mut surface = IdeSurface::new(
            NodeAgentIdeFileSystem::new(
                NodeRouter::new(SshConnectionRegistry::default()),
                NodeAgentMode::Disabled,
            ),
            oxideterm_theme::default_tokens(),
            IdeLabels::default(),
            IdeRuntimeSettings::default(),
            Arc::new(tokio::runtime::Runtime::new().unwrap()),
            cx,
        );
        surface
            .workspace
            .open_project(IdeLocation::remote("test", "/repo"), "repo");
        surface.node_id = Some("test".into());
        surface.root_path = Some("/repo".into());
        surface.load_state = IdeLoadState::Ready;
        for name in ["README.MD", "second.md", "plain.txt"] {
            let location = IdeLocation::remote("test", format!("/repo/{name}"));
            surface
                .workspace
                .open_file(location.clone(), "# Original", SavedFileVersion::unknown())
                .unwrap();
            let tab = surface.workspace.active_tab().unwrap();
            surface.create_editor(tab, &location, "# Original".into(), cx);
        }
        let first = surface.workspace.tabs()[0].id;
        surface.workspace.set_active_tab(first).unwrap();
        surface.set_mount(IdeSurfaceMount::MainWindow, cx);
        surface
    }

    #[gpui::test]
    fn markdown_preview_copies_rendered_text_without_editing_source(cx: &mut TestAppContext) {
        let (surface, cx) = cx.add_window_view(|_, cx| ready_surface(cx));
        let source = "# Title\n\nAlpha **bold** 与🙂.\n\nNext line.\n\n```sh\nprintf ok\n```";
        surface.update(cx, |surface, cx| {
            surface
                .active_editor()
                .unwrap()
                .update(cx, |editor, cx| editor.replace_text_external(source, cx));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (start, end) = surface.read_with(cx, |surface, _| {
            let tab = surface.workspace.active_tab().unwrap();
            let selection = surface.markdown_previews[&tab].selection.as_ref().unwrap();
            let layouts = selection.layouts.borrow();
            let first = &layouts
                .iter()
                .find(|(range, _)| range.start == "Title\n".len())
                .unwrap()
                .1;
            let second = &layouts
                .iter()
                .find(|(range, _)| range.start == "Title\nAlpha bold 与🙂.\n".len())
                .unwrap()
                .1;
            (
                first.position_for_index(0).unwrap()
                    + gpui::point(px(1.0), first.line_height() / 2.0),
                second.position_for_index("Next line.".len()).unwrap()
                    + gpui::point(px(8.0), second.line_height() / 2.0),
            )
        });
        cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        });
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Alpha bold 与🙂.\nNext line.")
        );
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-a cmd-c"
        } else {
            "ctrl-a ctrl-c"
        });
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Title\nAlpha bold 与🙂.\nNext line.\nprintf ok")
        );
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-x cmd-v"
        } else {
            "ctrl-x ctrl-v"
        });
        cx.simulate_input("must not edit");
        surface.update(cx, |surface, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("previous clipboard".into()));
            surface.select_all_in_active_editor(cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Title\nAlpha bold 与🙂.\nNext line.\nprintf ok")
            );
            assert_eq!(
                surface
                    .active_editor()
                    .unwrap()
                    .read(cx)
                    .buffer()
                    .text_snapshot()
                    .as_ref(),
                source
            );
        });
    }

    #[gpui::test]
    fn markdown_preview_selection_spans_virtual_rows_and_stays_with_its_tab(
        cx: &mut TestAppContext,
    ) {
        let (surface, cx) = cx.add_window_view(|_, cx| ready_surface(cx));
        let source = format!("{}Last paragraph.", "**Visible text**\n\n".repeat(120));
        let expected = format!("{}Last paragraph.", "Visible text\n".repeat(120));
        let (first, second) = surface.update(cx, |surface, cx| {
            surface
                .active_editor()
                .unwrap()
                .update(cx, |editor, cx| editor.replace_text_external(source, cx));
            (
                surface.workspace.tabs()[0].id,
                surface.workspace.tabs()[1].id,
            )
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.update(cx, |surface, cx| {
            surface.select_all_in_active_editor(cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected.as_str())
            );
            surface.markdown_previews[&first]
                .scroll
                .set_offset(gpui::point(px(0.0), px(-1400.0)));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.update(cx, |surface, cx| {
            let selection = surface.markdown_previews[&first]
                .selection
                .as_ref()
                .unwrap();
            assert!(
                selection.layouts.borrow().len() < selection.fragments.len(),
                "keep virtual rendering"
            );
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected.as_str())
            );
            surface.activate_tab(second, cx);
            let state = &surface.markdown_previews[&first];
            assert!(state.document.is_none());
            assert!(state.source.is_none());
            assert!(state.selection.is_none());
            assert_eq!(state.scroll.offset().y, px(-1400.0));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.update(cx, |surface, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("unchanged")
            );
            surface.select_all_in_active_editor(cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Original")
            );
            surface.activate_tab(first, cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected.as_str())
            );
            assert_eq!(
                surface.markdown_previews[&first].scroll.offset().y,
                px(-1400.0)
            );
            surface.set_mount(IdeSurfaceMount::Hidden, cx);
            assert!(surface.markdown_previews[&first].document.is_none());
            surface.set_mount(IdeSurfaceMount::DetachedWindow, cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected.as_str())
            );
            surface.active_editor().unwrap().update(cx, |editor, cx| {
                editor.replace_text_external("# Updated", cx)
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.update(cx, |surface, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("new revision".into()));
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("new revision")
            );
            surface.select_all_in_active_editor(cx);
            surface.copy_active_editor_selection(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Updated")
            );
        });
    }

    #[gpui::test]
    fn markdown_preview_preserves_unsaved_text_undo_focus_and_tab_modes(cx: &mut TestAppContext) {
        let (surface, cx) = cx.add_window_view(|_, cx| ready_surface(cx));
        let first = surface.read_with(cx, |surface, _| surface.workspace.tabs()[0].id);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let preview_bounds = cx.debug_bounds("ide-markdown-preview").unwrap();
        let toggle_bounds = cx.debug_bounds("ide-markdown-toggle").unwrap();
        assert!(preview_bounds.size.width > px(0.0));
        cx.simulate_click(toggle_bounds.center(), gpui::Modifiers::default());
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                assert!(!surface.is_markdown_preview());
                assert!(surface.markdown_previews[&first].document.is_none());
                let editor = surface.active_editor().unwrap();
                assert!(editor.focus_handle(cx).is_focused(window));
                editor.update(cx, |editor, cx| {
                    editor.insert_text("changed\n", cx);
                    editor.select_all(cx);
                });
                surface.toggle_markdown_mode(window, cx);
                assert!(surface.focus_handle.is_focused(window));
                assert_eq!(
                    surface.workspace.buffer(first).unwrap().text.to_string(),
                    "changed\n# Original"
                );
                assert!(surface.is_tab_dirty(first, cx));
                assert_eq!(
                    surface
                        .workspace
                        .buffer(first)
                        .unwrap()
                        .saved_text
                        .to_string(),
                    "# Original"
                );
                cx.write_to_clipboard(ClipboardItem::new_string("must not paste".into()));
                surface.select_all_in_active_editor(cx);
                surface.cut_active_editor_selection(cx);
                surface.paste_into_active_editor(cx);
                assert_eq!(
                    editor.read(cx).buffer().text_snapshot().as_ref(),
                    "changed\n# Original"
                );
                let second = surface.workspace.tabs()[1].id;
                surface.activate_tab(second, cx);
                assert!(surface.is_markdown_preview());
                surface.toggle_markdown_mode(window, cx);
                surface.activate_tab(first, cx);
                assert!(surface.is_markdown_preview());
                surface.toggle_markdown_mode(window, cx);
                assert!(surface.copy_active_editor_selection(cx));
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("changed\n# Original")
                );
                editor.update(cx, |editor, cx| editor.undo_external(cx));
                assert_eq!(
                    editor.read(cx).buffer().text_snapshot().as_ref(),
                    "# Original"
                );
                surface.activate_tab(second, cx);
                assert!(!surface.is_markdown_preview());
                surface.close_tab(second, cx);
                assert!(!surface.markdown_previews.contains_key(&second));
                let plain = surface
                    .workspace
                    .tabs()
                    .iter()
                    .find(|tab| tab.title == "plain.txt")
                    .unwrap()
                    .id;
                surface.activate_tab(plain, cx);
                assert!(!surface.is_markdown_preview());
            })
        });
    }

    #[gpui::test]
    fn markdown_preview_renders_latest_buffer_and_search_reveals_source(cx: &mut TestAppContext) {
        let surface = cx.new(ready_surface);
        let tab = surface.read_with(cx, |surface, _| surface.workspace.active_tab().unwrap());
        surface.update(cx, |surface, cx| {
            // Two requests before completion must not let an old parse replace new content.
            surface.prepare_markdown_preview(tab, "# Old".into(), cx);
            let editor = surface.active_editor().unwrap();
            editor.update(cx, |editor, cx| {
                editor.replace_text_external("# Unsaved", cx)
            });
            surface.render_markdown_preview(cx);
        });
        cx.run_until_parked();
        surface.update(cx, |surface, cx| {
            let state = &surface.markdown_previews[&tab];
            assert_eq!(
                state.document.as_ref().unwrap().blocks,
                vec![Block::Heading {
                    level: 1,
                    id: "unsaved".into(),
                    inlines: vec![Inline::Text("Unsaved".into())],
                }]
            );
            let source = state.source.as_ref().unwrap().clone();
            surface.render_markdown_preview(cx);
            assert!(Arc::ptr_eq(
                surface.markdown_previews[&tab].source.as_ref().unwrap(),
                &source
            ));
            assert!(
                surface.markdown_previews[&tab].parse_task.is_none(),
                "unchanged buffer must reuse its parsed document"
            );
            assert!(surface.is_tab_dirty(tab, cx));
            surface.sync_editor_to_workspace(tab, cx);
            let request = surface
                .prepare_file_save(tab, oxideterm_audit::AuditSource::User, cx)
                .unwrap();
            assert_eq!(request.buffer.text.to_string(), "# Unsaved");
            surface.complete_file_save(request, Ok(SavedFileVersion::unknown()), cx);
            assert_eq!(
                surface
                    .workspace
                    .buffer(tab)
                    .unwrap()
                    .saved_text
                    .to_string(),
                "# Unsaved"
            );
            assert!(!surface.is_tab_dirty(tab, cx));
            let location = surface.workspace.tabs()[0].location.clone();
            surface
                .pending_search_queries
                .insert("/repo/README.MD".into(), "Unsaved".into());
            surface.apply_pending_search_query_for_location(&location, cx);
            assert!(!surface.is_markdown_preview());
            assert_eq!(
                surface.active_editor().unwrap().read(cx).find_query(),
                "Unsaved"
            );
            surface.markdown_previews.get_mut(&tab).unwrap().editing = false;
            surface.pending_editor_reveals.insert(
                "/repo/README.MD".into(),
                PendingEditorReveal { line: 1, column: 3 },
            );
            surface.apply_pending_editor_reveal_for_location(&location, cx);
            assert!(!surface.is_markdown_preview());
            let other = surface.workspace.tabs()[1].clone();
            surface.pending_editor_reveals.insert(
                "/repo/second.md".into(),
                PendingEditorReveal { line: 1, column: 2 },
            );
            surface.apply_pending_editor_reveal_for_location(&other.location, cx);
            surface.activate_tab(other.id, cx);
            assert!(
                !surface.is_markdown_preview(),
                "background file-open completion must retain source reveal intent"
            );
            let options = surface.markdown_options();
            assert_eq!(options.allowed_link_schemes, ["http", "https", "mailto"]);
            assert_eq!(options.allowed_image_schemes, ["http", "https", "data"]);
        });
    }

    #[gpui::test]
    #[ignore = "manual Markdown parse/cache and headless render comparison; excludes GPU paint"]
    fn markdown_preview_large_document_cache_benchmark(cx: &mut TestAppContext) {
        let source: Arc<str> =
            "# Heading\n\nParagraph with **bold**, `code`, and a [link](https://example.com).\n\n"
                .repeat(16384)
                .into();
        let tokens = oxideterm_theme::default_tokens();
        let options = MarkdownOptions::from_theme(&tokens);
        let scroll = MarkdownVirtualListScrollHandle::new();
        let frames = 5;
        let start = Instant::now();
        for _ in 0..frames {
            std::hint::black_box(oxideterm_gpui_markdown::markdown_virtual_with_options(
                "markdown-baseline",
                &tokens,
                &source,
                &options,
                &scroll,
            ));
        }
        let baseline = start.elapsed();
        let surface = cx.new(ready_surface);
        let tab = surface.read_with(cx, |surface, _| surface.workspace.active_tab().unwrap());
        surface.update(cx, |surface, cx| {
            surface.active_editor().unwrap().update(cx, |editor, cx| {
                editor.replace_text_external(source.as_ref(), cx)
            });
            surface.render_markdown_preview(cx);
        });
        cx.run_until_parked();
        let start = Instant::now();
        surface.update(cx, |surface, cx| {
            for _ in 0..frames {
                std::hint::black_box(surface.render_markdown_preview(cx));
            }
            assert!(surface.markdown_previews[&tab].parse_task.is_none());
        });
        eprintln!(
            "Markdown {} bytes, {frames} headless renders: parse each frame={baseline:?}; cached={:?}",
            source.len(),
            start.elapsed()
        );
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    #[ignore = "manual active/background/restored tab live-allocation comparison"]
    fn ide_background_tab_memory(cx: &mut TestAppContext) {
        #[repr(C)]
        #[derive(Default)]
        struct Statistics {
            blocks: u32,
            live: usize,
            peak: usize,
            allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
        }
        let allocated = || {
            let mut stats = Statistics::default();
            // A null zone measures live allocations across zones, not RSS.
            unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
            stats.live
        };
        for markdown in [false, true] {
            let baseline = allocated();
            let (surface, cx) = cx.add_window_view(|_, cx| ready_surface(cx));
            let (tab, other) = surface.update(cx, |surface, cx| {
                surface.set_mount(IdeSurfaceMount::MainWindow, cx);
                let tab = surface.workspace.tabs()[0].id;
                let other = surface.workspace.tabs()[2].id;
                let text = if markdown {
                    "# Heading\n\nParagraph with **bold**, `code`, and a link.\n\n".repeat(16384)
                } else {
                    format!("[{}0]", "{\"key\":42,\"text\":\"sample\"},\n".repeat(32768))
                };
                if !markdown {
                    surface.show_markdown_source(tab, cx);
                }
                surface.editors[&tab].update(cx, |editor, cx| {
                    editor.replace_text_external(text, cx);
                    if !markdown {
                        editor.set_language(Some(oxideterm_editor_syntax::LanguageId::Json), cx);
                    }
                });
                (tab, other)
            });
            let draw = |cx: &mut gpui::VisualTestContext| {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
                cx.update(|window, cx| window.draw(cx).clear(cx));
            };
            draw(cx);
            let active = allocated();
            surface.update(cx, |surface, cx| surface.activate_tab(other, cx));
            draw(cx);
            let background = allocated();
            let start = Instant::now();
            surface.update(cx, |surface, cx| surface.activate_tab(tab, cx));
            draw(cx);
            let restore_ms = start.elapsed().as_secs_f64() * 1000.0;
            let restored = allocated();
            cx.update(|window, _| window.remove_window());
            drop(surface);
            cx.cx.update(|_| {});
            cx.run_until_parked();
            eprintln!("IDE_TAB_MEMORY markdown={markdown} baseline={baseline} active={active} background={background} restored={restored} closed={} restore_ms={restore_ms:.3}", allocated());
        }
    }
}
