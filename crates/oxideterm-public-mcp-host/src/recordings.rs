use std::time::{Duration, Instant};

use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PublicToolCall, RecordingExportFormat, RecordingRef,
    RecordingStatusTarget, RecordingsControlArgs, TerminalRef, ToolEnvelope, ToolGroup,
};
use oxideterm_terminal_recording::{
    AsciicastRecording, TerminalRecordingPlayback, TerminalRecordingState, TerminalRecordingStatus,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{PublicMcpBackground, PublicMcpHost, PublicMcpRecordingRecord, finish_serialized};

const ACTIVE_RECORDINGS_PER_CLIENT: usize = 4;
const RECORDING_HANDLES_PER_CLIENT: usize = 64;
const RECORDING_CONTENT_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const RECORDING_CONTENT_BYTES_PER_CLIENT: usize = 64 * 1024 * 1024;
const RECORDING_CONTENT_RETENTION: Duration = Duration::from_secs(15 * 60);
const RECORDING_ARTIFACT_MEDIA_TYPE: &str = "application/x-asciicast";
const DEFAULT_RECORDING_ARTIFACT_NAME: &str = "terminal-recording.cast";

enum PublicMcpRecordingStatus {
    Live(TerminalRef),
    Stopped(Value),
}

pub enum RecordingPaneAction {
    Status,
    Start(Option<String>),
    Pause,
    Resume,
    Stop,
}

impl RecordingPaneAction {
    pub fn validate(&self, state: TerminalRecordingState) -> Result<(), String> {
        let error = match self {
            Self::Start(_) if state != TerminalRecordingState::Idle => {
                "The terminal is already being recorded by OxideTerm"
            }
            Self::Pause if state != TerminalRecordingState::Recording => {
                "The recording is not currently running"
            }
            Self::Resume if state != TerminalRecordingState::Paused => {
                "The recording is not paused"
            }
            _ => return Ok(()),
        };
        Err(error.to_owned())
    }
}

pub struct RecordingPaneSnapshot {
    pub status: TerminalRecordingStatus,
    pub size: (usize, usize),
    pub stopped_content: Option<Zeroizing<String>>,
}

type RecordingPaneResult = Result<RecordingPaneSnapshot, String>;

impl PublicMcpHost {
    pub fn recording_request(
        &self,
        request: DomainRequest,
        runtime: &tokio::runtime::Handle,
        mut pane: impl FnMut(&TerminalRef, RecordingPaneAction) -> RecordingPaneResult,
    ) {
        let client = &request.client_ref;
        let result = (|| match &request.call {
            PublicToolCall::RecordingsControl(action) => match action {
                RecordingsControlArgs::Start {
                    terminal_ref,
                    title,
                    capture_input,
                } => {
                    self.check_recording_start(client, terminal_ref, *capture_input)?;
                    let snapshot = pane(terminal_ref, RecordingPaneAction::Start(title.clone()))?;
                    let id = self.register_recording(client, terminal_ref.clone(), snapshot.size);
                    Ok(project_live_recording(Some(&id), terminal_ref, snapshot))
                }
                RecordingsControlArgs::Pause { recording_ref }
                | RecordingsControlArgs::Resume { recording_ref } => {
                    let terminal = self
                        .recording_terminal(client, recording_ref)
                        .ok()
                        .flatten()
                        .ok_or_else(|| "The active recording handle is unavailable".to_owned())?;
                    let action = if matches!(action, RecordingsControlArgs::Pause { .. }) {
                        RecordingPaneAction::Pause
                    } else {
                        RecordingPaneAction::Resume
                    };
                    let snapshot = pane(&terminal, action)?;
                    Ok(project_live_recording(
                        Some(recording_ref),
                        &terminal,
                        snapshot,
                    ))
                }
                RecordingsControlArgs::Stop { recording_ref } => {
                    self.stop_recording(client, recording_ref, runtime, &mut pane)?;
                    self.project_recording(client, recording_ref, &mut pane)
                }
            },
            PublicToolCall::RecordingsStatus(args) => match &args.target {
                RecordingStatusTarget::Recording { recording_ref } => {
                    self.project_recording(client, recording_ref, &mut pane)
                }
                RecordingStatusTarget::Terminal { terminal_ref } => {
                    let id = self
                        .live_recordings(client, Some(terminal_ref))
                        .into_iter()
                        .next();
                    let snapshot = pane(terminal_ref, RecordingPaneAction::Status)?;
                    Ok(project_live_recording(id.as_ref(), terminal_ref, snapshot))
                }
            },
            _ => unreachable!(),
        })();
        match result {
            Ok(recording) => finish_serialized(request, json!({ "recording": recording })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub fn stop_recordings(
        &self,
        client: &ClientRef,
        terminal: Option<&TerminalRef>,
        forget: bool,
        runtime: &tokio::runtime::Handle,
        mut pane: impl FnMut(&TerminalRef, RecordingPaneAction) -> RecordingPaneResult,
    ) {
        for id in self.live_recordings(client, terminal) {
            let _ = self.stop_recording(client, &id, runtime, &mut pane);
        }
        if forget {
            self.forget_client_recordings(client);
        }
    }

    fn stop_recording(
        &self,
        client: &ClientRef,
        id: &RecordingRef,
        runtime: &tokio::runtime::Handle,
        pane: &mut impl FnMut(&TerminalRef, RecordingPaneAction) -> RecordingPaneResult,
    ) -> Result<(), String> {
        let Some(terminal) = self.recording_terminal(client, id)? else {
            return Ok(());
        };
        let snapshot = pane(&terminal, RecordingPaneAction::Stop)?;
        let content = snapshot
            .stopped_content
            .ok_or_else(|| "The terminal recorder is no longer active".to_owned())?;
        self.finish_recording(client, id, snapshot.status, snapshot.size, content, runtime)
    }

    fn project_recording(
        &self,
        client: &ClientRef,
        id: &RecordingRef,
        pane: &mut impl FnMut(&TerminalRef, RecordingPaneAction) -> RecordingPaneResult,
    ) -> Result<Value, String> {
        match self.recording_status(client, id)? {
            PublicMcpRecordingStatus::Live(terminal) => {
                let snapshot = pane(&terminal, RecordingPaneAction::Status)?;
                Ok(project_live_recording(Some(id), &terminal, snapshot))
            }
            PublicMcpRecordingStatus::Stopped(value) => Ok(value),
        }
    }
}

fn project_live_recording(
    id: Option<&RecordingRef>,
    terminal: &TerminalRef,
    snapshot: RecordingPaneSnapshot,
) -> Value {
    live_recording_projection(id, terminal, snapshot.status, snapshot.size)
}

impl PublicMcpBackground {
    /// Only active recorders need a pane to report their current state.
    pub(crate) fn stopped_recording_status(
        &self,
        request: DomainRequest,
        recording_ref: RecordingRef,
    ) -> Result<(), DomainRequest> {
        let status = recording_status(&self.recordings.lock(), &request.client_ref, &recording_ref);
        match status {
            Ok(PublicMcpRecordingStatus::Live(_)) => Err(request),
            Ok(PublicMcpRecordingStatus::Stopped(value)) => {
                finish_serialized(request, json!({ "recording": value }));
                Ok(())
            }
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                Ok(())
            }
        }
    }

    pub(crate) fn handle_public_mcp_recordings_search(&self, request: DomainRequest) {
        let PublicToolCall::RecordingsSearch(args) = &request.call else {
            return;
        };
        let args = args.clone();
        let records = self.recordings.lock();
        let Some(record) = records
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

    pub(crate) fn handle_public_mcp_recordings_export(&self, request: DomainRequest) {
        let PublicToolCall::RecordingsExport(args) = &request.call else {
            return;
        };
        let args = args.clone();
        let mut records = self.recordings.lock();
        let Some(record) = records
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
        let artifact_store = self.state.artifacts.clone();
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
        if let Some(record) = records.get_mut(&args.recording_ref) {
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
}

impl PublicMcpHost {
    fn check_recording_start(
        &self,
        client_ref: &ClientRef,
        terminal_ref: &TerminalRef,
        capture_input: bool,
    ) -> Result<(), String> {
        if capture_input {
            return Err("Terminal input capture is not supported by the recorder".to_owned());
        }
        let records = self.recordings.lock();
        let active = records
            .values()
            .filter(|record| record.client_ref == *client_ref && record.active);
        if active
            .clone()
            .any(|record| &record.terminal_ref == terminal_ref)
        {
            return Err("This terminal already has an active MCP recording".to_owned());
        }
        if active.count() >= ACTIVE_RECORDINGS_PER_CLIENT {
            return Err("The client already owns the maximum active recordings".to_owned());
        }
        drop(records);
        self.make_public_mcp_recording_capacity(client_ref)
    }

    fn register_recording(
        &self,
        client_ref: &ClientRef,
        terminal_ref: TerminalRef,
        (cols, rows): (usize, usize),
    ) -> RecordingRef {
        let recording_ref = RecordingRef::new();
        self.recordings.lock().insert(
            recording_ref.clone(),
            PublicMcpRecordingRecord {
                client_ref: client_ref.clone(),
                terminal_ref,
                cols,
                rows,
                elapsed_ms: 0,
                event_count: 0,
                active: true,
                truncated: false,
                stopped_at: None,
                content: None,
                artifact_refs: Default::default(),
            },
        );
        recording_ref
    }

    fn live_recordings(
        &self,
        client_ref: &ClientRef,
        terminal_ref: Option<&TerminalRef>,
    ) -> Vec<RecordingRef> {
        self.recordings
            .lock()
            .iter()
            .filter(|(_, record)| {
                record.client_ref == *client_ref
                    && record.active
                    && terminal_ref.is_none_or(|terminal| &record.terminal_ref == terminal)
            })
            .map(|(recording_ref, _)| recording_ref.clone())
            .collect()
    }

    fn recording_status(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
    ) -> Result<PublicMcpRecordingStatus, String> {
        recording_status(&self.recordings.lock(), client_ref, recording_ref)
    }

    fn recording_terminal(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
    ) -> Result<Option<TerminalRef>, String> {
        self.recordings
            .lock()
            .get(recording_ref)
            .filter(|record| record.client_ref == *client_ref)
            .map(|record| record.active.then(|| record.terminal_ref.clone()))
            .ok_or_else(|| "The recording handle is unavailable".to_owned())
    }

    fn finish_recording(
        &self,
        client_ref: &ClientRef,
        recording_ref: &RecordingRef,
        status: TerminalRecordingStatus,
        (cols, rows): (usize, usize),
        mut content: Zeroizing<String>,
        runtime: &tokio::runtime::Handle,
    ) -> Result<(), String> {
        let stopped_at = Instant::now();
        let content_enabled = self
            .state
            .clients
            .get(client_ref)
            .is_some_and(|client| client.tool_groups.contains(&ToolGroup::RecordingContent));
        let truncated = truncate_recording_content(&mut content);
        let mut records = self.recordings.lock();
        let record = records
            .get_mut(recording_ref)
            .filter(|record| record.client_ref == *client_ref && record.active)
            .ok_or_else(|| "The active recording handle is unavailable".to_owned())?;
        record.cols = cols;
        record.rows = rows;
        record.elapsed_ms = duration_ms(status.elapsed);
        record.event_count = status.event_count;
        record.active = false;
        record.truncated = truncated;
        record.stopped_at = Some(stopped_at);
        // Recording control may remain enabled after content access is revoked.
        record.content = content_enabled.then_some(content);
        drop(records);
        self.enforce_public_mcp_recording_content_budget(client_ref);
        self.schedule_recording_expiry(runtime, recording_ref.clone(), stopped_at);
        Ok(())
    }

    fn forget_client_recordings(&self, client_ref: &ClientRef) {
        self.revoke_public_mcp_client_recording_content(client_ref);
        self.recordings
            .lock()
            .retain(|_, record| record.client_ref != *client_ref);
    }

    fn schedule_recording_expiry(
        &self,
        runtime: &tokio::runtime::Handle,
        recording_ref: RecordingRef,
        stopped_at: Instant,
    ) {
        let recordings = std::sync::Arc::downgrade(&self.recordings);
        runtime.spawn(async move {
            tokio::time::sleep(RECORDING_CONTENT_RETENTION).await;
            if let Some(recordings) = recordings.upgrade()
                && let Some(record) = recordings
                    .lock()
                    .get_mut(&recording_ref)
                    .filter(|record| record.stopped_at == Some(stopped_at))
            {
                record.content.take();
            }
        });
    }

    pub fn revoke_public_mcp_client_recording_content(&self, client_ref: &ClientRef) {
        let mut records = self.recordings.lock();
        for record in records
            .values_mut()
            .filter(|record| record.client_ref == *client_ref)
        {
            // Dropping Zeroizing content makes the read revocation irreversible.
            record.content.take();
            for artifact_ref in record.artifact_refs.drain() {
                self.state.artifacts.revoke(client_ref, &artifact_ref);
            }
        }
    }

    fn make_public_mcp_recording_capacity(&self, client_ref: &ClientRef) -> Result<(), String> {
        let mut records = self.recordings.lock();
        let client_count = records
            .values()
            .filter(|record| record.client_ref == *client_ref)
            .count();
        if client_count < RECORDING_HANDLES_PER_CLIENT {
            return Ok(());
        }
        let oldest = records
            .iter()
            .filter(|(_, record)| record.client_ref == *client_ref && !record.active)
            .min_by_key(|(_, record)| record.stopped_at)
            .map(|(recording_ref, _)| recording_ref.clone())
            .ok_or_else(|| "The client recording handle capacity has been reached".to_owned())?;
        if let Some(record) = records.remove(&oldest) {
            for artifact_ref in record.artifact_refs {
                self.state.artifacts.revoke(client_ref, &artifact_ref);
            }
        }
        Ok(())
    }

    fn enforce_public_mcp_recording_content_budget(&self, client_ref: &ClientRef) {
        let mut records = self.recordings.lock();
        while records
            .values()
            .filter(|record| record.client_ref == *client_ref)
            .filter_map(|record| record.content.as_ref())
            .map(|content| content.len())
            .sum::<usize>()
            > RECORDING_CONTENT_BYTES_PER_CLIENT
        {
            let oldest = records
                .iter()
                .filter(|(_, record)| record.client_ref == *client_ref && record.content.is_some())
                .min_by_key(|(_, record)| record.stopped_at)
                .map(|(recording_ref, _)| recording_ref.clone());
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(record) = records.get_mut(&oldest) {
                // Expiring the oldest retained body keeps metadata and any exported artifact valid.
                record.content.take();
            }
        }
    }
}

fn recording_status(
    records: &std::collections::HashMap<RecordingRef, PublicMcpRecordingRecord>,
    client_ref: &ClientRef,
    recording_ref: &RecordingRef,
) -> Result<PublicMcpRecordingStatus, String> {
    let record = records
        .get(recording_ref)
        .filter(|record| record.client_ref == *client_ref)
        .ok_or_else(|| "The recording handle is unavailable".to_owned())?;
    if record.active {
        return Ok(PublicMcpRecordingStatus::Live(record.terminal_ref.clone()));
    }
    Ok(PublicMcpRecordingStatus::Stopped(recording_projection(
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
    )))
}

fn live_recording_projection(
    recording_ref: Option<&RecordingRef>,
    terminal_ref: &TerminalRef,
    status: TerminalRecordingStatus,
    (cols, rows): (usize, usize),
) -> Value {
    recording_projection(
        recording_ref,
        terminal_ref,
        recording_state_name(status.state),
        duration_ms(status.elapsed),
        status.event_count,
        cols,
        rows,
        recording_ref.is_some(),
        false,
        false,
    )
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
