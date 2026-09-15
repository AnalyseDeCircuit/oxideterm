use super::*;
use oxideterm_gpui_ui::{
    menu::{MenuItemKind, menu_content, menu_item},
    modal::popover_backdrop,
};

#[derive(Clone)]
pub(super) enum KnowledgeAction {
    Select(String),
    Import,
    CreateNotebook,
    RenameNotebook,
    RenameNote,
    DeleteNotebook,
    Move,
    MoveTo(String),
    Embeddings,
    Reindex,
    Settings,
}

#[derive(Clone, Copy)]
enum MenuKind {
    Notebooks,
    Actions,
    Move,
}

pub(super) struct KnowledgeMenu {
    window_id: gpui::WindowId,
    position: gpui::Point<Pixels>,
    kind: MenuKind,
    selected: usize,
    pub(super) focus: FocusHandle,
}

pub(super) struct KnowledgeRename {
    notebook: bool,
    id: String,
    version: u64,
    pub(super) editor: Entity<TextEditorView>,
    window_id: gpui::WindowId,
    error: Option<String>,
}

impl WorkspaceApp {
    pub(super) fn queue_knowledge_search(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |workspace, cx| {
            Timer::after(Duration::from_millis(180)).await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.refresh_knowledge_navigator(true, cx)
            });
        });
        self.knowledge_workspace
            .update(cx, |state, _| state.search_task = Some(task));
    }

    pub(super) fn knowledge_workspace_header(
        &self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.two))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.knowledge_navigator_action(
                "notes-toggle-navigation",
                LucideIcon::PanelLeft,
                self.i18n.t("settings_view.knowledge.toggle_navigation"),
                false,
                false,
                move |this, _, _, cx| {
                    this.knowledge_workspace.update(cx, |state, _| {
                        if narrow {
                            state.mobile_navigator_open = !state.mobile_navigator_open;
                        } else {
                            state.navigator_hidden = !state.navigator_hidden;
                        }
                        state.menu = None;
                    });
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.i18n.t("settings_view.knowledge.notes_title")),
            )
            .into_any_element()
    }

    fn open_knowledge_menu(
        &mut self,
        kind: MenuKind,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.knowledge_workspace.update(cx, |state, _| {
            state.menu = Some(KnowledgeMenu {
                kind,
                position,
                selected: 0,
                focus,
                window_id: window.window_handle().window_id(),
            });
        });
        cx.notify();
    }

    pub(super) fn knowledge_navigator_toolbar(
        &self,
        collection: Option<&oxideterm_ai::RagCollectionResponse>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = collection.map(|item| item.name.clone()).unwrap_or_default();
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Ghost,
            ButtonRadius::Sm,
            28.0,
            8.0,
            self.tokens.metrics.ui_text_sm,
        );
        options.show_label = false;
        div()
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .px(px(self.tokens.spacing.two))
            .gap(px(self.tokens.spacing.one))
            .child(
                oxideterm_gpui_ui::toolbar_button(
                    &self.tokens,
                    String::new(),
                    Some(
                        svg()
                            .path(LucideIcon::BookOpen.path())
                            .size(px(KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE))
                            .flex_none()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .into_any_element(),
                    ),
                    options,
                )
                .id("notes-notebooks")
                .flex_1()
                .min_w_0()
                .justify_start()
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.knowledge.notebooks")),
                )
                .child(div().flex_1().min_w_0().truncate().child(label))
                .child(
                    svg()
                        .path(LucideIcon::ChevronDown.path())
                        .size(px(KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE))
                        .flex_none()
                        .text_color(rgb(self.tokens.ui.text_muted)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.open_knowledge_menu(MenuKind::Notebooks, event.position, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(self.knowledge_navigator_action(
                "notes-more",
                LucideIcon::MoreVertical,
                self.i18n.t("settings_view.knowledge.more"),
                false,
                false,
                |this, event, window, cx| {
                    this.open_knowledge_menu(MenuKind::Actions, event.position, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn knowledge_navigator_documents_header(
        &self,
        collection_id: &str,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collection_id = collection_id.to_owned();
        div()
            .h(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(self.tokens.spacing.three))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(count.to_string())
            .child(self.knowledge_navigator_action(
                "notes-new",
                LucideIcon::FilePlus,
                self.i18n.t("settings_view.knowledge.new_document"),
                false,
                false,
                move |this, _, window, cx| {
                    this.open_knowledge_document_dialog(collection_id.clone(), true, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_knowledge_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.knowledge_workspace.read(cx);
        let menu = state.menu.as_ref()?;
        if menu.window_id != window.window_handle().window_id() {
            return None;
        }
        let (position, selected, focus, kind) =
            (menu.position, menu.selected, menu.focus.clone(), menu.kind);
        let collection = state.navigator_snapshot.selected_collection_id.clone();
        let doc = state.editor.as_ref().map(|editor| editor.read(cx));
        let can_edit = state.metadata_task.is_none()
            && doc.is_some_and(|doc| {
                !doc.is_dirty() && matches!(doc.save_state, KnowledgeDocumentSaveState::Saved)
            });
        let entries: Vec<(String, KnowledgeAction, bool)> = match kind {
            MenuKind::Notebooks => state
                .navigator_snapshot
                .collections
                .iter()
                .map(|item| {
                    (
                        item.name.clone(),
                        KnowledgeAction::Select(item.id.clone()),
                        false,
                    )
                })
                .chain(std::iter::once((
                    self.i18n.t("settings_view.knowledge.new_notebook"),
                    KnowledgeAction::CreateNotebook,
                    false,
                )))
                .collect(),
            MenuKind::Move => state
                .navigator_snapshot
                .collections
                .iter()
                .filter(|item| doc.is_some_and(|doc| doc.collection_id != item.id))
                .map(|item| {
                    (
                        item.name.clone(),
                        KnowledgeAction::MoveTo(item.id.clone()),
                        !can_edit,
                    )
                })
                .collect(),
            MenuKind::Actions => vec![
                (
                    self.i18n.t("settings_view.knowledge.new_notebook"),
                    KnowledgeAction::CreateNotebook,
                    false,
                ),
                (
                    self.i18n.t("settings_view.knowledge.rename_notebook"),
                    KnowledgeAction::RenameNotebook,
                    collection.is_none(),
                ),
                (
                    self.i18n.t("settings_view.knowledge.delete_notebook"),
                    KnowledgeAction::DeleteNotebook,
                    collection.is_none(),
                ),
                (
                    self.i18n.t("settings_view.knowledge.rename_note"),
                    KnowledgeAction::RenameNote,
                    !can_edit,
                ),
                (
                    self.i18n.t("settings_view.knowledge.move_note"),
                    KnowledgeAction::Move,
                    !can_edit || state.navigator_snapshot.collections.len() < 2,
                ),
                (
                    self.i18n.t("settings_view.knowledge.import_files"),
                    KnowledgeAction::Import,
                    collection.is_none(),
                ),
                (
                    self.i18n.t("settings_view.knowledge.generate_embeddings"),
                    KnowledgeAction::Embeddings,
                    collection.is_none()
                        || self
                            .ai_entity
                            .read(cx)
                            .knowledge_embedding_progress()
                            .is_some(),
                ),
                (
                    self.i18n.t("settings_view.knowledge.reindex"),
                    KnowledgeAction::Reindex,
                    collection.is_none()
                        || self
                            .ai_entity
                            .read(cx)
                            .knowledge_reindex_progress()
                            .is_some(),
                ),
                (
                    self.i18n.t("settings_view.knowledge.configure_embeddings"),
                    KnowledgeAction::Settings,
                    false,
                ),
            ],
        };
        let keyboard_entries = entries.clone();
        let mut panel = menu_content(&self.tokens)
            .id("notes-menu")
            .track_focus(&focus)
            .max_h(px(360.0))
            .overflow_y_scroll()
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.knowledge_workspace
                            .update(cx, |state, _| state.menu = None);
                        window.focus(&this.focus_handle, cx);
                    }
                    "up" | "down" => {
                        this.knowledge_workspace.update(cx, |state, _| {
                            if let Some(menu) = state.menu.as_mut() {
                                menu.selected = if event.keystroke.key == "up" {
                                    menu.selected.saturating_sub(1)
                                } else {
                                    (menu.selected + 1)
                                        .min(keyboard_entries.len().saturating_sub(1))
                                };
                            }
                        });
                    }
                    "enter" => {
                        if let Some((_, action, false)) = keyboard_entries.get(selected) {
                            this.run_knowledge_action(action.clone(), position, window, cx);
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }));
        for (index, (label, action, disabled)) in entries.into_iter().enumerate() {
            let checked =
                matches!(&action, KnowledgeAction::Select(id) if collection.as_ref() == Some(id));
            panel = panel.child(
                menu_item(
                    &self.tokens,
                    label,
                    if matches!(kind, MenuKind::Notebooks) {
                        MenuItemKind::Radio(checked)
                    } else {
                        MenuItemKind::Plain
                    },
                    false,
                    disabled,
                )
                .id(("notes-menu-item", index))
                .when(index == selected, |row| {
                    row.bg(rgb(self.tokens.ui.bg_hover))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.run_knowledge_action(action.clone(), position, window, cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.knowledge_workspace
                            .update(cx, |state, _| state.menu = None);
                        window.focus(&this.focus_handle, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    gpui::deferred(
                        gpui::anchored()
                            .position(position)
                            .position_mode(gpui::AnchoredPositionMode::Window)
                            .child(panel),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    fn run_knowledge_action(
        &mut self,
        action: KnowledgeAction,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collection = self
            .knowledge_workspace
            .read(cx)
            .navigator_snapshot
            .selected_collection_id
            .clone();
        self.knowledge_workspace
            .update(cx, |state, _| state.menu = None);
        window.focus(&self.focus_handle, cx);
        match action {
            KnowledgeAction::Select(id) => self.change_knowledge_notebook(id, false, cx),
            KnowledgeAction::CreateNotebook => self.open_knowledge_create_dialog(cx),
            KnowledgeAction::DeleteNotebook => {
                if let Some(col) = self
                    .knowledge_workspace
                    .read(cx)
                    .navigator_snapshot
                    .selected_collection
                    .clone()
                {
                    self.ai_entity.update(cx, |ai, _| {
                        ai.request_delete_knowledge_collection(col.id, col.name)
                    });
                    self.reset_standard_confirm_focus();
                }
            }
            KnowledgeAction::Import => {
                if let Some(id) = collection {
                    self.knowledge_import_files(id, window, cx);
                }
            }
            KnowledgeAction::Embeddings => {
                if let Some(id) = collection {
                    self.knowledge_generate_embeddings(id, cx);
                }
            }
            KnowledgeAction::Reindex => {
                if let Some(id) = collection {
                    self.knowledge_reindex(id, cx);
                }
            }
            KnowledgeAction::Settings => self.open_knowledge_settings(window, cx),
            KnowledgeAction::Move => self.open_knowledge_menu(MenuKind::Move, position, window, cx),
            KnowledgeAction::RenameNotebook => self.open_knowledge_rename(true, window, cx),
            KnowledgeAction::RenameNote => self.open_knowledge_rename(false, window, cx),
            KnowledgeAction::MoveTo(target) => self.save_knowledge_metadata(None, Some(target), cx),
        }
        cx.notify();
    }

    fn open_knowledge_rename(
        &mut self,
        notebook: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.knowledge_workspace.read(cx);
        let data = if notebook {
            state
                .navigator_snapshot
                .selected_collection
                .as_ref()
                .map(|col| (col.id.clone(), col.name.clone(), 0))
        } else {
            state.editor.as_ref().map(|editor| {
                let doc = editor.read(cx);
                (doc.document_id.clone(), doc.title.clone(), doc.version)
            })
        };
        let Some((id, name, version)) = data else {
            return;
        };
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(name, &self.tokens, cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.apply_runtime_settings(
                &self.tokens,
                settings_ui_font_family(&self.settings_store.settings().appearance.ui_font_family)
                    .to_string(),
                self.tokens.metrics.ui_text_sm,
                1.5,
                false,
                false,
                cx,
            );
            editor
        });
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let weak = cx.entity().downgrade();
        editor.update(cx, |editor, _| {
            editor.set_on_save(Box::new(move |name, _, cx| {
                let name = name.to_string();
                weak.update(cx, |this, cx| {
                    this.save_knowledge_metadata(Some(name), None, cx)
                })
                .map_err(|_| "Workspace closed".to_owned())?;
                Ok(())
            }))
        });
        self.knowledge_workspace.update(cx, |state, _| {
            state.rename = Some(KnowledgeRename {
                notebook,
                id,
                version,
                editor,
                window_id: window.window_handle().window_id(),
                error: None,
            })
        });
    }

    pub(super) fn render_knowledge_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.knowledge_workspace.read(cx);
        let rename = state.rename.as_ref()?;
        if rename.window_id != window.window_handle().window_id() {
            return None;
        }
        let editor = rename.editor.clone();
        let error = rename.error.clone();
        let busy = state.metadata_task.is_some();
        let title = self.i18n.t(if rename.notebook {
            "settings_view.knowledge.rename_notebook"
        } else {
            "settings_view.knowledge.rename_note"
        });
        let dialog = oxideterm_gpui_ui::dialog::dialog_content(&self.tokens)
            .w(px(420.0))
            .max_w_full()
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !busy {
                    this.knowledge_workspace
                        .update(cx, |state, _| state.rename = None);
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                oxideterm_gpui_ui::dialog::dialog_header(&self.tokens)
                    .child(oxideterm_gpui_ui::dialog::dialog_title(&self.tokens, title)),
            )
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .h(px(32.0))
                            .border_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(editor.clone()),
                    )
                    .when_some(error, |body, error| {
                        body.child(self.knowledge_error_row(&error))
                    }),
            )
            .child(
                oxideterm_gpui_ui::dialog::dialog_footer(&self.tokens)
                    .child(self.knowledge_switch_dialog_button(
                        "notes-rename-cancel",
                        self.i18n.t("common.actions.cancel"),
                        false,
                        cx.listener(move |this, _, window, cx| {
                            if !busy {
                                this.knowledge_workspace
                                    .update(cx, |state, _| state.rename = None);
                                window.focus(&this.focus_handle, cx);
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.knowledge_switch_dialog_button(
                        "notes-rename-save",
                        self.i18n.t("settings_view.knowledge.editor_save"),
                        true,
                        cx.listener(move |this, _, _, cx| {
                            if !busy {
                                this.save_knowledge_metadata(
                                    Some(editor.read(cx).buffer().text()),
                                    None,
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    )),
            );
        Some(oxideterm_gpui_ui::modal_overlay(&self.tokens, dialog))
    }

    fn save_knowledge_metadata(
        &mut self,
        name: Option<String>,
        target: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let state = self.knowledge_workspace.read(cx);
        if state.metadata_task.is_some() {
            return;
        }
        let data = if let Some(rename) = state.rename.as_ref() {
            Some((rename.notebook, rename.id.clone(), rename.version))
        } else {
            state.editor.as_ref().map(|editor| {
                let doc = editor.read(cx);
                (false, doc.document_id.clone(), doc.version)
            })
        };
        let Some((notebook, id, version)) = data else {
            return;
        };
        let store = self.ai_entity.read(cx).rag_store();
        let task = cx.spawn(async move |workspace, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if notebook {
                        store
                            .rename_collection(&id, name.as_deref().unwrap_or_default())
                            .map(|_| None)
                    } else {
                        let doc = store.edit_document_metadata(
                            &id,
                            name.as_deref(),
                            target.as_deref(),
                            version,
                        )?;
                        let _ = store.queue_bm25_rebuild();
                        Ok(Some(doc))
                    }
                })
                .await;
            let _ = workspace.update(cx, |workspace, cx| {
                let succeeded = result.is_ok();
                let moved_collection = result
                    .as_ref()
                    .ok()
                    .and_then(|doc| doc.as_ref())
                    .map(|doc| doc.collection_id.clone());
                workspace.knowledge_workspace.update(cx, |state, cx| {
                    state.metadata_task = None;
                    match result {
                        Ok(doc) => {
                            state.rename = None;
                            if let Some(doc) = doc
                                && let Some(editor) = state.editor.as_ref()
                            {
                                editor.update(cx, |editor, cx| {
                                    if editor.document_id == doc.id {
                                        editor.title = doc.title;
                                        editor.collection_id = doc.collection_id;
                                        editor.version = doc.version;
                                        editor.start_index_state_poll(cx);
                                        cx.notify();
                                    }
                                });
                            }
                        }
                        Err(_) => {
                            let message =
                                workspace.i18n.t("settings_view.knowledge.metadata_failed");
                            if let Some(rename) = state.rename.as_mut() {
                                rename.error = Some(message);
                            } else {
                                state.metadata_error = Some(message);
                            }
                        }
                    }
                });
                if succeeded {
                    if let Some(collection) = moved_collection {
                        workspace
                            .ai_entity
                            .update(cx, |ai, _| ai.select_knowledge_collection(collection));
                    }
                    workspace.refresh_knowledge_navigator(true, cx);
                }
                cx.notify();
            });
        });
        self.knowledge_workspace.update(cx, |state, _| {
            state.metadata_error = None;
            state.metadata_task = Some(task);
        });
    }

    pub(super) fn change_knowledge_notebook(
        &mut self,
        id: String,
        discard: bool,
        cx: &mut Context<Self>,
    ) {
        let state = self.knowledge_workspace.read(cx);
        if state.navigator_snapshot.selected_collection_id.as_ref() == Some(&id) {
            return;
        }
        if !discard
            && state
                .editor
                .as_ref()
                .is_some_and(|editor| editor.read(cx).is_dirty())
        {
            self.knowledge_workspace
                .update(cx, |state, _| state.pending_collection_id = Some(id));
            cx.notify();
            return;
        }
        self.knowledge_workspace.update(cx, |state, _| {
            state.editor = None;
            state._editor_subscription = None;
            state.selected_document_id = None;
            state.load_generation = state.load_generation.wrapping_add(1);
            state.loading = false;
            state.pending_collection_id = None;
            state.pending_document_id = None;
            state.switch_after_save = false;
            state.navigator_query = Arc::from("");
            state.navigator_search_editor = None;
            state._navigator_search_subscription = None;
        });
        self.ai_entity
            .update(cx, |ai, _| ai.select_knowledge_collection(id));
        self.refresh_knowledge_navigator(true, cx);
    }

    pub(super) fn knowledge_resize_handle(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        super::super::sidebar::sidebar_resize_hotzone_chrome(
            "notes-resize",
            rgb(self.tokens.ui.border),
            true,
        )
        .right_0()
        .top_0()
        .bottom_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.knowledge_workspace.update(cx, |state, _| {
                    state.navigator_resize = Some((
                        window.window_handle().window_id(),
                        f32::from(event.position.x),
                        width,
                    ))
                });
                window.prevent_default();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn knowledge_resize_active(&self, cx: &App) -> bool {
        self.knowledge_workspace.read(cx).navigator_resize.is_some()
    }

    pub(in crate::workspace) fn update_knowledge_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, start, width)) = self.knowledge_workspace.read(cx).navigator_resize else {
            return;
        };
        if id != window.window_handle().window_id() {
            return;
        }
        let width = (width + f32::from(event.position.x) - start).clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        self.knowledge_workspace
            .update(cx, |state, _| state.navigator_width = Some(width));
        cx.notify();
    }

    pub(in crate::workspace) fn finish_knowledge_resize(&mut self, cx: &mut Context<Self>) {
        self.knowledge_workspace
            .update(cx, |state, _| state.navigator_resize = None);
    }
}
