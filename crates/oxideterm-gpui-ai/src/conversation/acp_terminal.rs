use std::{hash::Hash, path::PathBuf};

use gpui::{App, AppContext, Window};
use oxideterm_ai::{
    AcpClientEvent, AcpTerminalCreateSpec, acp_internal_error, acp_kill_terminal_request_id,
    acp_kill_terminal_response, acp_method_not_found, acp_release_terminal_request_id,
    acp_release_terminal_response, acp_terminal_create_spec, acp_terminal_created_response,
    acp_terminal_not_found_error, acp_terminal_output_request_id, acp_terminal_output_response,
    acp_wait_terminal_request_id, acp_wait_terminal_response, next_acp_terminal_id,
    resolve_acp_terminal_working_directory,
};

use super::AiConversationController;

pub enum AcpTerminalStatus {
    Running,
    Exited(Option<i32>),
    Closed,
}

impl AcpTerminalStatus {
    fn exit_code(&self) -> Option<i32> {
        match self {
            Self::Exited(code) => *code,
            _ => None,
        }
    }
}

impl AcpTerminalBridge<oxideterm_gpui_terminal::SharedTerminalSession> {
    pub fn for_native_terminal(
        launch: impl Fn(oxideterm_terminal::LocalPtyConfig, String, &mut gpui::Window, &mut gpui::App)
            -> Option<oxideterm_gpui_terminal::SharedTerminalSession> + 'static,
    ) -> Self {
        use oxideterm_terminal::{LocalPtyConfig, ShellInfo, TerminalLifecycle};

        fn status(lifecycle: TerminalLifecycle) -> AcpTerminalStatus {
            match lifecycle {
                TerminalLifecycle::Running => AcpTerminalStatus::Running,
                TerminalLifecycle::Exited(exit_code) => AcpTerminalStatus::Exited(exit_code),
                TerminalLifecycle::Closed => AcpTerminalStatus::Closed,
            }
        }

        Self {
            launch: Box::new(move |mut spec, cwd, window, cx| {
                let title = std::path::Path::new(&spec.command)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("ACP")
                    .to_string();
                let config = LocalPtyConfig {
                    command: None,
                    shell: Some(
                        ShellInfo::new(
                            title.clone(),
                            title.clone(),
                            std::path::PathBuf::from(std::mem::take(&mut spec.command)),
                        )
                        .with_args(std::mem::take(&mut spec.args)),
                    ),
                    cwd: Some(cwd),
                    env: std::mem::take(&mut spec.env),
                    remove_env: Vec::new(),
                    load_profile: false,
                    current_directory_shell_integration: false,
                    oh_my_posh_enabled: false,
                    oh_my_posh_theme: None,
                };
                launch(config, title, window, cx)
            }),
            read_output: |session| {
                let mut session = session.lock();
                session.read_pending();
                (session.buffer_text(), status(session.lifecycle()))
            },
            status: |session| status(session.lock().lifecycle()),
            stop: |session| session.lock().shutdown(),
        }
    }
}

pub struct AcpTerminalBridge<Terminal> {
    pub launch:
        Box<dyn FnOnce(AcpTerminalCreateSpec, PathBuf, &mut Window, &mut App) -> Option<Terminal>>,
    pub read_output: fn(&Terminal) -> (String, AcpTerminalStatus),
    pub status: fn(&Terminal) -> AcpTerminalStatus,
    pub stop: fn(&Terminal),
}

impl<Node, Terminal> AiConversationController<Node, Terminal>
where
    Node: Clone + Eq + Hash + 'static,
    Terminal: Clone + Send + 'static,
{
    pub fn handle_acp_terminal_event(
        &self,
        agent_id: String,
        event: AcpClientEvent,
        bridge: AcpTerminalBridge<Terminal>,
        window: &mut Window,
        cx: &mut App,
    ) {
        use AcpClientEvent::*;
        match event {
            CreateTerminal {
                request,
                response_tx,
            } => {
                let session_id = request.session_id.to_string();
                let Some((root, policy)) =
                    self.acp.read(cx).session_context(&agent_id, &session_id)
                else {
                    let _ = response_tx.send(Err(acp_internal_error(
                        "ACP session context is unavailable",
                    )));
                    return;
                };
                if !policy.terminal {
                    let _ = response_tx.send(Err(acp_method_not_found("terminal/create")));
                    return;
                }
                let spec = match acp_terminal_create_spec(&request) {
                    Ok(spec) => spec,
                    Err(error) => {
                        let _ = response_tx.send(Err(error));
                        return;
                    }
                };
                let window = window.window_handle();
                self.acp.update(cx, |_, cx| {
                    cx.spawn(async move |acp, cx| {
                        let cwd =
                            resolve_acp_terminal_working_directory(&root, spec.cwd.as_deref())
                                .await;
                        let _ = cx.update_window(window, |_, window, cx| {
                            let cwd = match cwd {
                                Ok(cwd) => cwd,
                                Err(error) => {
                                    let _ = response_tx.send(Err(error));
                                    return;
                                }
                            };
                            let output_limit = spec.output_byte_limit;
                            let terminal_id = next_acp_terminal_id();
                            let Some(acp) = acp.upgrade() else { return };
                            let Some(session) = (bridge.launch)(spec, cwd, window, cx) else {
                                let _ = response_tx.send(Err(acp_internal_error(
                                    "ACP terminal could not be created",
                                )));
                                return;
                            };
                            acp.update(cx, |acp, _| {
                                acp.register_terminal(
                                    terminal_id.clone(),
                                    agent_id,
                                    session_id,
                                    session,
                                    output_limit,
                                );
                            });
                            let _ =
                                response_tx.send(Ok(acp_terminal_created_response(&terminal_id)));
                        });
                    })
                    .detach();
                });
            }
            TerminalOutput {
                request,
                response_tx,
            } => {
                let terminal = self.acp.read(cx).terminal(
                    &acp_terminal_output_request_id(&request),
                    &agent_id,
                    &request.session_id.to_string(),
                );
                let response = terminal
                    .ok_or_else(acp_terminal_not_found_error)
                    .map(|terminal| {
                        let (output, status) = (bridge.read_output)(&terminal.session);
                        acp_terminal_output_response(
                            output,
                            terminal.output_byte_limit,
                            status.exit_code(),
                        )
                    });
                let _ = response_tx.send(response);
            }
            ReleaseTerminal {
                request,
                response_tx,
            } => {
                let terminal = self.acp.update(cx, |acp, _| {
                    acp.release_terminal(
                        &acp_release_terminal_request_id(&request),
                        &agent_id,
                        &request.session_id.to_string(),
                    )
                });
                let response = terminal
                    .ok_or_else(acp_terminal_not_found_error)
                    .map(|terminal| {
                        (bridge.stop)(&terminal.session);
                        acp_release_terminal_response()
                    });
                let _ = response_tx.send(response);
            }
            KillTerminal {
                request,
                response_tx,
            } => {
                let terminal = self.acp.read(cx).terminal(
                    &acp_kill_terminal_request_id(&request),
                    &agent_id,
                    &request.session_id.to_string(),
                );
                let response = terminal
                    .ok_or_else(acp_terminal_not_found_error)
                    .map(|terminal| {
                        (bridge.stop)(&terminal.session);
                        acp_kill_terminal_response()
                    });
                let _ = response_tx.send(response);
            }
            WaitForTerminalExit {
                request,
                mut response_tx,
            } => {
                let Some(terminal) = self.acp.read(cx).terminal(
                    &acp_wait_terminal_request_id(&request),
                    &agent_id,
                    &request.session_id.to_string(),
                ) else {
                    let _ = response_tx.send(Err(acp_terminal_not_found_error()));
                    return;
                };
                let status = bridge.status;
                self.state.read(cx).task_runtime().spawn(async move {
                    loop {
                        let current = status(&terminal.session);
                        if !matches!(current, AcpTerminalStatus::Running) {
                            let _ = response_tx
                                .send(Ok(acp_wait_terminal_response(current.exit_code())));
                            return;
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                            _ = response_tx.closed() => return,
                        }
                    }
                });
            }
            _ => unreachable!("non-terminal ACP requests are handled before terminal dispatch"),
        }
    }
}
