use gpui::{Entity, Subscription};
use oxideterm_gpui_ai::{
    background::AiBackgroundTaskEntity,
    inline::AiInlinePanelView,
    model_picker::AiModelPickerViews,
    sidebar::transcript::AiAssistantSidebarView,
    state::AiWorkspaceEntity,
};

use super::acp_workspace::AcpWorkspaceEntity;

pub(super) struct AiWorkspaceViews {
    pub state: Entity<AiWorkspaceEntity>,
    pub authority: Entity<super::ai_runtime_context::AiRuntimeContextEntity>,
    pub conversations: oxideterm_gpui_ai::conversation::AiConversationController<oxideterm_ssh::NodeId, super::SharedTerminalSession>,
    pub acp: Entity<AcpWorkspaceEntity>,
    pub background: Entity<AiBackgroundTaskEntity>,
    pub inline_panel: Entity<AiInlinePanelView>,
    pub transcript: Entity<AiAssistantSidebarView>,
    pub model_picker: AiModelPickerViews,
    pub skills: oxideterm_gpui_ai::state::skills::AiSkillCatalog,
    pub _state_subscription: Subscription,
    pub _background_subscription: Subscription,
}
