use super::*;

impl TerminalCommandSenderEntity {
    pub fn start(
        &mut self,
        sender_id: TerminalCommandSenderId,
        current_pane_id: Option<PaneId>,
        candidates: Vec<TerminalCommandSenderTarget>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = self.document(sender_id) else {
            return false;
        };
        // Starting is not an implicit restart gesture. Refusing a second Start
        // keeps the active task and its target ownership unchanged.
        if document.status == TerminalCommandSenderStatus::Running {
            return false;
        }
        let input = if self.is_expanded() {
            Zeroizing::new(document.editor.read(cx).buffer().text())
        } else {
            Zeroizing::new(document.compact_draft.to_string())
        };
        let mode = document.input_mode;
        let pacing = document.pacing;
        let repeat_count = document.repeat_count;
        let interval = Duration::from_millis(document.interval_ms);
        let scope = document.target_scope;
        let selected_targets = document.selected_targets.clone();
        let plan = match build_terminal_sender_plan(input, mode, pacing, repeat_count) {
            Ok(plan) => plan,
            Err(error) => {
                self.set_failure(sender_id, plan_failure(error), cx);
                return false;
            }
        };

        let targets = resolve_run_targets(scope, current_pane_id, &selected_targets, candidates);
        if targets.is_empty() {
            self.set_failure(sender_id, TerminalCommandSenderFailure::NoTargets, cx);
            return false;
        }
        if targets.iter().any(|target| {
            self.target_owners
                .get(&target.pane_id)
                .is_some_and(|owner| *owner != sender_id)
        }) {
            self.set_failure(sender_id, TerminalCommandSenderFailure::TargetBusy, cx);
            return false;
        }

        self.cancel_task(sender_id);
        let generation = {
            let document = self
                .document_mut(sender_id)
                .expect("sender document disappeared before task start");
            document.generation = document.generation.wrapping_add(1);
            document.status = TerminalCommandSenderStatus::Running;
            document.failure = None;
            document.completed_units = 0;
            document.total_units = plan.total_units();
            document.accepted_writes = 0;
            document.skipped_writes = 0;
            document.last_progress_notify = None;
            document.generation
        };
        for target in &targets {
            self.target_owners.insert(target.pane_id, sender_id);
        }
        if let Some(editor) = self
            .document(sender_id)
            .map(|document| document.editor.clone())
        {
            editor.update(cx, |editor, _cx| editor.set_read_only(true));
        }

        let task = cx.spawn(async move |sender, cx| {
            let mut stopped_for_missing_targets = false;
            'repeats: for repeat_index in 0..plan.repeat_count() {
                for (frame_index, frame) in plan.frames().iter().enumerate() {
                    let is_final_unit = repeat_index + 1 == plan.repeat_count()
                        && frame_index + 1 == plan.frames().len();
                    let should_continue = sender
                        .update(cx, |sender, cx| {
                            sender.dispatch_frame(sender_id, generation, frame, &targets, cx)
                        })
                        .unwrap_or(false);
                    if !should_continue {
                        stopped_for_missing_targets = true;
                        break 'repeats;
                    }
                    if !is_final_unit {
                        cx.background_executor().timer(interval).await;
                    }
                }
            }
            let _ = sender.update(cx, |sender, cx| {
                sender.finish_run(
                    sender_id,
                    generation,
                    if stopped_for_missing_targets {
                        TerminalCommandSenderOutcome::Stopped
                    } else {
                        TerminalCommandSenderOutcome::Completed
                    },
                    cx,
                );
            });
        });
        self.active_tasks.insert(sender_id, task);
        cx.notify();
        true
    }

    pub fn stop(&mut self, sender_id: TerminalCommandSenderId, cx: &mut Context<Self>) -> bool {
        let running = self
            .document(sender_id)
            .is_some_and(|document| document.status == TerminalCommandSenderStatus::Running);
        if !running {
            return false;
        }
        self.cancel_task(sender_id);
        if let Some(document) = self.document_mut(sender_id) {
            document.generation = document.generation.wrapping_add(1);
            document.status = TerminalCommandSenderStatus::Stopped;
            document.editor.update(cx, |editor, _cx| {
                editor.set_read_only(false);
            });
        }
        cx.notify();
        true
    }

    pub fn stop_all(&mut self, cx: &mut Context<Self>) {
        let running_ids = self
            .documents
            .iter()
            .filter(|document| document.status == TerminalCommandSenderStatus::Running)
            .map(|document| document.id)
            .collect::<Vec<_>>();
        for sender_id in running_ids {
            let _ = self.stop(sender_id, cx);
        }
    }

    fn dispatch_frame(
        &mut self,
        sender_id: TerminalCommandSenderId,
        generation: u64,
        frame: &TerminalSenderFrame,
        targets: &[TerminalCommandSenderTarget],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = self.document(sender_id) else {
            return false;
        };
        if document.generation != generation
            || document.status != TerminalCommandSenderStatus::Running
        {
            return false;
        }

        let mut accepted = 0u64;
        let mut skipped = 0u64;
        let mut live_targets = 0usize;
        for target in targets {
            let result = (target.write)(frame, cx);
            match result {
                Some(true) => {
                    live_targets += 1;
                    accepted = accepted.saturating_add(1);
                }
                Some(false) => {
                    live_targets += 1;
                    skipped = skipped.saturating_add(1);
                }
                None => skipped = skipped.saturating_add(1),
            }
        }
        let panel_expanded = self.is_expanded();
        let Some(document) = self.document_mut(sender_id) else {
            return false;
        };
        document.completed_units = document.completed_units.saturating_add(1);
        document.accepted_writes = document.accepted_writes.saturating_add(accepted);
        document.skipped_writes = document.skipped_writes.saturating_add(skipped);
        let now = Instant::now();
        let progress_complete = document.completed_units >= document.total_units;
        let repaint_due = document.last_progress_notify.is_none_or(|last| {
            now.duration_since(last) >= TERMINAL_SENDER_PROGRESS_NOTIFY_INTERVAL
        });
        if panel_expanded && (progress_complete || repaint_due) {
            document.last_progress_notify = Some(now);
            cx.notify();
        }
        // A live pane that rejected the frame is not a successful local write.
        accepted > 0 && live_targets > 0
    }

    fn finish_run(
        &mut self,
        sender_id: TerminalCommandSenderId,
        generation: u64,
        outcome: TerminalCommandSenderOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document_mut(sender_id) else {
            return;
        };
        if document.generation != generation
            || document.status != TerminalCommandSenderStatus::Running
        {
            return;
        }
        document.status = match outcome {
            TerminalCommandSenderOutcome::Completed => TerminalCommandSenderStatus::Completed,
            TerminalCommandSenderOutcome::Stopped => TerminalCommandSenderStatus::Stopped,
        };
        document.editor.update(cx, |editor, _cx| {
            editor.set_read_only(false);
        });
        self.release_target_ownership(sender_id);
        cx.notify();
    }

    fn set_failure(
        &mut self,
        sender_id: TerminalCommandSenderId,
        failure: TerminalCommandSenderFailure,
        cx: &mut Context<Self>,
    ) {
        if let Some(document) = self.document_mut(sender_id) {
            document.status = TerminalCommandSenderStatus::Failed;
            document.failure = Some(failure);
            document.completed_units = 0;
            document.total_units = 0;
            document.accepted_writes = 0;
            document.skipped_writes = 0;
            cx.notify();
        }
    }

    pub(super) fn cancel_task(&mut self, sender_id: TerminalCommandSenderId) {
        // Dropping the Entity-owned GPUI task cancels its pending timer.
        self.active_tasks.remove(&sender_id);
        self.release_target_ownership(sender_id);
    }

    fn release_target_ownership(&mut self, sender_id: TerminalCommandSenderId) {
        self.target_owners.retain(|_, owner| *owner != sender_id);
    }
}

fn resolve_run_targets(
    scope: TerminalCommandSenderTargetScope,
    current_pane_id: Option<PaneId>,
    selected_targets: &HashSet<PaneId>,
    candidates: Vec<TerminalCommandSenderTarget>,
) -> Vec<TerminalCommandSenderTarget> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|target| scope.includes(target.pane_id, current_pane_id, selected_targets))
        .filter(|target| seen.insert(target.pane_id))
        .collect()
}

pub(super) fn plan_failure(error: TerminalSenderPlanError) -> TerminalCommandSenderFailure {
    match error {
        TerminalSenderPlanError::EmptyInput | TerminalSenderPlanError::EmptyHexInput => {
            TerminalCommandSenderFailure::EmptyInput
        }
        TerminalSenderPlanError::InvalidHexDigit | TerminalSenderPlanError::OddHexDigitCount => {
            TerminalCommandSenderFailure::InvalidHex
        }
        TerminalSenderPlanError::ZeroRepeatCount | TerminalSenderPlanError::UnitCountOverflow => {
            TerminalCommandSenderFailure::EmptyInput
        }
    }
}
