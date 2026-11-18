use super::*;
use oxideterm_public_mcp::history::{
    HISTORY_SNAPSHOT_BYTES, HistoryBuffer, HistoryCapture, HistoryError, HistoryRequest,
    HistoryRow, HistorySelection, HistoryStamp, HistoryTarget,
};
use oxideterm_terminal::{TerminalHistoryError, TerminalHistoryStamp};

impl PublicMcpBackground {
    pub fn terminal_history(
        &self,
        request: DomainRequest,
        session: Option<(TerminalSessionId, PublicMcpTerminalSession)>,
    ) {
        let backend = self.clone();
        self.forwarding_runtime.spawn(async move {
            backend.run_terminal_history(request, session).await;
        });
    }

    fn history_authorized(&self, request: &DomainRequest, suspended: bool) -> bool {
        !suspended
            && !request.is_cancelled()
            && self
                .state
                .clients
                .get(&request.client_ref)
                .is_some_and(|c| c.enabled && c.tool_groups.contains(&ToolGroup::TerminalObserve))
    }

    fn history_target_live(&self, client: &ClientRef, target: &HistoryTarget) -> bool {
        self.terminals
            .lock()
            .get(&target.terminal)
            .is_some_and(|r| {
                &r.client_ref == client && r.session_id.0.to_string() == target.session
            })
    }

    async fn run_terminal_history(
        self,
        request: DomainRequest,
        session: Option<(TerminalSessionId, PublicMcpTerminalSession)>,
    ) {
        let PublicToolCall::TerminalHistory(args) = &request.call else {
            return;
        };
        let args = args.clone();
        let gate = {
            let mut cache = self.history.lock();
            cache.expire(Instant::now());
            let slot = match &args.request {
                HistoryRequest::Snapshot { slot, .. } | HistoryRequest::ReleaseSlot { slot } => {
                    Ok(*slot)
                }
                HistoryRequest::Page { cursor } => cache.cursor_slot(&request.client_ref, cursor),
                HistoryRequest::Release { snapshot_ref } => {
                    cache.cursor_slot(&request.client_ref, snapshot_ref)
                }
            };
            match slot {
                Ok(slot) => cache.gate(&request.client_ref, slot),
                Err(_) if matches!(args.request, HistoryRequest::Release { .. }) => {
                    finish_serialized(request, json!({"status":"released"}));
                    return;
                }
                Err(error) => {
                    request.finish(error.envelope());
                    return;
                }
            }
        };
        let cancellation = request.cancellation_token();
        let _guard = tokio::select! {
            guard = gate.lock() => guard,
            _ = cancellation.cancelled() => return,
        };

        let HistoryRequest::Snapshot {
            terminal_ref,
            buffer,
            selection,
            slot,
            after_revision,
        } = &args.request
        else {
            let suspended = self.admission.lock();
            if request.is_cancelled() {
                return;
            }
            if !self.history_authorized(&request, *suspended) {
                request.finish(HistoryError::TerminalUnavailable.envelope());
                return;
            }
            let mut cache = self.history.lock();
            let result = match &args.request {
                HistoryRequest::Page { cursor } => cache
                    .target(&request.client_ref, cursor)
                    .and_then(|target| {
                        if !self.history_target_live(&request.client_ref, &target) {
                            cache.revoke_terminal(&target.terminal, true);
                            return Err(HistoryError::SnapshotUnavailable);
                        }
                        cache.page(&request.client_ref, cursor, &args, Instant::now())
                    }),
                HistoryRequest::Release { snapshot_ref } => {
                    cache.release(&request.client_ref, snapshot_ref);
                    Ok(json!({"status":"released"}))
                }
                HistoryRequest::ReleaseSlot { slot } => {
                    cache.release_slot(&request.client_ref, *slot);
                    Ok(json!({"status":"released"}))
                }
                _ => unreachable!(),
            };
            match result {
                Ok(value) => finish_serialized(request, value),
                Err(error) => request.finish(error.envelope()),
            }
            return;
        };
        let Some((session_id, session)) = session else {
            request.finish(HistoryError::TerminalUnavailable.envelope());
            return;
        };
        let target = HistoryTarget {
            terminal: terminal_ref.clone(),
            session: session_id.0.to_string(),
        };
        let reservation = {
            let suspended = self.admission.lock();
            let mut cache = self.history.lock();
            if !self.history_authorized(&request, *suspended)
                || !self.history_target_live(&request.client_ref, &target)
            {
                request.finish(HistoryError::TerminalUnavailable.envelope());
                return;
            }
            match cache.reserve(
                &request.client_ref,
                *slot,
                target.clone(),
                *buffer,
                *selection,
                after_revision.as_deref(),
                Instant::now(),
            ) {
                Ok(reservation) => reservation,
                Err(error) => {
                    request.finish(error.envelope());
                    return;
                }
            }
        };
        let alternate = *buffer == HistoryBuffer::Alternate;
        let screen = *selection == HistorySelection::Screen {};
        let previous = reservation.unchanged_stamp().map(|s| TerminalHistoryStamp {
            identity: s.identity,
            content: s.content,
            switches: s.switches,
            cleared: s.cleared,
            evicted: s.evicted,
        });
        let id = reservation.id_for_cleanup();
        let captured =
            self.forwarding_runtime
                .spawn_blocking(move || {
                    let session = session.lock();
                    let mut result = session.history_capture(
                        alternate,
                        screen,
                        previous.as_ref(),
                        HISTORY_SNAPSHOT_BYTES - 8192,
                        &|| cancellation.is_cancelled(),
                    );
                    // A revision may expire while waiting for the backend lock. Freeze full
                    // content in that case instead of returning an expired unchanged token.
                    if result.as_ref().is_ok_and(|c| {
                        c.rows.is_none() && !reservation.revision_valid_at(c.captured)
                    }) {
                        result = session.history_capture(
                            alternate,
                            screen,
                            None,
                            HISTORY_SNAPSHOT_BYTES - 8192,
                            &|| cancellation.is_cancelled(),
                        );
                    }
                    (
                        reservation,
                        result.map(|capture| HistoryCapture {
                            stamp: HistoryStamp {
                                identity: capture.stamp.identity,
                                content: capture.stamp.content,
                                switches: capture.stamp.switches,
                                cleared: capture.stamp.cleared,
                                evicted: capture.stamp.evicted,
                            },
                            captured: capture.captured,
                            wall_time: capture.wall_time,
                            screen_rows: capture.screen_rows,
                            retained_rows: capture.retained_rows,
                            active_buffer: if capture.alternate_active {
                                HistoryBuffer::Alternate
                            } else {
                                HistoryBuffer::Primary
                            },
                            rows: capture.rows.map(|rows| {
                                rows.into_iter()
                                    .map(|row| HistoryRow {
                                        text: row.text,
                                        soft_wrapped: row.soft_wrapped,
                                    })
                                    .collect()
                            }),
                        }),
                    )
                })
                .await;
        let suspended = self.admission.lock();
        let mut cache = self.history.lock();
        let Ok((reservation, captured)) = captured else {
            cache.cancel_id(&id);
            request.finish(HistoryError::TerminalUnavailable.envelope());
            return;
        };
        if !self.history_authorized(&request, *suspended)
            || !self.history_target_live(&request.client_ref, &target)
        {
            // Erase a completed freeze before releasing its budget.
            drop(captured);
            cache.cancel(&reservation);
            request.finish(HistoryError::TerminalUnavailable.envelope());
            return;
        }
        let result = match captured {
            Ok(capture) => cache.publish(reservation, capture, &args, Instant::now()),
            Err(error) => {
                cache.cancel(&reservation);
                Err(match error {
                    TerminalHistoryError::Unsupported => HistoryError::Unsupported,
                    TerminalHistoryError::BufferUnavailable => HistoryError::BufferUnavailable,
                    TerminalHistoryError::TooLarge => HistoryError::SnapshotTooLarge,
                    TerminalHistoryError::Cancelled => HistoryError::TerminalUnavailable,
                })
            }
        };
        match result {
            Ok(value) => finish_serialized(request, value),
            Err(error) => request.finish(error.envelope()),
        }
    }
}
