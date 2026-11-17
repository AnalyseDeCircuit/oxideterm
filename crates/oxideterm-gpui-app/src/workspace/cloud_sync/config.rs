use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_cloud_sync_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let i18n = &self.i18n;
        self.cloud_sync
            .update(cx, |sync, cx| sync.handle_select_key(event, i18n, cx))
    }
}
