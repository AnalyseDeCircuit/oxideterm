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
        // The detached native window owns this tab consumer. Releasing the
        // window closes that tab while shared node-owned transports stay live.
        let release_subscription = cx.on_release_in(window, move |detached, window, cx| {
            session_on_release.update(cx, |session, cx| {
                session.release_detached_tab_window(
                    detached.tab_id,
                    detached.mount_id,
                    detached.window_registration,
                    window,
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
            .child(window_shell::render_resizable_window_content(
                content, window,
            ))
    }
}
