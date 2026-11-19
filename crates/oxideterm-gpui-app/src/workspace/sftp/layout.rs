use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn update_sftp_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let view = self.sftp_view();
        let state = view.read(cx);
        if !state.pane_resize_active() && !state.queue_resize_active() {
            return;
        }
        let ended = view.update(cx, |state, cx| {
            state.update_resize(event.position, window.viewport_size(), event.dragging(), cx)
        });
        if ended {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_sftp_resize(&mut self, cx: &mut Context<Self>) {
        if self
            .sftp_view()
            .update(cx, |state, cx| state.finish_resize(cx))
        {
            cx.notify();
        }
    }
}
