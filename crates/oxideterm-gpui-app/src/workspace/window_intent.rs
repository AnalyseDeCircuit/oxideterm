// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::TryRecvError,
    },
    time::Instant,
};

use gpui::{Context, EventEmitter, Task, Window};
use oxideterm_ssh_launch::NativeConnectionLaunch;
use tokio::sync::Notify;

use super::{WorkspaceApp, delivery};

/// A window-scoped action emitted by platform integrations.
pub(in crate::workspace) enum WindowIntentAction {
    ShowMainWindow,
    HideMainWindow,
    NewConnection,
    OpenSettings,
    CheckForUpdates,
    Quit,
    OpenNativeConnection(NativeConnectionLaunch),
    OpenExternalConnectionUri(NativeConnectionLaunch),
}

/// Moves a platform action into the root window adapter exactly once.
pub(in crate::workspace) struct WindowIntent {
    action: RefCell<Option<WindowIntentAction>>,
}

impl WindowIntent {
    fn new(action: WindowIntentAction) -> Self {
        Self {
            action: RefCell::new(Some(action)),
        }
    }

    pub(in crate::workspace) fn take(&self) -> Option<WindowIntentAction> {
        self.action.borrow_mut().take()
    }
}

enum DesktopPresenceSourceReceiver {
    Platform(oxideterm_desktop_presence::DesktopPresenceReceiver),
    #[cfg(test)]
    Test(std::sync::mpsc::Receiver<oxideterm_desktop_presence::DesktopPresenceEvent>),
}

struct DesktopPresenceSource {
    receiver: DesktopPresenceSourceReceiver,
    notification: Arc<Notify>,
}

impl DesktopPresenceSource {
    fn platform(receiver: oxideterm_desktop_presence::DesktopPresenceReceiver) -> Self {
        let notification = receiver.notification();
        Self {
            receiver: DesktopPresenceSourceReceiver::Platform(receiver),
            notification,
        }
    }

    #[cfg(test)]
    fn test(
        receiver: std::sync::mpsc::Receiver<oxideterm_desktop_presence::DesktopPresenceEvent>,
        notification: Arc<Notify>,
    ) -> Self {
        Self {
            receiver: DesktopPresenceSourceReceiver::Test(receiver),
            notification,
        }
    }

    fn drain(&self) -> delivery::ChannelDrain<oxideterm_desktop_presence::DesktopPresenceEvent> {
        match &self.receiver {
            DesktopPresenceSourceReceiver::Platform(receiver) => drain_with(|| receiver.try_recv()),
            #[cfg(test)]
            DesktopPresenceSourceReceiver::Test(receiver) => {
                delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET)
            }
        }
    }
}

enum SingleInstanceSourceReceiver {
    Platform(crate::single_instance::SingleInstanceReceiver),
    #[cfg(test)]
    Test(std::sync::mpsc::Receiver<crate::single_instance::SingleInstanceEvent>),
}

struct SingleInstanceSource {
    receiver: SingleInstanceSourceReceiver,
    notification: Arc<Notify>,
}

impl SingleInstanceSource {
    fn platform(receiver: crate::single_instance::SingleInstanceReceiver) -> Self {
        let notification = receiver.notification();
        Self {
            receiver: SingleInstanceSourceReceiver::Platform(receiver),
            notification,
        }
    }

    #[cfg(test)]
    fn test(
        receiver: std::sync::mpsc::Receiver<crate::single_instance::SingleInstanceEvent>,
        notification: Arc<Notify>,
    ) -> Self {
        Self {
            receiver: SingleInstanceSourceReceiver::Test(receiver),
            notification,
        }
    }

    fn drain(&self) -> delivery::ChannelDrain<crate::single_instance::SingleInstanceEvent> {
        match &self.receiver {
            SingleInstanceSourceReceiver::Platform(receiver) => {
                // Release the shared application receiver before emitting any intent.
                let receiver = receiver.lock().expect("single-instance receiver poisoned");
                drain_with(|| receiver.try_recv())
            }
            #[cfg(test)]
            SingleInstanceSourceReceiver::Test(receiver) => {
                delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET)
            }
        }
    }
}

/// Stops one external-source waiter even when its platform sender remains alive.
#[derive(Clone)]
struct WindowIntentWaiterStop {
    stopped: Arc<AtomicBool>,
    notification: Arc<Notify>,
    #[cfg(test)]
    finished: Arc<AtomicBool>,
}

impl Default for WindowIntentWaiterStop {
    fn default() -> Self {
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            notification: Arc::new(Notify::new()),
            #[cfg(test)]
            finished: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl WindowIntentWaiterStop {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    async fn wait(&self) {
        self.notification.notified().await;
    }

    #[cfg(test)]
    fn mark_finished(&self) {
        self.finished.store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

/// Owns platform window-intent receivers and their foreground delivery lifetime.
pub(in crate::workspace) struct WorkspaceWindowIntentEntity {
    desktop_presence: Option<DesktopPresenceSource>,
    single_instance: Option<SingleInstanceSource>,
    #[cfg(test)]
    desktop_presence_waiter_stop: WindowIntentWaiterStop,
    #[cfg(test)]
    single_instance_waiter_stop: WindowIntentWaiterStop,
    _desktop_presence_task: Option<Task<()>>,
    _single_instance_task: Option<Task<()>>,
}

impl EventEmitter<WindowIntent> for WorkspaceWindowIntentEntity {}

impl WorkspaceWindowIntentEntity {
    pub(in crate::workspace) fn new(
        desktop_presence_receiver: Option<oxideterm_desktop_presence::DesktopPresenceReceiver>,
        single_instance_receiver: Option<crate::single_instance::SingleInstanceReceiver>,
        cx: &mut Context<Self>,
    ) -> Self {
        let desktop_presence = desktop_presence_receiver.map(DesktopPresenceSource::platform);
        let single_instance = single_instance_receiver.map(SingleInstanceSource::platform);
        Self::new_with_sources(desktop_presence, single_instance, cx)
    }

    fn new_with_sources(
        desktop_presence: Option<DesktopPresenceSource>,
        single_instance: Option<SingleInstanceSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        let desktop_presence_waiter_stop = WindowIntentWaiterStop::default();
        let single_instance_waiter_stop = WindowIntentWaiterStop::default();
        let desktop_presence_task = desktop_presence.as_ref().map(|source| {
            Self::spawn_desktop_presence_waiter(
                source.notification.clone(),
                desktop_presence_waiter_stop.clone(),
                cx,
            )
        });
        let single_instance_task = single_instance.as_ref().map(|source| {
            Self::spawn_single_instance_waiter(
                source.notification.clone(),
                single_instance_waiter_stop.clone(),
                cx,
            )
        });

        Self {
            desktop_presence,
            single_instance,
            #[cfg(test)]
            desktop_presence_waiter_stop,
            #[cfg(test)]
            single_instance_waiter_stop,
            _desktop_presence_task: desktop_presence_task,
            _single_instance_task: single_instance_task,
        }
    }

    fn spawn_desktop_presence_waiter(
        source_notification: Arc<Notify>,
        waiter_stop: WindowIntentWaiterStop,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let release_stop = waiter_stop.clone();
        cx.on_release(move |_, _| {
            release_stop.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                tokio::select! {
                    _ = source_notification.notified() => {}
                    _ = waiter_stop.wait() => {}
                }
                if waiter_stop.is_stopped() {
                    break;
                }
                let Some(backlog_remaining) = entity
                    .update(cx, |entity, cx| entity.drain_desktop_presence(cx))
                    .unwrap_or(None)
                else {
                    break;
                };
                if backlog_remaining {
                    source_notification.notify_one();
                }
            }
            #[cfg(test)]
            waiter_stop.mark_finished();
        })
    }

    fn spawn_single_instance_waiter(
        source_notification: Arc<Notify>,
        waiter_stop: WindowIntentWaiterStop,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let release_stop = waiter_stop.clone();
        cx.on_release(move |_, _| {
            release_stop.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                tokio::select! {
                    _ = source_notification.notified() => {}
                    _ = waiter_stop.wait() => {}
                }
                if waiter_stop.is_stopped() {
                    break;
                }
                let Some(backlog_remaining) = entity
                    .update(cx, |entity, cx| entity.drain_single_instance(cx))
                    .unwrap_or(None)
                else {
                    break;
                };
                if backlog_remaining {
                    source_notification.notify_one();
                }
            }
            #[cfg(test)]
            waiter_stop.mark_finished();
        })
    }

    fn drain_desktop_presence(&mut self, cx: &mut Context<Self>) -> Option<bool> {
        let Some(source) = self.desktop_presence.as_ref() else {
            return None;
        };
        let batch = source.drain();
        for event in batch.items {
            let action = match event {
                oxideterm_desktop_presence::DesktopPresenceEvent::ShowMainWindow => {
                    WindowIntentAction::ShowMainWindow
                }
                oxideterm_desktop_presence::DesktopPresenceEvent::HideMainWindow => {
                    WindowIntentAction::HideMainWindow
                }
                oxideterm_desktop_presence::DesktopPresenceEvent::NewConnection => {
                    WindowIntentAction::NewConnection
                }
                oxideterm_desktop_presence::DesktopPresenceEvent::OpenSettings => {
                    WindowIntentAction::OpenSettings
                }
                oxideterm_desktop_presence::DesktopPresenceEvent::CheckForUpdates => {
                    WindowIntentAction::CheckForUpdates
                }
                oxideterm_desktop_presence::DesktopPresenceEvent::Quit => WindowIntentAction::Quit,
            };
            cx.emit(WindowIntent::new(action));
        }
        if batch.disconnected {
            self.desktop_presence = None;
            return None;
        }
        Some(batch.outcome.backlog_remaining)
    }

    fn drain_single_instance(&mut self, cx: &mut Context<Self>) -> Option<bool> {
        let Some(source) = self.single_instance.as_ref() else {
            return None;
        };
        let batch = source.drain();
        for event in batch.items {
            let action = match event {
                crate::single_instance::SingleInstanceEvent::ShowMainWindow => {
                    WindowIntentAction::ShowMainWindow
                }
                crate::single_instance::SingleInstanceEvent::OpenNativeConnection(launch) => {
                    WindowIntentAction::OpenNativeConnection(launch)
                }
                crate::single_instance::SingleInstanceEvent::OpenExternalConnectionUri(launch) => {
                    WindowIntentAction::OpenExternalConnectionUri(launch)
                }
            };
            cx.emit(WindowIntent::new(action));
        }
        if batch.disconnected {
            self.single_instance = None;
            return None;
        }
        Some(batch.outcome.backlog_remaining)
    }

    #[cfg(test)]
    fn desktop_presence_connected(&self) -> bool {
        self.desktop_presence.is_some()
    }

    #[cfg(test)]
    fn single_instance_connected(&self) -> bool {
        self.single_instance.is_some()
    }
}

fn drain_with<T>(
    mut receive: impl FnMut() -> Result<T, TryRecvError>,
) -> delivery::ChannelDrain<T> {
    let started_at = Instant::now();
    let mut items = Vec::new();
    let mut source_exhausted = false;
    let mut disconnected = false;
    while delivery::USER_ACTION_DELIVERY_BUDGET.allows_next(items.len(), started_at.elapsed()) {
        match receive() {
            Ok(item) => items.push(item),
            Err(TryRecvError::Empty) => {
                source_exhausted = true;
                break;
            }
            Err(TryRecvError::Disconnected) => {
                source_exhausted = true;
                disconnected = true;
                break;
            }
        }
    }
    delivery::ChannelDrain {
        outcome: delivery::USER_ACTION_DELIVERY_BUDGET.outcome(
            items.len(),
            started_at.elapsed(),
            source_exhausted,
        ),
        items,
        disconnected,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_window_intent(
        &mut self,
        action: WindowIntentAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            WindowIntentAction::ShowMainWindow => {
                oxideterm_desktop_presence::show_main_window();
            }
            WindowIntentAction::HideMainWindow => {
                oxideterm_desktop_presence::hide_main_window();
            }
            WindowIntentAction::NewConnection => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::NewConnection);
            }
            WindowIntentAction::OpenSettings => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::OpenSettings);
            }
            WindowIntentAction::CheckForUpdates => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::OpenSettings);
                self.check_native_update(cx);
            }
            WindowIntentAction::Quit => {
                oxideterm_desktop_presence::request_quit();
                cx.quit();
            }
            WindowIntentAction::OpenNativeConnection(launch) => {
                oxideterm_desktop_presence::show_main_window();
                if let Err(error) = self.open_native_connection_launch(launch, window, cx) {
                    eprintln!("failed to open forwarded connection launch: {error:#}");
                }
                window.activate_window();
            }
            WindowIntentAction::OpenExternalConnectionUri(launch) => {
                if !self
                    .settings_store
                    .settings()
                    .general
                    .external_connection_uris_enabled
                {
                    return;
                }
                oxideterm_desktop_presence::show_main_window();
                if let Err(error) = self.open_native_connection_launch(launch, window, cx) {
                    eprintln!("failed to open external connection URI: {error:#}");
                }
                window.activate_window();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    };

    use gpui::{AppContext, TestAppContext};

    use super::*;

    struct TestSourceSender<T> {
        sender: Sender<T>,
        notification: Arc<Notify>,
    }

    impl<T> TestSourceSender<T> {
        fn send(&self, value: T) {
            self.sender.send(value).expect("window intent source send");
            self.notification.notify_one();
        }
    }

    impl<T> Drop for TestSourceSender<T> {
        fn drop(&mut self) {
            // Platform sources notify once after shutdown so the waiter can observe disconnect.
            self.notification.notify_one();
        }
    }

    fn test_source<T>() -> (
        TestSourceSender<T>,
        std::sync::mpsc::Receiver<T>,
        Arc<Notify>,
    ) {
        let (sender, receiver) = mpsc::channel();
        let notification = Arc::new(Notify::new());
        (
            TestSourceSender {
                sender,
                notification: notification.clone(),
            },
            receiver,
            notification,
        )
    }

    #[gpui::test]
    fn hidden_entity_continues_budgeted_delivery_exactly_once(cx: &mut TestAppContext) {
        let (desktop_sender, desktop_receiver, desktop_notification) = test_source();
        let entity = cx.new(|cx| {
            WorkspaceWindowIntentEntity::new_with_sources(
                Some(DesktopPresenceSource::test(
                    desktop_receiver,
                    desktop_notification,
                )),
                None,
                cx,
            )
        });
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let duplicate_takes = Arc::new(AtomicBool::new(false));
        let observed = delivered.clone();
        let duplicate_observed = duplicate_takes.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, intent: &WindowIntent, _cx| {
                let label = match intent.take().expect("first intent take") {
                    WindowIntentAction::ShowMainWindow => "show",
                    WindowIntentAction::HideMainWindow => "hide",
                    _ => "unexpected",
                };
                observed.lock().expect("delivered intents").push(label);
                if intent.take().is_some() {
                    duplicate_observed.store(true, Ordering::Release);
                }
            })
        });

        // More than two user-action budgets proves continuation without mounting or rendering.
        for index in 0..70 {
            let event = if index % 2 == 0 {
                oxideterm_desktop_presence::DesktopPresenceEvent::ShowMainWindow
            } else {
                oxideterm_desktop_presence::DesktopPresenceEvent::HideMainWindow
            };
            desktop_sender.send(event);
        }
        cx.run_until_parked();

        let delivered = delivered.lock().expect("delivered intents");
        assert_eq!(delivered.len(), 70);
        assert_eq!(delivered.first(), Some(&"show"));
        assert_eq!(delivered.last(), Some(&"hide"));
        assert!(!duplicate_takes.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn disconnected_sources_are_cleared_and_waiters_finish(cx: &mut TestAppContext) {
        let (desktop_sender, desktop_receiver, desktop_notification) = test_source();
        let (single_sender, single_receiver, single_notification) = test_source();
        let entity = cx.new(|cx| {
            WorkspaceWindowIntentEntity::new_with_sources(
                Some(DesktopPresenceSource::test(
                    desktop_receiver,
                    desktop_notification,
                )),
                Some(SingleInstanceSource::test(
                    single_receiver,
                    single_notification,
                )),
                cx,
            )
        });

        drop(desktop_sender);
        drop(single_sender);
        cx.run_until_parked();

        cx.read(|cx| {
            let entity = entity.read(cx);
            assert!(!entity.desktop_presence_connected());
            assert!(!entity.single_instance_connected());
            assert!(entity.desktop_presence_waiter_stop.is_finished());
            assert!(entity.single_instance_waiter_stop.is_finished());
        });
    }

    #[gpui::test]
    fn release_stops_both_waiters_while_sources_remain_live(cx: &mut TestAppContext) {
        let (_desktop_sender, desktop_receiver, desktop_notification) = test_source();
        let (_single_sender, single_receiver, single_notification) = test_source();
        let entity = cx.new(|cx| {
            WorkspaceWindowIntentEntity::new_with_sources(
                Some(DesktopPresenceSource::test(
                    desktop_receiver,
                    desktop_notification,
                )),
                Some(SingleInstanceSource::test(
                    single_receiver,
                    single_notification,
                )),
                cx,
            )
        });
        let (desktop_stop, single_stop) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.desktop_presence_waiter_stop.clone(),
                entity.single_instance_waiter_stop.clone(),
            )
        });

        drop(entity);
        cx.update(|_cx| {});

        assert!(desktop_stop.is_stopped());
        assert!(single_stop.is_stopped());
    }
}
