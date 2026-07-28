use super::*;

pub(super) struct DetachedTabWindow {
    workspace: WeakEntity<WorkspaceApp>,
    tab_id: TabId,
    mount_id: tabs::TabMountId,
    entry_handoff_origin: Option<TabWindowHandoffOrigin>,
    entry_handoff_duration: Duration,
    focus_handle: FocusHandle,
    ready: bool,
    applied_window_opacity: Option<f32>,
    _release_subscription: Subscription,
}

impl DetachedTabWindow {
    pub(super) fn new(
        workspace: WeakEntity<WorkspaceApp>,
        tab_id: TabId,
        mount_id: tabs::TabMountId,
        entry_handoff_origin: Option<TabWindowHandoffOrigin>,
        entry_handoff_duration: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let workspace_on_release = workspace.clone();
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
            let _ = workspace_on_release.update(cx, |workspace, cx| {
                workspace.release_detached_tab_window(
                    detached.tab_id,
                    detached.mount_id,
                    window_id,
                    cx,
                );
            });
        });

        Self {
            workspace,
            tab_id,
            mount_id,
            entry_handoff_origin,
            entry_handoff_duration,
            focus_handle,
            ready: false,
            applied_window_opacity: None,
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
        let window_opacity = self
            .workspace
            .read_with(cx, |workspace, _cx| {
                normalized_window_opacity(
                    workspace
                        .settings_store
                        .settings()
                        .appearance
                        .window_opacity,
                )
            })
            .unwrap_or(1.0);
        if self.applied_window_opacity != Some(window_opacity) {
            // Detached tabs own native windows, so they retain an independent
            // applied value while reading the shared persisted preference.
            let _ = apply_window_opacity(window, window_opacity as f64);
            self.applied_window_opacity = Some(window_opacity);
        }
        let tab_id = self.tab_id;
        let entry_handoff_origin = self.entry_handoff_origin;
        let content = if self.ready {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.render_detached_tab_window(tab_id, entry_handoff_origin, window, cx)
                })
                .unwrap_or_else(|_| {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0x9ca3af))
                        .child("Workspace closed")
                        .into_any_element()
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
            .child(content)
    }
}
