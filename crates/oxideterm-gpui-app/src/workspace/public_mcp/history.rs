use super::{WorkspaceApp, finish_serialized};
use gpui::Context;
use oxideterm_public_mcp::history::{
    HISTORY_SNAPSHOT_BYTES, HistoryBuffer, HistoryCapture, HistoryError, HistoryRequest,
    HistoryRow, HistorySelection, HistoryStamp, HistoryTarget,
};
use oxideterm_public_mcp::{DomainRequest, PublicToolCall, ToolGroup};
use oxideterm_terminal::{TerminalHistoryError, TerminalHistoryStamp};
use serde_json::json;
use std::time::{Duration, Instant};

impl WorkspaceApp {
    pub(super) fn start_public_mcp_history_cleanup(&mut self, cx: &mut Context<Self>) {
        if self.public_mcp.history_cleanup.is_some() {
            return;
        }
        let store = self.public_mcp.history.clone();
        // The workspace retains this timer; it cannot keep a terminal or SSH transport alive.
        self.public_mcp.history_cleanup = Some(cx.spawn(async move |workspace, cx| {
            loop {
                gpui::Timer::after(Duration::from_secs(15)).await;
                if workspace
                    .update(cx, |_, _| store.lock().expire(Instant::now()))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(super) fn handle_public_mcp_terminal_history(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::TerminalHistory(args) = &request.call else {
            return;
        };
        let args = args.clone();
        self.start_public_mcp_history_cleanup(cx);
        let store = self.public_mcp.history.clone();
        let slot = {
            let mut cache = store.lock();
            cache.expire(Instant::now());
            match &args.request {
                HistoryRequest::Snapshot { slot, .. } | HistoryRequest::ReleaseSlot { slot } => {
                    Ok(*slot)
                }
                HistoryRequest::Page { cursor } => cache.cursor_slot(&request.client_ref, cursor),
                HistoryRequest::Release { snapshot_ref } => {
                    cache.cursor_slot(&request.client_ref, snapshot_ref)
                }
            }
        };
        let slot = match slot {
            Ok(slot) => slot,
            Err(_) if matches!(args.request, HistoryRequest::Release { .. }) => {
                finish_serialized(request, json!({"status":"released"}));
                return;
            }
            Err(error) => {
                request.finish(error.envelope());
                return;
            }
        };
        let gate = store.lock().gate(&request.client_ref, slot);
        // This finite job is bounded by the broker cancellation token. Publication returns to
        // the workspace, sharing the same serialization boundary as revocation and reconnect.
        cx.spawn(async move |workspace, cx| {
            let cancellation = request.cancellation_token();
            let _guard = tokio::select! {
                guard=gate.lock() => guard,
                _=cancellation.cancelled()=>return,
            };
            let prepared = workspace.update(cx, |workspace, cx| {
                if request.is_cancelled() {
                    return Err(HistoryError::TerminalUnavailable);
                }
                let authorized = workspace.public_mcp.clients().iter().any(|c| {
                    c.client_ref == request.client_ref
                        && c.enabled
                        && c.tool_groups.contains(&ToolGroup::TerminalObserve)
                });
                if !authorized {
                    return Err(HistoryError::TerminalUnavailable);
                }
                match &args.request {
                    HistoryRequest::Snapshot {
                        terminal_ref,
                        buffer,
                        selection,
                        slot,
                        after_revision,
                    } => {
                        let (record, pane) = workspace
                            .public_mcp_terminal_pane(&request.client_ref, terminal_ref, cx)
                            .map_err(|_| HistoryError::TerminalUnavailable)?;
                        let record = record.session_id;
                        let session = pane.read(cx).shared_session();
                        let target = HistoryTarget {
                            terminal: terminal_ref.clone(),
                            session: record.0.to_string(),
                        };
                        let reservation = store.lock().reserve(
                            &request.client_ref,
                            *slot,
                            target,
                            *buffer,
                            *selection,
                            after_revision.as_deref(),
                            Instant::now(),
                        )?;
                        Ok(Some((session, reservation, record)))
                    }
                    _ => Ok(None),
                }
            });
            let prepared = match prepared {
                Ok(Ok(p)) => p,
                Ok(Err(error)) => {
                    request.finish(error.envelope());
                    return;
                }
                Err(_) => return,
            };
            if let Some((session, reservation, session_id)) = prepared {
                let (alternate, screen) = match &args.request {
                    HistoryRequest::Snapshot {
                        buffer, selection, ..
                    } => (
                        *buffer == HistoryBuffer::Alternate,
                        *selection == HistorySelection::Screen {},
                    ),
                    _ => unreachable!(),
                };
                let previous = reservation.unchanged_stamp().map(|s| TerminalHistoryStamp {
                    identity: s.identity,
                    content: s.content,
                    switches: s.switches,
                    cleared: s.cleared,
                    evicted: s.evicted,
                });
                let task = cx.background_executor().spawn(async move {
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
                });
                let (reservation, result) = task.await;
                let fallback_store = store.clone();
                let id = reservation.id_for_cleanup();
                let updated = workspace.update(cx, move |workspace, _| {
                    let live = match &args.request {
                        HistoryRequest::Snapshot { terminal_ref, .. } => workspace
                            .public_mcp
                            .terminals
                            .get(terminal_ref)
                            .is_some_and(|r| {
                                r.client_ref == request.client_ref && r.session_id == session_id
                            }),
                        _ => false,
                    };
                    let authorized = workspace.public_mcp.clients().iter().any(|c| {
                        c.client_ref == request.client_ref
                            && c.enabled
                            && c.tool_groups.contains(&ToolGroup::TerminalObserve)
                    });
                    let mut cache = store.lock();
                    if request.is_cancelled() || !live || !authorized {
                        // Erase any completed freeze before returning its reserved memory.
                        drop(result);
                        cache.cancel(&reservation);
                        request.finish(HistoryError::TerminalUnavailable.envelope());
                        return;
                    }
                    let result = match result {
                        Ok(capture) => cache.publish(reservation, capture, &args, Instant::now()),
                        Err(error) => {
                            cache.cancel(&reservation);
                            Err(match error {
                                TerminalHistoryError::Unsupported => HistoryError::Unsupported,
                                TerminalHistoryError::BufferUnavailable => {
                                    HistoryError::BufferUnavailable
                                }
                                TerminalHistoryError::TooLarge => HistoryError::SnapshotTooLarge,
                                TerminalHistoryError::Cancelled => {
                                    HistoryError::TerminalUnavailable
                                }
                            })
                        }
                    };
                    match result {
                        Ok(value) => finish_serialized(request, value),
                        Err(error) => request.finish(error.envelope()),
                    }
                });
                if updated.is_err() {
                    fallback_store.lock().cancel_id(&id);
                }
            } else {
                let _ = workspace.update(cx, move |workspace, _| {
                    if request.is_cancelled() {
                        return;
                    }
                    let mut cache = store.lock();
                    let result = match &args.request {
                        HistoryRequest::Page { cursor } => cache
                            .target(&request.client_ref, cursor)
                            .and_then(|target| {
                                let live = workspace
                                    .public_mcp
                                    .terminals
                                    .get(&target.terminal)
                                    .is_some_and(|r| {
                                        r.client_ref == request.client_ref
                                            && r.session_id.0.to_string() == target.session
                                    });
                                if !live {
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
                });
            }
        })
        .detach();
    }
}
