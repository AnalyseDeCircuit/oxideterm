use oxideterm_ai::tool_loop::AiModelBackendServices;
use oxideterm_ai::tool_resources::AiOrchestratorRuntimeSnapshot;

use oxideterm_ai::tool_resources::{AiLiveFileOwner, AiLiveToolServices};
use oxideterm_gpui_ai::conversation::AiAcpChatLaunch;

pub(in crate::workspace) use oxideterm_gpui_ai::stream::AiStreamDeliveryEvent;

pub(in crate::workspace) use oxideterm_gpui_ai::state::local_command::AiToolRunContext;

impl WorkspaceApp {
    fn ai_owned_resource(
        &self,
        owner: &AiToolRunContext,
        cx: &App,
        kind: oxideterm_ai::agent::OwnedResourceKind,
        label: &str,
    ) -> Option<oxideterm_ai::agent::AgentResourceRecord> {
        let ai = self.ai.state.read(cx);
        let run = ai.agent_run(owner.generation)?;
        ai.agents
            .services
            .runtime
            .register_resource(&run, kind, AgentText::new(label))
            .ok()
    }
}
