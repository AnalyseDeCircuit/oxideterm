const AI_AGENT_CONCURRENCY_SETTING: &str = "subagentConcurrency";

impl WorkspaceApp {
    fn agent_usage_label(&self, usage: oxideterm_ai::agent::AgentUsage) -> String {
        match (usage.input_tokens, usage.output_tokens) {
            (Some(input), Some(output)) => self
                .i18n
                .t("ai.agents.usage")
                .replace("{{input}}", &input.to_string())
                .replace("{{output}}", &output.to_string()),
            _ => self.i18n.t("ai.agents.usage_unknown"),
        }
    }
    fn agent_control(
        &self,
        id: String,
        label: String,
        body: Div,
        action: impl Fn(&mut WorkspaceApp, &mut Window, &mut Context<WorkspaceApp>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let action = std::rc::Rc::new(action);
        let click = action.clone();
        body.id(gpui::SharedString::from(id))
            .role(gpui::Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .focus_visible(|style| style.border_1().border_color(rgb(self.tokens.ui.accent)))
            .on_click(cx.listener(move |this, _event, window, cx| {
                click(this, window, cx);
                cx.stop_propagation();
            }))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        action(this, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
    }

    fn agent_button(
        &self,
        id: String,
        label: String,
        action: impl Fn(&mut WorkspaceApp, &mut Window, &mut Context<WorkspaceApp>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = oxideterm_gpui_ui::button::button_with(
            &self.tokens,
            label.clone(),
            oxideterm_gpui_ui::button::ButtonOptions {
                variant: oxideterm_gpui_ui::button::ButtonVariant::Ghost,
                size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                ..Default::default()
            },
        );
        self.agent_control(id, label, body, action, cx)
            .into_any_element()
    }

    fn agent_state_label(&self, state: AgentState) -> String {
        self.i18n.t(match state {
            AgentState::Queued => "ai.agents.queued",
            AgentState::Running => "ai.agents.running",
            AgentState::AwaitingApproval => "ai.agents.approval",
            AgentState::AwaitingParent => "ai.agents.reply",
            AgentState::AwaitingResource => "ai.agents.resource",
            AgentState::Stopping => "ai.agents.stopping",
            AgentState::Completed => "ai.agents.completed",
            AgentState::Failed => "ai.agents.failed",
            AgentState::Cancelled => "ai.agents.cancelled",
            AgentState::Interrupted => "ai.agents.interrupted",
        })
    }

    fn configured_agent_models(&self) -> Vec<AgentModel> {
        ai_provider_views(&self.settings_store.settings().ai.providers)
            .into_iter()
            .filter(|provider| provider.enabled)
            .flat_map(|provider| {
                provider.models.into_iter().map(move |model| AgentModel {
                    provider_id: provider.id.clone(),
                    model,
                })
            })
            .collect()
    }

    fn select_agent_model(
        &mut self,
        target: Option<oxideterm_ai::agent::AgentRunId>,
        model: Option<AgentModel>,
        cx: &mut Context<Self>,
    ) {
        let changed = self.ai_entity.update(cx, |ai, _cx| {
            let changed = if let Some(id) = target {
                ai.agents
                    .records
                    .get(&id)
                    .zip(model)
                    .is_some_and(|(record, model)| {
                        ai.agents
                            .services
                            .runtime
                            .change_queued_model(&record.snapshot.run, model)
                            .is_ok()
                    })
            } else if let Some(id) = ai.conversation_state().active_conversation_id.clone() {
                let mut options = ai.agent_options(&id);
                options.default_model = model;
                ai.set_agent_options(&id, options);
                true
            } else {
                false
            };
            ai.agents.model_picker_open = false;
            ai.agents.settings_model_picker_open = false;
            changed
        });
        if !changed {
            self.push_ai_settings_toast(
                self.i18n.t("ai.agents.model_locked"),
                TerminalNoticeVariant::Warning,
                cx,
            );
        }
        cx.notify();
    }

    fn render_agent_model_picker(
        &self,
        target: Option<oxideterm_ai::agent::AgentRunId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .id("agent-model-options")
            .max_h(px(180.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one));
        if target.is_none() {
            list = list.child(self.agent_button(
                "agent-inherit-model".into(),
                self.i18n.t("ai.agents.inherit_model"),
                |this, _, cx| this.select_agent_model(None, None, cx),
                cx,
            ));
        }
        for model in self.configured_agent_models() {
            let target = target.clone();
            list = list.child(self.agent_button(
                format!("agent-model-{}-{}", model.provider_id, model.model),
                format!("{} / {}", model.provider_id, model.model),
                move |this, _, cx| this.select_agent_model(target.clone(), Some(model.clone()), cx),
                cx,
            ));
        }
        list.into_any_element()
    }

    fn agent_concurrency(&self) -> usize {
        self.settings_store
            .settings()
            .ai
            .extra
            .get(AI_AGENT_CONCURRENCY_SETTING)
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(oxideterm_ai::agent::DEFAULT_AGENT_CONCURRENCY)
            .clamp(1, oxideterm_ai::agent::MAX_AGENT_CONCURRENCY)
    }

    pub(in crate::workspace) fn render_ai_agent_settings(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ai = self.ai_entity.read(cx);
        let conversation = ai
            .conversation_state()
            .active_conversation()
            .map(|conversation| (conversation.id.clone(), conversation.title.clone()));
        let mut section = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(self.ai_section_title("settings_view.ai.conversation_agents"))
            .child(div().text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.ai.conversation_agents_hint")));
        if let Some((id, title)) = conversation {
            let options = ai.agent_options(&id);
            let picker_open = ai.agents.settings_model_picker_open;
            section = section.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("ai.agents.current_conversation")
                            .replace("{{title}}", &title),
                    ),
            );
            let checked = options.enabled;
            let toggle = self
                .agent_control(
                    "allow-subagents".into(),
                    self.i18n.t("ai.agents.allow"),
                    oxideterm_gpui_ui::checkbox::checkbox(&self.tokens, String::new(), checked),
                    move |this, _, cx| {
                        this.ai_entity.update(cx, |ai, _cx| {
                            let mut options = ai.agent_options(&id);
                            options.enabled = !options.enabled;
                            ai.set_agent_options(&id, options);
                        });
                        cx.notify();
                    },
                    cx,
                )
                .role(gpui::Role::CheckBox)
                .aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .into_any_element();
            section = section.child(self.setting_row(
                "ai.agents.allow",
                "ai.agents.scope_hint",
                toggle,
                cx,
            ));
            let label = options
                .default_model
                .map(|model| format!("{} / {}", model.provider_id, model.model))
                .unwrap_or_else(|| self.i18n.t("ai.agents.inherit_model"));
            let picker = self.agent_button(
                "agent-default-model".into(),
                label,
                |this, _, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.agents.settings_model_picker_open = !ai.agents.settings_model_picker_open
                    });
                    cx.notify();
                },
                cx,
            );
            section = section.child(self.setting_row(
                "ai.agents.model",
                "ai.agents.model_hint",
                picker,
                cx,
            ));
            if picker_open {
                section = section.child(self.render_agent_model_picker(None, cx));
            }
        } else {
            section = section.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.agents.no_conversation")),
            );
        }
        section.into_any_element()
    }

    pub(in crate::workspace) fn render_ai_agent_concurrency_setting(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.agent_concurrency();
        let mut choices = oxideterm_gpui_ui::tabs::segmented_tabs(&self.tokens);
        for limit in 1..=oxideterm_ai::agent::MAX_AGENT_CONCURRENCY {
            choices = choices.child(
                self.agent_control(
                    format!("agent-limit-{limit}"),
                    limit.to_string(),
                    oxideterm_gpui_ui::tabs::segmented_tab(
                        &self.tokens,
                        limit.to_string(),
                        current == limit,
                    ),
                    move |this, _, cx| {
                        this.edit_settings(
                            move |settings| {
                                settings.ai.extra.insert(
                                    AI_AGENT_CONCURRENCY_SETTING.into(),
                                    serde_json::json!(limit),
                                );
                            },
                            cx,
                        );
                        this.ai_entity
                            .read(cx)
                            .agents
                            .services
                            .runtime
                            .set_concurrency(limit);
                        cx.notify();
                    },
                    cx,
                )
                .aria_selected(current == limit),
            );
        }
        self.setting_row(
                "ai.agents.concurrency",
                "ai.agents.concurrency_hint",
                choices.into_any_element(),
                cx,
            )
    }

    fn render_ai_agent_resource_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ai = self.ai_entity.read(cx);
        let id = ai.conversation_state().active_conversation_id.as_deref()?;
        let resources = ai.agents.services.resources.clone();
        let keys = ai
            .agents
            .services
            .runtime
            .unresolved_resources(id, &resources);
        if keys.is_empty() {
            return None;
        }
        Some(
            ai_tool_block(&self.tokens)
                .px(px(self.tokens.spacing.three))
                .child(
                    div()
                        .text_size(px(11.0))
                        .child(self.i18n.t("ai.agents.remote_unknown")),
                )
                .child(self.agent_button(
                    "agent-return-conversation-control".into(),
                    self.i18n.t("ai.agents.return_control"),
                    move |_, _, cx| {
                        for key in &keys {
                            resources.invalidate(key);
                            resources.allow_new_requests(key);
                        }
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
        )
    }

    fn render_ai_agent_group(
        &self,
        message_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut latest = std::collections::BTreeMap::new();
        for record in self
            .ai_entity
            .read(cx)
            .agents
            .records
            .values()
            .filter(|record| record.parent_message_id == message_id)
        {
            let slot = latest
                .entry(record.snapshot.run.agent_id.clone())
                .or_insert(record);
            if slot.created_at_ms < record.created_at_ms {
                *slot = record;
            }
        }
        if latest.is_empty() {
            return None;
        }
        let mut rows: Vec<_> = latest
            .values()
            .map(|record| {
                (
                    record.created_at_ms,
                    record.snapshot.clone(),
                    record
                        .target_labels
                        .iter()
                        .map(AgentText::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            })
            .collect();
        rows.sort_by_key(|(created_at, _, _)| *created_at);
        let group_id = rows[0].1.run.group_id.clone();
        let active = rows
            .iter()
            .any(|(_, snapshot, _)| !snapshot.state.is_terminal());
        let done = rows
            .iter()
            .filter(|(_, snapshot, _)| snapshot.state == AgentState::Completed)
            .count();
        let running = rows
            .iter()
            .filter(|(_, snapshot, _)| snapshot.state == AgentState::Running)
            .count();
        let expanded = self
            .ai_entity
            .read(cx)
            .agents
            .expanded_groups
            .get(&group_id)
            .copied()
            .unwrap_or(false);
        let count = self
            .i18n
            .t("ai.agents.heading")
            .replace("{{done}}", &done.to_string())
            .replace("{{total}}", &rows.len().to_string())
            .replace("{{running}}", &running.to_string());
        let toggle_group = group_id.clone();
        let parent_message_id = message_id.to_owned();
        let mut header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .child(self.agent_control(
                format!("agent-group-{group_id}"),
                count.clone(),
                oxideterm_gpui_ui::ai::ai_tool_condensed_toggle(
                    &self.tokens,
                    count,
                    Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                    expanded,
                ),
                move |this, _, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.agents
                            .expanded_groups
                            .insert(toggle_group.clone(), !expanded);
                        *ai.agents
                            .parent_revisions
                            .entry(parent_message_id.clone())
                            .or_default() += 1;
                    });
                    cx.notify();
                },
                cx,
            ));
        if active {
            header = header.child(self.agent_button(
                format!("agent-stop-group-{group_id}"),
                self.i18n.t("ai.agents.stop_all"),
                move |this, _, cx| {
                    let ai = this.ai_entity.read(cx);
                    if let Some(group) = ai.agents.groups.get(&group_id) {
                        let _ = ai.agents.services.runtime.cancel_group(&group.parent);
                    }
                    cx.notify();
                },
                cx,
            ));
        }
        let mut block = ai_tool_block(&self.tokens).child(header);
        let records: Vec<_> = self
            .ai_entity
            .read(cx)
            .agents
            .records
            .values()
            .filter(|record| record.parent_message_id == message_id)
            .collect();
        let parent_usage = records
            .iter()
            .max_by_key(|record| record.revision)
            .map(|record| record.parent_usage)
            .unwrap_or_default();
        let child_usage = oxideterm_ai::agent::AgentUsage::total(
            records.iter().map(|record| record.snapshot.usage),
        );
        let total_usage = oxideterm_ai::agent::AgentUsage::total([parent_usage, child_usage]);
        if expanded {
            block = block.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("ai.agents.group_usage")
                            .replace("{{parent}}", &self.agent_usage_label(parent_usage))
                            .replace("{{children}}", &self.agent_usage_label(child_usage))
                            .replace("{{total}}", &self.agent_usage_label(total_usage)),
                    ),
            );
        }
        if expanded {
            for (_, snapshot, labels) in rows {
                let id = snapshot.run.run_id.clone();
                let status = self.agent_state_label(snapshot.state);
                let summary = snapshot
                    .result
                    .as_ref()
                    .map(|result| result.summary.as_str())
                    .filter(|summary| !summary.is_empty())
                    .unwrap_or(snapshot.progress.as_str());
                let body = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .p(px(self.tokens.spacing.two))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(self.tokens.spacing.two))
                            .items_center()
                            .child(Self::render_lucide_icon(
                                match snapshot.state {
                                    AgentState::Completed => LucideIcon::Check,
                                    AgentState::Failed => LucideIcon::AlertTriangle,
                                    AgentState::Running => LucideIcon::LoaderCircle,
                                    _ => LucideIcon::Clock,
                                },
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(snapshot.title.as_str().to_owned()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(status),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .text_size(px(11.0))
                            .child(labels),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .child(summary.to_owned()),
                    );
                let task_row = self
                    .agent_control(
                        format!("agent-task-{id}"),
                        snapshot.title.as_str().to_owned(),
                        body,
                        move |this, _, cx| {
                            this.ai_entity
                                .update(cx, |ai, cx| ai.open_agent_detail(id.clone(), cx));
                            cx.notify();
                        },
                        cx,
                    )
                    .flex_1()
                    .min_w_0();
                let mut row = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(task_row);
                if !snapshot.state.is_terminal() {
                    let stop_run = snapshot.run.clone();
                    row = row.child(self.agent_button(
                        format!("agent-row-stop-{}", stop_run.run_id),
                        self.i18n.t("ai.agents.stop"),
                        move |this, _, cx| {
                            let runtime = &this.ai_entity.read(cx).agents.services.runtime;
                            if let Ok(parent) = runtime.parent_run(&stop_run) {
                                let _ = runtime.stop(&parent, &stop_run);
                            }
                            cx.notify();
                        },
                        cx,
                    ));
                }
                block = block.child(row);
            }
        }
        Some(block.into_any_element())
    }

    fn render_agent_markdown(&self, id: String, text: &str, cx: &mut Context<Self>) -> AnyElement {
        let message = AiChatMessage {
            id,
            role: AiChatRole::Assistant,
            content: text.to_owned(),
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        self.render_ai_message_content(&message, None, cx)
    }

    fn render_ai_agent_detail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.ai_entity.read(cx).agents.detail.clone()?;
        let record = self.ai_entity.read(cx).agents.records.get(&id)?.clone();
        let run = record.snapshot.run.clone();
        let state = record.snapshot.state;
        let mut header = div()
            .w_full()
            .min_w_0()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(self.tokens.spacing.one))
            .child(self.agent_button(
                "agent-back".into(),
                self.i18n.t("ai.agents.back"),
                |this, _, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.agents.detail = None;
                        ai.agents.model_picker_open = false;
                    });
                    cx.notify();
                },
                cx,
            ));
        header = header.items_center().child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(record.snapshot.title.as_str().to_owned()),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.agent_state_label(state)),
                ),
        );
        if !state.is_terminal() {
            let stop_run = run.clone();
            header = header.child(self.agent_button(
                "agent-stop-one".into(),
                self.i18n.t("ai.agents.stop"),
                move |this, _, cx| {
                    let runtime = &this.ai_entity.read(cx).agents.services.runtime;
                    if let Ok(parent) = runtime.parent_run(&stop_run) {
                        let _ = runtime.stop(&parent, &stop_run);
                    }
                    cx.notify();
                },
                cx,
            ));
        }
        let mut body = div()
            .id("agent-detail-body")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .p(px(self.tokens.spacing.three));
        if let Some(scroll) = self.ai_entity.read(cx).agents.detail_scroll.get(&id) {
            body = body.track_scroll(scroll);
        }
        if state == AgentState::Queued {
            body = body.child(self.agent_button(
                "agent-queued-model".into(),
                self.i18n.t("ai.agents.change_model"),
                |this, _, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.agents.model_picker_open = !ai.agents.model_picker_open
                    });
                    cx.notify();
                },
                cx,
            ));
            if self.ai_entity.read(cx).agents.model_picker_open {
                body = body.child(self.render_agent_model_picker(Some(id.clone()), cx));
            }
        }
        if self.ai_entity.read(cx).agents.details_loading.contains(&id) {
            body = body.child(ai_tool_heading(
                &self.tokens,
                self.i18n.t("ai.agents.loading"),
            ));
        } else if self.ai_entity.read(cx).agents.detail_errors.contains(&id) {
            body = body.child(
                div()
                    .text_color(rgb(self.tokens.ui.error))
                    .child(self.i18n.t("ai.agents.load_error")),
            );
        }
        for message in &record.messages {
            if message.role != AiChatRole::Assistant {
                continue;
            }
            let mut content = div()
                .w_full()
                .min_w_0()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.two));
            if ai_turn_parts(message).is_some_and(|parts| !parts.is_empty()) {
                content = self.render_ai_turn_parts(content, message, None, cx);
            } else {
                if !message.content.is_empty() {
                    content = content.child(self.render_ai_message_content(message, None, cx));
                }
                if !message.tool_calls.is_empty() {
                    content = content.child(self.render_ai_tool_calls(message, cx));
                }
            }
            body = body.child(content);
        }
        let auxiliary_key = format!("agent-{id}-context");
        let auxiliary_open = self
            .ai_entity
            .read(cx)
            .chat_ui()
            .tool_call_expansion_state
            .contains(&auxiliary_key);
        body = body.child(self.agent_control(
            auxiliary_key.clone(),
            self.i18n.t("ai.agents.context"),
            oxideterm_gpui_ui::ai::ai_tool_condensed_toggle(
                &self.tokens,
                self.i18n.t("ai.agents.context"),
                Self::render_lucide_icon(
                    if auxiliary_open {
                        LucideIcon::ChevronDown
                    } else {
                        LucideIcon::ChevronRight
                    },
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ),
                auxiliary_open,
            ),
            move |this, _, cx| {
                this.ai_entity.update(cx, |ai, _| {
                    ai.toggle_tool_call_expansion(auxiliary_key.clone());
                });
                cx.notify();
            },
            cx,
        ));
        if auxiliary_open {
            body = body
                .child(ai_tool_heading(
                    &self.tokens,
                    format!(
                        "{}: {} / {}",
                        self.i18n.t("ai.agents.model"),
                        record.snapshot.model.provider_id,
                        record.snapshot.model.model
                    ),
                ))
                .child(ai_tool_heading(
                    &self.tokens,
                    record
                        .target_labels
                        .iter()
                        .map(AgentText::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            body = body.child(self.render_agent_markdown(
                format!("agent-{id}-task"),
                record.snapshot.task.as_str(),
                cx,
            ));
            let mut runs: Vec<_> = self
                .ai_entity
                .read(cx)
                .agents
                .records
                .values()
                .filter(|other| {
                    other.snapshot.run.agent_id == run.agent_id
                        && other.snapshot.run.group_id == run.group_id
                })
                .map(|other| {
                    (
                        other.created_at_ms,
                        other.snapshot.run.run_id.clone(),
                        other.snapshot.state,
                    )
                })
                .collect();
            runs.sort_by_key(|(created, _, _)| *created);
            if runs.len() > 1 {
                body = body.child(ai_tool_heading(&self.tokens, self.i18n.t("ai.agents.runs")));
                for (index, (_, other, state)) in runs.into_iter().enumerate() {
                    let label = format!("#{} · {}", index + 1, self.agent_state_label(state));
                    body = body.child(self.agent_button(
                        format!("agent-run-{other}"),
                        label,
                        move |this, _, cx| {
                            this.ai_entity
                                .update(cx, |ai, cx| ai.open_agent_detail(other.clone(), cx));
                            cx.notify();
                        },
                        cx,
                    ));
                }
            }
            if !record.communication.is_empty() {
                body = body.child(ai_tool_heading(
                    &self.tokens,
                    self.i18n.t("ai.agents.communication"),
                ));
                for (index, message) in record.communication.iter().enumerate() {
                    let label = self.i18n.t(if message.consumed {
                        "ai.agents.consumed"
                    } else {
                        "ai.agents.received"
                    });
                    body = body.child(
                        div()
                            .text_size(px(11.0))
                            .child(format!(
                                "{} → {} · {label}",
                                if message.from.agent_id == run.agent_id {
                                    record.snapshot.title.as_str().to_owned()
                                } else {
                                    self.i18n.t("ai.agents.parent")
                                },
                                if message.to.agent_id == run.agent_id {
                                    record.snapshot.title.as_str().to_owned()
                                } else {
                                    self.i18n.t("ai.agents.parent")
                                }
                            ))
                            .child(self.render_agent_markdown(
                                format!("agent-{id}-communication-{index}"),
                                message.text.as_str(),
                                cx,
                            )),
                    );
                }
            }
            body = body.child(self.agent_usage_label(record.snapshot.usage));
        }
        let resources = self.ai_entity.read(cx).agents.services.resources.clone();
        let keys = resources.unresolved_owned_by(&run, state.is_terminal());
        if !keys.is_empty() {
            body = body.child(self.i18n.t("ai.agents.remote_unknown"));
            body = body.child(self.agent_button(
                "agent-return-control".into(),
                self.i18n.t("ai.agents.return_control"),
                move |_, _, cx| {
                    for key in &keys {
                        resources.invalidate(key);
                        resources.allow_new_requests(key);
                    }
                    cx.notify();
                },
                cx,
            ));
        }
        Some(
            div()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .child(header)
                .child(body)
                .into_any_element(),
        )
    }
}
