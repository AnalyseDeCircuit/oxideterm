// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

const QUICK_OPEN_RESULTS: usize = 80;

struct QuickOpenState {
    input: Entity<TextEditorView>,
    input_version: u64,
    query: String,
    previous_focus: Option<FocusHandle>,
    index: oxideterm_ide_core::ProjectFileIndex,
    results: Vec<IdeLocation>,
    selected: Option<IdeLocation>,
    scroll: ScrollHandle,
    scan: Option<Task<()>>,
    finished: bool,
    partial: bool,
    error: Option<String>,
    _input_subscription: gpui::Subscription,
}

impl QuickOpenState {
    fn selected_index(&self) -> usize {
        self.selected
            .as_ref()
            .and_then(|selected| self.results.iter().position(|result| result == selected))
            .unwrap_or(0)
    }

    fn refresh_matches(&mut self) {
        if !self.query.is_empty() {
            self.results = self.index.matches(&self.query, QUICK_OPEN_RESULTS);
        }
    }
}

impl IdeSurface {
    fn reset_quick_open(&mut self) {
        self.quick_open = None;
        self.quick_open_focus = None;
    }

    fn open_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_modal() || self.shell_return_requested() || !self.ensure_remote_actions_ready(cx)
        {
            return;
        }
        if let Some(picker) = &self.quick_open {
            window.focus(&picker.input.focus_handle(cx), cx);
            return;
        }
        let Some(root) = self.workspace.project_root().cloned() else {
            return;
        };
        if self.search.open {
            self.close_project_search(window, cx);
        }
        self.tree_context_menu = None;
        self.tab_context_menu = None;
        self.file_format_menu = None;
        #[cfg(not(target_family = "wasm"))]
        {
            self.desktop.agent_status_menu = None;
        }

        let input = TextEditorView::new_search_input(
            "",
            &self.tokens,
            self.labels.find_file_by_name.clone(),
            EditorContextMenuLabels {
                copy: self.labels.editor_copy.clone(),
                cut: self.labels.editor_cut.clone(),
                paste: self.labels.editor_paste.clone(),
                select_all: self.labels.editor_select_all.clone(),
            },
            cx,
        );
        let subscription = cx.observe(&input, |this, input, cx| {
            let version = input.read(cx).buffer().version();
            let Some(picker) = this.quick_open.as_mut() else {
                return;
            };
            if picker.input_version == version {
                return;
            }
            picker.input_version = version;
            picker.query = input.read(cx).buffer().text().trim().to_owned();
            picker.selected = None;
            picker.scroll.scroll_to_item(0);
            if picker.query.is_empty() {
                picker.results = this
                    .workspace
                    .tabs()
                    .iter()
                    .rev()
                    .map(|tab| tab.location.clone())
                    .take(QUICK_OPEN_RESULTS)
                    .collect();
            } else {
                picker.refresh_matches();
                if picker.scan.is_none() {
                    this.start_quick_open_scan(cx);
                }
            }
            cx.notify();
        });
        let previous_focus = window.focused(cx);
        window.focus(&input.focus_handle(cx), cx);
        self.quick_open = Some(QuickOpenState {
            input_version: input.read(cx).buffer().version(),
            input,
            previous_focus,
            _input_subscription: subscription,
            query: String::new(),
            index: oxideterm_ide_core::ProjectFileIndex::new(root),
            results: self
                .workspace
                .tabs()
                .iter()
                .rev()
                .map(|tab| tab.location.clone())
                .take(QUICK_OPEN_RESULTS)
                .collect(),
            selected: None,
            scroll: ScrollHandle::new(),
            scan: None,
            finished: false,
            partial: false,
            error: None,
        });
        cx.notify();
    }

    fn start_quick_open_scan(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.quick_open.as_mut() else {
            return;
        };
        let Some(root) = picker.index.next_directory() else {
            return;
        };
        let input_id = picker.input.entity_id();
        let fs = self.fs.clone();
        let jobs = self.file_jobs();
        let task = cx.spawn(async move |weak, cx| {
            let mut next = Some(root.clone());
            let mut last_paint = Instant::now();
            let mut cached_directories = 0;
            while let Some(directory) = next {
                let cached = weak.read_with(cx, |this, _| {
                    this.workspace
                        .file_tree()
                        .children(&directory)
                        .map(<[_]>::to_vec)
                });
                let Ok(cached) = cached else { break };
                let listing = if let Some(entries) = cached {
                    cached_directories += 1;
                    if cached_directories == 16 {
                        // A fully expanded tree can otherwise finish the entire walk in a single UI turn.
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                        cached_directories = 0;
                    }
                    Ok(entries)
                } else {
                    cached_directories = 0;
                    jobs.run({
                        let fs = fs.clone();
                        let directory = directory.clone();
                        async move { fs.list_dir(&directory).await }
                    })
                    .await
                };
                next = weak
                    .update(cx, |this, cx| {
                        let picker = this
                            .quick_open
                            .as_mut()
                            .filter(|picker| picker.input.entity_id() == input_id)?;
                        match listing {
                            Ok(entries) => picker.index.add_entries(entries),
                            Err(e)
                                if directory != root
                                    && matches!(
                                        e.kind,
                                        IdeFileErrorKind::PermissionDenied
                                            | IdeFileErrorKind::NotFound
                                    ) =>
                            {
                                picker.partial = true
                            }
                            Err(e) => picker.error = Some(e.message),
                        }
                        let next = if picker.error.is_some() {
                            None
                        } else {
                            picker.index.next_directory()
                        };
                        picker.finished = next.is_none();
                        // Fast directory listings share one paint; a slow remote directory
                        // publishes immediately without waiting for the rest of the tree.
                        if directory == root
                            || picker.finished
                            || last_paint.elapsed() >= Duration::from_millis(100)
                        {
                            last_paint = Instant::now();
                            picker.refresh_matches();
                            cx.notify();
                        }
                        next
                    })
                    .ok()
                    .flatten();
            }
        });
        self.quick_open.as_mut().unwrap().scan = Some(task);
    }

    fn close_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.quick_open.take() {
            window.focus(
                picker.previous_focus.as_ref().unwrap_or(&self.focus_handle),
                cx,
            );
            cx.notify();
        }
    }

    fn accept_quick_open(
        &mut self,
        location: IdeLocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quick_open.take();
        window.focus(&self.focus_handle, cx);
        self.quick_open_focus = self.open_remote_file(location, Some(FileOrigin::User), cx);
        self.focus_quick_open_result_if_ready(window, cx);
        cx.notify();
    }

    fn focus_quick_open_result_if_ready(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.quick_open_focus else {
            return;
        };
        if self.workspace.active_tab() == Some(tab)
            && self.quick_open.is_none()
            && !self.has_modal()
            && self.focus_handle.is_focused(window)
        {
            if self.loading_file_tabs.contains(&tab) {
                return;
            }
            if let Some(editor) = self.active_editor().filter(|_| !self.is_markdown_preview()) {
                window.focus(&editor.focus_handle(cx), cx);
            }
        }
        self.quick_open_focus = None;
    }

    fn handle_quick_open_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !matches!(
            key,
            "escape" | "enter" | "up" | "arrowup" | "down" | "arrowdown"
        ) {
            return;
        }
        let Some(picker) = self.quick_open.as_mut() else {
            return;
        };
        if picker.input.update(cx, |input, cx| {
            gpui::EntityInputHandler::marked_text_range(input, window, cx).is_some()
        }) {
            return;
        }
        let selected = picker.selected_index();
        match key {
            "escape" => self.close_quick_open(window, cx),
            "enter" => {
                if let Some(location) = picker.results.get(selected).cloned() {
                    self.accept_quick_open(location, window, cx);
                }
            }
            _ if !picker.results.is_empty() => {
                let index = if matches!(key, "up" | "arrowup") {
                    selected.saturating_sub(1)
                } else {
                    (selected + 1).min(picker.results.len() - 1)
                };
                picker.selected = Some(picker.results[index].clone());
                picker.scroll.scroll_to_item(index);
                cx.notify();
            }
            _ => {}
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn render_quick_open(&self, cx: &mut Context<Self>) -> AnyElement {
        let picker = self.quick_open.as_ref().unwrap();
        let tokens = &self.tokens;
        let selected = picker.selected_index();
        let mut list = div()
            .id("ide-quick-open-results")
            .min_h_0()
            .max_h(px(336.0))
            .overflow_y_scroll()
            .track_scroll(&picker.scroll)
            .on_scroll(|_, _, cx| cx.stop_propagation());
        let root = self
            .workspace
            .project_root()
            .and_then(remote_path)
            .unwrap_or_default()
            .trim_end_matches('/');
        for (i, location) in picker.results.iter().enumerate() {
            let name = location.display_name();
            let path = remote_path(location).unwrap_or_default();
            let relative = path
                .strip_prefix(root)
                .and_then(|p| p.strip_prefix('/'))
                .unwrap_or(path);
            let parent = relative
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_owned());
            let location = location.clone();
            list = list.child(
                oxideterm_gpui_ui::command::command_item(
                    tokens,
                    i == selected,
                    file_icons::file_icon(&name).render(IDE_FILE_ICON_SIZE, tokens),
                )
                .id(("ide-quick-open-result", i))
                .debug_selector(move || format!("ide-quick-open-result-{i}"))
                .h(px(28.0))
                .min_w_0()
                .px_2()
                .py_0()
                .rounded(px(tokens.radii.sm))
                .hover(|el| el.bg(rgb(tokens.ui.bg_hover)))
                .child(div().min_w_0().truncate().child(name))
                .when_some(parent, |el, parent| {
                    el.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(parent),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.accept_quick_open(location.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        let status = if picker.query.is_empty() {
            None
        } else if let Some(error) = &picker.error {
            Some(error)
        } else if picker.scan.is_some() && !picker.finished {
            Some(&self.labels.searching)
        } else if picker.partial || picker.index.is_truncated() {
            Some(&self.labels.file_search_partial)
        } else if picker.results.is_empty() {
            Some(&self.labels.no_results)
        } else {
            None
        };
        let panel = oxideterm_gpui_ui::command_panel(
            tokens,
            oxideterm_gpui_ui::CommandPanelOptions::new()
                .width(560.0)
                .max_width_ratio(1.)
                .padding(oxideterm_gpui_ui::SurfacePadding::Compact),
        )
        .debug_selector(|| "ide-quick-open".into())
        .min_h_0()
        .max_h_full()
        .gap_1()
        .occlude()
        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            this.handle_quick_open_key(event, window, cx)
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("ide-quick-open-input")
                .debug_selector(|| "ide-quick-open-input".into())
                .aria_label(self.labels.find_file_by_name.clone())
                .h(px(32.0))
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .rounded(px(tokens.radii.sm))
                .bg(rgb(tokens.ui.bg_sunken))
                .border_1()
                .border_color(rgb(tokens.ui.accent))
                .child(self.icon(
                    "lucide/search.svg",
                    IDE_FILE_ICON_SIZE,
                    tokens.ui.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(picker.input.read(cx).line_height()))
                        .child(picker.input.clone()),
                ),
        )
        .when(!picker.results.is_empty(), |el| el.child(list))
        .when_some(status, |el, text| {
            el.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(text.clone()),
            )
        });
        popover_backdrop()
            .flex()
            .flex_col()
            .items_center()
            .justify_start()
            .px_2()
            .pt_2()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_quick_open(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(panel)
            .into_any_element()
    }
}
