use gpui::{App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Window};

use super::*;

/// Owns native-window state while retaining the shared workspace session.
pub(crate) struct WorkspaceWindowShell {
    session: Entity<WorkspaceApp>,
    focus_handle: FocusHandle,
    native_style: WorkspaceWindowNativeStyle,
    background: Entity<WorkspaceWindowBackgroundEntity>,
    _session_observation: Subscription,
    _background_observation: Subscription,
    _release_subscription: Subscription,
}

impl WorkspaceWindowShell {
    pub(crate) fn new(
        session: Entity<WorkspaceApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = session.read(cx).focus_handle.clone();
        let native_style = WorkspaceWindowNativeStyle::unapplied();
        let background = WorkspaceWindowBackgroundEntity::for_session(&session, cx);
        let session_observation = observe_window_session(&session, cx);
        let background_observation = observe_window_background(&background, cx);
        let window_handle = window.window_handle();
        let window_registration = session.update(cx, |session, _cx| {
            session.reserve_workspace_window(window_registry::WindowRole::Main)
        });
        let window_registered = session.update(cx, |session, cx| {
            session.commit_workspace_window(window_registration, window_handle, cx)
        });
        debug_assert!(window_registered, "main window registration must commit");
        let session_on_release = session.clone();
        let release_subscription = cx.on_release_in(window, move |_shell, window, cx| {
            session_on_release.update(cx, |session, cx| {
                session.release_workspace_window(
                    window_registration,
                    window.window_handle().window_id(),
                    cx,
                );
            });
        });
        Self {
            session,
            focus_handle,
            native_style,
            background,
            _session_observation: session_observation,
            _background_observation: background_observation,
            _release_subscription: release_subscription,
        }
    }
}

pub(in crate::workspace) fn observe_window_background<Owner>(
    background: &Entity<WorkspaceWindowBackgroundEntity>,
    cx: &mut Context<Owner>,
) -> Subscription
where
    Owner: 'static,
{
    cx.observe(background, |_owner, _background, cx| {
        // A completed decode repaints only shells that own this cache.
        cx.notify();
    })
}

impl Focusable for WorkspaceWindowShell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceWindowShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.native_style.apply(&self.session, window, cx);
        self.session.update(cx, |session, cx| {
            session.render_main_window(&self.background, window, cx)
        })
    }
}

pub(in crate::workspace) fn observe_window_session<Owner, Session>(
    session: &Entity<Session>,
    cx: &mut Context<Owner>,
) -> Subscription
where
    Owner: 'static,
    Session: 'static,
{
    cx.observe(session, |_owner, _session, cx| {
        // Session changes repaint every native shell that currently mounts it.
        cx.notify();
    })
}

/// Owns one native window's image cache and its bounded completion task.
pub(in crate::workspace) struct WorkspaceWindowBackgroundEntity {
    pub(in crate::workspace) cache: BackgroundImageRenderCache,
    pub(in crate::workspace) poll_task: Option<Task<()>>,
}

impl WorkspaceWindowBackgroundEntity {
    pub(in crate::workspace) fn for_session<Owner>(
        session: &Entity<WorkspaceApp>,
        cx: &mut Context<Owner>,
    ) -> Entity<Self>
    where
        Owner: 'static,
    {
        let byte_limit = session.read(cx).render_policy.image_cache_bytes;
        cx.new(move |_| {
            let mut cache = BackgroundImageRenderCache::default();
            cache.set_byte_limit(byte_limit);
            Self {
                cache,
                poll_task: None,
            }
        })
    }
}

/// Tracks values already applied to one native window.
pub(in crate::workspace) struct WorkspaceWindowNativeStyle {
    applied_vibrancy_mode: Option<NativeVibrancyMode>,
    applied_window_opacity: Option<f32>,
}

impl WorkspaceWindowNativeStyle {
    pub(in crate::workspace) fn unapplied() -> Self {
        Self {
            applied_vibrancy_mode: None,
            applied_window_opacity: None,
        }
    }

    pub(in crate::workspace) fn apply<Owner>(
        &mut self,
        session: &Entity<WorkspaceApp>,
        window: &mut Window,
        cx: &mut Context<Owner>,
    ) {
        let (vibrancy_mode, window_opacity) = session.read_with(cx, |session, _cx| {
            (
                effective_vibrancy_mode(session.settings_store.settings(), &session.render_policy),
                normalized_window_opacity(
                    session.settings_store.settings().appearance.window_opacity,
                ),
            )
        });
        if self.applied_window_opacity != Some(window_opacity) {
            let _ = apply_window_opacity(window, window_opacity as f64);
            self.applied_window_opacity = Some(window_opacity);
        }
        if self.applied_vibrancy_mode != Some(vibrancy_mode) {
            let support = apply_window_vibrancy(window, vibrancy_mode);
            session.update(cx, |session, _cx| {
                // The capability is platform-wide UI diagnostics; the applied
                // mode remains exclusively owned by this native window shell.
                session.vibrancy_support = support;
            });
            self.applied_vibrancy_mode = Some(vibrancy_mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use gpui::TestAppContext;

    use super::*;

    struct SessionDropProbe {
        drops: Arc<AtomicUsize>,
    }

    impl Drop for SessionDropProbe {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::AcqRel);
        }
    }

    struct SessionLeaseWindow {
        _session: Entity<SessionDropProbe>,
    }

    impl Render for SessionLeaseWindow {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct NotificationSource;

    struct ObservingWindowRoot {
        render_count: Arc<AtomicUsize>,
        _session_observation: Subscription,
        _background_observation: Subscription,
    }

    impl ObservingWindowRoot {
        fn new(
            session: &Entity<NotificationSource>,
            background: &Entity<WorkspaceWindowBackgroundEntity>,
            render_count: Arc<AtomicUsize>,
            cx: &mut Context<Self>,
        ) -> Self {
            Self {
                render_count,
                _session_observation: observe_window_session(session, cx),
                _background_observation: observe_window_background(background, cx),
            }
        }
    }

    impl Render for ObservingWindowRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            self.render_count.fetch_add(1, Ordering::AcqRel);
            div()
        }
    }

    #[gpui::test]
    fn shared_session_outlives_main_and_one_of_two_detached_windows(cx: &mut TestAppContext) {
        let drops = Arc::new(AtomicUsize::new(0));
        let session = cx.new({
            let drops = drops.clone();
            move |_| SessionDropProbe { drops }
        });
        let main_window = cx.add_window({
            let session = session.clone();
            move |_window, _cx| SessionLeaseWindow { _session: session }
        });
        let first_detached_window = cx.add_window({
            let session = session.clone();
            move |_window, _cx| SessionLeaseWindow { _session: session }
        });
        let second_detached_window = cx.add_window({
            let session = session.clone();
            move |_window, _cx| SessionLeaseWindow { _session: session }
        });
        drop(session);

        main_window
            .update(cx, |_root, window, _cx| window.remove_window())
            .expect("main window release");
        cx.run_until_parked();
        assert_eq!(drops.load(Ordering::Acquire), 0);

        first_detached_window
            .update(cx, |_root, window, _cx| window.remove_window())
            .expect("first detached release");
        cx.run_until_parked();
        assert_eq!(drops.load(Ordering::Acquire), 0);

        second_detached_window
            .update(cx, |_root, window, _cx| window.remove_window())
            .expect("last detached release");
        cx.run_until_parked();
        assert_eq!(drops.load(Ordering::Acquire), 1);
        cx.update(|_| {});
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[gpui::test]
    fn session_and_background_notifications_repaint_each_observing_window(cx: &mut TestAppContext) {
        let session = cx.new(|_| NotificationSource);
        let background = cx.new(|_| WorkspaceWindowBackgroundEntity {
            cache: BackgroundImageRenderCache::default(),
            poll_task: None,
        });
        let render_count = Arc::new(AtomicUsize::new(0));
        let (_, cx) = cx.add_window_view({
            let session = session.clone();
            let background = background.clone();
            let render_count = render_count.clone();
            move |_window, cx| ObservingWindowRoot::new(&session, &background, render_count, cx)
        });
        let initial_render_count = render_count.load(Ordering::Acquire);

        session.update(cx, |_session, cx| cx.notify());
        cx.run_until_parked();
        let after_session_notification = render_count.load(Ordering::Acquire);
        assert!(after_session_notification > initial_render_count);

        background.update(cx, |_background, cx| cx.notify());
        cx.run_until_parked();
        assert!(render_count.load(Ordering::Acquire) > after_session_notification);
    }

    #[gpui::test]
    fn releasing_window_background_cancels_its_pending_poll(cx: &mut TestAppContext) {
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        let background = cx.new(|cx| WorkspaceWindowBackgroundEntity {
            cache: BackgroundImageRenderCache::default(),
            poll_task: Some(cx.spawn(async move |_, _| {
                let _ = release_receiver.await;
            })),
        });
        cx.run_until_parked();

        drop(background);
        cx.update(|_| {});
        cx.run_until_parked();

        assert!(release_sender.send(()).is_err());
    }
}
