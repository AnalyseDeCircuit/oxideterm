impl IdeSurface {
    fn render_editor_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let editor_content = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(match self.active_editor() {
                Some(editor) => editor.into_any_element(),
                None if self
                    .workspace
                    .active_tab()
                    .is_some_and(|tab_id| self.loading_file_tabs.contains(&tab_id)) =>
                {
                    self.render_loading_file()
                }
                None => self.render_empty_editor(cx),
            })
            .when(self.editor_search.open, |this| {
                this.child(self.render_editor_search_bar(cx))
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
        let mut row = div()
            .id("ide-tabs-scroll")
            .h(px(IDE_WORKSPACE_HEADER_HEIGHT))
            .flex()
            .items_center()
            .overflow_x_scroll()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA));

        for tab in tabs {
            let active = Some(tab.id) == active_tab;
            let dirty = self.is_tab_dirty(tab.id, cx);
            let loading = self.loading_file_tabs.contains(&tab.id);
            let tab_id = tab.id;
            let is_dragging = self
                .tab_drag
                .is_some_and(|drag| drag.activated && drag.tab_id == tab_id);
            let file_icon = file_icons::file_icon(&tab.title, &self.tokens);
            row = row.child(
                div()
                    .h_full()
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
                    .child(self.icon(file_icon.path, IDE_FILE_ICON_SIZE, file_icon.color))
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
        row.into_any_element()
    }

    fn render_tab_context_menu(
        &self,
        menu: TabContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - IDE_TAB_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT * 2.0 - 16.0)
            .max(8.0);
        let pinned = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.id == menu.tab_id)
            .map(|tab| tab.is_pinned)
            .unwrap_or(false);

        // Tauri `IdeEditorTabs.tsx` uses a fixed z-50 elevated menu with
        // min-w-[140px], rounded-md, py-1, and two text-xs actions.
        let popup = div()
            .w(px(IDE_TAB_CONTEXT_MENU_WIDTH))
            .py(px(IDE_TAB_CONTEXT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .child(self.render_tab_context_menu_item(
                "lucide/pin.svg",
                if pinned {
                    self.labels.unpin_tab.clone()
                } else {
                    self.labels.pin_tab.clone()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_tab_pin(menu.tab_id, cx);
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .child(self.render_tab_context_menu_item(
                "lucide/x.svg",
                self.labels.close_tab.clone(),
                cx.listener(move |this, _event, _window, cx| {
                    this.close_tab(menu.tab_id, cx);
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| {
                    cx.stop_propagation();
                }),
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
                        .child(popup),
                )
                .with_priority(IDE_TAB_CONTEXT_MENU_Z),
            )
            .into_any_element()
    }

    fn render_tab_context_menu_item(
        &self,
        icon: &'static str,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .h(px(IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.icon(icon, 12.0, self.tokens.ui.text))
            .child(div().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
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
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - IDE_TREE_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - IDE_TREE_CONTEXT_MENU_MAX_HEIGHT - 8.0)
            .max(8.0);
        let remote_disabled = !self.remote_actions_ready();
        let paste_disabled = remote_disabled || self.tree_clipboard.is_none();

        let popup = div()
            .w(px(IDE_TREE_CONTEXT_MENU_WIDTH))
            .py(px(IDE_TREE_CONTEXT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .shadow_lg()
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
            .child(self.render_tree_context_menu_divider())
            .child(self.render_tree_context_menu_item(
                "lucide/edit-3.svg",
                self.labels.context_rename.clone(),
                Some("F2"),
                false,
                remote_disabled,
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
                        this.request_copy_tree_item(
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
                        this.request_cut_tree_item(
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
                "lucide/clipboard-paste.svg",
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
                    let path = location_path(menu.location);
                    move |this, _event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                        this.tree_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
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
                        .child(popup),
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
        let text_color = if danger {
            TAILWIND_RED_400
        } else {
            self.tokens.ui.text
        };
        let hover_bg = if danger {
            rgba((TAILWIND_RED_500 << 8) | IDE_TREE_CONTEXT_MENU_DANGER_BG_ALPHA)
        } else {
            rgb(self.tokens.ui.bg_hover)
        };
        div()
            .h(px(IDE_TREE_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(text_color))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .when(!disabled, |this| {
                this.cursor_pointer().hover(move |style| style.bg(hover_bg))
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0))
                    .text_color(rgba((text_color << 8) | IDE_TREE_CONTEXT_MENU_ICON_ALPHA)),
            )
            .child(div().w(px(8.0)))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when_some(shortcut, |this, shortcut| {
                this.child(
                    div()
                        .ml_4()
                        .text_size(px(IDE_TREE_CONTEXT_MENU_SHORTCUT_SIZE))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                        .child(shortcut),
                )
            })
            .when(!disabled, |this| {
                this.on_mouse_down(MouseButton::Left, listener)
            })
            .into_any_element()
    }

    fn render_tree_context_menu_divider(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(IDE_TREE_CONTEXT_MENU_PADDING_Y))
            .bg(rgb(self.tokens.ui.border))
            .into_any_element()
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
            .child(self.spinner_icon(
                "ide-editor-file-loading",
                24.0,
                self.tokens.ui.text_muted,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .child(self.labels.loading_file.clone()),
            )
            .into_any_element()
    }

}
