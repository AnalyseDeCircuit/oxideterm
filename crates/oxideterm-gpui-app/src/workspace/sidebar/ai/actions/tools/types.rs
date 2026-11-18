use oxideterm_ai::tool_loop::AiModelBackendServices;
use oxideterm_ai::tool_resources::AiOrchestratorRuntimeSnapshot;

/// Concrete application adapters used only after the GPUI broker validates a
/// live capability handle. This type must never enter a provider task.
#[derive(Clone)]
pub(in crate::workspace) struct AiLiveToolServices {
    pub(in crate::workspace) node_router: NodeRouter,
    pub(in crate::workspace) sftp_transfer_manager: std::sync::Arc<SftpTransferManager>,
    pub(in crate::workspace) backend_runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

use oxideterm_gpui_ai::conversation::AiAcpChatLaunch;

/// Separates a stale SFTP authority from an operation failure without retaining
/// raw transfer arguments in an error's debug representation.
pub(in crate::workspace) enum AiSftpTransferError {
    OwnerReplaced,
    Operation(String),
}

pub(in crate::workspace) use oxideterm_gpui_ai::stream::AiStreamDeliveryEvent;

#[derive(Clone)]
pub(in crate::workspace) struct AiToolRunContext {
    arguments: zeroize::Zeroizing<String>,
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
}

impl AiToolRunContext {
    fn resource(
        &self,
        workspace: &WorkspaceApp,
        cx: &App,
        kind: oxideterm_ai::agent::OwnedResourceKind,
        label: &str,
    ) -> Option<oxideterm_ai::agent::AgentResourceRecord> {
        let ai = workspace.ai.state.read(cx);
        let run = ai.agent_run(self.generation)?;
        ai.agents
            .services
            .runtime
            .register_resource(&run, kind, AgentText::new(label))
            .ok()
    }
}
