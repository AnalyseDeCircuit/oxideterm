use std::{collections::HashMap, path::PathBuf, sync::Arc};

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
    pub acp: Entity<AcpWorkspaceEntity>,
    pub background: Entity<AiBackgroundTaskEntity>,
    pub inline_panel: Entity<AiInlinePanelView>,
    pub transcript: Entity<AiAssistantSidebarView>,
    pub model_picker: AiModelPickerViews,
    pub skills: AiWorkspaceSkills,
    pub _state_subscription: Subscription,
    pub _background_subscription: Subscription,
}

pub(super) struct AiWorkspaceSkills {
    pub registry: Arc<parking_lot::RwLock<oxideterm_skills::SkillRegistry>>,
    pub workspace_root: Option<PathBuf>,
    pub loaded_by_conversation: HashMap<String, HashMap<String, String>>,
}
