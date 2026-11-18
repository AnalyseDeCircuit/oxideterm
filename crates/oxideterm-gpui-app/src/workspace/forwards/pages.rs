use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn enter_forwarding_page(
        &self,
        id: TabId,
        cx: &App,
    ) -> ForwardingPageScope {
        self.forwarding.read(cx).enter_page(Some(id))
    }
}
