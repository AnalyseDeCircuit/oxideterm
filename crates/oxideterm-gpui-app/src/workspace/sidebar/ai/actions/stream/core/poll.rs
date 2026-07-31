#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum PendingAiStreamTextKind {
    Content,
    Thinking,
}

pub(in crate::workspace) struct PendingAiStreamText {
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    kind: PendingAiStreamTextKind,
    text: String,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn schedule_ai_chat_stream_delivery_apply(
        &mut self,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let deliveries = self
            .ai_entity
            .update(cx, |ai, _cx| ai.take_chat_stream_deliveries());
        if deliveries.is_empty() {
            return;
        }
        cx.spawn(async move |weak, cx| {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    workspace.apply_ai_chat_stream_deliveries(deliveries, window, cx);
                })
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn apply_ai_chat_stream_deliveries(
        &mut self,
        deliveries: VecDeque<AiStreamDelivery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut pending_text: Option<PendingAiStreamText> = None;
        for delivery in deliveries {
            if !self
                .ai_entity
                .read(cx)
                .is_chat_stream_generation(delivery.generation)
            {
                // Dropping a stale delivery also drops any retained approval
                // sender, matching the old generation-scoped receiver.
                continue;
            }
            match delivery.event {
                AiStreamDeliveryEvent::PromptUsage {
                    last_user_message_id,
                    provider_id,
                    model,
                    breakdown,
                    max_tokens,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    let usage = AiPreparedPromptUsage {
                        conversation_id: delivery.conversation_id,
                        last_user_message_id,
                        provider_id,
                        model,
                        breakdown: ai_context_token_breakdown_from_prompt(
                            breakdown,
                            max_tokens,
                        ),
                    };
                    self.ai_entity
                        .update(cx, |ai, _cx| ai.set_prepared_prompt_usage(usage));
                }
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(chunk)) => {
                    self.merge_or_flush_pending_ai_stream_text(
                        &mut pending_text,
                        delivery.generation,
                        delivery.conversation_id,
                        delivery.assistant_id,
                        PendingAiStreamTextKind::Content,
                        chunk,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(chunk)) => {
                    self.merge_or_flush_pending_ai_stream_text(
                        &mut pending_text,
                        delivery.generation,
                        delivery.conversation_id,
                        delivery.assistant_id,
                        PendingAiStreamTextKind::Thinking,
                        chunk,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Stream(event) => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_stream_event(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        event,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AcpClientEvent { agent_id, event } => {
                    match event {
                        oxideterm_ai::AcpClientEvent::RequestPermission {
                            request,
                            response_tx,
                        } => {
                            self.flush_pending_ai_stream_text(&mut pending_text, cx);
                            if self.ai_entity.read(cx).chat_stream_generation()
                                != delivery.generation
                            {
                                let _ = response_tx
                                    .send(Ok(oxideterm_ai::acp_permission_cancelled_response()));
                                continue;
                            }
                            let projection =
                                oxideterm_ai::acp_permission_request_projection(&request);
                            let permission_options = projection
                                .options
                                .iter()
                                .map(|option| {
                                    serde_json::json!({
                                        "id": option.option_id,
                                        "name": option.name,
                                        "kind": option.kind,
                                    })
                                })
                                .collect::<Vec<_>>();
                            let (choice_tx, choice_rx) = tokio::sync::oneshot::channel();
                            self.apply_ai_tool_status(
                                delivery.generation,
                                &delivery.conversation_id,
                                &delivery.assistant_id,
                                &projection.tool_call_id,
                                &projection.name,
                                &projection.arguments,
                                "pending_user_approval",
                                Some(serde_json::json!({
                                    "acpPermissionOptions": permission_options,
                                })),
                                Some(projection.risk),
                                Some(projection.summary),
                                false,
                                None,
                                None,
                                None,
                                cx,
                            );
                            self.ai_entity.update(cx, |ai, _cx| {
                                ai.register_acp_permission_choice(
                                    delivery.generation,
                                    projection.tool_call_id,
                                    choice_tx,
                                );
                            });
                            let forwarding_runtime = self.forwarding_runtime.clone();
                            forwarding_runtime.spawn(async move {
                                let selected_option_id = choice_rx.await.ok().flatten();
                                let response = oxideterm_ai::acp_permission_response_for_option(
                                    &request,
                                    selected_option_id.as_deref(),
                                );
                                let _ = response_tx.send(Ok(response));
                            });
                        }
                        oxideterm_ai::AcpClientEvent::ReadTextFile {
                            request,
                            response_tx,
                        } => {
                            let owner_session_id = request.session_id.to_string();
                            let Some((workspace_root, policy)) =
                                self.acp_entity
                                    .read(cx)
                                    .session_context(&agent_id, &owner_session_id)
                            else {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                                    "ACP session context is unavailable",
                                )));
                                continue;
                            };
                            if !policy.fs_read_text_file {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_method_not_found(
                                    "fs/read_text_file",
                                )));
                                continue;
                            }
                            cx.spawn(async move |_weak, _cx| {
                                let response =
                                    oxideterm_ai::resolve_acp_read_text_file_request(
                                        &workspace_root,
                                        &request,
                                    )
                                    .await;
                                let _ = response_tx.send(response);
                            })
                            .detach();
                        }
                        oxideterm_ai::AcpClientEvent::WriteTextFile {
                            mut request,
                            response_tx,
                        } => {
                            self.flush_pending_ai_stream_text(&mut pending_text, cx);
                            let owner_session_id = request.session_id.to_string();
                            let Some((workspace_root, policy)) =
                                self.acp_entity
                                    .read(cx)
                                    .session_context(&agent_id, &owner_session_id)
                            else {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                                    "ACP session context is unavailable",
                                )));
                                continue;
                            };
                            if !policy.fs_write_text_file {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_method_not_found(
                                    "fs/write_text_file",
                                )));
                                continue;
                            }
                            let requested_path = request.path.clone();
                            let proposed_content =
                                zeroize::Zeroizing::new(std::mem::take(&mut request.content));
                            let tool_call_id = oxideterm_ai::next_acp_file_review_id();
                            let generation = delivery.generation;
                            let conversation_id = delivery.conversation_id.clone();
                            let assistant_id = delivery.assistant_id.clone();
                            cx.spawn(async move |weak, cx| {
                                let target_path =
                                    match oxideterm_ai::resolve_acp_write_text_file_target(
                                        &workspace_root,
                                        &requested_path,
                                    )
                                    .await
                                    {
                                        Ok(path) => path,
                                        Err(error) => {
                                            let _ = response_tx.send(Err(error));
                                            return;
                                        }
                                    };
                                let existing_content = match tokio::fs::read_to_string(&target_path)
                                    .await
                                {
                                    Ok(content) => zeroize::Zeroizing::new(content),
                                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                        zeroize::Zeroizing::new(String::new())
                                    }
                                    Err(_) => {
                                        let _ = response_tx.send(Err(
                                            oxideterm_ai::acp_internal_error(
                                                "ACP file review could not read the target",
                                            ),
                                        ));
                                        return;
                                    }
                                };
                                let preview =
                                    crate::workspace::acp_workspace::acp_file_write_preview(
                                        &existing_content,
                                        &proposed_content,
                                    );
                                let (choice_tx, choice_rx) = tokio::sync::oneshot::channel();
                                let safe_arguments = serde_json::json!({
                                    "path": requested_path.display().to_string(),
                                })
                                .to_string();
                                let registered = weak
                                    .update(cx, |workspace, cx| {
                                        workspace.acp_entity.update(cx, |entity, _cx| {
                                            entity.register_file_write_preview(
                                                tool_call_id.clone(),
                                                preview,
                                            );
                                        });
                                        workspace.apply_ai_tool_status(
                                            generation,
                                            &conversation_id,
                                            &assistant_id,
                                            &tool_call_id,
                                            "write_file",
                                            &safe_arguments,
                                            "pending_user_approval",
                                            Some(serde_json::json!({
                                                "acpFileWriteReview": true,
                                                "acpPermissionOptions": [
                                                    {
                                                        "id": "allow_once",
                                                        "name": workspace.i18n.t("ai.tool_use.approve"),
                                                        "kind": "allow_once",
                                                    },
                                                    {
                                                        "id": "reject_once",
                                                        "name": workspace.i18n.t("ai.tool_use.reject"),
                                                        "kind": "reject_once",
                                                    }
                                                ],
                                            })),
                                            Some("write".to_string()),
                                            Some(requested_path.display().to_string()),
                                            false,
                                            None,
                                            None,
                                            None,
                                            cx,
                                        );
                                        workspace.ai_entity.update(cx, |ai, _cx| {
                                            ai.register_acp_permission_choice(
                                                generation,
                                                tool_call_id.clone(),
                                                choice_tx,
                                            )
                                        })
                                    })
                                    .unwrap_or(false);
                                if !registered {
                                    let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                                        "ACP file review is no longer active",
                                    )));
                                    return;
                                }
                                let approved = choice_rx
                                    .await
                                    .ok()
                                    .flatten()
                                    .as_deref()
                                    .is_some_and(|option| option.starts_with("allow_"));
                                let response = if approved {
                                    match oxideterm_ai::resolve_acp_write_text_file_target(
                                        &workspace_root,
                                        &requested_path,
                                    )
                                    .await
                                    {
                                        Ok(revalidated_target) => {
                                            oxideterm_ai::write_acp_validated_text_file(
                                                &revalidated_target,
                                                &proposed_content,
                                            )
                                            .await
                                        }
                                        Err(error) => Err(error),
                                    }
                                } else {
                                    Err(oxideterm_ai::acp_internal_error(
                                        "ACP file write was declined by the user",
                                    ))
                                };
                                let succeeded = response.is_ok();
                                let _ = response_tx.send(response);
                                let _ = weak.update(cx, |workspace, cx| {
                                    workspace.acp_entity.update(cx, |entity, _cx| {
                                        entity.remove_file_write_preview(&tool_call_id);
                                    });
                                    workspace.apply_ai_tool_status(
                                        generation,
                                        &conversation_id,
                                        &assistant_id,
                                        &tool_call_id,
                                        "write_file",
                                        &safe_arguments,
                                        if succeeded {
                                            "completed"
                                        } else if approved {
                                            "error"
                                        } else {
                                            "rejected"
                                        },
                                        Some(serde_json::json!({
                                            "ok": succeeded,
                                        })),
                                        Some("write".to_string()),
                                        Some(requested_path.display().to_string()),
                                        false,
                                        None,
                                        None,
                                        None,
                                        cx,
                                    );
                                });
                            })
                            .detach();
                        }
                        oxideterm_ai::AcpClientEvent::CreateTerminal {
                            request,
                            response_tx,
                        } => {
                            self.flush_pending_ai_stream_text(&mut pending_text, cx);
                            let owner_session_id = request.session_id.to_string();
                            let Some((workspace_root, policy)) =
                                self.acp_entity
                                    .read(cx)
                                    .session_context(&agent_id, &owner_session_id)
                            else {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                                    "ACP session context is unavailable",
                                )));
                                continue;
                            };
                            if !policy.terminal {
                                let _ = response_tx.send(Err(oxideterm_ai::acp_method_not_found(
                                    "terminal/create",
                                )));
                                continue;
                            }
                            let mut terminal_spec =
                                match oxideterm_ai::acp_terminal_create_spec(&request) {
                                    Ok(spec) => spec,
                                    Err(error) => {
                                        let _ = response_tx.send(Err(error));
                                        continue;
                                    }
                                };
                            let requested_cwd = terminal_spec.cwd.clone();
                            let window_handle = window.window_handle();
                            cx.spawn(async move |weak, cx| {
                                let resolved_cwd =
                                    oxideterm_ai::resolve_acp_terminal_working_directory(
                                        &workspace_root,
                                        requested_cwd.as_deref(),
                                    )
                                    .await;
                                let _ = cx.update_window(window_handle, |_, window, cx| {
                                    weak.update(cx, |workspace, cx| {
                                        let resolved_cwd = match resolved_cwd {
                                            Ok(cwd) => cwd,
                                            Err(error) => {
                                                let _ = response_tx.send(Err(error));
                                                return;
                                            }
                                        };
                                        let command = std::mem::take(&mut terminal_spec.command);
                                        let args = std::mem::take(&mut terminal_spec.args);
                                        let env = std::mem::take(&mut terminal_spec.env);
                                        let title = std::path::Path::new(&command)
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .filter(|name| !name.is_empty())
                                            .unwrap_or("ACP")
                                            .to_string();
                                        let shell = ShellInfo::new(
                                            title.clone(),
                                            title.clone(),
                                            std::path::PathBuf::from(&command),
                                        )
                                        .with_args(args);
                                        let config = LocalPtyConfig {
                                            shell: Some(shell),
                                            cwd: Some(resolved_cwd),
                                            env,
                                            load_profile: false,
                                            current_directory_shell_integration: false,
                                            oh_my_posh_enabled: false,
                                            oh_my_posh_theme: None,
                                        };
                                        let terminal_id = oxideterm_ai::next_acp_terminal_id();
                                        match workspace
                                            .create_local_terminal_tab_with_owned_session(
                                                config,
                                                title,
                                                window,
                                                cx,
                                            ) {
                                            Ok((_workspace_session_id, session)) => {
                                                workspace.acp_entity.update(
                                                    cx,
                                                    |entity, _cx| {
                                                        entity.register_terminal(
                                                            terminal_id.clone(),
                                                            agent_id,
                                                            owner_session_id,
                                                            session,
                                                            terminal_spec.output_byte_limit,
                                                        );
                                                    },
                                                );
                                                let _ = response_tx.send(Ok(
                                                    oxideterm_ai::acp_terminal_created_response(
                                                        &terminal_id,
                                                    ),
                                                ));
                                            }
                                            Err(_) => {
                                                let _ = response_tx.send(Err(
                                                    oxideterm_ai::acp_internal_error(
                                                        "ACP terminal could not be created",
                                                    ),
                                                ));
                                            }
                                        }
                                    })
                                });
                            })
                            .detach();
                        }
                        oxideterm_ai::AcpClientEvent::TerminalOutput {
                            request,
                            response_tx,
                        } => {
                            let owner_session_id = request.session_id.to_string();
                            let terminal_id =
                                oxideterm_ai::acp_terminal_output_request_id(&request);
                            let terminal = self
                                .acp_entity
                                .read(cx)
                                .terminal(&terminal_id, &agent_id, &owner_session_id);
                            let Some(terminal) = terminal else {
                                let _ = response_tx
                                    .send(Err(oxideterm_ai::acp_terminal_not_found_error()));
                                continue;
                            };
                            let mut session = terminal.session.lock();
                            session.read_pending();
                            let output = session.buffer_text();
                            let exit_code = match session.lifecycle() {
                                TerminalLifecycle::Running => None,
                                TerminalLifecycle::Exited(exit_code) => exit_code,
                                TerminalLifecycle::Closed => None,
                            };
                            drop(session);
                            let _ = response_tx.send(Ok(
                                oxideterm_ai::acp_terminal_output_response(
                                    output,
                                    terminal.output_byte_limit,
                                    exit_code,
                                ),
                            ));
                        }
                        oxideterm_ai::AcpClientEvent::ReleaseTerminal {
                            request,
                            response_tx,
                        } => {
                            let owner_session_id = request.session_id.to_string();
                            let terminal_id =
                                oxideterm_ai::acp_release_terminal_request_id(&request);
                            let terminal = self.acp_entity.update(cx, |entity, _cx| {
                                entity.release_terminal(
                                    &terminal_id,
                                    &agent_id,
                                    &owner_session_id,
                                )
                            });
                            let Some(terminal) = terminal else {
                                let _ = response_tx
                                    .send(Err(oxideterm_ai::acp_terminal_not_found_error()));
                                continue;
                            };
                            terminal.session.lock().shutdown();
                            let _ = response_tx
                                .send(Ok(oxideterm_ai::acp_release_terminal_response()));
                        }
                        oxideterm_ai::AcpClientEvent::KillTerminal {
                            request,
                            response_tx,
                        } => {
                            let owner_session_id = request.session_id.to_string();
                            let terminal_id =
                                oxideterm_ai::acp_kill_terminal_request_id(&request);
                            let terminal = self
                                .acp_entity
                                .read(cx)
                                .terminal(&terminal_id, &agent_id, &owner_session_id);
                            let Some(terminal) = terminal else {
                                let _ = response_tx
                                    .send(Err(oxideterm_ai::acp_terminal_not_found_error()));
                                continue;
                            };
                            terminal.session.lock().shutdown();
                            let _ =
                                response_tx.send(Ok(oxideterm_ai::acp_kill_terminal_response()));
                        }
                        oxideterm_ai::AcpClientEvent::WaitForTerminalExit {
                            request,
                            response_tx,
                        } => {
                            let owner_session_id = request.session_id.to_string();
                            let terminal_id =
                                oxideterm_ai::acp_wait_terminal_request_id(&request);
                            let terminal = self
                                .acp_entity
                                .read(cx)
                                .terminal(&terminal_id, &agent_id, &owner_session_id);
                            let Some(terminal) = terminal else {
                                let _ = response_tx
                                    .send(Err(oxideterm_ai::acp_terminal_not_found_error()));
                                continue;
                            };
                            // Waiting is owned by the ACP task runtime. It never
                            // invalidates the root renderer while the process runs.
                            self.forwarding_runtime.spawn(async move {
                                loop {
                                    let lifecycle = terminal.session.lock().lifecycle();
                                    match lifecycle {
                                        TerminalLifecycle::Running => {
                                            tokio::time::sleep(std::time::Duration::from_millis(
                                                50,
                                            ))
                                            .await;
                                        }
                                        TerminalLifecycle::Exited(exit_code) => {
                                            let _ = response_tx.send(Ok(
                                                oxideterm_ai::acp_wait_terminal_response(
                                                    exit_code,
                                                ),
                                            ));
                                            break;
                                        }
                                        TerminalLifecycle::Closed => {
                                            let _ = response_tx.send(Ok(
                                                oxideterm_ai::acp_wait_terminal_response(None),
                                            ));
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        event => {
                            for stream_event in
                                oxideterm_ai::acp_client_event_to_ai_stream_events(event)
                            {
                                match stream_event {
                                    AiStreamEvent::Content(chunk) => {
                                        self.merge_or_flush_pending_ai_stream_text(
                                            &mut pending_text,
                                            delivery.generation,
                                            delivery.conversation_id.clone(),
                                            delivery.assistant_id.clone(),
                                            PendingAiStreamTextKind::Content,
                                            chunk,
                                            cx,
                                        );
                                    }
                                    AiStreamEvent::Thinking(chunk) => {
                                        self.merge_or_flush_pending_ai_stream_text(
                                            &mut pending_text,
                                            delivery.generation,
                                            delivery.conversation_id.clone(),
                                            delivery.assistant_id.clone(),
                                            PendingAiStreamTextKind::Thinking,
                                            chunk,
                                            cx,
                                        );
                                    }
                                    event => {
                                        self.flush_pending_ai_stream_text(&mut pending_text, cx);
                                        self.apply_ai_stream_event(
                                            delivery.generation,
                                            &delivery.conversation_id,
                                            &delivery.assistant_id,
                                            event,
                                            cx,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                AiStreamDeliveryEvent::AcpSessionStarted {
                    session_id,
                    session_metadata,
                    session_config_options,
                    session_modes,
                    agent_id,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    if self.apply_ai_acp_session_started(
                        delivery.generation,
                        &delivery.conversation_id,
                        &session_id,
                        session_metadata,
                        session_config_options,
                        session_modes,
                        &agent_id,
                        cx,
                    ) {
                        cx.notify();
                    }
                }
                AiStreamDeliveryEvent::Guardrail {
                    code,
                    message,
                    raw_text,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_guardrail(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &code,
                        &message,
                        raw_text,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AssistantRound {
                    round_id,
                    round_number,
                    response_length,
                    tool_call_ids,
                    synthetic,
                    retry_attempt,
                    hard_deny_triggered,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.persist_ai_assistant_round(
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        round_id,
                        round_number,
                        response_length,
                        tool_call_ids,
                        synthetic,
                        retry_attempt,
                        hard_deny_triggered,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::RoundSummary {
                    round_id,
                    text,
                    metadata,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_round_summary(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        &text,
                        metadata,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_round_stateful_marker(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        marker,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Diagnostic {
                    event_type,
                    round_id,
                    data,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.persist_ai_stream_diagnostic(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &event_type,
                        round_id,
                        data,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolStatus {
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result,
                    risk,
                    summary,
                    synthetic_denied,
                    raw_text,
                    round_id,
                    round_number,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        &status,
                        result,
                        risk,
                        summary,
                        synthetic_denied,
                        raw_text,
                        round_id,
                        round_number,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id,
                    name,
                    arguments,
                    risk,
                    summary,
                    sender,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        "pending_user_approval",
                        None,
                        Some(risk),
                        Some(summary),
                        false,
                        None,
                        None,
                        None,
                        cx,
                    );
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.register_tool_approval(delivery.generation, tool_call_id, sender);
                    });
                }
                AiStreamDeliveryEvent::ToolCandidateSelectionRequested {
                    tool_call_id,
                    name,
                    arguments,
                    candidates,
                    sender,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    let candidate_count = candidates.len();
                    let result = serde_json::json!({
                        "ok": false,
                        "summary": self.i18n.t("ai.tool_use.choose_target"),
                        "disambiguation": {
                            "candidates": candidates,
                        },
                        "recoverable": true,
                    });
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        "pending_user_selection",
                        Some(result),
                        Some("read".to_string()),
                        Some(self.i18n.t("ai.tool_use.choose_target")),
                        false,
                        None,
                        None,
                        None,
                        cx,
                    );
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.register_tool_candidate_selection(
                            delivery.generation,
                            tool_call_id,
                            candidate_count,
                            sender,
                        );
                    });
                    window.focus(&self.focus_handle, cx);
                    cx.notify();
                }
                AiStreamDeliveryEvent::ToolPreflightRequested {
                    tool_session_id,
                    tool_call_id,
                    name,
                    args,
                    sender,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    let session_is_active = self.ai_runtime_context.read(cx).is_active_tool_session(
                        delivery.generation,
                        &tool_session_id,
                    );
                    let rejection = if !session_is_active {
                        Some(rejected_ai_tool_result(
                            tool_call_id,
                            name,
                            "operation_cancelled",
                            "The AI tool session is no longer active.",
                        ))
                    } else {
                        self.preflight_ai_ui_orchestrator_tool(
                            &tool_session_id,
                            &name,
                            &args,
                            cx,
                        )
                        .err()
                        .map(|error| {
                            let snapshot = self.ai_orchestrator_snapshot_for_tool_session(
                                Some(&tool_session_id),
                                cx,
                            );
                            snapshot.to_executed_tool_result(
                                tool_call_id,
                                name,
                                snapshot.fail(
                                    "Runtime target is unavailable.",
                                    error.public_code(),
                                    "Rediscover the current terminal target before retrying.",
                                    "interactive",
                                ),
                                0,
                            )
                        })
                    };
                    let _ = sender.send(rejection);
                }
                AiStreamDeliveryEvent::RuntimeContextRequested {
                    tool_session_id,
                    sender,
                } => {
                    let session_is_active = self.ai_runtime_context.read(cx).is_active_tool_session(
                        delivery.generation,
                        &tool_session_id,
                    );
                    let context = session_is_active.then(|| {
                        self.ai_runtime_context_prompt(&tool_session_id, cx)
                    });
                    let _ = sender.send(context);
                }
                AiStreamDeliveryEvent::ToolExecutionRequested {
                    tool_session_id,
                    tool_call_id,
                    name,
                    args,
                    post_user_approval,
                    dangerous_command_approved,
                    sender,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    let session_is_active = self.ai_runtime_context.read(cx).is_active_tool_session(
                        delivery.generation,
                        &tool_session_id,
                    );
                    if !session_is_active {
                        let _ = sender.send(rejected_ai_tool_result(
                            tool_call_id,
                            name,
                            "operation_cancelled",
                            "The AI tool session is no longer active.",
                        ));
                        continue;
                    }
                    self.start_ai_ui_orchestrator_tool_execution(
                        delivery.conversation_id.clone(),
                        tool_session_id,
                        tool_call_id,
                        name,
                        args,
                        post_user_approval,
                        dangerous_command_approved,
                        sender,
                        window,
                        cx,
                    );
                }
            }
        }
        self.flush_pending_ai_stream_text(&mut pending_text, cx);
    }

    pub(in crate::workspace) fn merge_or_flush_pending_ai_stream_text(
        &mut self,
        pending: &mut Option<PendingAiStreamText>,
        generation: u64,
        conversation_id: String,
        assistant_id: String,
        kind: PendingAiStreamTextKind,
        chunk: String,
        cx: &mut Context<Self>,
    ) {
        if chunk.is_empty() {
            return;
        }
        if let Some(existing) = pending.as_mut()
            && existing.generation == generation
            && existing.conversation_id == conversation_id
            && existing.assistant_id == assistant_id
            && existing.kind == kind
        {
            existing.text.push_str(&chunk);
            return;
        }

        self.flush_pending_ai_stream_text(pending, cx);
        *pending = Some(PendingAiStreamText {
            generation,
            conversation_id,
            assistant_id,
            kind,
            text: chunk,
        });
    }

    pub(in crate::workspace) fn flush_pending_ai_stream_text(
        &mut self,
        pending: &mut Option<PendingAiStreamText>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = pending.take() else {
            return;
        };
        let event = match pending.kind {
            PendingAiStreamTextKind::Content => AiStreamEvent::Content(pending.text),
            PendingAiStreamTextKind::Thinking => AiStreamEvent::Thinking(pending.text),
        };
        self.apply_ai_stream_event(
            pending.generation,
            &pending.conversation_id,
            &pending.assistant_id,
            event,
            cx,
        );
    }

    pub(in crate::workspace) fn apply_ai_compaction_deliveries(
        &mut self,
        deliveries: VecDeque<AiCompactionDelivery>,
        cx: &mut Context<Self>,
    ) {
        for delivery in deliveries {
            match delivery.kind {
                AiCompactionDeliveryKind::Compact => {
                    if let Some(plan) = delivery.plan {
                        self.finish_ai_compaction(
                            delivery.conversation_id,
                            delivery.base_ids,
                            plan,
                            delivery.summary,
                            delivery.failed,
                            delivery.resume_after,
                            delivery.silent,
                            cx,
                        );
                    }
                }
                AiCompactionDeliveryKind::Summary => {
                    self.finish_ai_summary(
                        delivery.conversation_id,
                        delivery.base_ids,
                        delivery.summary,
                        delivery.failed,
                        cx,
                    );
                }
            }
        }
    }
}
