// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_gpui_ai::knowledge::{
    editor::{KnowledgePreviewFrame, KnowledgePreviewSource},
    workspace::{
        KnowledgeBindings, KnowledgeEnvironment, KnowledgeInput, KnowledgeInputFrame,
        KnowledgeInputRequest, KnowledgeRequest, KnowledgeWorkspaceEntity,
    },
};

pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT: f32 = 28.0;

fn ime_target(input: KnowledgeInput) -> WorkspaceImeTarget {
    match input {
        KnowledgeInput::Search => WorkspaceImeTarget::KnowledgeSearch,
        KnowledgeInput::Rename => WorkspaceImeTarget::KnowledgeRename,
    }
}

fn knowledge_input(target: WorkspaceImeTarget) -> Option<KnowledgeInput> {
    match target {
        WorkspaceImeTarget::KnowledgeSearch => Some(KnowledgeInput::Search),
        WorkspaceImeTarget::KnowledgeRename => Some(KnowledgeInput::Rename),
        _ => None,
    }
}

impl WorkspaceApp {
    pub(super) fn knowledge_input_owner(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<gpui::WindowId> {
        self.knowledge_state()
            .read(cx)
            .input_owner(knowledge_input(target)?)
    }

    pub(super) fn knowledge_input_text(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> Option<String> {
        self.knowledge_state()
            .read(cx)
            .input_value(knowledge_input(target)?)
            .map(str::to_owned)
    }

    pub(super) fn set_knowledge_input_text(
        &self,
        target: WorkspaceImeTarget,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = knowledge_input(target) {
            self.knowledge_workspace
                .view
                .update(cx, |view, cx| view.set_input_value(input, value, cx));
        }
    }

    pub(super) fn knowledge_bindings(
        page: Entity<oxideterm_gpui_ai::knowledge::page::KnowledgePage>,
        cx: &Context<Self>,
    ) -> KnowledgeBindings {
        let owner = cx.entity().downgrade();
        KnowledgeBindings {
            page,
            environment: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(KnowledgeEnvironment {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        background: app.background_surface_active("knowledge"),
                        preferences: app
                            .settings_store
                            .settings()
                            .window_ui
                            .knowledge_editor
                            .clone(),
                    })
                }
            }),
            preview: Self::knowledge_preview_source(owner.clone()),
            frame: Rc::new({
                let owner = owner.clone();
                move |input, cx| {
                    let target = ime_target(input);
                    let anchor_id = target.anchor_id();
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    let anchors = app.text_input_anchors.clone();
                    Some(KnowledgeInputFrame {
                        focused: app.active_ime_target(cx) == Some(target),
                        caret_visible: app.input_caret.visible(),
                        selected_range: app.ime_selected_range_for_target(target, cx),
                        active_offset: app.ime_active_selection_offset(target),
                        marked_text: app.marked_text_for_target(target, cx).map(str::to_owned),
                        anchor_id,
                        viewport: anchors.viewport(anchor_id),
                        update_anchor: Rc::new(move |anchor| anchors.update(anchor)),
                    })
                }
            }),
            input: Rc::new({
                let owner = owner.clone();
                move |request, window, cx| {
                    let owner = owner.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = owner.update(cx, |app, cx| match request {
                            KnowledgeInputRequest::Focus(input) => {
                                app.clear_ime_selection();
                                app.selected_ime_target = Some(ime_target(input));
                                window.focus(&app.focus_handle, cx);
                                app.show_active_input_caret(cx);
                                App::notify(cx, app.knowledge_workspace.view.entity_id());
                            }
                            KnowledgeInputRequest::PointerDown(input, event) => {
                                app.ime_marked_text = None;
                                window.focus(&app.focus_handle, cx);
                                app.show_active_input_caret(cx);
                                app.begin_ime_selection_from_mouse_down(
                                    ime_target(input),
                                    &event,
                                    window,
                                    cx,
                                );
                            }
                            KnowledgeInputRequest::Drag(event) => {
                                app.update_ime_selection_drag_from_mouse_move(&event, window, cx)
                            }
                            KnowledgeInputRequest::Clear => {
                                app.clear_ime_selection();
                            }
                            KnowledgeInputRequest::DismissSearchOutside(position) => {
                                let target = WorkspaceImeTarget::KnowledgeSearch;
                                if app.active_ime_target(cx) == Some(target)
                                    && app
                                        .text_input_anchors
                                        .get(target.anchor_id())
                                        .is_none_or(|anchor| !anchor.bounds.contains(&position))
                                {
                                    app.clear_ime_selection();
                                    App::notify(cx, app.knowledge_workspace.view.entity_id());
                                }
                            }
                        });
                    });
                }
            }),
            request: Rc::new(move |request, window, cx| {
                let owner = owner.clone();
                if let Some(window) = window {
                    window.defer(cx, move |window, cx| {
                        let _ = owner.update(cx, |app, cx| match request {
                            KnowledgeRequest::CreateDocument(id) => {
                                app.open_knowledge_document_dialog(id, true, window, cx)
                            }
                            KnowledgeRequest::CreateNotebook => {
                                app.open_knowledge_create_dialog(cx)
                            }
                            KnowledgeRequest::Settings => app.open_knowledge_settings(window, cx),
                            KnowledgeRequest::Import(id) => {
                                app.knowledge_import_files(id, window, cx)
                            }
                            KnowledgeRequest::Embed(id) => {
                                app.knowledge_generate_embeddings(id, cx)
                            }
                            KnowledgeRequest::Reindex(id) => app.knowledge_reindex(id, cx),
                            KnowledgeRequest::DeleteDocument(id, title) => {
                                app.knowledge_page(cx).update(cx, |ai, _| {
                                    ai.request_delete_knowledge_document(id, title)
                                });
                                app.reset_standard_confirm_focus();
                                cx.notify();
                            }
                            KnowledgeRequest::DeleteNotebook(id, title) => {
                                app.knowledge_page(cx).update(cx, |ai, _| {
                                    ai.request_delete_knowledge_collection(id, title)
                                });
                                app.reset_standard_confirm_focus();
                                cx.notify();
                            }
                            _ => {}
                        });
                    });
                } else {
                    cx.defer(move |cx| match request {
                        KnowledgeRequest::Quit => super::request_app_quit(cx),
                        KnowledgeRequest::Close(tab, window) => {
                            let _ = window.update(cx, |_, window, cx| {
                                let _ = owner.update(cx, |app, cx| app.close_tab_by_id(tab, window, cx));
                            });
                        }
                        KnowledgeRequest::SaveEditorPreferences(preferences) => {
                            let _ = owner.update(cx, |app, cx| {
                                app.settings_store.settings_mut().window_ui.knowledge_editor = preferences;
                                if let Err(error) = app.settings_store.save() {
                                    tracing::warn!(%error, "failed to save local note editor preferences");
                                } else {
                                    app.settings_workspace.update(cx, |settings, _| settings.acknowledge_external_store_state());
                                }
                            });
                        }
                        _ => {}
                    });
                }
            }),
        }
    }

    pub(super) fn knowledge_page(
        &self,
        cx: &App,
    ) -> Entity<oxideterm_gpui_ai::knowledge::page::KnowledgePage> {
        self.ai.state.read(cx).knowledge_page().clone()
    }

    pub(in crate::workspace) fn knowledge_state(&self) -> &Entity<KnowledgeWorkspaceEntity> {
        &self.knowledge_workspace.state
    }

    pub(in crate::workspace) fn render_knowledge_workspace_surface(&self) -> AnyElement {
        self.knowledge_workspace.view.clone().into_any_element()
    }

    pub(in crate::workspace) fn sync_knowledge_cache_visibility(&self, cx: &mut App) {
        let view = self.knowledge_workspace.view.clone();
        let visible = !self.app_lock.locked
            && self
                .knowledge_state()
                .read(cx)
                .tab_id()
                .is_some_and(|id| self.tab_host.read(cx).surface_is_visible(id));
        view.update(cx, |view, cx| view.set_cache_visible(visible, cx));
    }

    pub(in crate::workspace) fn handle_knowledge_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.keyboard_content_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Knowledge)
            && !self.knowledge_text_editor_focused(window, cx)
            && self.knowledge_workspace.view.update(cx, |view, cx| {
                view.handle_knowledge_workspace_key(event, window, cx)
            })
    }

    pub(in crate::workspace) fn knowledge_text_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        matches!(
            self.active_ime_target_for_window(window.window_handle().window_id(), cx),
            Some(WorkspaceImeTarget::KnowledgeSearch | WorkspaceImeTarget::KnowledgeRename)
        ) || self
            .knowledge_workspace
            .view
            .read(cx)
            .text_editor_focused(window, cx)
    }

    pub(in crate::workspace) fn handle_knowledge_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self
            .active_ime_target_for_window(window.window_handle().window_id(), cx)
            .and_then(knowledge_input)
        else {
            return false;
        };
        self.knowledge_workspace.view.update(cx, |view, cx| {
            view.handle_knowledge_input_key(input, event, window, cx)
        })
    }

    pub(in crate::workspace) fn knowledge_leave_confirmation_open(&self, cx: &App) -> bool {
        self.knowledge_workspace
            .view
            .read(cx)
            .knowledge_leave_confirmation_open(cx)
    }

    pub(in crate::workspace) fn handle_knowledge_leave_confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.knowledge_workspace.view.update(cx, |view, cx| {
            view.handle_knowledge_leave_confirmation_key(event, window, cx)
        })
    }

    pub(super) fn notify_knowledge_preview_selection(&self, group: u64, cx: &mut App) -> bool {
        let Some(editor) = self.knowledge_workspace.state.read(cx).editor.clone() else {
            return false;
        };
        if super::selectable_text::selectable_text_id(
            "notes-preview",
            editor.read(cx).document_id.as_str(),
        ) != group
        {
            return false;
        }
        cx.notify(editor.entity_id());
        true
    }
    pub(in crate::workspace) fn open_knowledge_workspace_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.knowledge_state().read(cx).tab_id()
            && self.tabs(cx).iter().any(|tab| tab.id == tab_id)
        {
            self.set_active_tab(tab_id, window, cx);
            self.knowledge_workspace
                .view
                .update(cx, |view, cx| view.refresh_knowledge_navigator(true, cx));
            return;
        }
        let tab_id = self.alloc_tab_id(cx);
        self.knowledge_state()
            .update(cx, |workspace, _cx| workspace.register_tab(tab_id));
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::Knowledge,
                title: self.i18n.t("sidebar.panels.knowledge"),
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            },
            cx,
        );
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        self.knowledge_workspace
            .view
            .update(cx, |view, cx| view.refresh_knowledge_navigator(true, cx));
        cx.notify();
    }

    pub(in crate::workspace) fn guard_dirty_knowledge_tab_close(
        &mut self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(knowledge_tab) = self.knowledge_state().read(cx).tab_id() else {
            return false;
        };
        if tab_id != knowledge_tab
            && self.tab_host.read(cx).container_tab_id(knowledge_tab) != tab_id
        {
            return false;
        }
        let editor = self.knowledge_state().read(cx).editor.clone();
        if !editor.is_some_and(|editor| editor.read(cx).is_dirty()) {
            return false;
        }
        self.knowledge_state().update(cx, |knowledge, cx| {
            knowledge.request_close(tab_id, window.window_handle());
            cx.notify();
        });
        if self.tab_host.read(cx).is_detached(knowledge_tab) {
            self.tab_host
                .update(cx, |host, _| host.focus_content_page(knowledge_tab));
        } else {
            self.set_main_window_active_tab(Some(knowledge_tab), cx);
            self.sync_active_tab_surface(cx);
        }
        cx.notify();
        true
    }

    pub(crate) fn guard_dirty_knowledge_app_quit(&mut self, cx: &mut Context<Self>) -> bool {
        let editor = self.knowledge_state().read(cx).editor.clone();
        if !editor.is_some_and(|editor| editor.read(cx).is_dirty()) {
            return false;
        }
        self.knowledge_state().update(cx, |knowledge, cx| {
            knowledge.request_app_quit();
            cx.notify();
        });
        let knowledge_tab_id = self.knowledge_state().read(cx).tab_id();
        if let Some(knowledge_tab_id) = knowledge_tab_id
            && !self.focus_detached_tab_window(knowledge_tab_id, cx)
        {
            // The confirmation is owned by the Knowledge surface. Bring that surface into the
            // main window before blocking the global quit action so the decision is always visible.
            self.set_main_window_active_tab(Some(knowledge_tab_id), cx);
            self.sync_active_tab_surface(cx);
            // Tray quit can arrive while the main native window is hidden. Activating a hidden
            // AppKit or Win32 window does not make its confirmation visible.
            oxideterm_desktop_presence::show_main_window();
            if let Some(handle) = self
                .window_registry
                .handle_for_role(window_registry::WindowRole::Main)
            {
                let _ = handle.update(cx, |_root, window, _cx| window.activate_window());
            }
        }
        cx.notify();
        true
    }

    pub(in crate::workspace) fn guard_detached_knowledge_window_close(
        &mut self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .window_registry
            .is_only_window_with_role(window_registry::WindowRole::Detached { tab_id })
        {
            return self.guard_dirty_knowledge_app_quit(cx);
        }
        // Native detached-window release removes its tab on the current workspace host.
        // Resolve the draft before that release even when other windows remain open.
        self.guard_dirty_knowledge_tab_close(tab_id, window, cx)
    }

    fn knowledge_preview_source(owner: gpui::WeakEntity<Self>) -> KnowledgePreviewSource {
        Rc::new(move |cx| {
            use super::selectable_text::{SelectableTextRole, selectable_text_id};
            let entity = owner.upgrade()?;
            let workspace = entity.read(cx);
            let options = workspace.localized_markdown_options();
            let state = workspace.selectable_text_render_state_for_entity(entity.clone(), cx);
            Some(KnowledgePreviewFrame {
                options,
                text: Some(Rc::new(move |document, key, order, text, runs, links| {
                    let group = selectable_text_id("notes-preview", document);
                    let join_previous = key.join_previous;
                    state.render_styled_text_in_group(
                        SelectableTextRole::PlainDocument,
                        group,
                        selectable_text_id("notes-preview-fragment", (group, key)),
                        order,
                        text,
                        runs,
                        links,
                        join_previous,
                    )
                })),
                actions: Some(oxideterm_gpui_markdown::MarkdownCodeBlockActions {
                    on_run: None,
                    on_mermaid_zoom: Some(Self::mermaid_zoom_handler_for_workspace(entity)),
                }),
            })
        })
    }
}

#[cfg(test)]
pub(in crate::workspace) mod tests {
    use super::*;
    pub(in crate::workspace) fn assert_locked_quit_preserves_draft(
        workspace: &Entity<WorkspaceApp>,
        cx: &mut gpui::VisualTestContext,
    ) {
        let store =
            workspace.read_with(cx, |workspace, cx| workspace.ai.state.read(cx).rag_store());
        let collection = oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: "Quit regression".into(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .unwrap();
        let document = oxideterm_ai::rag_create_blank_document(
            &store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id: collection.id,
                title: "Private draft".into(),
                format: "markdown".into(),
            },
        )
        .unwrap();
        oxideterm_ai::rag_save_document(&store, &document.id, "saved".into(), Some(0)).unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_knowledge_workspace_tab(window, cx);
                workspace.select_knowledge_document(document.id.clone(), cx);
            })
        });
        cx.run_until_parked();
        let editor = workspace.read_with(cx, |workspace, cx| {
            workspace.knowledge_state().read(cx).editor.clone().unwrap()
        });
        oxideterm_ai::rag_save_document(&store, &document.id, "saved".into(), None).unwrap();
        editor.update(cx, |editor, cx| {
            editor
                .editor
                .update(cx, |text, cx| text.replace_text_external("unsaved", cx));
            editor.save_current_draft(cx);
        });
        cx.run_until_parked();
        workspace.update(cx, |workspace, cx| {
            workspace.app_lock.locked = true;
            cx.notify();
        });
        cx.update(|window, cx| window.request_close(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("enter");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("menu.unlock_to_finish_quit").is_some());
        assert!(cx.debug_bounds("knowledge-leave-confirmation").is_none());
        workspace.read_with(cx, |workspace, cx| {
            assert!(workspace.app_lock.locked);
            assert!(!workspace.ai.state.read(cx).history.quitting);
        });
        assert_eq!(
            oxideterm_ai::rag_get_document_content(&store, &document.id).unwrap(),
            "saved"
        );
        editor.read_with(cx, |editor, _| assert_eq!(editor.draft(), "unsaved"));

        // Authentication is covered by the lock store; resume from its successful unlocked state.
        workspace.update(cx, |workspace, cx| {
            workspace.app_lock.locked = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("knowledge-leave-confirmation").is_some());
        assert!(cx.debug_bounds("menu.unlock_to_finish_quit").is_none());
        cx.simulate_keystrokes("escape");
        workspace.read_with(cx, |workspace, cx| {
            assert!(!workspace.knowledge_state().read(cx).app_quit_pending());
        });
        editor.read_with(cx, |editor, _| assert_eq!(editor.draft(), "unsaved"));
        editor.update(cx, |editor, cx| {
            editor
                .editor
                .update(cx, |text, cx| text.replace_text_external("saved", cx));
        });
        cx.run_until_parked();
    }
}
