use gpui::{Entity, Subscription};
use oxideterm_gpui_ai::{
    background::{AiBackgroundTaskEntity, AiBackgroundTaskView},
    inline::AiInlinePanelView,
    model_picker::AiModelPickerViews,
    sidebar::{AiChatHeaderView, editor::AiEditorView},
    state::AiWorkspaceEntity,
};

use super::acp_workspace::AcpWorkspaceEntity;

pub(super) struct AiWorkspaceViews {
    pub state: Entity<AiWorkspaceEntity>,
    pub acp: Entity<AcpWorkspaceEntity>,
    pub background: Entity<AiBackgroundTaskEntity>,
    pub background_view: Entity<AiBackgroundTaskView>,
    pub inline_panel: Entity<AiInlinePanelView>,
    pub chat_header: Entity<AiChatHeaderView>,
    pub composer_editor: Entity<AiEditorView>,
    pub message_editor: Entity<AiEditorView>,
    pub model_picker: AiModelPickerViews,
    pub _state_subscription: Subscription,
    pub _background_subscription: Subscription,
}
