use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_session_manager_sidebar_surface(
        &mut self,
        _window: &Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        self.connections.session_manager_sidebar_rows()
    }
}
