use gpui::Context;
use oxideterm_gpui_ai::acp;
use oxideterm_gpui_terminal::SharedTerminalSession;

pub(in crate::workspace) type AcpWorkspaceEntity = acp::AcpWorkspaceEntity<SharedTerminalSession>;

pub(in crate::workspace) fn new_acp_workspace(
    task_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    cx: &mut Context<AcpWorkspaceEntity>,
) -> AcpWorkspaceEntity {
    AcpWorkspaceEntity::new(task_runtime, |session| session.lock().shutdown(), cx)
}

impl super::WorkspaceApp {
    pub(in crate::workspace) fn acp_terminal_bridge(
        cx: &Context<Self>,
    ) -> oxideterm_gpui_ai::conversation::acp_terminal::AcpTerminalBridge<SharedTerminalSession>
    {
        let owner = cx.weak_entity();
        oxideterm_gpui_ai::conversation::acp_terminal::AcpTerminalBridge::for_native_terminal(
            move |config, title, window, cx| {
                owner
                    .update(cx, |app, cx| {
                        app.create_local_terminal_tab_with_owned_session(config, title, window, cx)
                    })
                    .ok()?
                    .ok()
                    .map(|(_, session)| session)
            },
        )
    }
}
