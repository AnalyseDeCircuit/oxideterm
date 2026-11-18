use super::WorkspaceApp;
use gpui::Context;
use oxideterm_public_mcp::{ClientRef, DomainRequest, TerminalRef};
use oxideterm_public_mcp_host::{RecordingPaneAction, RecordingPaneSnapshot};
use zeroize::Zeroizing;

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_recording(
        &self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let client = request.client_ref.clone();
        self.public_mcp.recording_request(
            request,
            self.forwarding_runtime.handle(),
            |terminal, action| self.public_mcp_recording_pane(&client, terminal, action, cx),
        );
    }

    fn public_mcp_recording_pane(
        &self,
        client: &ClientRef,
        terminal: &TerminalRef,
        action: RecordingPaneAction,
        cx: &mut Context<Self>,
    ) -> Result<RecordingPaneSnapshot, String> {
        let (_, pane) = self.public_mcp_terminal_pane(client, terminal, cx)?;
        let before = pane.read(cx).recording_status();
        action.validate(before.state)?;
        let size = pane.read(cx).ai_screen_snapshot();
        let stopping = matches!(action, RecordingPaneAction::Stop);
        let stopped_content = match action {
            RecordingPaneAction::Status => None,
            RecordingPaneAction::Start(title) => {
                pane.update(cx, |pane, cx| pane.start_recording(title, cx));
                None
            }
            RecordingPaneAction::Pause => {
                pane.update(cx, |pane, cx| pane.pause_recording(cx));
                None
            }
            RecordingPaneAction::Resume => {
                pane.update(cx, |pane, cx| pane.resume_recording(cx));
                None
            }
            RecordingPaneAction::Stop => pane
                .update(cx, |pane, cx| pane.stop_recording(cx))
                .map(Zeroizing::new),
        };
        let status = if stopping {
            before
        } else {
            pane.read(cx).recording_status()
        };
        Ok(RecordingPaneSnapshot {
            status,
            size: (size.cols, size.rows),
            stopped_content,
        })
    }

    pub(super) fn stop_public_mcp_recordings(
        &self,
        client: &ClientRef,
        terminal: Option<&TerminalRef>,
        forget: bool,
        cx: &mut Context<Self>,
    ) {
        self.public_mcp.stop_recordings(
            client,
            terminal,
            forget,
            self.forwarding_runtime.handle(),
            |terminal, action| self.public_mcp_recording_pane(client, terminal, action, cx),
        );
    }
}
