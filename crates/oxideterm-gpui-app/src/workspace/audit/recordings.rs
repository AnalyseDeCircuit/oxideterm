use super::*;
use oxideterm_audit::{
    AuditOperation, RecordingPage, RecordingState, RecordingSummary, StoredRecordingFrame,
    StoredRecordingFrameKind,
};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Default)]
pub(super) struct AuditRecordings {
    items: Arc<Vec<Arc<RecordingSummary>>>,
    cursors: Vec<Option<(i64, String)>>,
    next_cursor: Option<(i64, String)>,
    session_id: Option<String>,
    player: Option<AuditPlayer>,
    playback_task: Option<gpui::Task<()>>,
}

struct AuditPlayer {
    recording: Arc<RecordingSummary>,
    pane: Entity<TerminalPane>,
    pending: VecDeque<StoredRecordingFrame>,
    cursor: Option<i64>,
    eof: bool,
    playing: bool,
    buffering: bool,
    gaps: bool,
    position_ms: i64,
    seek_to_ms: Option<i64>,
    speed: u32,
    last_tick: Instant,
    generation: u64,
    poll_after: Instant,
    view_operation: Option<AuditOperation>,
    ended_at_ms: Option<i64>,
}

const ACTIVE_RECORDING_POLL_INTERVAL: Duration = Duration::from_millis(500);

fn due_recording_frames(
    pending: &mut VecDeque<StoredRecordingFrame>,
    started_at_ms: i64,
    position_ms: i64,
    seek_to_ms: Option<i64>,
    playing: bool,
) -> Vec<StoredRecordingFrame> {
    if !playing && seek_to_ms.is_none() {
        return Vec::new();
    }
    let until = seek_to_ms.unwrap_or(position_ms);
    let mut frames = Vec::new();
    let mut consumed_bytes = 0;
    while frames.len() < 256
        && pending
            .front()
            .is_some_and(|frame| frame.occurred_at_ms.saturating_sub(started_at_ms) <= until)
    {
        let frame = pending.pop_front().expect("front was present");
        consumed_bytes += match &frame.kind {
            StoredRecordingFrameKind::Output(bytes) => bytes.len(),
            _ => 16,
        };
        frames.push(frame);
        if consumed_bytes >= 64 * 1024 {
            break;
        }
    }
    frames
}

fn page_cursor(page: &RecordingPage, previous: Option<i64>) -> Option<i64> {
    page.next_cursor.or_else(|| {
        page.chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .chain(page.expired_sequences.iter().copied())
            .max()
            .or(previous)
    })
}

fn recording_eof(page: &RecordingPage) -> bool {
    page.next_cursor.is_none() && page.state != RecordingState::InProgress
}

impl WorkspaceApp {
    pub(super) fn audit_view_button(&self, view: AuditView, cx: &Context<Self>) -> AnyElement {
        let (label, icon) = match view {
            AuditView::Events => ("event_log.audit.title", LucideIcon::History),
            AuditView::Sessions => ("event_log.sessions.title", LucideIcon::Terminal),
            AuditView::Recordings => ("event_log.recordings.title", LucideIcon::Play),
        };
        self.workspace_toolbar_action_button(
            self.i18n.t(label),
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                text_color: Some(rgb(if self.audit.view == view {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text
                })),
                ..Default::default()
            },
            cx.listener(move |this, _, _, cx| {
                if this.audit.view != view {
                    this.audit.policy_draft = None;
                    this.clear_ime_selection();
                }
                this.audit.view = view;
                this.audit.open_filter = None;
                this.stop_audit_recording_playback();
                if view == AuditView::Recordings {
                    this.audit.recordings.session_id = None;
                }
                this.refresh_audit(true, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn stop_audit_recording_playback(&mut self) {
        self.audit.recordings.playback_task = None;
        self.audit.recordings.player = None;
    }

    pub(in crate::workspace) fn hide_audit_page(&mut self) {
        if self.audit.recordings.playback_task.is_none() && self.audit.task.is_none() {
            return;
        }
        self.stop_audit_recording_playback();
        self.audit.task = None;
        self.audit.generation += 1;
        self.audit.loading = false;
    }

    pub(super) fn open_session_recordings(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.stop_audit_recording_playback();
        self.audit.recordings.session_id = Some(session_id);
        self.audit.view = AuditView::Recordings;
        self.audit.policy_draft = None;
        self.clear_ime_selection();
        self.refresh_audit_recordings(true, cx);
    }

    pub(super) fn refresh_audit_recordings(&mut self, reset: bool, cx: &mut Context<Self>) {
        if reset || self.audit.recordings.cursors.is_empty() {
            self.audit.recordings.cursors = vec![None];
        }
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        let before = self.audit.recordings.cursors.last().cloned().flatten();
        let session_id = self.audit.recordings.session_id.clone();
        self.audit.generation += 1;
        let generation = self.audit.generation;
        self.audit.loading = true;
        self.audit.error = None;
        self.audit.task = Some(cx.spawn(async move |weak, cx| {
            let policy = client.policy().await;
            let result = match session_id {
                Some(session_id) => {
                    client
                        .list_recordings_for_session(session_id, before, 100)
                        .await
                }
                None => client.list_recordings(before, 100).await,
            };
            let _ = weak.update(cx, |this, cx| {
                if this.audit.generation != generation {
                    return;
                }
                this.audit.loading = false;
                if let Ok(policy) = policy {
                    this.audit.policy = policy;
                }
                match result {
                    Ok(page) => {
                        this.audit.recordings.items =
                            Arc::new(page.recordings.into_iter().map(Arc::new).collect());
                        this.audit.recordings.next_cursor = page.next_cursor;
                        if reset {
                            this.audit.scroll = UniformListScrollHandle::new();
                        }
                    }
                    Err(error) => this.audit.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn toggle_audit_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audit.settings_task.is_some() {
            return;
        }
        let mut policy = self.audit.policy;
        if policy.record_output {
            policy.record_output = false;
            self.save_audit_policy(policy, cx);
            return;
        }
        let prompt = window.prompt(
            gpui::PromptLevel::Warning,
            &self.i18n.t("event_log.recordings.enable"),
            Some(&self.i18n.t("event_log.recordings.consent")),
            &[
                self.i18n.t("common.actions.cancel").as_str(),
                self.i18n.t("event_log.recordings.enable").as_str(),
            ],
            cx,
        );
        self.audit.settings_task = Some(cx.spawn(async move |weak, cx| {
            let enabled = matches!(prompt.await, Ok(1));
            let _ = weak.update(cx, |this, cx| {
                this.audit.settings_task = None;
                if enabled {
                    let mut policy = this.audit.policy;
                    policy.record_output = true;
                    this.save_audit_policy(policy, cx);
                }
                cx.notify();
            });
        }));
    }

    fn open_audit_recording(
        &mut self,
        recording: Arc<RecordingSummary>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        // Audit playback retains a bounded terminal screen and scrollback, never the entire recording.
        preferences.scrollback_lines = preferences.scrollback_lines.min(2000);
        let pane = cx.new(|cx| {
            TerminalPane::new_recording_playback(80, 24, preferences, window, cx)
                .expect("audit playback does not create a PTY")
        });
        let mut context = oxideterm_audit::AuditContext::new(client.clone(), AuditSource::User);
        context.session_id = Some(recording.details.session_id.clone());
        let view_operation = context.operation(
            AuditCategory::Audit,
            "audit_content_view",
            Some(&recording.id),
        );
        self.audit.recordings.player = Some(AuditPlayer {
            ended_at_ms: recording.ended_at_ms,
            gaps: recording.has_expired_content
                || matches!(
                    recording.state,
                    RecordingState::Gaps | RecordingState::Interrupted | RecordingState::Expired
                ),
            recording,
            pane,
            pending: VecDeque::new(),
            cursor: None,
            eof: false,
            playing: true,
            buffering: true,
            position_ms: 0,
            seek_to_ms: None,
            speed: 1,
            last_tick: Instant::now(),
            generation: 0,
            poll_after: Instant::now(),
            view_operation: Some(view_operation),
        });
        self.audit.recordings.playback_task = Some(cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(16)).await;
                let Ok(request) = weak.update(cx, |this, cx| {
                    let Some(player) = this.audit.recordings.player.as_mut() else {
                        return None;
                    };
                    let was_playing = player.playing;
                    let was_seeking = player.seek_to_ms.is_some();
                    let elapsed = player.last_tick.elapsed().as_millis().min(250) as i64;
                    player.last_tick = Instant::now();
                    if player.playing && !player.buffering {
                        player.position_ms += elapsed * i64::from(player.speed);
                    }
                    let frames = due_recording_frames(
                        &mut player.pending,
                        player.recording.started_at_ms,
                        player.position_ms,
                        player.seek_to_ms,
                        player.playing,
                    );
                    for frame in frames {
                        match frame.kind {
                            StoredRecordingFrameKind::Output(bytes) => {
                                player
                                    .pane
                                    .update(cx, |pane, cx| pane.feed_recording_output(&bytes, cx));
                            }
                            StoredRecordingFrameKind::Resize { columns, rows } => {
                                player.pane.update(cx, |pane, cx| {
                                    pane.resize_recording_playback(
                                        columns as usize,
                                        rows as usize,
                                        cx,
                                    )
                                })
                            }
                            StoredRecordingFrameKind::Gap { .. } => player.gaps = true,
                        }
                    }
                    if let Some(target) = player.seek_to_ms {
                        if player.pending.front().is_some_and(|f| {
                            f.occurred_at_ms
                                .saturating_sub(player.recording.started_at_ms)
                                > target
                        }) || (player.eof && player.pending.is_empty())
                        {
                            player.position_ms = target;
                            player.seek_to_ms = None;
                        }
                    }
                    if player.eof && player.pending.is_empty() {
                        player.playing = false;
                        player.buffering = false;
                    }
                    let request = if player.pending.is_empty()
                        && !player.eof
                        && (player.playing
                            || player.seek_to_ms.is_some()
                            || player.view_operation.is_some())
                        && Instant::now() >= player.poll_after
                    {
                        player.buffering = true;
                        Some((
                            player.recording.id.clone(),
                            player.cursor,
                            player.generation,
                        ))
                    } else {
                        None
                    };
                    if was_playing || was_seeking || request.is_some() {
                        cx.notify();
                    }
                    request
                }) else {
                    break;
                };
                if let Some((id, cursor, generation)) = request {
                    let result = client
                        .read_recording_page(id.clone(), cursor, None, 4)
                        .await;
                    let _ = weak.update(cx, |this, cx| {
                        let Some(player) = this.audit.recordings.player.as_mut().filter(|p| {
                            p.recording.id == id && p.cursor == cursor && p.generation == generation
                        }) else {
                            return;
                        };
                        player.last_tick = Instant::now();
                        player.buffering = false;
                        match result {
                            Ok(page) => {
                                player.ended_at_ms = page.ended_at_ms;
                                if let Some(operation) = player.view_operation.take() {
                                    operation.finish(
                                        AuditOutcome::Succeeded,
                                        oxideterm_audit::AuditEvidence::Protocol,
                                        None,
                                        None,
                                    );
                                }
                                player.gaps |= !page.expired_sequences.is_empty();
                                let at_tail = page.next_cursor.is_none();
                                player.eof = recording_eof(&page);
                                player.cursor = page_cursor(&page, player.cursor);
                                player.poll_after = if at_tail {
                                    Instant::now() + ACTIVE_RECORDING_POLL_INTERVAL
                                } else {
                                    Instant::now()
                                };
                                player.pending = page
                                    .chunks
                                    .into_iter()
                                    .flat_map(|chunk| chunk.frames)
                                    .collect();
                            }
                            Err(error) => {
                                if let Some(operation) = player.view_operation.take() {
                                    operation.finish(
                                        AuditOutcome::Failed,
                                        oxideterm_audit::AuditEvidence::Protocol,
                                        None,
                                        None,
                                    );
                                }
                                this.audit.error = Some(error);
                                player.playing = false;
                                player.eof = true;
                                player.gaps = true;
                            }
                        }
                        cx.notify();
                    });
                }
            }
        }));
        cx.notify();
    }

    fn seek_audit_recording(&mut self, target_ms: i64, cx: &mut Context<Self>) {
        if let Some(player) = &mut self.audit.recordings.player {
            player
                .pane
                .update(cx, |pane, cx| pane.reset_recording_playback(80, 24, cx));
            player.generation += 1;
            player.pending.clear();
            player.cursor = None;
            player.eof = false;
            player.buffering = true;
            player.position_ms = 0;
            player.seek_to_ms = Some(target_ms.max(0));
            player.last_tick = Instant::now();
        }
        cx.notify();
    }

    pub(super) fn render_audit_recordings(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let state = &self.audit.recordings;
        let body = if let Some(player) = &state.player {
            let duration = player
                .ended_at_ms
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
                .saturating_sub(player.recording.started_at_ms)
                .max(1);
            let seek_width = Arc::new(Mutex::new(None::<Bounds<Pixels>>));
            let bounds = seek_width.clone();
            let controls = div()
                .flex_none()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .child(self.audit_icon_button(
                    if player.playing {
                        LucideIcon::Pause
                    } else {
                        LucideIcon::Play
                    },
                    if player.playing {
                        "event_log.recordings.pause"
                    } else {
                        "event_log.recordings.play"
                    },
                    false,
                    |this, _, _, cx| {
                        if let Some(p) = &mut this.audit.recordings.player {
                            p.playing = !p.playing;
                            p.last_tick = Instant::now();
                        }
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.audit_icon_button(
                    LucideIcon::RotateCcw,
                    "event_log.recordings.restart",
                    false,
                    |this, _, _, cx| this.seek_audit_recording(0, cx),
                    cx,
                ))
                .child(div().flex_none().child(format!(
                    "{} / {}",
                    duration_text(player.position_ms),
                    duration_text(duration)
                )))
                .child(
                    div()
                        .id("audit-recording-timeline")
                        .flex_1()
                        .h(px(24.0))
                        .relative()
                        .cursor_pointer()
                        .child(
                            div()
                                .absolute()
                                .top(px(10.0))
                                .w_full()
                                .h(px(3.0))
                                .bg(rgb(theme.border)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(10.0))
                                .w(relative(
                                    (player.position_ms as f32 / duration as f32).clamp(0.0, 1.0),
                                ))
                                .h(px(3.0))
                                .bg(rgb(theme.accent)),
                        )
                        .child(
                            canvas(
                                move |b, _, _| {
                                    *bounds.lock() = Some(b);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                if let Some(b) = *seek_width.lock() {
                                    let ratio = (f32::from(event.position.x - b.left())
                                        / f32::from(b.size.width).max(1.0))
                                    .clamp(0.0, 1.0);
                                    this.seek_audit_recording((ratio * duration as f32) as i64, cx);
                                }
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(self.workspace_toolbar_action_button(
                    format!("{}×", player.speed),
                    None,
                    ToolbarButtonOptions::default(),
                    cx.listener(|this, _, _, cx| {
                        if let Some(p) = &mut this.audit.recordings.player {
                            p.speed = match p.speed {
                                1 => 2,
                                2 => 4,
                                _ => 1,
                            };
                        }
                        cx.notify();
                    }),
                ))
                .child(self.audit_icon_button(
                    LucideIcon::X,
                    "common.actions.close",
                    false,
                    |this, _, _, cx| {
                        this.stop_audit_recording_playback();
                        cx.notify();
                    },
                    cx,
                ));
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(controls)
                .when(player.gaps, |v| {
                    v.child(
                        div()
                            .px_3()
                            .py_1()
                            .text_color(rgb(theme.warning))
                            .child(self.i18n.t("event_log.recordings.gaps")),
                    )
                })
                .when(player.buffering || player.seek_to_ms.is_some(), |v| {
                    v.child(
                        div()
                            .px_3()
                            .py_1()
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("event_log.audit.loading")),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(player.pane.clone()),
                )
                .into_any_element()
        } else if state.items.is_empty() {
            oxideterm_gpui_ui::empty_state(
                &self.tokens,
                Self::render_lucide_icon(LucideIcon::History, 20.0, rgb(theme.accent)),
                self.i18n.t(if self.audit.loading {
                    "event_log.audit.loading"
                } else {
                    "event_log.recordings.empty"
                }),
                None,
                None,
            )
            .into_any_element()
        } else {
            let items = state.items.clone();
            let owner = cx.entity();
            tauri_virtual_uniform_list("audit-recording-list", items.len(), self.audit.scroll.clone(), TauriVirtualListSpec::new(px(AUDIT_ROW_HEIGHT), 4), move |range, _, app| owner.update(app, |this, cx| {
                range.filter_map(|index| items.get(index).map(|recording| {
                    let recording = recording.clone();
                    div().id(("audit-recording", index)).h(px(AUDIT_ROW_HEIGHT)).px_3().flex().items_center().gap_3().border_b_1().border_color(rgb(theme.border)).hover(|v| v.bg(rgb(theme.bg_hover))).cursor_pointer()
                        .child(Self::render_lucide_icon(LucideIcon::Play, 16.0, rgb(theme.text_muted)))
                        .child(div().flex_1().min_w(px(0.0)).flex().flex_col().gap_1()
                            .child(div().truncate().child(recording.details.endpoint.as_ref().map(|s| s.to_string()).unwrap_or_else(|| this.i18n.t("event_log.audit.unknown_target"))))
                            .child(div().truncate().text_color(rgb(theme.text_muted)).text_size(px(this.tokens.metrics.ui_text_xs)).child(format!("{} · {}", audit_time(recording.started_at_ms), this.i18n.t(recording_state_key(recording.state))))))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| { this.open_audit_recording(recording.clone(), window, cx); cx.stop_propagation(); }))
                        .into_any_element()
                })).collect()
            })).into_any_element()
        };
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .text_color(rgb(theme.text))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(
                div()
                    .flex_none()
                    .min_h(px(40.0))
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.audit_view_button(AuditView::Events, cx))
                    .child(self.audit_view_button(AuditView::Sessions, cx))
                    .child(self.audit_view_button(AuditView::Recordings, cx))
                    .child(div().flex_1())
                    .child(self.render_audit_toggle(cx))
                    .child(self.audit_icon_button(
                        LucideIcon::Settings,
                        "event_log.audit.settings",
                        false,
                        |this, _, _, cx| {
                            this.audit.settings_open = !this.audit.settings_open;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::RefreshCw,
                        "event_log.audit.refresh",
                        self.audit.loading,
                        |this, _, _, cx| this.refresh_audit_recordings(true, cx),
                        cx,
                    )),
            )
            .when(self.audit.settings_open, |v| {
                v.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_3()
                                .child(
                                    oxideterm_gpui_ui::checkbox::checkbox(
                                        &self.tokens,
                                        self.i18n.t("event_log.recordings.enable"),
                                        self.audit.policy.record_output,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.toggle_audit_recording(window, cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                                )
                                .child(self.render_audit_policy_inputs(cx)),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("event_log.recordings.scope")),
                        ),
                )
            })
            .when(!self.audit.policy.enabled, |v| {
                v.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(rgb(theme.warning))
                        .child(self.i18n.t("event_log.recordings.audit_disabled")),
                )
            })
            .when(self.audit.error.is_some(), |v| {
                v.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(rgb(theme.error))
                        .child(self.i18n.t("event_log.audit.unavailable")),
                )
            })
            .child(body)
            .when(state.player.is_none(), |v| {
                v.child(
                    div()
                        .flex_none()
                        .h(px(36.0))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_t_1()
                        .border_color(rgb(theme.border))
                        .child(
                            self.i18n
                                .t("event_log.audit.page_summary")
                                .replace("{{page}}", &state.cursors.len().to_string())
                                .replace("{{count}}", &state.items.len().to_string()),
                        )
                        .child(div().flex_1())
                        .child(self.audit_icon_button(
                            LucideIcon::ChevronLeft,
                            "event_log.audit.newer",
                            self.audit.loading || state.cursors.len() <= 1,
                            |this, _, _, cx| {
                                this.audit.recordings.cursors.pop();
                                this.refresh_audit_recordings(false, cx);
                            },
                            cx,
                        ))
                        .child(self.audit_icon_button(
                            LucideIcon::ChevronRight,
                            "event_log.audit.older",
                            self.audit.loading || state.next_cursor.is_none(),
                            |this, _, _, cx| {
                                this.audit
                                    .recordings
                                    .cursors
                                    .push(this.audit.recordings.next_cursor.clone());
                                this.refresh_audit_recordings(false, cx);
                            },
                            cx,
                        )),
                )
            })
            .into_any_element()
    }
}

fn duration_text(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn recording_state_key(state: RecordingState) -> &'static str {
    match state {
        RecordingState::InProgress => "event_log.recordings.active",
        RecordingState::Finished => "event_log.recordings.finished",
        RecordingState::Interrupted => "event_log.recordings.interrupted",
        RecordingState::Gaps => "event_log.recordings.partial",
        RecordingState::Expired => "event_log.recordings.expired",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_audit::{RecordingChunk, RecordingPage};

    fn output(time: i64, value: &str) -> StoredRecordingFrame {
        StoredRecordingFrame {
            occurred_at_ms: time,
            kind: StoredRecordingFrameKind::Output(Zeroizing::new(value.as_bytes().to_vec())),
        }
    }

    #[test]
    fn pause_keeps_due_output_unapplied_but_seek_replays_it() {
        let mut pending = VecDeque::from([
            output(100, "first"),
            output(100, "second"),
            output(101, "later"),
        ]);
        assert!(due_recording_frames(&mut pending, 100, 0, None, false).is_empty());
        assert_eq!(pending.len(), 3);
        let seeking = due_recording_frames(&mut pending, 100, 0, Some(0), false);
        assert_eq!(seeking.len(), 2);
        assert!(
            matches!(&seeking[0].kind, StoredRecordingFrameKind::Output(bytes) if bytes.as_slice() == b"first")
        );
        assert!(
            matches!(&seeking[1].kind, StoredRecordingFrameKind::Output(bytes) if bytes.as_slice() == b"second")
        );
        assert_eq!(pending.front().unwrap().occurred_at_ms, 101);
    }

    #[test]
    fn active_tail_preserves_cursor_and_waits_for_more_chunks() {
        let mut page = RecordingPage {
            chunks: vec![RecordingChunk {
                sequence: 4,
                frames: vec![output(100, "visible")],
            }],
            expired_sequences: vec![5],
            next_cursor: None,
            state: RecordingState::InProgress,
            ended_at_ms: None,
        };
        assert_eq!(page_cursor(&page, Some(3)), Some(5));
        assert!(!recording_eof(&page));
        page.chunks.clear();
        page.expired_sequences.clear();
        assert_eq!(page_cursor(&page, Some(5)), Some(5));
        page.state = RecordingState::Finished;
        assert!(recording_eof(&page));
    }

    #[test]
    fn resize_burst_is_bounded_per_tick() {
        let mut pending = (0..300)
            .map(|_| StoredRecordingFrame {
                occurred_at_ms: 100,
                kind: StoredRecordingFrameKind::Resize {
                    columns: 80,
                    rows: 24,
                },
            })
            .collect();
        assert_eq!(
            due_recording_frames(&mut pending, 100, 0, None, true).len(),
            256
        );
        assert_eq!(pending.len(), 44);
    }
}
