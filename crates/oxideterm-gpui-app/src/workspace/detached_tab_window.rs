use super::*;

pub(super) struct DetachedTabWindow {
    workspace: WeakEntity<WorkspaceApp>,
    tab_id: TabId,
    focus_handle: FocusHandle,
    ready: bool,
    _release_subscription: Subscription,
}

impl DetachedTabWindow {
    pub(super) fn new(
        workspace: WeakEntity<WorkspaceApp>,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let workspace_on_release = workspace.clone();
        cx.on_next_frame(window, |detached, _window, cx| {
            detached.ready = true;
            cx.notify();
        });
        // Closing a detached window should behave like docking the tab back
        // into the main tab strip, not like closing the underlying session.
        let release_subscription = cx.on_release_in(window, move |detached, _window, cx| {
            let _ = workspace_on_release.update(cx, |workspace, cx| {
                workspace.return_detached_tab_to_main(detached.tab_id, cx);
            });
        });

        Self {
            workspace,
            tab_id,
            focus_handle,
            ready: false,
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
        let content = if self.ready {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.render_detached_tab_window(tab_id, window, cx)
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
