use gpui::{
    AnyElement, App, Context, CursorStyle, Decorations, Entity, FocusHandle, Focusable,
    IntoElement, MouseButton, Render, ResizeEdge, Tiling, Window, div, prelude::*, px,
};

use super::*;

const CLIENT_WINDOW_RESIZE_EDGE_SIZE: f32 = 6.0;
const CLIENT_WINDOW_RESIZE_CORNER_SIZE: f32 = 12.0;

fn client_window_resize_enabled(
    is_linux: bool,
    decorations: Decorations,
    maximized: bool,
    fullscreen: bool,
) -> bool {
    is_linux && matches!(decorations, Decorations::Client { .. }) && !maximized && !fullscreen
}

fn client_window_resize_edge_enabled(edge: ResizeEdge, tiling: Tiling) -> bool {
    match edge {
        ResizeEdge::Top => !tiling.top,
        ResizeEdge::TopRight => !tiling.top && !tiling.right,
        ResizeEdge::Right => !tiling.right,
        ResizeEdge::BottomRight => !tiling.bottom && !tiling.right,
        ResizeEdge::Bottom => !tiling.bottom,
        ResizeEdge::BottomLeft => !tiling.bottom && !tiling.left,
        ResizeEdge::Left => !tiling.left,
        ResizeEdge::TopLeft => !tiling.top && !tiling.left,
    }
}

fn client_window_resize_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn render_client_window_resize_handles(window: &Window) -> Option<AnyElement> {
    let decorations = window.window_decorations();
    if !client_window_resize_enabled(
        cfg!(target_os = "linux"),
        decorations,
        window.is_maximized(),
        window.is_fullscreen(),
    ) {
        return None;
    }
    let Decorations::Client { tiling } = decorations else {
        return None;
    };
    let edge_size = px(CLIENT_WINDOW_RESIZE_EDGE_SIZE);
    let corner_size = px(CLIENT_WINDOW_RESIZE_CORNER_SIZE);
    let handle = |id, edge| {
        div()
            .id(id)
            .absolute()
            .occlude()
            .cursor(client_window_resize_cursor(edge))
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.start_window_resize(edge);
                cx.stop_propagation();
            })
    };

    Some(
        div()
            .id("client-window-resize-handles")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .when(
                client_window_resize_edge_enabled(ResizeEdge::Top, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-top", ResizeEdge::Top)
                            .top_0()
                            .left(corner_size)
                            .right(corner_size)
                            .h(edge_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::Right, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-right", ResizeEdge::Right)
                            .top(corner_size)
                            .right_0()
                            .bottom(corner_size)
                            .w(edge_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::Bottom, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-bottom", ResizeEdge::Bottom)
                            .right(corner_size)
                            .bottom_0()
                            .left(corner_size)
                            .h(edge_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::Left, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-left", ResizeEdge::Left)
                            .top(corner_size)
                            .bottom(corner_size)
                            .left_0()
                            .w(edge_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::TopLeft, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-top-left", ResizeEdge::TopLeft)
                            .top_0()
                            .left_0()
                            .size(corner_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::TopRight, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-top-right", ResizeEdge::TopRight)
                            .top_0()
                            .right_0()
                            .size(corner_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::BottomRight, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-bottom-right", ResizeEdge::BottomRight)
                            .right_0()
                            .bottom_0()
                            .size(corner_size),
                    )
                },
            )
            .when(
                client_window_resize_edge_enabled(ResizeEdge::BottomLeft, tiling),
                |handles| {
                    handles.child(
                        handle("client-window-resize-bottom-left", ResizeEdge::BottomLeft)
                            .bottom_0()
                            .left_0()
                            .size(corner_size),
                    )
                },
            )
            .into_any_element(),
    )
}

pub(in crate::workspace) fn render_resizable_window_content(
    content: AnyElement,
    window: &Window,
) -> AnyElement {
    let Some(handles) = render_client_window_resize_handles(window) else {
        return content;
    };
    div()
        .size_full()
        .relative()
        .child(content)
        // Keep native resize hit targets above content-owned overlays.
        .child(handles)
        .into_any_element()
}

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
        let (focus_handle, background_cache_byte_limit) = session.read_with(cx, |session, _cx| {
            (
                session.focus_handle.clone(),
                session.render_policy.image_cache_bytes,
            )
        });
        let native_style = WorkspaceWindowNativeStyle::unapplied();
        let background =
            WorkspaceWindowBackgroundEntity::with_byte_limit(background_cache_byte_limit, cx);
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
        let content = self.session.update(cx, |session, cx| {
            session.render_main_window(&self.background, window, cx)
        });
        render_resizable_window_content(content, window)
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
    pub(in crate::workspace) decode_completion_task: Option<Task<()>>,
}

impl WorkspaceWindowBackgroundEntity {
    pub(in crate::workspace) fn with_byte_limit<Owner>(
        byte_limit: usize,
        cx: &mut Context<Owner>,
    ) -> Entity<Self>
    where
        Owner: 'static,
    {
        cx.new(move |_| {
            let mut cache = BackgroundImageRenderCache::default();
            cache.set_byte_limit(byte_limit);
            Self {
                cache,
                decode_completion_task: None,
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

    #[test]
    fn client_resize_requires_linux_client_decorations_in_windowed_state() {
        let decorations = Decorations::Client {
            tiling: Tiling::default(),
        };

        assert!(client_window_resize_enabled(
            true,
            decorations,
            false,
            false
        ));
        assert!(!client_window_resize_enabled(
            false,
            decorations,
            false,
            false
        ));
        assert!(!client_window_resize_enabled(
            true,
            Decorations::Server,
            false,
            false
        ));
        assert!(!client_window_resize_enabled(
            true,
            decorations,
            true,
            false
        ));
        assert!(!client_window_resize_enabled(
            true,
            decorations,
            false,
            true
        ));
    }

    #[test]
    fn client_resize_omits_tiled_edges_and_their_corners() {
        let tiling = Tiling {
            top: true,
            left: false,
            right: false,
            bottom: false,
        };

        assert!(!client_window_resize_edge_enabled(ResizeEdge::Top, tiling));
        assert!(!client_window_resize_edge_enabled(
            ResizeEdge::TopLeft,
            tiling
        ));
        assert!(client_window_resize_edge_enabled(ResizeEdge::Left, tiling));
        assert!(client_window_resize_edge_enabled(
            ResizeEdge::BottomLeft,
            tiling
        ));
    }

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

    struct WindowBootstrapSession {
        background_cache_byte_limit: usize,
        detached_window_opened: bool,
    }

    struct BackgroundBootstrapWindow {
        _background: Entity<WorkspaceWindowBackgroundEntity>,
    }

    impl Render for BackgroundBootstrapWindow {
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
    fn captured_background_budget_bootstraps_window_during_session_update(cx: &mut TestAppContext) {
        let session = cx.new(|_| WindowBootstrapSession {
            background_cache_byte_limit: 1024,
            detached_window_opened: false,
        });

        session.update(cx, |session, cx| {
            // Opening a window draws it synchronously, so the builder must not
            // read the session Entity that owns this active update.
            let background_cache_byte_limit = session.background_cache_byte_limit;
            cx.open_window(gpui::WindowOptions::default(), move |_window, cx| {
                cx.new(|cx| BackgroundBootstrapWindow {
                    _background: WorkspaceWindowBackgroundEntity::with_byte_limit(
                        background_cache_byte_limit,
                        cx,
                    ),
                })
            })
            .expect("background-only detached window should open");
            session.detached_window_opened = true;
        });

        assert!(session.read_with(cx, |session, _cx| { session.detached_window_opened }));
    }

    #[gpui::test]
    fn session_and_background_notifications_repaint_each_observing_window(cx: &mut TestAppContext) {
        let session = cx.new(|_| NotificationSource);
        let background = cx.new(|_| WorkspaceWindowBackgroundEntity {
            cache: BackgroundImageRenderCache::default(),
            decode_completion_task: None,
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
    fn releasing_window_background_cancels_its_pending_decode_completion(cx: &mut TestAppContext) {
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        let background = cx.new(|cx| WorkspaceWindowBackgroundEntity {
            cache: BackgroundImageRenderCache::default(),
            decode_completion_task: Some(cx.spawn(async move |_, _| {
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
