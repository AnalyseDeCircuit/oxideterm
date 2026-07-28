// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const MAX_TAB_HISTORY: usize = 50;
const RECORDING_ELAPSED_TICK_INTERVAL: Duration = Duration::from_millis(530);

/// Owns workspace-wide tab identity, terminal mounts, navigation, and close lifecycle.
pub(in crate::workspace) struct WorkspaceTabHostEntity {
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    panes: HashMap<PaneId, Entity<TerminalPane>>,
    pane_subscriptions: HashMap<PaneId, Subscription>,
    terminal_locations: HashMap<TerminalSessionId, TerminalLocation>,
    navigation_history: Vec<TabId>,
    navigation_index: Option<usize>,
    navigation_replaying: bool,
    navigation_observed_tab: Option<TabId>,
    process_close_check_generation: u64,
    process_close_check_task: Option<gpui::Task<()>>,
    process_close_completion: Option<TabCloseProcessCompletion>,
    close_confirm: Option<TabCloseConfirm>,
    recording_elapsed_pane_id: Option<PaneId>,
    recording_elapsed_generation: u64,
    recording_elapsed_task: Option<gpui::Task<()>>,
}

/// Identifies the single tab and pane currently mounting one terminal session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct TerminalLocation {
    pub(in crate::workspace) tab_id: TabId,
    pub(in crate::workspace) pane_id: PaneId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceTabHostEvent {
    CloseProcessCheckReady,
    RecordingElapsedTick {
        pane_id: PaneId,
    },
    TerminalPaneDelivery {
        pane_id: PaneId,
        session_id: TerminalSessionId,
        window_handle: AnyWindowHandle,
        event: TerminalPaneEvent,
    },
}

pub(in crate::workspace) struct TabCloseProcessProbe {
    pub(in crate::workspace) pane_id: PaneId,
    pub(in crate::workspace) probe: Option<oxideterm_terminal::TerminalProcessProbe>,
    pub(in crate::workspace) cached: oxideterm_terminal::TerminalProcessInfo,
}

pub(in crate::workspace) struct TabCloseProcessCompletion {
    pub(in crate::workspace) request: LocalTerminalCloseCheck,
    pub(in crate::workspace) results: Vec<(PaneId, oxideterm_terminal::TerminalProcessInfo)>,
    pub(in crate::workspace) has_foreground_child: bool,
}

impl WorkspaceTabHostEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            panes: HashMap::new(),
            pane_subscriptions: HashMap::new(),
            terminal_locations: HashMap::new(),
            navigation_history: Vec::new(),
            navigation_index: None,
            navigation_replaying: false,
            navigation_observed_tab: None,
            process_close_check_generation: 0,
            process_close_check_task: None,
            process_close_completion: None,
            close_confirm: None,
            recording_elapsed_pane_id: None,
            recording_elapsed_generation: 0,
            recording_elapsed_task: None,
        }
    }

    pub(in crate::workspace) fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub(in crate::workspace) fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    pub(in crate::workspace) fn alloc_session_id(&mut self) -> TerminalSessionId {
        let id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    pub(in crate::workspace) fn sync_recording_elapsed_tick(
        &mut self,
        pane_id: Option<PaneId>,
        recording: bool,
        cx: &mut Context<Self>,
    ) {
        let pane_id = pane_id.filter(|_| recording);
        if self.recording_elapsed_pane_id == pane_id
            && (pane_id.is_none() || self.recording_elapsed_task.is_some())
        {
            return;
        }
        self.recording_elapsed_pane_id = pane_id;
        self.recording_elapsed_generation = self.recording_elapsed_generation.wrapping_add(1);
        self.recording_elapsed_task = None;
        let Some(pane_id) = pane_id else {
            return;
        };
        let generation = self.recording_elapsed_generation;
        self.recording_elapsed_task = Some(cx.spawn(async move |tab_host, cx| {
            loop {
                Timer::after(RECORDING_ELAPSED_TICK_INTERVAL).await;
                let should_continue = tab_host
                    .update(cx, |tab_host, cx| {
                        if tab_host.recording_elapsed_generation != generation
                            || tab_host.recording_elapsed_pane_id != Some(pane_id)
                        {
                            return false;
                        }
                        cx.emit(WorkspaceTabHostEvent::RecordingElapsedTick { pane_id });
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn bind_terminal_location(
        &mut self,
        session_id: TerminalSessionId,
        location: TerminalLocation,
    ) {
        // A session may be registered repeatedly at the same mount boundary,
        // but moving it requires the previous pane lifecycle to unbind first.
        let previous = self.terminal_locations.insert(session_id, location);
        debug_assert!(
            previous.is_none_or(|previous| previous == location),
            "terminal session was rebound without removing its previous location"
        );
    }

    pub(in crate::workspace) fn register_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: Entity<TerminalPane>,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        // TabHost owns pane delivery and its cancellation together with the
        // registered Entity; the window adapter consumes only typed intents.
        let subscription = cx.subscribe(&pane, move |_tab_host, _pane, event, cx| {
            cx.emit(WorkspaceTabHostEvent::TerminalPaneDelivery {
                pane_id,
                session_id,
                window_handle,
                event: *event,
            });
        });
        self.pane_subscriptions.insert(pane_id, subscription);
        self.panes.insert(pane_id, pane);
    }

    pub(in crate::workspace) fn remove_terminal_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<Entity<TerminalPane>> {
        self.pane_subscriptions.remove(&pane_id);
        self.unbind_terminal_location_for_pane(pane_id);
        self.panes.remove(&pane_id)
    }

    pub(in crate::workspace) fn panes(&self) -> &HashMap<PaneId, Entity<TerminalPane>> {
        &self.panes
    }

    pub(in crate::workspace) fn unbind_terminal_location_for_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<TerminalSessionId> {
        let session_id = self
            .terminal_locations
            .iter()
            .find_map(|(session_id, location)| {
                (location.pane_id == pane_id).then_some(*session_id)
            })?;
        self.terminal_locations.remove(&session_id);
        Some(session_id)
    }

    pub(in crate::workspace) fn terminal_location(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<TerminalLocation> {
        self.terminal_locations.get(&session_id).copied()
    }

    pub(in crate::workspace) fn observe_active_tab(&mut self, active_tab_id: Option<TabId>) {
        if self.navigation_observed_tab == active_tab_id {
            return;
        }
        self.navigation_observed_tab = active_tab_id;

        let Some(tab_id) = active_tab_id else {
            return;
        };
        if self.navigation_replaying {
            self.navigation_replaying = false;
            return;
        }

        if let Some(index) = self.navigation_index {
            self.navigation_history.truncate(index.saturating_add(1));
        }
        if self.navigation_history.last().copied() != Some(tab_id) {
            self.navigation_history.push(tab_id);
        }
        if self.navigation_history.len() > MAX_TAB_HISTORY {
            let overflow = self.navigation_history.len() - MAX_TAB_HISTORY;
            self.navigation_history.drain(0..overflow);
        }
        self.navigation_index = self.navigation_history.len().checked_sub(1);
    }

    pub(in crate::workspace) fn navigate_history(
        &mut self,
        forward: bool,
        existing_tab_ids: &HashSet<TabId>,
    ) -> Option<TabId> {
        self.prune_navigation_history(existing_tab_ids);
        let mut index = self.navigation_index?;

        loop {
            if forward {
                if index + 1 >= self.navigation_history.len() {
                    return None;
                }
                index += 1;
            } else if index == 0 {
                return None;
            } else {
                index -= 1;
            }

            let tab_id = self.navigation_history[index];
            if existing_tab_ids.contains(&tab_id) {
                self.navigation_index = Some(index);
                self.navigation_replaying = true;
                return Some(tab_id);
            }
        }
    }

    fn prune_navigation_history(&mut self, existing_tab_ids: &HashSet<TabId>) {
        let current = self
            .navigation_index
            .and_then(|index| self.navigation_history.get(index).copied());
        self.navigation_history
            .retain(|tab_id| existing_tab_ids.contains(tab_id));
        self.navigation_index = current
            .and_then(|tab_id| {
                self.navigation_history
                    .iter()
                    .position(|candidate| *candidate == tab_id)
            })
            .or_else(|| self.navigation_history.len().checked_sub(1));
    }

    pub(in crate::workspace) fn start_close_process_check(
        &mut self,
        request: LocalTerminalCloseCheck,
        probes: Vec<TabCloseProcessProbe>,
        cx: &mut Context<Self>,
    ) {
        let probe_task = cx.background_executor().spawn(async move {
            // Each probe owns its duplicated PTY descriptor, so no terminal mutex is held while
            // platform process and cwd commands run on the background executor.
            probes
                .into_iter()
                .map(|probe| {
                    let info = probe
                        .probe
                        .map(|probe| probe.collect_foreground_only())
                        .unwrap_or(probe.cached);
                    (probe.pane_id, info)
                })
                .collect::<Vec<_>>()
        });

        self.start_close_process_check_with_future(request, probe_task, cx);
    }

    fn start_close_process_check_with_future(
        &mut self,
        request: LocalTerminalCloseCheck,
        probe_task: impl std::future::Future<
            Output = Vec<(PaneId, oxideterm_terminal::TerminalProcessInfo)>,
        > + 'static,
        cx: &mut Context<Self>,
    ) {
        self.process_close_check_generation = self.process_close_check_generation.wrapping_add(1);
        // A newer user request invalidates a completion that has not reached the window adapter.
        self.process_close_completion = None;
        let generation = self.process_close_check_generation;
        // The Entity is the sole task owner. Replacing this handle cancels the
        // older check, and releasing the Entity cancels the current check.
        self.process_close_check_task = Some(cx.spawn(async move |entity, cx| {
            let results = probe_task.await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.process_close_check_generation != generation {
                    return;
                }
                let has_foreground_child = results
                    .iter()
                    .any(|(_, info)| terminal_process_info_has_foreground_child_process(info));
                entity.process_close_completion = Some(TabCloseProcessCompletion {
                    request,
                    results,
                    has_foreground_child,
                });
                cx.emit(WorkspaceTabHostEvent::CloseProcessCheckReady);
            });
        }));
    }

    pub(in crate::workspace) fn take_close_process_completion(
        &mut self,
    ) -> Option<TabCloseProcessCompletion> {
        self.process_close_completion.take()
    }

    pub(in crate::workspace) fn open_close_confirm(&mut self, confirm: TabCloseConfirm) {
        self.close_confirm = Some(confirm);
    }

    pub(in crate::workspace) fn close_confirm(&self) -> Option<&TabCloseConfirm> {
        self.close_confirm.as_ref()
    }

    pub(in crate::workspace) fn close_confirm_cloned(&self) -> Option<TabCloseConfirm> {
        // The exit animation retains the original while the window executes the accepted action.
        self.close_confirm.clone()
    }

    pub(in crate::workspace) fn clear_close_confirm(&mut self) {
        self.close_confirm = None;
    }
}

fn terminal_process_info_has_foreground_child_process(
    info: &oxideterm_terminal::TerminalProcessInfo,
) -> bool {
    let Some(shell_pid) = info.shell_pid else {
        return false;
    };
    info.foreground_process_group_id
        .is_some_and(|foreground_group| foreground_group != shell_pid)
        || info
            .foreground_pid
            .is_some_and(|foreground_pid| foreground_pid != shell_pid)
}

impl gpui::EventEmitter<WorkspaceTabHostEvent> for WorkspaceTabHostEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, div};

    struct TabHostTestRoot;

    impl Render for TabHostTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct TabHostEventRecorder {
        events: Vec<WorkspaceTabHostEvent>,
        _subscription: Option<Subscription>,
    }

    #[test]
    fn typed_workspace_ids_are_monotonic_and_independent() {
        let mut tab_host = WorkspaceTabHostEntity::new();

        assert_eq!(tab_host.alloc_tab_id(), TabId(1));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(1));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(1));
        assert_eq!(tab_host.alloc_tab_id(), TabId(2));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(2));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(2));
    }

    #[test]
    fn terminal_location_lifecycle_is_owned_by_tab_host() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first_session = TerminalSessionId(1);
        let second_session = TerminalSessionId(2);
        let first_location = TerminalLocation {
            tab_id: TabId(3),
            pane_id: PaneId(4),
        };
        let second_location = TerminalLocation {
            tab_id: TabId(3),
            pane_id: PaneId(5),
        };

        tab_host.bind_terminal_location(first_session, first_location);
        tab_host.bind_terminal_location(second_session, second_location);
        assert_eq!(
            tab_host.terminal_location(first_session),
            Some(first_location)
        );
        assert_eq!(
            tab_host.unbind_terminal_location_for_pane(first_location.pane_id),
            Some(first_session)
        );
        assert!(tab_host.terminal_location(first_session).is_none());
        assert_eq!(
            tab_host.terminal_location(second_session),
            Some(second_location)
        );
    }

    #[gpui::test]
    fn removing_pane_drops_delivery_subscription_and_terminal_location(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    80,
                    24,
                    oxideterm_gpui_terminal::TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("recording pane")
            })
        });
        let window_handle = cx.window_handle();
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(*event);
                },
            ));
        });
        let pane_id = PaneId(4);
        let session_id = TerminalSessionId(5);

        tab_host.update(cx, |tab_host, cx| {
            tab_host.register_terminal_pane(pane_id, session_id, pane.clone(), window_handle, cx);
            tab_host.bind_terminal_location(
                session_id,
                TerminalLocation {
                    tab_id: TabId(3),
                    pane_id,
                },
            );
            assert_eq!(tab_host.panes().len(), 1);
            assert_eq!(tab_host.pane_subscriptions.len(), 1);
        });
        pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::Exited { exit_code: Some(0) });
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.clone()),
            vec![WorkspaceTabHostEvent::TerminalPaneDelivery {
                pane_id,
                session_id,
                window_handle,
                event: TerminalPaneEvent::Exited { exit_code: Some(0) },
            }]
        );

        tab_host.update(cx, |tab_host, _cx| {
            assert!(tab_host.remove_terminal_pane(pane_id).is_some());
            assert!(tab_host.panes().is_empty());
            assert!(tab_host.pane_subscriptions.is_empty());
            assert!(tab_host.terminal_location(session_id).is_none());
        });
        pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::ContextActionRequested);
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.len()),
            1
        );
    }

    #[test]
    fn navigation_replay_does_not_create_a_new_history_branch() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first = TabId(1);
        let second = TabId(2);
        let third = TabId(3);
        let existing = HashSet::from([first, second, third]);

        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(second));
        tab_host.observe_active_tab(Some(third));
        assert_eq!(tab_host.navigate_history(false, &existing), Some(second));
        tab_host.observe_active_tab(Some(second));
        assert_eq!(tab_host.navigate_history(false, &existing), Some(first));
        tab_host.observe_active_tab(Some(first));
        assert_eq!(tab_host.navigate_history(true, &existing), Some(second));
        tab_host.observe_active_tab(Some(second));
        assert_eq!(tab_host.navigate_history(true, &existing), Some(third));
    }

    #[test]
    fn navigation_prunes_closed_tabs_and_new_selection_replaces_forward_history() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first = TabId(1);
        let second = TabId(2);
        let third = TabId(3);
        let replacement = TabId(4);

        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(second));
        tab_host.observe_active_tab(Some(third));
        assert_eq!(
            tab_host.navigate_history(false, &HashSet::from([first, third])),
            Some(first)
        );
        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(replacement));

        let existing = HashSet::from([first, third, replacement]);
        assert_eq!(tab_host.navigate_history(true, &existing), None);
        assert_eq!(tab_host.navigate_history(false, &existing), Some(first));
    }

    #[test]
    fn local_close_warning_detects_foreground_child_process() {
        let shell_only = oxideterm_terminal::TerminalProcessInfo {
            shell_pid: Some(10),
            foreground_pid: Some(10),
            foreground_process_group_id: Some(10),
            ..Default::default()
        };
        assert!(!terminal_process_info_has_foreground_child_process(
            &shell_only
        ));

        let foreground_child = oxideterm_terminal::TerminalProcessInfo {
            shell_pid: Some(10),
            foreground_pid: Some(42),
            foreground_process_group_id: Some(42),
            ..Default::default()
        };
        assert!(terminal_process_info_has_foreground_child_process(
            &foreground_child
        ));
    }

    #[gpui::test]
    fn recording_elapsed_task_follows_only_the_visible_recording_pane(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        tab_host.update(cx, |tab_host, cx| {
            tab_host.sync_recording_elapsed_tick(Some(PaneId(1)), true, cx);
            let first_generation = tab_host.recording_elapsed_generation;
            assert_eq!(tab_host.recording_elapsed_pane_id, Some(PaneId(1)));
            assert!(tab_host.recording_elapsed_task.is_some());

            tab_host.sync_recording_elapsed_tick(Some(PaneId(2)), true, cx);
            assert_ne!(tab_host.recording_elapsed_generation, first_generation);
            assert_eq!(tab_host.recording_elapsed_pane_id, Some(PaneId(2)));
            assert!(tab_host.recording_elapsed_task.is_some());

            tab_host.sync_recording_elapsed_tick(Some(PaneId(2)), false, cx);
            assert_eq!(tab_host.recording_elapsed_pane_id, None);
            assert!(tab_host.recording_elapsed_task.is_none());
        });
    }

    #[gpui::test]
    fn newer_close_process_check_cancels_and_replaces_the_previous_task(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let (first_sender, first_receiver) = tokio::sync::oneshot::channel();
        let (replacement_sender, replacement_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(1) },
                async move {
                    first_receiver.await.expect("first result released");
                    Vec::new()
                },
                cx,
            );
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Batch {
                    tab_ids: vec![TabId(2), TabId(3)],
                },
                async move {
                    replacement_receiver
                        .await
                        .expect("replacement result released");
                    Vec::new()
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert!(
            first_sender.send(()).is_err(),
            "replacing the retained task must cancel the older future"
        );
        replacement_sender
            .send(())
            .expect("current task remains retained");
        cx.run_until_parked();

        let completion = tab_host
            .update(cx, |tab_host, _| tab_host.take_close_process_completion())
            .expect("latest close process completion");
        assert_eq!(
            completion.request,
            LocalTerminalCloseCheck::Batch {
                tab_ids: vec![TabId(2), TabId(3)]
            }
        );
        assert!(completion.results.is_empty());
        assert!(!completion.has_foreground_child);
    }

    #[gpui::test]
    fn entity_release_cancels_close_process_check_without_completion(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let completion_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let completion_count_for_task = completion_count.clone();
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(1) },
                async move {
                    result_receiver.await.expect("test result released");
                    completion_count_for_task.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Vec::new()
                },
                cx,
            );
        });

        drop(tab_host);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(
            result_sender.send(()).is_err(),
            "releasing the Entity must cancel its retained check"
        );
        assert_eq!(
            completion_count.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[gpui::test]
    fn current_close_process_check_completes_and_notifies_exactly_once(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(*event);
                },
            ));
        });
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) },
                async move {
                    result_receiver.await.expect("test result released");
                    Vec::new()
                },
                cx,
            );
        });

        result_sender.send(()).expect("current task retained");
        cx.run_until_parked();

        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.clone()),
            vec![WorkspaceTabHostEvent::CloseProcessCheckReady]
        );
        tab_host.update(cx, |tab_host, _cx| {
            let completion = tab_host
                .take_close_process_completion()
                .expect("current completion");
            assert_eq!(
                completion.request,
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) }
            );
            assert!(tab_host.take_close_process_completion().is_none());
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.len()),
            1
        );
    }

    #[test]
    fn close_confirmation_state_is_opened_and_cleared_by_tab_host() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let confirm = TabCloseConfirm::Other {
            tab_ids: vec![TabId(2), TabId(3)],
        };

        tab_host.open_close_confirm(confirm.clone());
        assert_eq!(tab_host.close_confirm(), Some(&confirm));
        tab_host.clear_close_confirm();
        assert!(tab_host.close_confirm().is_none());
    }
}
