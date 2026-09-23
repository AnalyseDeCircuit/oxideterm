use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn enter_forwarding_page(
        &self,
        id: TabId,
        cx: &App,
    ) -> ForwardingPageScope {
        self.forwarding.read(cx).enter_page(Some(id))
    }

    pub(in crate::workspace) fn forwarding_listener<E: ?Sized + 'static>(
        &self,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let page = self.forwarding.read(cx).page_id();
        cx.listener(move |workspace, event, window, cx| {
            let Some(page) = page.filter(|id| workspace.forwarding.read(cx).has_page(*id)) else {
                return;
            };
            let _scope = workspace.enter_forwarding_page(page, cx);
            f(workspace, event, window, cx);
        })
    }
}
