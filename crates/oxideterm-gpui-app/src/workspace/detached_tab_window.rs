use super::*;

pub(super) struct DetachedTabWindow {
    session: Entity<WorkspaceApp>,
    tab_id: TabId,
    entry_handoff_origin: Option<TabWindowHandoffOrigin>,
    entry_handoff_duration: Duration,
    focus_handle: FocusHandle,
    ready: bool,
    native_style: window_shell::WorkspaceWindowNativeStyle,
    background: Entity<window_shell::WorkspaceWindowBackgroundEntity>,
    _session_observation: Subscription,
    _background_observation: Subscription,
    _close_subscription: Subscription,
}

impl DetachedTabWindow {
    pub(super) fn new(
        session: Entity<WorkspaceApp>,
        tab_id: TabId,
        mount_id: tabs::TabMountId,
        window_registration: window_registry::WindowRegistration,
        entry_handoff_origin: Option<TabWindowHandoffOrigin>,
        entry_handoff_duration: Duration,
        background_cache_byte_limit: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let background = window_shell::WorkspaceWindowBackgroundEntity::with_byte_limit(
            background_cache_byte_limit,
            cx,
        );
        let session_observation = window_shell::observe_window_session(&session, cx);
        let background_observation = window_shell::observe_window_background(&background, cx);
        let session_on_close = session.clone();
        window.on_window_should_close(cx, move |window, cx| {
            session_on_close.update(cx, |session, cx| {
                !session.guard_detached_knowledge_window_close(tab_id, window, cx)
            })
        });
        let session_on_close = session.clone();
        cx.on_next_frame(window, |detached, _window, cx| {
            detached.ready = true;
            if detached.entry_handoff_origin.is_some() && !detached.entry_handoff_duration.is_zero()
            {
                let delay = detached.entry_handoff_duration;
                // The relay is a bounded visual snapshot. Drop it after the
                // one-shot transition so detached windows retain no stale state.
                cx.spawn(async move |weak, cx| {
                    Timer::after(delay).await;
                    let _ = weak.update(cx, |detached, cx| {
                        detached.entry_handoff_origin = None;
                        cx.notify();
                    });
                })
                .detach();
            }
            cx.notify();
        });
        // The detached native window owns this tab consumer. Releasing the
        // window closes that tab while shared node-owned transports stay live.
        let close_subscription =
            window_shell::observe_window_close(window, cx, move |window_id, cx| {
                session_on_close.update(cx, |session, cx| {
                    session.release_detached_tab_window(
                        tab_id,
                        mount_id,
                        window_registration,
                        window_id,
                        cx,
                    );
                });
            });

        Self {
            session,
            tab_id,
            entry_handoff_origin,
            entry_handoff_duration,
            focus_handle,
            ready: false,
            native_style: window_shell::WorkspaceWindowNativeStyle::unapplied(),
            background,
            _session_observation: session_observation,
            _background_observation: background_observation,
            _close_subscription: close_subscription,
        }
    }
}

impl Focusable for DetachedTabWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DetachedTabWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_id = self.tab_id;
        let entry_handoff_origin = self.entry_handoff_origin;
        let content = if self.ready {
            // Native style reads and updates the shared session, so it must
            // remain behind the same next-frame gate as detached content.
            self.native_style.apply(&self.session, window, cx);
            self.session.update(cx, |session, cx| {
                session.render_detached_tab_window(
                    tab_id,
                    entry_handoff_origin,
                    &self.background,
                    window,
                    cx,
                )
            })
        } else {
            // GPUI draws a newly opened window synchronously. Wait one frame
            // before reading Workspace so creation never re-enters the source
            // Workspace update that opened this detached window.
            div().size_full().bg(rgb(0x0b0d12)).into_any_element()
        };

        div()
            .id(("detached-tab-window", tab_id.0))
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|detached, event: &MouseMoveEvent, window, cx| {
                detached.session.update(cx, |session, cx| {
                    session.update_detached_tab_return_drag(detached.tab_id, event, window, cx);
                    if session.split_drag_belongs_to_tab(detached.tab_id) {
                        session.update_split_drag(event, window, cx);
                    }
                });
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|detached, event: &MouseUpEvent, window, cx| {
                    detached.session.update(cx, |session, cx| {
                        session.finish_detached_tab_return_drag(detached.tab_id, event, window, cx);
                        if session.split_drag_belongs_to_tab(detached.tab_id) {
                            session.finish_split_drag(cx);
                        }
                    });
                }),
            )
            .capture_key_down(cx.listener(|detached, event: &KeyDownEvent, window, cx| {
                let handled = detached.session.update(cx, |session, cx| {
                    if event.keystroke.key == "escape" && session.cancel_tab_merge_drag(cx) {
                        return true;
                    }
                    let window_id = window.window_handle().window_id();
                    if session.app_lock.locked {
                        return false;
                    }
                    let page = session.tab_host.read(cx).focused_page_id(detached.tab_id);
                    let kind = session.tab_by_id(page, cx).map(|tab| tab.kind.clone());
                    let _sftp_scope = (kind == Some(TabKind::Sftp))
                        .then(|| session.enter_sftp_surface(sftp::SftpSurfaceId::Tab(page)));
                    let _forward_scope = (kind == Some(TabKind::Forwards))
                        .then(|| session.enter_forwarding_page(page, cx));

                    if session
                        .mermaid_zoom
                        .as_ref()
                        .is_some_and(|state| state.window_id == window_id)
                    {
                        if event.keystroke.key == "escape" {
                            session.mermaid_zoom = None;
                            cx.notify();
                        }
                        return true;
                    }
                    if !session.app_lock.locked
                        && let Some(pane_id) = session
                            .tab_by_id(detached.tab_id, cx)
                            .and_then(|tab| tab.active_pane_id)
                    {
                        let input = session.active_ime_target_for_window(window_id, cx);
                        if input.is_none_or(|target| {
                            matches!(target, super::ime::WorkspaceImeTarget::Search(_))
                        }) && crate::keybindings::keystroke_matches_action(
                            &event.keystroke,
                            "terminal.search",
                            &session.settings_store.settings().keybindings.overrides,
                        ) {
                            session.open_search_for_pane(pane_id, window, cx);
                            return true;
                        }
                        if matches!(
                            session.active_ime_target_for_window(window_id, cx),
                            Some(super::ime::WorkspaceImeTarget::Search(_))
                        ) {
                            if session.defer_active_ime_key(&event.keystroke, window, cx) {
                                return false;
                            }
                            if session.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                                || session
                                    .handle_active_text_input_delete_selection(&event.keystroke, cx)
                                || session.handle_active_text_input_transpose(&event.keystroke, cx)
                                || session.handle_active_text_input_navigation(&event.keystroke, cx)
                            {
                                return true;
                            }
                            match event.keystroke.key.as_str() {
                                "escape" => {
                                    session.hide_search(pane_id, cx);
                                    if let Some(pane) =
                                        session.tab_host.read(cx).panes().get(&pane_id).cloned()
                                    {
                                        pane.update(cx, |pane, cx| pane.focus(window, cx));
                                    }
                                    return true;
                                }
                                "enter" => {
                                    session.search_next_for_pane(
                                        pane_id,
                                        !event.keystroke.modifiers.shift,
                                        cx,
                                    );
                                    return true;
                                }
                                _ => {}
                            }
                        }
                    }
                    if !session.app_lock.locked
                        && crate::keybindings::keystroke_matches_action(
                            &event.keystroke,
                            "terminal.aiPanel",
                            &session.settings_store.settings().keybindings.overrides,
                        )
                    {
                        session.toggle_terminal_ai_inline_panel(window, cx);
                        return true;
                    }
                    if session.active_ime_target_for_window(window_id, cx)
                        == Some(super::ime::WorkspaceImeTarget::AiInlinePrompt)
                    {
                        if session.defer_active_ime_key(&event.keystroke, window, cx) {
                            return false;
                        }
                        if session.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                            || session
                                .handle_active_text_input_delete_selection(&event.keystroke, cx)
                            || session.handle_active_text_input_transpose(&event.keystroke, cx)
                            || session.handle_active_text_input_navigation(&event.keystroke, cx)
                        {
                            return true;
                        }
                        return session.handle_ai_inline_panel_key(event, window, cx);
                    }
                    if matches!(
                        session.active_ime_target_for_window(window_id, cx),
                        Some(
                            super::ime::WorkspaceImeTarget::KnowledgeSearch
                                | super::ime::WorkspaceImeTarget::KnowledgeRename
                                | super::ime::WorkspaceImeTarget::ReadOnlyText(_)
                        )
                    ) {
                        if session.defer_active_ime_key(&event.keystroke, window, cx) {
                            return false;
                        }
                        if session.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                            || session
                                .handle_active_text_input_delete_selection(&event.keystroke, cx)
                            || session.handle_active_text_input_newline(&event.keystroke, cx)
                            || session.handle_active_text_input_transpose(&event.keystroke, cx)
                            || session.handle_active_text_input_navigation(&event.keystroke, cx)
                        {
                            return true;
                        }
                        return session.handle_knowledge_input_key(event, window, cx);
                    }
                    if session
                        .ai_entity
                        .read(cx)
                        .knowledge_document_dialog_owned_by(window_id)
                    {
                        if session.defer_active_ime_key(&event.keystroke, window, cx) {
                            return false;
                        }
                        if session.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                            || session
                                .handle_active_text_input_delete_selection(&event.keystroke, cx)
                            || session.handle_active_text_input_newline(&event.keystroke, cx)
                            || session.handle_active_text_input_transpose(&event.keystroke, cx)
                            || session.handle_active_text_input_navigation(&event.keystroke, cx)
                        {
                            return true;
                        }
                        return session.handle_knowledge_document_dialog_key(event, cx);
                    }
                    if matches!(kind, Some(TabKind::Sftp | TabKind::Forwards)) {
                        if session.defer_active_ime_key(&event.keystroke, window, cx) {
                            return false;
                        }
                        if session.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                            || session
                                .handle_active_text_input_delete_selection(&event.keystroke, cx)
                            || session.handle_active_text_input_transpose(&event.keystroke, cx)
                            || session.handle_active_text_input_navigation(&event.keystroke, cx)
                        {
                            return true;
                        }
                        if kind == Some(TabKind::Sftp) {
                            return session.handle_sftp_key(event, window, cx);
                        }
                        if session.handle_forward_delete_confirm_key(event, cx)
                            || session.handle_forward_edit_modal_key(event, cx)
                        {
                            return true;
                        }
                        return session.handle_forwards_key(event, cx);
                    }
                    let is_knowledge_window = session
                        .tabs(cx)
                        .iter()
                        .any(|tab| tab.id == detached.tab_id && tab.kind == TabKind::Knowledge);
                    if is_knowledge_window && session.knowledge_leave_confirmation_open(cx) {
                        session.handle_knowledge_leave_confirmation_key(event, window, cx)
                    } else {
                        false
                    }
                });
                if handled {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|detached, _: &Quit, _window, cx| {
                let intercepted = detached
                    .session
                    .update(cx, |session, cx| session.guard_dirty_knowledge_app_quit(cx));
                if intercepted {
                    cx.stop_propagation();
                } else {
                    // Detached roots also stop actions during bubbling, so clean exits must reach
                    // the application-level handler explicitly.
                    cx.propagate();
                }
            }))
            .map(|root| {
                // Window creation can paint this root while its Workspace is leased.
                // Check readiness before reading the shared keyboard-capture owner.
                if !self.ready {
                    return root;
                }
                let workspace = self.session.read(cx);
                if workspace.app_lock.locked {
                    return root;
                }
                let Some(session) = workspace.remote_desktop_session_entity(tab_id, cx) else {
                    return root;
                };
                remote_desktop::remote_desktop_keyboard_capture(
                    root,
                    session,
                    workspace
                        .settings_store
                        .settings()
                        .keybindings
                        .overrides
                        .clone(),
                )
            })
            .child(window_shell::render_resizable_window_content(
                content, window,
            ))
            .when(
                self.ready
                    && self
                        .session
                        .read(cx)
                        .detached_tab_return_drag
                        .is_some_and(|drag| drag.tab_id == tab_id),
                |root| {
                    root.child(self.session.update(cx, |workspace, cx| {
                        workspace.render_tab_drag_capture(Some(tab_id), cx)
                    }))
                },
            )
    }
}
