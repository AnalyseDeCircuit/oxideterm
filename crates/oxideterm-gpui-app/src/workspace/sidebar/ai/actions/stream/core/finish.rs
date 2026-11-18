impl WorkspaceApp {
    pub(in crate::workspace) fn finish_ai_compaction(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: AiCompactionPlan,
        summary: String,
        failed: bool,
        resume_after: Option<AiPendingChatStream>,
        silent: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai.state
            .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
        if failed {
            self.ai.state.update(cx, |ai, _| { ai.history.compaction_sources.remove(&conversation_id); });
            if silent {
                self.ai.state.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            if !silent {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                    TerminalNoticeVariant::Error,
                    cx,
                );
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        if summary.trim().is_empty() {
            self.ai.state.update(cx, |ai, _| { ai.history.compaction_sources.remove(&conversation_id); });
            if silent {
                self.ai.state.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let anchor_id = self.next_ai_chat_id(now, cx);
        let compaction_result = self.ai.state.update(cx, |ai, _cx| {
            ai.apply_compaction_result(
                &conversation_id,
                &base_ids,
                plan,
                &summary,
                &anchor_id,
                now,
            )
        });
        let Some((
            summary_source_transcript_ref,
            summary_round_id,
            compacted_until_entry_id,
            total_compacted,
        )) = compaction_result
        else {
            if silent {
                self.ai.state.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        };
        self.persist_ai_summary_created(
            &conversation_id,
            &anchor_id,
            "compaction",
            &summary,
            summary_round_id,
            Some(summary_source_transcript_ref),
            Some(total_compacted),
            compacted_until_entry_id,
            Some(if silent { "background" } else { "manual" }),
            now,
            cx,
        );
        if silent {
            self.ai.state.update(cx, |ai, cx| {
                ai.set_compaction_notice_done(&conversation_id, total_compacted, cx);
            });
        }
        self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn resume_ai_chat_after_pre_send_compaction(
        &mut self,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = resume_after else {
            return;
        };
        if !self
            .ai.state
            .read(cx)
            .chat_launch_matches(&pending.conversation_id, pending.launch_id.as_deref())
        {
            return;
        }
        self.start_ai_chat_stream_after_budget_preflight(
            pending.conversation_id,
            pending.config,
            pending.request_content,
            pending.task_system_prompt,
            pending.rag_system_prompt,
            false,
            cx,
        );
    }

    pub(in crate::workspace) fn finish_ai_summary(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        summary: String,
        failed: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai.state
            .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
        self.ai.state.update(cx, |ai, _cx| ai.set_conversation_loading(&conversation_id, false));
        if failed {
            self.ai.state.update(cx, |ai, _| { ai.history.compaction_sources.remove(&conversation_id); });
            self.push_ai_settings_toast(
                self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                TerminalNoticeVariant::Error,
                cx,
            );
            cx.notify();
            return;
        }
        if summary.trim().is_empty() {
            self.ai.state.update(cx, |ai, _| { ai.history.compaction_sources.remove(&conversation_id); });
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let summary_id = self.next_ai_chat_id(now, cx);
        let original_count = base_ids.len();
        let prefix = self
            .i18n
            .t("ai.context.summary_prefix")
            .replace("{{count}}", &original_count.to_string());
        let summary_result = self.ai.state.update(cx, |ai, _cx| {
            ai.apply_conversation_summary(
                &conversation_id,
                &base_ids,
                &summary_id,
                &summary,
                &prefix,
                now,
            )
        });
        let Some((summary_source_transcript_ref, summary_round_id)) = summary_result else {
            cx.notify();
            return;
        };
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_model_switch_warning(None);
        });
        self.persist_ai_summary_created(
            &conversation_id,
            &summary_id,
            "conversation",
            &summary,
            summary_round_id,
            Some(summary_source_transcript_ref),
            Some(original_count),
            None,
            Some("manual"),
            now,
            cx,
        );
        cx.notify();
    }
}
