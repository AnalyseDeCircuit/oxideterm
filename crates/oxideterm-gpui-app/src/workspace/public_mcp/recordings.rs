use gpui::Context;
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PublicToolCall, RecordingRef, RecordingStatusTarget,
    RecordingsControlArgs, TerminalRef, ToolEnvelope,
};
use oxideterm_public_mcp_host::{PublicMcpRecordingStatus, live_recording_projection};
use oxideterm_terminal_recording::TerminalRecordingState;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{WorkspaceApp, finish_serialized};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_recordings_control(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let action = match &request.call {
            PublicToolCall::RecordingsControl(action) => action.clone(),
            _ => return,
        };
        let result = match action {
            RecordingsControlArgs::Start {
                terminal_ref,
                title,
                capture_input,
            } => self.start_public_mcp_recording(
                &request.client_ref,
                terminal_ref,
                title,
                capture_input,
                cx,
            ),
            RecordingsControlArgs::Pause { recording_ref } => self
                .pause_public_mcp_recording(&request.client_ref, &recording_ref, cx)
                .map(|()| recording_ref),
            RecordingsControlArgs::Resume { recording_ref } => self
                .resume_public_mcp_recording(&request.client_ref, &recording_ref, cx)
                .map(|()| recording_ref),
            RecordingsControlArgs::Stop { recording_ref } => self
                .stop_public_mcp_recording(&request.client_ref, &recording_ref, cx)
                .map(|()| recording_ref),
        };
        match result.and_then(|recording_ref| {
            self.public_mcp_recording_projection(&request.client_ref, &recording_ref, cx)
        }) {
            Ok(recording) => finish_serialized(request, json!({ "recording": recording })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_recordings_status(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::RecordingsStatus(args) = &request.call else {
            return;
        };
        let result = match &args.target {
            RecordingStatusTarget::Recording { recording_ref } => {
                self.public_mcp_recording_projection(&request.client_ref, recording_ref, cx)
            }
            RecordingStatusTarget::Terminal { terminal_ref } => {
                self.public_mcp_terminal_recording_projection(&request.client_ref, terminal_ref, cx)
            }
        };
        match result {
            Ok(recording) => finish_serialized(request, json!({ "recording": recording })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    fn start_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        terminal_ref: TerminalRef,
        title: Option<String>,
        capture_input: bool,
        cx: &mut Context<Self>,
    ) -> Result<RecordingRef, String> {
        self.public_mcp
            .check_recording_start(client_ref, &terminal_ref, capture_input)?;
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        if pane.read(cx).recording_status().state != TerminalRecordingState::Idle {
            return Err("The terminal is already being recorded by OxideTerm".to_owned());
        }
        let snapshot = pane.read(cx).ai_screen_snapshot();
        pane.update(cx, |pane, cx| pane.start_recording(title, cx));
        Ok(self.public_mcp.register_recording(
            client_ref,
            terminal_ref,
            (snapshot.cols, snapshot.rows),
        ))
    }

    fn pause_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let terminal_ref = self.active_public_mcp_recording_terminal(client_ref, recording_ref)?;
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        if pane.read(cx).recording_status().state != TerminalRecordingState::Recording {
            return Err("The recording is not currently running".to_owned());
        }
        pane.update(cx, |pane, cx| pane.pause_recording(cx));
        Ok(())
    }

    fn resume_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let terminal_ref = self.active_public_mcp_recording_terminal(client_ref, recording_ref)?;
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        if pane.read(cx).recording_status().state != TerminalRecordingState::Paused {
            return Err("The recording is not paused".to_owned());
        }
        pane.update(cx, |pane, cx| pane.resume_recording(cx));
        Ok(())
    }

    fn active_public_mcp_recording_terminal(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
    ) -> Result<TerminalRef, String> {
        self.public_mcp
            .recording_terminal(client_ref, recording_ref)
            .ok()
            .flatten()
            .ok_or_else(|| "The active recording handle is unavailable".to_owned())
    }

    fn stop_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(terminal_ref) = self
            .public_mcp
            .recording_terminal(client_ref, recording_ref)?
        else {
            return Ok(());
        };
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        let status = pane.read(cx).recording_status();
        let snapshot = pane.read(cx).ai_screen_snapshot();
        let Some(content) = pane.update(cx, |pane, cx| pane.stop_recording(cx)) else {
            return Err("The terminal recorder is no longer active".to_owned());
        };
        self.public_mcp.finish_recording(
            client_ref,
            recording_ref,
            status,
            (snapshot.cols, snapshot.rows),
            Zeroizing::new(content),
            self.forwarding_runtime.handle(),
        )
    }

    fn public_mcp_recording_projection(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &gpui::App,
    ) -> Result<Value, String> {
        match self
            .public_mcp
            .recording_status(client_ref, recording_ref)?
        {
            PublicMcpRecordingStatus::Live(terminal_ref) => self
                .public_mcp_live_recording_projection(
                    client_ref,
                    &terminal_ref,
                    Some(recording_ref),
                    cx,
                ),
            PublicMcpRecordingStatus::Stopped(value) => Ok(value),
        }
    }

    fn public_mcp_terminal_recording_projection(
        &self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        cx: &gpui::App,
    ) -> Result<Value, String> {
        let recording_ref = self
            .public_mcp
            .live_recordings(client_ref, Some(terminal_ref))
            .into_iter()
            .next();
        self.public_mcp_live_recording_projection(
            client_ref,
            terminal_ref,
            recording_ref.as_ref(),
            cx,
        )
    }

    fn public_mcp_live_recording_projection(
        &self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        recording_ref: Option<&RecordingRef>,
        cx: &gpui::App,
    ) -> Result<Value, String> {
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, terminal_ref, cx)?;
        let status = pane.read(cx).recording_status();
        let snapshot = pane.read(cx).ai_screen_snapshot();
        Ok(live_recording_projection(
            recording_ref,
            terminal_ref,
            status,
            (snapshot.cols, snapshot.rows),
        ))
    }

    pub(super) fn stop_public_mcp_recordings_for_terminal(
        &mut self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        cx: &mut Context<Self>,
    ) {
        for recording_ref in self
            .public_mcp
            .live_recordings(client_ref, Some(terminal_ref))
        {
            let _ = self.stop_public_mcp_recording(client_ref, &recording_ref, cx);
        }
    }

    pub(super) fn stop_public_mcp_client_recordings(
        &mut self,
        client_ref: &ClientRef,
        cx: &mut Context<Self>,
    ) {
        for recording_ref in self.public_mcp.live_recordings(client_ref, None) {
            let _ = self.stop_public_mcp_recording(client_ref, &recording_ref, cx);
        }
    }

    pub(super) fn revoke_public_mcp_client_recordings(
        &mut self,
        client_ref: &ClientRef,
        cx: &mut Context<Self>,
    ) {
        self.stop_public_mcp_client_recordings(client_ref, cx);
        self.public_mcp.forget_client_recordings(client_ref);
    }
}
