use gpui::{App, Entity};
use oxideterm_ai::{
    RuntimeCapability, RuntimeValidationError,
    tool_resources::{AiActionResultLite, AiOrchestratorRuntimeSnapshot},
};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_workspace::TerminalSessionId;

use crate::conversation::terminal_watch::{AiTerminalWatchRecord, AiTerminalWatchSample};

pub enum AiTerminalTool {
    Observe,
    Status,
    Input,
}

impl AiTerminalTool {
    pub fn capability(&self) -> RuntimeCapability {
        match self {
            Self::Input => RuntimeCapability::TerminalSendInput,
            Self::Observe | Self::Status => RuntimeCapability::TerminalObserve,
        }
    }

    pub fn denied(
        &self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        error: &RuntimeValidationError,
    ) -> AiActionResultLite {
        let (recovery, risk) = match self {
            Self::Observe => (
                "Rediscover the current terminal before observing it.",
                "read",
            ),
            Self::Status => (
                "Rediscover the current terminal before reading command status.",
                "read",
            ),
            Self::Input => (
                "Rediscover the current terminal before sending input.",
                "interactive",
            ),
        };
        snapshot.fail(
            "Runtime terminal is unavailable.",
            error.public_code(),
            recovery,
            risk,
        )
    }

    pub fn execute(
        self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        session: TerminalSessionId,
        pane: Option<&Entity<TerminalPane>>,
        args: &serde_json::Value,
        cx: &mut App,
    ) -> AiActionResultLite {
        let target = snapshot.terminal_target(session);
        match self {
            Self::Input => {
                let Some(pane) = pane else {
                    return snapshot
                        .terminal_owner_closed("interactive")
                        .with_optional_target(target);
                };
                let payload = match snapshot.prepare_terminal_input(args) {
                    Ok(payload) => payload,
                    Err(result) => return result.with_optional_target(target),
                };
                if !super::pane::send_input(pane, &payload, cx) {
                    return snapshot
                        .fail(
                            "Failed to send terminal input.",
                            "terminal_send_failed",
                            "The terminal writer is no longer available.",
                            "interactive",
                        )
                        .with_optional_target(target);
                }
                snapshot.terminal_input_sent().with_optional_target(target)
            }
            Self::Observe => {
                let Some(target) = target else {
                    return snapshot.terminal_owner_closed("read");
                };
                let pane = pane.map(|pane| pane.read(cx));
                let records = pane
                    .map(TerminalPane::ai_command_records)
                    .unwrap_or_default();
                snapshot.terminal_observed(
                    target,
                    args,
                    records.iter().rev().map(|record| {
                        ai_terminal_command_capture(
                            record,
                            pane.is_some_and(|pane| {
                                pane.ai_command_prompt_returned(&record.command_id)
                            }),
                        )
                    }),
                    pane.is_some_and(TerminalPane::ai_waiting_for_secret),
                )
            }
            Self::Status => {
                let Some(pane) = pane else {
                    return snapshot
                        .terminal_owner_closed("read")
                        .with_optional_target(target);
                };
                let pane = pane.read(cx);
                let records = pane.ai_command_records();
                snapshot.terminal_command_status(
                    target,
                    args,
                    records.iter().rev().map(|record| {
                        ai_terminal_command_capture(
                            record,
                            pane.ai_command_prompt_returned(&record.command_id),
                        )
                    }),
                )
            }
        }
    }
}

pub fn capture_terminal_watch(
    pane: &TerminalPane,
    run_command: bool,
    command: Option<&str>,
) -> AiTerminalWatchSample {
    let records = pane.ai_command_records();
    let prompt_returned = command
        .or_else(|| {
            if run_command {
                None
            } else {
                records.last().map(|record| record.command_id.as_str())
            }
        })
        .is_some_and(|id| pane.ai_command_prompt_returned(id));
    let records = records
        .into_iter()
        .filter(|record| !run_command || command == Some(record.command_id.as_str()))
        .map(|record| {
            let status = ai_terminal_command_capture(&record, false).status;
            AiTerminalWatchRecord {
                id: record.command_id,
                command: zeroize::Zeroizing::new(record.command),
                status,
                started_at: record.started_at,
                finished_at: record.finished_at,
                exit_code: record.exit_code,
            }
        })
        .collect();
    AiTerminalWatchSample::Current {
        buffer: zeroize::Zeroizing::new(pane.ai_buffer_snapshot()),
        alternate_screen: !run_command && pane.ai_screen_is_alternate_buffer(),
        waiting_for_secret: pane.ai_waiting_for_secret(),
        prompt_returned,
        records,
    }
}

use oxideterm_ai::tool_resources::{AiTerminalCommand, AiTerminalCommandStatus};

fn ai_terminal_command_capture(
    record: &oxideterm_gpui_terminal::TerminalAiCommandRecord,
    prompt_returned: bool,
) -> AiTerminalCommand<'_> {
    use oxideterm_gpui_terminal::TerminalCommandFactStatus;
    let status = match record.status {
        TerminalCommandFactStatus::Open if prompt_returned => AiTerminalCommandStatus::ShellReady,
        TerminalCommandFactStatus::Open => AiTerminalCommandStatus::Running,
        TerminalCommandFactStatus::Closed => AiTerminalCommandStatus::Completed,
        TerminalCommandFactStatus::Stale => AiTerminalCommandStatus::Stale,
    };
    AiTerminalCommand {
        id: &record.command_id,
        command: &record.command,
        status,
        started_at: record.started_at,
        finished_at: record.finished_at,
        exit_code: record.exit_code,
    }
}

pub fn ai_terminal_screen_snapshot_json(
    snapshot: &oxideterm_terminal::TerminalSnapshot,
    is_alternate_buffer: bool,
) -> serde_json::Value {
    // Keep the payload shape close to Tauri's readScreen result while avoiding
    // renderer-only fields that are not useful to an AI tool.
    serde_json::json!({
        "lines": snapshot
            .lines
            .iter()
            .map(|row| row.text().trim_end().to_string())
            .collect::<Vec<_>>(),
        "cursorX": snapshot.cursor_col + 1,
        "cursorY": snapshot.cursor_row + 1,
        "rows": snapshot.rows,
        "cols": snapshot.cols,
        "isAlternateBuffer": is_alternate_buffer,
        "scrollbackLines": snapshot.scrollback_lines,
        "displayOffset": snapshot.display_offset,
    })
}

use gpui::{AsyncApp, Subscription};
use oxideterm_ai::{
    AiExecutedToolResult,
    agent::{AgentResourceRecord, AgentText, AgentToolLease, OwnedResourceKind},
    tool_resources::{
        AiTerminalCommandPlan, AiTerminalOutputWait, ai_run_command_preflight_risk,
        ai_runtime_validation_public_code, ai_runtime_validation_recovery_message,
    },
    tool_result_protocol::rejected_ai_tool_result,
};
use oxideterm_terminal::TerminalCommandMarkDetectionSource;
use tokio::sync::{oneshot, watch};

use crate::conversation::terminal_watch::{
    AiTerminalCommandObservation, AiTerminalWatch, AiTerminalWatchStatus,
};
use crate::state::{AiWorkspaceEntity, local_command::AiToolRunContext};

pub struct AiTerminalToolReply {
    pub snapshot: AiOrchestratorRuntimeSnapshot,
    pub call: String,
    pub name: String,
    pub sender: oneshot::Sender<AiExecutedToolResult>,
    started: web_time::Instant,
    approved: bool,
}

impl AiTerminalToolReply {
    pub fn new(
        snapshot: AiOrchestratorRuntimeSnapshot,
        call: String,
        name: String,
        sender: oneshot::Sender<AiExecutedToolResult>,
        approved: bool,
    ) -> Self {
        Self {
            snapshot,
            call,
            name,
            sender,
            started: web_time::Instant::now(),
            approved,
        }
    }

    pub fn complete(self, action: AiActionResultLite) {
        let _ = self.sender.send(self.snapshot.to_executed_tool_result(
            self.call,
            self.name,
            action,
            self.started.elapsed().as_millis(),
        ));
    }

    pub fn deny(self, error: &RuntimeValidationError, run: bool) {
        let result = self.snapshot.fail(
            if run {
                "Runtime command target is unavailable."
            } else {
                "Runtime terminal is unavailable."
            },
            ai_runtime_validation_public_code(error, self.approved),
            ai_runtime_validation_recovery_message(self.approved),
            if run {
                ai_run_command_preflight_risk()
            } else {
                "read"
            },
        );
        self.complete(result);
    }

    pub fn owner_closed(self, session: TerminalSessionId, run: bool) {
        let mut result =
            self.snapshot
                .terminal_owner_closed(if run { "interactive" } else { "read" });
        if run {
            result = result.with_optional_target(self.snapshot.terminal_target(session));
        }
        self.complete(result);
    }

    fn reject(self, code: &str, reason: &str) {
        let _ = self
            .sender
            .send(rejected_ai_tool_result(self.call, self.name, code, reason));
    }
}

pub struct AiTerminalWatchBindings<S, F, P> {
    pub activity: watch::Receiver<u64>,
    pub pane_observer: Subscription,
    pub runtime_observer: Subscription,
    pub sample: S,
    pub finish: F,
    pub status: P,
}

pub enum AiTerminalRequest<'a> {
    Run(AiTerminalCommandPlan<'a>),
    Wait(serde_json::Value),
}

impl AiTerminalRequest<'_> {
    pub fn start<C, S, F, P>(
        self,
        reply: AiTerminalToolReply,
        pane: &Entity<TerminalPane>,
        session: TerminalSessionId,
        leases: Vec<AgentToolLease>,
        owner: AiToolRunContext,
        state: &Entity<AiWorkspaceEntity>,
        command_label: &str,
        observation_label: &str,
        bindings: AiTerminalWatchBindings<S, F, P>,
        monitor: impl FnOnce(
            Option<String>,
            &str,
            Vec<AgentToolLease>,
            Option<AgentResourceRecord>,
            &mut C,
        ),
        cx: &mut C,
    ) where
        C: std::ops::DerefMut<Target = App>,
        S: FnMut(bool, Option<&str>, &mut AsyncApp) -> AiTerminalWatchSample + 'static,
        F: FnMut(&mut AsyncApp) -> Option<AiOrchestratorRuntimeSnapshot> + 'static,
        P: FnMut(Option<&str>, AiTerminalWatchStatus, i64, &mut AsyncApp) + 'static,
    {
        let resource = |kind, label: &str, cx: &App| {
            let ai = state.read(cx);
            let run = ai.agent_run(owner.generation)?;
            ai.agents
                .services
                .runtime
                .register_resource(&run, kind, AgentText::new(label))
                .ok()
        };
        let is_run = matches!(self, Self::Run(_));
        let target = is_run
            .then(|| reply.snapshot.terminal_target(session))
            .flatten();
        let task = match self {
            Self::Run(request) => {
                if !pane.read(cx).ai_accepts_input() {
                    let action = reply
                        .snapshot
                        .fail(
                            "Terminal is not ready.",
                            "terminal_not_ready",
                            "The terminal writer is no longer available.",
                            "interactive",
                        )
                        .with_optional_target(target);
                    return reply.complete(action);
                }
                if pane.read(cx).ai_command_input_pending() {
                    return reply.reject(
                        "terminal_command_pending",
                        "The previous command has not exited or returned a stable shell prompt. Observe the terminal, continue waiting, or handle its interactive input before running another command.",
                    );
                }
                let before = zeroize::Zeroizing::new(pane.read(cx).ai_buffer_snapshot());
                if leases.iter().any(|lease| !lease.is_current()) {
                    return reply.reject(
                        "terminal_taken_over",
                        "User input took control of the terminal before dispatch.",
                    );
                }
                for lease in &leases {
                    lease.dispatched();
                }
                let command = request.with_terminal_cwd();
                let id = pane.update(&mut **cx, |pane, cx| {
                    let id = pane.begin_command_mark(
                        &command,
                        TerminalCommandMarkDetectionSource::Ai,
                        cx,
                    );
                    pane.send_command_line(&command, cx);
                    id
                });
                let record = resource(OwnedResourceKind::TerminalCommand, command_label, cx);
                monitor(id.clone(), &before, leases.clone(), record, cx);
                if !request.await_output {
                    let action = reply.snapshot.terminal_command_dispatched(id, target);
                    return reply.complete(action);
                }
                TerminalObservation::Command(AiTerminalCommandObservation {
                    command_id: id,
                    before,
                    timeout: request.timeout,
                    leases,
                    target,
                    post_user_approval: reply.approved,
                })
            }
            Self::Wait(args) => TerminalObservation::Output(
                args,
                pane.read(cx).ai_buffer_snapshot(),
                pane.read(cx).ai_screen_is_alternate_buffer(),
            ),
        };
        let observation = resource(OwnedResourceKind::Observation, observation_label, cx);
        let AiTerminalWatchBindings {
            activity,
            pane_observer,
            runtime_observer,
            sample,
            finish,
            mut status,
        } = bindings;
        let watcher = AiTerminalWatch {
            call_id: reply.call,
            tool_name: reply.name,
            started: reply.started,
            sender: reply.sender,
            activity,
            direction: owner.dispatch,
        };
        cx.spawn(async move |cx| {
            let _subscriptions = (pane_observer, runtime_observer);
            let _observation = observation;
            match task {
                TerminalObservation::Command(command) => {
                    let id = command.command_id.clone();
                    watcher
                        .observe_command(
                            command,
                            sample,
                            finish,
                            |phase, deadline, cx| {
                                status(id.as_deref(), phase, deadline, cx);
                            },
                            cx,
                        )
                        .await;
                }
                TerminalObservation::Output(args, buffer, alternate) => {
                    let wait = AiTerminalOutputWait::new(
                        &args,
                        buffer,
                        alternate,
                        web_time::Instant::now(),
                    );
                    watcher
                        .observe_output(
                            wait,
                            reply.snapshot,
                            sample,
                            finish,
                            |phase, deadline, cx| {
                                status(None, phase, deadline, cx);
                            },
                            cx,
                        )
                        .await;
                }
            }
        })
        .detach();
    }
}

enum TerminalObservation {
    Command(AiTerminalCommandObservation),
    Output(serde_json::Value, String, bool),
}

use oxideterm_ai::tool_resources::{
    AiSerialControl, AiSerialSessionCapture, AiTransportLifecycle, ai_serial_control,
    ai_transport_session_action_accepted,
};

pub fn transport_state(pane: &Entity<TerminalPane>, cx: &App) -> Result<serde_json::Value, String> {
    let pane = pane.read(cx);
    use oxideterm_terminal::{TerminalLifecycle, TerminalSessionKind};
    let lifecycle = match pane.lifecycle() {
        TerminalLifecycle::Running => AiTransportLifecycle::Running,
        TerminalLifecycle::Exited(code) => AiTransportLifecycle::Exited(code),
        TerminalLifecycle::Closed => AiTransportLifecycle::Closed,
    };
    match pane.session_kind() {
        TerminalSessionKind::Telnet => Ok(lifecycle.telnet()),
        TerminalSessionKind::Serial => {
            let status = pane
                .serial_status()
                .ok_or_else(|| "Serial session state is unavailable.".to_string())?;
            Ok(AiSerialSessionCapture {
                port: status.config.port_path,
                available: status.port_available,
                baud_rate: status.config.baud_rate,
                data_bits: status.config.data_bits,
                stop_bits: status.config.stop_bits,
                parity: format!("{:?}", status.config.parity).to_lowercase(),
                flow_control: format!("{:?}", status.config.flow_control).to_lowercase(),
                dtr: status.control_state.data_terminal_ready,
                rts: status.control_state.request_to_send,
                local_echo: status.runtime_options.local_echo,
                line_ending: format!("{:?}", status.runtime_options.line_ending).to_lowercase(),
                output_line_ending: format!("{:?}", status.runtime_options.output_line_ending)
                    .to_lowercase(),
                display_mode: format!("{:?}", status.runtime_options.display_mode).to_lowercase(),
                send_mode: format!("{:?}", status.runtime_options.send_mode).to_lowercase(),
                can_reconnect: status.can_reconnect,
            }
            .json(lifecycle))
        }
        _ => Err("The selected terminal is not a serial or Telnet session.".to_string()),
    }
}

pub fn serial_control(
    pane: &Entity<TerminalPane>,
    session_id: TerminalSessionId,
    arguments: &serde_json::Value,
    cx: &mut App,
) -> Result<serde_json::Value, String> {
    use oxideterm_gpui_connections::form_state::{
        terminal_serial_display_mode_from_profile, terminal_serial_line_ending_from_profile,
        terminal_serial_send_mode_from_profile,
    };
    use oxideterm_gpui_terminal::TerminalSerialAction as Serial;
    let (action, operation) = ai_serial_control(arguments)?;
    let serial_action = match operation {
        AiSerialControl::RefreshPort => Serial::RefreshPortPresence,
        AiSerialControl::SendBreak => Serial::SendBreak,
        AiSerialControl::SetDtr(value) => Serial::SetDataTerminalReady(value),
        AiSerialControl::SetRts(value) => Serial::SetRequestToSend(value),
        AiSerialControl::SetLocalEcho(value) => Serial::SetLocalEcho(value),
        AiSerialControl::SetLineEnding(value) => {
            Serial::SetLineEnding(terminal_serial_line_ending_from_profile(value))
        }
        AiSerialControl::SetOutputLineEnding(value) => {
            Serial::SetOutputLineEnding(terminal_serial_line_ending_from_profile(value))
        }
        AiSerialControl::SetDisplayMode(value) => {
            Serial::SetDisplayMode(terminal_serial_display_mode_from_profile(value))
        }
        AiSerialControl::SetSendMode(value) => {
            Serial::SetSendMode(terminal_serial_send_mode_from_profile(value))
        }
    };
    pane.update(cx, |pane, cx| pane.apply_serial_action(serial_action, cx))?;
    Ok(ai_transport_session_action_accepted(action, session_id))
}

pub fn telnet_control(
    pane: &Entity<TerminalPane>,
    session_id: TerminalSessionId,
    arguments: &serde_json::Value,
    cx: &mut App,
) -> Result<serde_json::Value, String> {
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A Telnet action is required.".to_string())?;
    let telnet_action = match action {
        "noop" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::NoOperation,
        ),
        "break" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::Break,
        ),
        "interrupt_process" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::InterruptProcess,
        ),
        "abort_output" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::AbortOutput,
        ),
        "are_you_there" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::AreYouThere,
        ),
        "erase_character" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::EraseCharacter,
        ),
        "erase_line" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::EraseLine,
        ),
        "go_ahead" => oxideterm_gpui_terminal::TerminalTelnetAction::SendControl(
            oxideterm_terminal::TelnetControlCommand::GoAhead,
        ),
        "disconnect" => {
            return Err("Telnet disconnect must be requested from Active Sessions.".to_string());
        }
        _ => return Err("The requested Telnet action is unsupported.".to_string()),
    };
    pane.update(cx, |pane, cx| pane.apply_telnet_action(telnet_action, cx))?;
    Ok(ai_transport_session_action_accepted(action, session_id))
}

pub fn capture_terminal_target<'a>(
    pane: &TerminalPane,
    session_id: TerminalSessionId,
    tab_id: oxideterm_workspace::TabId,
    pane_id: oxideterm_workspace::PaneId,
    pane_label: String,
    serial_config: Option<&'a oxideterm_terminal::SerialSessionConfig>,
) -> oxideterm_ai::tool_resources::AiTargetCapture<'a> {
    use oxideterm_ai::tool_resources::{
        AiSerialTargetInput, AiTargetCapture, AiTerminalTargetInput, AiTerminalTransport,
    };
    let screen = pane.ai_screen_snapshot();
    let session_kind = pane.session_kind();
    let terminal_buffer = pane.ai_buffer_snapshot();
    let terminal_screen =
        ai_terminal_screen_snapshot_json(&screen, pane.ai_screen_is_alternate_buffer());
    let accepts_input = pane.ai_accepts_input();
    let terminal_running = pane.lifecycle().is_running();
    AiTargetCapture::Terminal(AiTerminalTargetInput {
        session_id,
        tab_id,
        pane_id,
        pane_label,
        transport: match session_kind {
            oxideterm_terminal::TerminalSessionKind::LocalPty => AiTerminalTransport::Local,
            oxideterm_terminal::TerminalSessionKind::Serial => AiTerminalTransport::Serial,
            oxideterm_terminal::TerminalSessionKind::Telnet => AiTerminalTransport::Telnet,
            oxideterm_terminal::TerminalSessionKind::Ble => AiTerminalTransport::Ble,
            _ => AiTerminalTransport::Other,
        },
        serial: serial_config.map(|config| AiSerialTargetInput {
            port_path: &config.port_path,
            baud_rate: config.baud_rate,
            data_bits: config.data_bits,
            stop_bits: config.stop_bits,
            parity: format!("{:?}", config.parity).to_lowercase(),
            flow_control: format!("{:?}", config.flow_control).to_lowercase(),
        }),
        terminal_buffer,
        terminal_screen,
        accepts_input,
        terminal_running,
    })
}
