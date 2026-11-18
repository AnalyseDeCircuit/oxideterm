use super::WorkspaceApp;
use gpui::Context;
use oxideterm_public_mcp::{DomainRequest, PublicToolCall, history::HistoryRequest};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_terminal_history(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::TerminalHistory(args) = &request.call else {
            return;
        };
        let HistoryRequest::Snapshot { terminal_ref, .. } = &args.request else {
            return;
        };
        let session = self
            .public_mcp_terminal_pane(&request.client_ref, terminal_ref, cx)
            .ok()
            .map(|(record, pane)| (record.session_id, pane.read(cx).shared_session()));
        self.public_mcp
            .background()
            .terminal_history(request, session);
    }
}
