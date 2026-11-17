use super::*;
use oxideterm_gpui_host_tools::*;

mod health;
mod helpers;
mod lifecycle;
mod modal_keys;
mod multiplexer;
mod runtime;
mod topology;

use helpers::*;

pub(in crate::workspace) use oxideterm_gpui_host_tools::LOCAL_HOST_CONNECTION_ID;
pub(super) use oxideterm_gpui_host_tools::{
    ConnectionRuntimeSection, HostSnapshotFeedback, HostToolsEntity, HostToolsEvent,
    HostToolsMessages, HostToolsNotice, HostToolsTextInput, HostToolsWindowIntent,
    HostToolsWindowModalSnapshot, HostToolsWindowRequest, ScheduleActionNoticeKind,
};

impl WorkspaceApp {
    fn host_tools_selectable_text(&self, cx: &mut Context<Self>) -> HostToolsText {
        let state = self.selectable_text_render_state(cx);
        let group = selectable_text::selectable_document_group_id();
        HostToolsText::new(move |role, scope, key, text, color, cx| {
            let role = match role {
                HostToolsTextRole::PlainDocument => SelectableTextRole::PlainDocument,
                HostToolsTextRole::NonSelectable => SelectableTextRole::NonSelectable,
            };
            state
                .render_display_text_with_role_in_group(role, group, scope, key, 0, text, color, cx)
        })
    }
}
