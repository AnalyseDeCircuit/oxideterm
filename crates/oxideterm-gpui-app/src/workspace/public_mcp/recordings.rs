use std::time::{Duration, Instant};

use gpui::Context;
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PublicToolCall, RecordingExportFormat, RecordingRef,
    RecordingStatusTarget, RecordingsControlArgs, TerminalRef, ToolEnvelope, ToolGroup,
};
use oxideterm_terminal_recording::{
    AsciicastRecording, TerminalRecordingPlayback, TerminalRecordingState,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{PublicMcpRecordingRecord, WorkspaceApp, finish_serialized};

const ACTIVE_RECORDINGS_PER_CLIENT: usize = 4;
const RECORDING_HANDLES_PER_CLIENT: usize = 64;
const RECORDING_CONTENT_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const RECORDING_CONTENT_BYTES_PER_CLIENT: usize = 64 * 1024 * 1024;
const RECORDING_CONTENT_RETENTION: Duration = Duration::from_secs(15 * 60);
const RECORDING_ARTIFACT_MEDIA_TYPE: &str = "application/x-asciicast";
const DEFAULT_RECORDING_ARTIFACT_NAME: &str = "terminal-recording.cast";

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

    pub(super) fn handle_public_mcp_recordings_search(&self, request: DomainRequest) {
        let PublicToolCall::RecordingsSearch(args) = &request.call else {
            return;
        };
        let args = args.clone();
        let Some(record) = self
            .public_mcp
            .recordings
            .get(&args.recording_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The recording handle is unavailable"));
            return;
        };
        let Some(content) = record.content.as_deref() else {
            request.finish(ToolEnvelope::failed(
                "The stopped recording content is no longer available",
            ));
            return;
        };
        let recording = match AsciicastRecording::parse(DEFAULT_RECORDING_ARTIFACT_NAME, content) {
            Ok(recording) => recording,
            Err(_) => {
                request.finish(ToolEnvelope::failed(
                    "The retained recording content is invalid",
                ));
                return;
            }
        };
        let search_results = TerminalRecordingPlayback::new(recording).search(&args.query);
        let truncated = search_results.len() > args.limit as usize;
        let matches = search_results
            .into_iter()
            .take(args.limit as usize)
            .map(|entry| json!({ "at_seconds": entry.at, "snippet": entry.snippet }))
            .collect::<Vec<_>>();
        finish_serialized(
            request,
            json!({
                "recording_ref": args.recording_ref,
                "matches": matches,
                "truncated": truncated,
            }),
        );
    }

    pub(super) fn handle_public_mcp_recordings_export(&mut self, request: DomainRequest) {
        let PublicToolCall::RecordingsExport(args) = &request.call else {
            return;
        };
        let args = args.clone();
        let Some(record) = self
            .public_mcp
            .recordings
            .get(&args.recording_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The recording handle is unavailable"));
            return;
        };
        let Some(content) = record.content.as_deref() else {
            request.finish(ToolEnvelope::failed(
                "The stopped recording content is no longer available",
            ));
            return;
        };
        let media_type = match args.format {
            RecordingExportFormat::AsciicastV2 => RECORDING_ARTIFACT_MEDIA_TYPE,
        };
        let name = args
            .name
            .clone()
            .or_else(|| Some(DEFAULT_RECORDING_ARTIFACT_NAME.to_owned()));
        let artifact_store = self.public_mcp.state.artifacts.clone();
        let artifact = match artifact_store.stage(
            request.client_ref.clone(),
            content.as_bytes(),
            media_type.to_owned(),
            name,
        ) {
            Ok(artifact) => artifact,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error.to_string()));
                return;
            }
        };
        if let Some(record) = self.public_mcp.recordings.get_mut(&args.recording_ref) {
            record.artifact_refs.retain(|artifact_ref| {
                artifact_store.is_available(&request.client_ref, artifact_ref)
            });
            record.artifact_refs.insert(artifact.artifact_ref.clone());
        }
        finish_serialized(
            request,
            json!({
                "recording_ref": args.recording_ref,
                "format": args.format,
                "artifact": artifact,
            }),
        );
    }

    fn start_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        terminal_ref: TerminalRef,
        title: Option<String>,
        capture_input: bool,
        cx: &mut Context<Self>,
    ) -> Result<RecordingRef, String> {
        if capture_input {
            return Err("Terminal input capture is not supported by the recorder".to_owned());
        }
        if self.public_mcp.recordings.values().any(|record| {
            record.client_ref == *client_ref && record.terminal_ref == terminal_ref && record.active
        }) {
            return Err("This terminal already has an active MCP recording".to_owned());
        }
        let active_count = self
            .public_mcp
            .recordings
            .values()
            .filter(|record| record.client_ref == *client_ref && record.active)
            .count();
        if active_count >= ACTIVE_RECORDINGS_PER_CLIENT {
            return Err("The client already owns the maximum active recordings".to_owned());
        }
        self.make_public_mcp_recording_capacity(client_ref)?;
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        if pane.read(cx).recording_status().state != TerminalRecordingState::Idle {
            return Err("The terminal is already being recorded by OxideTerm".to_owned());
        }
        let snapshot = pane.read(cx).ai_screen_snapshot();
        pane.update(cx, |pane, cx| pane.start_recording(title, cx));
        let recording_ref = RecordingRef::new();
        self.public_mcp.recordings.insert(
            recording_ref.clone(),
            PublicMcpRecordingRecord {
                client_ref: client_ref.clone(),
                terminal_ref,
                cols: snapshot.cols,
                rows: snapshot.rows,
                elapsed_ms: 0,
                event_count: 0,
                active: true,
                truncated: false,
                stopped_at: None,
                content: None,
                artifact_refs: Default::default(),
            },
        );
        Ok(recording_ref)
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
            .recordings
            .get(recording_ref)
            .filter(|record| record.client_ref == *client_ref && record.active)
            .map(|record| record.terminal_ref.clone())
            .ok_or_else(|| "The active recording handle is unavailable".to_owned())
    }

    fn stop_public_mcp_recording(
        &mut self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(record) = self
            .public_mcp
            .recordings
            .get(recording_ref)
            .filter(|record| record.client_ref == *client_ref)
        else {
            return Err("The recording handle is unavailable".to_owned());
        };
        if !record.active {
            return Ok(());
        }
        let terminal_ref = record.terminal_ref.clone();
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, &terminal_ref, cx)?;
        let status = pane.read(cx).recording_status();
        let snapshot = pane.read(cx).ai_screen_snapshot();
        let Some(content) = pane.update(cx, |pane, cx| pane.stop_recording(cx)) else {
            return Err("The terminal recorder is no longer active".to_owned());
        };
        let mut content = Zeroizing::new(content);
        let truncated = truncate_recording_content(&mut content);
        let record = self
            .public_mcp
            .recordings
            .get_mut(recording_ref)
            .expect("recording ownership was checked before stopping");
        record.cols = snapshot.cols;
        record.rows = snapshot.rows;
        record.elapsed_ms = duration_ms(status.elapsed);
        record.event_count = status.event_count;
        record.active = false;
        record.truncated = truncated;
        let stopped_at = Instant::now();
        record.stopped_at = Some(stopped_at);
        let content_enabled = self
            .public_mcp
            .state
            .clients
            .get(client_ref)
            .is_some_and(|client| client.tool_groups.contains(&ToolGroup::RecordingContent));
        // Recording control may remain enabled after content access is revoked.
        record.content = content_enabled.then_some(content);
        self.enforce_public_mcp_recording_content_budget(client_ref);
        let client_ref = client_ref.clone();
        let recording_ref = recording_ref.clone();
        cx.spawn(async move |workspace, cx| {
            gpui::Timer::after(RECORDING_CONTENT_RETENTION).await;
            let _ = workspace.update(cx, |workspace, _cx| {
                let Some(record) = workspace
                    .public_mcp
                    .recordings
                    .get_mut(&recording_ref)
                    .filter(|record| {
                        record.client_ref == client_ref && record.stopped_at == Some(stopped_at)
                    })
                else {
                    return;
                };
                // Stopped terminal output must not remain resident without a bounded lifetime.
                record.content.take();
            });
        })
        .detach();
        Ok(())
    }

    fn public_mcp_recording_projection(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        cx: &gpui::App,
    ) -> Result<Value, String> {
        let record = self
            .public_mcp
            .recordings
            .get(recording_ref)
            .filter(|record| record.client_ref == *client_ref)
            .ok_or_else(|| "The recording handle is unavailable".to_owned())?;
        if record.active {
            let (_, pane) = self.public_mcp_terminal_pane(client_ref, &record.terminal_ref, cx)?;
            let status = pane.read(cx).recording_status();
            let snapshot = pane.read(cx).ai_screen_snapshot();
            return Ok(recording_projection(
                Some(recording_ref),
                &record.terminal_ref,
                recording_state_name(status.state),
                duration_ms(status.elapsed),
                status.event_count,
                snapshot.cols,
                snapshot.rows,
                true,
                false,
                false,
            ));
        }
        Ok(recording_projection(
            Some(recording_ref),
            &record.terminal_ref,
            "stopped",
            record.elapsed_ms,
            record.event_count,
            record.cols,
            record.rows,
            true,
            record.content.is_some(),
            record.truncated,
        ))
    }

    fn public_mcp_terminal_recording_projection(
        &self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        cx: &gpui::App,
    ) -> Result<Value, String> {
        if let Some(recording_ref) =
            self.public_mcp
                .recordings
                .iter()
                .find_map(|(recording_ref, record)| {
                    (record.client_ref == *client_ref
                        && record.terminal_ref == *terminal_ref
                        && record.active)
                        .then_some(recording_ref)
                })
        {
            return self.public_mcp_recording_projection(client_ref, recording_ref, cx);
        }
        let (_, pane) = self.public_mcp_terminal_pane(client_ref, terminal_ref, cx)?;
        let status = pane.read(cx).recording_status();
        let snapshot = pane.read(cx).ai_screen_snapshot();
        Ok(recording_projection(
            None,
            terminal_ref,
            recording_state_name(status.state),
            duration_ms(status.elapsed),
            status.event_count,
            snapshot.cols,
            snapshot.rows,
            false,
            false,
            false,
        ))
    }

    pub(super) fn stop_public_mcp_recordings_for_terminal(
        &mut self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        cx: &mut Context<Self>,
    ) {
        let recording_refs = self
            .public_mcp
            .recordings
            .iter()
            .filter_map(|(recording_ref, record)| {
                (record.client_ref == *client_ref
                    && record.terminal_ref == *terminal_ref
                    && record.active)
                    .then_some(recording_ref.clone())
            })
            .collect::<Vec<_>>();
        for recording_ref in recording_refs {
            let _ = self.stop_public_mcp_recording(client_ref, &recording_ref, cx);
        }
    }

    pub(super) fn stop_public_mcp_client_recordings(
        &mut self,
        client_ref: &ClientRef,
        cx: &mut Context<Self>,
    ) {
        let recording_refs = self
            .public_mcp
            .recordings
            .iter()
            .filter_map(|(recording_ref, record)| {
                (record.client_ref == *client_ref && record.active).then_some(recording_ref.clone())
            })
            .collect::<Vec<_>>();
        for recording_ref in recording_refs {
            let _ = self.stop_public_mcp_recording(client_ref, &recording_ref, cx);
        }
    }

    pub(super) fn revoke_public_mcp_client_recording_content(&mut self, client_ref: &ClientRef) {
        for record in self
            .public_mcp
            .recordings
            .values_mut()
            .filter(|record| record.client_ref == *client_ref)
        {
            // Dropping Zeroizing content makes the read revocation irreversible.
            record.content.take();
            for artifact_ref in record.artifact_refs.drain() {
                self.public_mcp
                    .state
                    .artifacts
                    .revoke(client_ref, &artifact_ref);
            }
        }
    }

    pub(super) fn revoke_public_mcp_client_recordings(
        &mut self,
        client_ref: &ClientRef,
        cx: &mut Context<Self>,
    ) {
        self.stop_public_mcp_client_recordings(client_ref, cx);
        self.revoke_public_mcp_client_recording_content(client_ref);
        self.public_mcp
            .recordings
            .retain(|_, record| record.client_ref != *client_ref);
    }

    fn make_public_mcp_recording_capacity(&mut self, client_ref: &ClientRef) -> Result<(), String> {
        let client_count = self
            .public_mcp
            .recordings
            .values()
            .filter(|record| record.client_ref == *client_ref)
            .count();
        if client_count < RECORDING_HANDLES_PER_CLIENT {
            return Ok(());
        }
        let oldest = self
            .public_mcp
            .recordings
            .iter()
            .filter(|(_, record)| record.client_ref == *client_ref && !record.active)
            .min_by_key(|(_, record)| record.stopped_at)
            .map(|(recording_ref, _)| recording_ref.clone())
            .ok_or_else(|| "The client recording handle capacity has been reached".to_owned())?;
        if let Some(record) = self.public_mcp.recordings.remove(&oldest) {
            for artifact_ref in record.artifact_refs {
                self.public_mcp
                    .state
                    .artifacts
                    .revoke(client_ref, &artifact_ref);
            }
        }
        Ok(())
    }

    fn enforce_public_mcp_recording_content_budget(&mut self, client_ref: &ClientRef) {
        while self
            .public_mcp
            .recordings
            .values()
            .filter(|record| record.client_ref == *client_ref)
            .filter_map(|record| record.content.as_ref())
            .map(|content| content.len())
            .sum::<usize>()
            > RECORDING_CONTENT_BYTES_PER_CLIENT
        {
            let oldest = self
                .public_mcp
                .recordings
                .iter()
                .filter(|(_, record)| record.client_ref == *client_ref && record.content.is_some())
                .min_by_key(|(_, record)| record.stopped_at)
                .map(|(recording_ref, _)| recording_ref.clone());
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(record) = self.public_mcp.recordings.get_mut(&oldest) {
                // Expiring the oldest retained body keeps metadata and any exported artifact valid.
                record.content.take();
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the projection keeps sensitive content separate from recording metadata"
)]
fn recording_projection(
    recording_ref: Option<&RecordingRef>,
    terminal_ref: &TerminalRef,
    state: &'static str,
    elapsed_ms: u64,
    event_count: usize,
    cols: usize,
    rows: usize,
    managed_by_client: bool,
    content_available: bool,
    truncated: bool,
) -> Value {
    json!({
        "recording_ref": recording_ref,
        "terminal_ref": terminal_ref,
        "state": state,
        "elapsed_ms": elapsed_ms,
        "event_count": event_count,
        "cols": cols,
        "rows": rows,
        "managed_by_client": managed_by_client,
        "capture_input": false,
        "content_available": content_available,
        "truncated": truncated,
    })
}

fn recording_state_name(state: TerminalRecordingState) -> &'static str {
    match state {
        TerminalRecordingState::Idle => "idle",
        TerminalRecordingState::Recording => "recording",
        TerminalRecordingState::Paused => "paused",
    }
}

fn duration_ms(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn truncate_recording_content(content: &mut String) -> bool {
    if content.len() <= RECORDING_CONTENT_LIMIT_BYTES {
        return false;
    }
    let mut boundary = RECORDING_CONTENT_LIMIT_BYTES;
    while boundary > 0 && !content.is_char_boundary(boundary) {
        boundary -= 1;
    }
    if let Some(line_end) = content[..boundary].rfind('\n') {
        boundary = line_end + 1;
    }
    content.truncate(boundary);
    true
}
