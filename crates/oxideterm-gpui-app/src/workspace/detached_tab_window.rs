use super::*;

pub(super) struct DetachedTabWindow {
    session: Entity<WorkspaceApp>,
    tab_id: TabId,
    mount_id: tabs::TabMountId,
    window_registration: window_registry::WindowRegistration,
    entry_handoff_origin: Option<TabWindowHandoffOrigin>,
    entry_handoff_duration: Duration,
    focus_handle: FocusHandle,
    ready: bool,
    native_style: window_shell::WorkspaceWindowNativeStyle,
    background: Entity<window_shell::WorkspaceWindowBackgroundEntity>,
    _session_observation: Subscription,
    _background_observation: Subscription,
    _release_subscription: Subscription,
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
        window.on_window_should_close(cx, move |_window, cx| {
            session_on_close.update(cx, |session, cx| {
                !session.guard_last_detached_knowledge_window_close(tab_id, cx)
            })
        });
        let session_on_release = session.clone();
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
        // Closing a detached window should behave like docking the tab back
        // into the main tab strip, not like closing the underlying session.
        let release_subscription = cx.on_release_in(window, move |detached, window, cx| {
            let window_id = window.window_handle().window_id();
            session_on_release.update(cx, |session, cx| {
                session.release_detached_tab_window(
                    detached.tab_id,
                    detached.mount_id,
                    detached.window_registration,
                    window_id,
                    cx,
                );
            });
        });

        Self {
            session,
            tab_id,
            mount_id,
            window_registration,
            entry_handoff_origin,
            entry_handoff_duration,
            focus_handle,
            ready: false,
            native_style: window_shell::WorkspaceWindowNativeStyle::unapplied(),
            background,
            _session_observation: session_observation,
            _background_observation: background_observation,
            _release_subscription: release_subscription,
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
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|detached, event: &KeyDownEvent, window, cx| {
                let handled = detached.session.update(cx, |session, cx| {
                    let window_id = window.window_handle().window_id();
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
            .child(content)
    }
}
