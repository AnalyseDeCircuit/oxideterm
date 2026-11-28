fn tab_scroll_viewport(scroll_handle: &ScrollHandle) -> gpui::Stateful<gpui::Div> {
    // Keep tabs as direct flex children: GPUI derives the horizontal scroll
    // range from this viewport's content bounds.
    div()
        .id("ide-tabs-scroll-viewport")
        .size_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .overflow_x_scroll()
        .track_scroll(scroll_handle)
}

impl IdeSurface {
    fn render_editor_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.sync_tab_render_caches(cx);
        let editor_content = div()
            .debug_selector(|| "ide-editor-content".into())
            .relative()
            .flex_1()
            .min_h_0()
            .child(match self.active_editor() {
                Some(_) if self.is_markdown_preview() => self.render_markdown_preview(cx),
                Some(editor) => editor.into_any_element(),
                None if self
                    .workspace
                    .active_tab()
                    .is_some_and(|tab_id| self.loading_file_tabs.contains(&tab_id)) =>
                {
                    self.render_loading_file()
                }
                None if let Some(error) = self
                    .workspace
                    .active_tab()
                    .and_then(|tab| self.file_open_errors.get(&tab)) =>
                {
                    oxideterm_gpui_ui::error_state(
                        &self.tokens,
                        self.icon("lucide/triangle-alert.svg", 32.0, self.tokens.ui.warning),
                        self.file_error_message(error),
                        None,
                        None,
                    )
                    .debug_selector(|| "ide-file-open-error".into())
                    .into_any_element()
                }
                None => self.render_empty_editor(cx),
            });

        div()
            .flex_1()
            .min_w_0()
            .size_full()
            .flex()
            .flex_col()
            .bg(self.ide_editor_content_bg(self.tokens.ui.bg))
            .child(self.render_tabs(cx))
            .child(editor_content)
            .into_any_element()
    }

    fn render_tabs(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = self.workspace.tabs().to_vec();
        let active_tab = self.workspace.active_tab();
        let mut scroll_viewport = tab_scroll_viewport(&self.tab_scroll_handle);

        for tab in tabs {
            let active = Some(tab.id) == active_tab;
            let dirty = self.is_tab_dirty(tab.id, cx);
            let loading = self.loading_file_tabs.contains(&tab.id);
            let tab_id = tab.id;
            let is_dragging = self
                .tab_drag
                .is_some_and(|drag| drag.activated && drag.tab_id == tab_id);
            let file_icon = file_icons::file_icon(&tab.title);
            scroll_viewport = scroll_viewport.child(
                div()
                    .h_full()
                    .flex_none()
                    .px(px(IDE_TAB_PADDING_X))
                    .py(px(IDE_TAB_PADDING_Y))
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .border_r_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA))
                    .relative()
                    .bg(if active {
                        rgb(self.tokens.ui.bg_hover)
                    } else {
                        self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA)
                    })
                    .opacity(if is_dragging { 0.7 } else { 1.0 })
                    .when(is_dragging, |this| {
                        this.shadow_lg().rounded(px(self.tokens.radii.sm))
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.tab_context_menu = None;
                            this.tree_context_menu = None;
                            this.start_tab_drag(tab_id, event.position);
                            if event.click_count >= 2 {
                                this.toggle_tab_pin(tab_id, cx);
                            } else {
                                this.activate_tab(tab_id, cx);
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_tab(tab_id, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                            this.update_tab_drag(tab_id, event, cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                            this.finish_tab_drag(cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.tab_context_menu = Some(TabContextMenu {
                                tab_id,
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y),
                            });
                            this.tree_context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .when(tab.is_pinned, |this| {
                        this.child(self.icon("lucide/pin.svg", 12.0, self.tokens.ui.accent))
                    })
                    .child(file_icon.render(IDE_FILE_ICON_SIZE, &self.tokens))
                    .child(
                        div()
                            .max_w(px(120.0))
                            .truncate()
                            .text_color(rgb(if active {
                                self.tokens.ui.text
                            } else {
                                self.tokens.ui.text_muted
                            }))
                            .when(dirty, |this| this.italic())
                            .child(tab.title.clone()),
                    )
                    .when(dirty && !loading, |this| {
                        this.child(
                            div()
                                .size(px(6.0))
                                .rounded(px(self.tokens.radii.active_indicator))
                                .bg(rgb(self.tokens.ui.accent)),
                        )
                    })
                    .child(if loading {
                        div()
                            .ml_1()
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(self.spinner_icon(
                                SharedString::from(format!("ide-tab-loading-{tab_id:?}")),
                                12.0,
                                self.tokens.ui.text_muted,
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .ml_1()
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover(|style| style.bg(rgba((self.tokens.ui.bg_active << 8) | 0xcc)))
                            .child(self.icon("lucide/x.svg", 12.0, self.tokens.ui.text_secondary))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.close_tab(tab_id, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element()
                    })
                    .when(active, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(2.0))
                                .bg(rgb(self.tokens.ui.accent)),
                        )
                    }),
            );
        }
        div()
            .id("ide-tabs-scroll")
            .relative()
            .w_full()
            .min_w(px(0.0))
            .flex_none()
            .h(px(IDE_WORKSPACE_HEADER_HEIGHT))
            .overflow_hidden()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA))
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(scroll_viewport)
                    .child(
                        Scrollbar::new(&self.tab_scroll_handle)
                            .id("ide-tabs-horizontal-scrollbar")
                            .axis(ScrollbarAxis::Horizontal),
                    ),
            )
            .when(
                self.active_markdown_tab().is_some() && self.active_editor().is_some(),
                |row| row.child(self.render_markdown_toggle(cx)),
            )
            .into_any_element()
    }

    fn render_tab_context_menu(
        &self,
        menu: TabContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pinned = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.id == menu.tab_id)
            .map(|tab| tab.is_pinned)
            .unwrap_or(false);
        let pin_label = if pinned {
            self.labels.unpin_tab.as_str()
        } else {
            self.labels.pin_tab.as_str()
        };
        let menu_labels = [
            pin_label,
            self.labels.close_tab.as_str(),
            self.labels.close_other_tabs.as_str(),
            self.labels.close_all_tabs.as_str(),
        ];
        let menu_width = oxideterm_gpui_ui::estimate_menu_width(
            &self.tokens,
            IDE_TAB_CONTEXT_MENU_WIDTH,
            280.0,
            true,
            menu_labels,
        );
        let rows_height = IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT * menu_labels.len() as f32;
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - menu_width - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - rows_height - 16.0)
            .max(8.0);

        let popup = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(menu_width))
                .min_w_0()
                .child(self.render_tab_context_menu_item(
                    "lucide/pin.svg",
                    pin_label.to_owned(),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_tab_pin(menu.tab_id, cx);
                        this.tab_context_menu = None;
                        cx.stop_propagation();
                    }),
                ))
                .child(context_menu_separator(&self.tokens))
                .child(self.render_tab_context_menu_item(
                    "lucide/x.svg",
                    self.labels.close_tab.clone(),
                    true,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_tab(menu.tab_id, cx);
                        this.tab_context_menu = None;
                        cx.stop_propagation();
                    }),
                ))
                .children(
                    [
                        (&self.labels.close_other_tabs, Some(menu.tab_id)),
                        (&self.labels.close_all_tabs, None),
                    ]
                    .into_iter()
                    .map(|(label, keep)| {
                        self.render_tab_context_menu_item(
                            "lucide/x.svg",
                            label.clone(),
                            true,
                            !self.can_close_file_tabs(keep),
                            cx.listener(move |this, _event, _window, cx| {
                                this.tab_context_menu = None;
                                this.close_file_tabs(keep, cx);
                                cx.stop_propagation();
                            }),
                        )
                    }),
                ),
        )
        .into_any_element();

        popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, _window, cx| {
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(gpui::point(px(x), px(y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                            &self.tokens,
                            "ide-context-menu-motion",
                            div().child(popup),
                            -4.0,
                            gpui::point(0.0, 0.0),
                        )),
                )
                .with_priority(IDE_TAB_CONTEXT_MENU_Z),
            )
            .into_any_element()
    }

    fn render_tab_context_menu_item(
        &self,
        icon: &'static str,
        label: String,
        danger: bool,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        context_menu_action(
            context_menu_item_with_icon(&self.tokens, icon, label, danger, disabled).w_full(),
            disabled,
            false,
            listener,
        )
        .into_any_element()
    }

    fn open_tree_context_menu(
        &mut self,
        location: IdeLocation,
        is_directory: bool,
        name: String,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.tab_context_menu = None;
        if !self.workspace.file_tree().selection().contains(&location) {
            let _ = self.workspace.select_tree_entry(Some(location.clone()));
        }
        self.tree_context_menu = Some(TreeContextMenu {
            location,
            is_directory,
            name,
            x: f32::from(position.x),
            y: f32::from(position.y),
        });
        cx.notify();
    }

    fn render_tree_context_menu(
        &self,
        menu: TreeContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_width = oxideterm_gpui_ui::estimate_menu_width_with_shortcuts(
            &self.tokens,
            IDE_TREE_CONTEXT_MENU_WIDTH,
            320.0,
            true,
            [
                (self.labels.context_new_file.as_str(), None),
                (self.labels.upload.as_str(), None),
                (self.labels.download.as_str(), None),
                (self.labels.context_new_folder.as_str(), None),
                (self.labels.context_rename.as_str(), Some("F2")),
                (self.labels.context_delete.as_str(), None),
                (self.labels.context_copy.as_str(), None),
                (self.labels.context_cut.as_str(), None),
                (self.labels.context_paste.as_str(), None),
                (self.labels.context_copy_path.as_str(), None),
            ],
        );
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - menu_width - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - IDE_TREE_CONTEXT_MENU_MAX_HEIGHT - 8.0)
            .max(8.0);
        let remote_disabled = !self.remote_actions_ready();
        let multiple = self.workspace.file_tree().selection().len() > 1;
        let paste_disabled = remote_disabled || self.tree_clipboard.is_none();

        #[cfg(not(target_family = "wasm"))]
        let transfer_item = Some(self.render_tree_context_menu_item(
            if menu.is_directory {
                "lucide/upload.svg"
            } else {
                "lucide/download.svg"
            },
            if menu.is_directory {
                self.labels.upload.clone()
            } else {
                self.labels.download.clone()
            },
            None,
            false,
            remote_disabled || self.desktop.transfer.is_some(),
            cx.listener(|this, _, window, cx| {
                if let Some(menu) = this.tree_context_menu.take() {
                    this.pick_transfer_file(menu, window, cx);
                }
                cx.stop_propagation();
                cx.notify();
            }),
        ));
        #[cfg(target_family = "wasm")]
        let transfer_item: Option<AnyElement> = None;

        let popup = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(menu_width))
                .min_w_0(),
        )
        .child(self.render_tree_context_menu_item(
            "lucide/file-plus.svg",
            self.labels.context_new_file.clone(),
            None,
            false,
            remote_disabled,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_tree_name_input(
                        TreeNameInputKind::NewFile,
                        location.clone(),
                        name.clone(),
                        is_directory,
                        cx,
                    );
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_item(
            "lucide/folder-plus.svg",
            self.labels.context_new_folder.clone(),
            None,
            false,
            remote_disabled,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_tree_name_input(
                        TreeNameInputKind::NewFolder,
                        location.clone(),
                        name.clone(),
                        is_directory,
                        cx,
                    );
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .children(transfer_item)
        .child(self.render_tree_context_menu_divider())
        .child(self.render_tree_context_menu_item(
            "lucide/edit-3.svg",
            self.labels.context_rename.clone(),
            Some("F2"),
            false,
            remote_disabled || multiple,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_tree_name_input(
                        TreeNameInputKind::Rename,
                        location.clone(),
                        name.clone(),
                        is_directory,
                        cx,
                    );
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_item(
            "lucide/trash-2.svg",
            self.labels.context_delete.clone(),
            None,
            true,
            remote_disabled,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_delete_tree_item(location.clone(), name.clone(), is_directory, cx);
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_divider())
        .child(self.render_tree_context_menu_item(
            "lucide/copy.svg",
            self.labels.context_copy.clone(),
            None,
            false,
            remote_disabled,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_copy_tree_item(location.clone(), name.clone(), is_directory, cx);
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_item(
            "lucide/scissors.svg",
            self.labels.context_cut.clone(),
            None,
            false,
            remote_disabled,
            cx.listener({
                let location = menu.location.clone();
                let name = menu.name.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.request_cut_tree_item(location.clone(), name.clone(), is_directory, cx);
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_item(
            "lucide/file-text.svg",
            self.labels.context_paste.clone(),
            None,
            false,
            paste_disabled,
            cx.listener({
                let location = menu.location.clone();
                let is_directory = menu.is_directory;
                move |this, _event, _window, cx| {
                    this.paste_tree_clipboard(location.clone(), is_directory, cx);
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                }
            }),
        ))
        .child(self.render_tree_context_menu_divider())
        .child(self.render_tree_context_menu_item(
            "lucide/copy.svg",
            self.labels.context_copy_path.clone(),
            None,
            false,
            false,
            cx.listener({
                let path = self
                    .workspace
                    .file_tree()
                    .selection()
                    .iter()
                    .cloned()
                    .map(location_path)
                    .collect::<Vec<_>>()
                    .join("\n");
                move |this, _event, _window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        ))
        .into_any_element();

        popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(gpui::point(px(x), px(y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                            &self.tokens,
                            "ide-context-menu-motion",
                            div().child(popup),
                            -4.0,
                            gpui::point(0.0, 0.0),
                        )),
                )
                .with_priority(IDE_TREE_CONTEXT_MENU_Z),
            )
            .into_any_element()
    }

    fn render_tree_context_menu_item(
        &self,
        icon: &'static str,
        label: String,
        shortcut: Option<&'static str>,
        danger: bool,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let row = if let Some(shortcut) = shortcut {
            context_menu_item_with_icon_and_shortcut(
                &self.tokens,
                icon,
                label,
                context_menu_shortcut(&self.tokens, shortcut),
                danger,
                disabled,
            )
        } else {
            context_menu_item_with_icon(&self.tokens, icon, label, danger, disabled)
        }
        .w_full();
        context_menu_action(row, disabled, false, listener).into_any_element()
    }

    fn render_tree_context_menu_divider(&self) -> AnyElement {
        context_menu_separator(&self.tokens).into_any_element()
    }

    fn render_empty_editor(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(self.ide_editor_content_bg(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.icon(
                "lucide/code-2.svg",
                IDE_EMPTY_ICON_SIZE,
                self.tokens.ui.text_muted,
            ))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.labels.no_open_files.clone()),
            )
            .child(self.labels.click_to_open.clone())
            .into_any_element()
    }

    fn render_loading_file(&self) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(self.ide_editor_content_bg(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.spinner_icon("ide-editor-file-loading", 24.0, self.tokens.ui.text_muted))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .child(self.labels.loading_file.clone()),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tab_scroll_tests {
    use super::*;
    use gpui::{TestAppContext, size};

    struct TabScrollFixture {
        scroll_handle: ScrollHandle,
    }

    impl Render for TabScrollFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let mut scroll_viewport = tab_scroll_viewport(&self.scroll_handle);
            for index in 0_usize..3 {
                scroll_viewport = scroll_viewport.child(
                    div()
                        .id(("test-ide-tab", index))
                        .h_full()
                        .w(px(120.0))
                        .flex_none(),
                );
            }

            div().size_full().child(
                div()
                    .w(px(200.0))
                    .h(px(IDE_WORKSPACE_HEADER_HEIGHT))
                    .child(scroll_viewport),
            )
        }
    }

    #[gpui::test]
    fn tab_scroll_viewport_measures_overflowing_tab_width(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| TabScrollFixture {
            scroll_handle: ScrollHandle::new(),
        });
        cx.simulate_resize(size(px(400.0), px(100.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let max_offset_x =
            view.read_with(cx, |view, _| f32::from(view.scroll_handle.max_offset().x));
        assert_eq!(max_offset_x, 160.0);
    }
}
