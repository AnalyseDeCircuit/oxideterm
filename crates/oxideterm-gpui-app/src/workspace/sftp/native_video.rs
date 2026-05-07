use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};
use oxideterm_preview::{PlatformVideoSnapshot, PlatformVideoState};

#[derive(Clone, Default)]
pub(super) struct SharedSftpNativeVideoSurface(Rc<RefCell<SftpNativeVideoSurface>>);

impl SharedSftpNativeVideoSurface {
    pub(super) fn sync(
        &self,
        path: &str,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PlatformVideoSnapshot {
        self.0.borrow_mut().sync(path, bounds, window, cx)
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
        self.0.borrow().snapshot()
    }

    pub(super) fn detach(&self) {
        self.0.borrow_mut().detach();
    }
}

pub(super) struct SftpNativeVideoElement {
    path: String,
    surface: SharedSftpNativeVideoSurface,
    child: Option<AnyElement>,
}

pub(super) fn sftp_native_video_element(
    path: String,
    surface: SharedSftpNativeVideoSurface,
    child: impl IntoElement,
) -> SftpNativeVideoElement {
    SftpNativeVideoElement {
        path,
        surface,
        child: Some(child.into_any_element()),
    }
}

impl IntoElement for SftpNativeVideoElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SftpNativeVideoElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("native video child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        // The native player view is an AppKit child surface. Keep it aligned in
        // the same frame as GPUI layout, otherwise scrolled dialogs can leave the
        // video one frame behind the preview body.
        self.surface.sync(&self.path, bounds, window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
    use objc2_app_kit::NSView;
    use objc2_av_foundation::AVPlayer;
    use objc2_av_kit::{AVPlayerView, AVPlayerViewControlsStyle};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::*;

    #[derive(Default)]
    pub(crate) struct SftpNativeVideoSurface {
        path: Option<String>,
        view: Option<Retained<AVPlayerView>>,
        player: Option<Retained<AVPlayer>>,
        snapshot: Option<PlatformVideoSnapshot>,
    }

    impl SftpNativeVideoSurface {
        pub(super) fn sync(
            &mut self,
            path: &str,
            bounds: Bounds<Pixels>,
            window: &mut Window,
            _cx: &mut App,
        ) -> PlatformVideoSnapshot {
            let result = self.sync_inner(path, bounds, window);
            match result {
                Ok(snapshot) => {
                    self.snapshot = Some(snapshot.clone());
                    snapshot
                }
                Err(error) => {
                    self.detach();
                    let snapshot = PlatformVideoSnapshot {
                        state: PlatformVideoState::Error,
                        position: Duration::ZERO,
                        duration: None,
                        backend: "AVFoundation",
                        error: Some(error),
                    };
                    self.snapshot = Some(snapshot.clone());
                    snapshot
                }
            }
        }

        pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
            self.snapshot.clone().unwrap_or(PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: Duration::ZERO,
                duration: None,
                backend: "AVFoundation",
                error: None,
            })
        }

        pub(super) fn detach(&mut self) {
            if let Some(player) = self.player.take() {
                unsafe {
                    player.pause();
                }
            }
            if let Some(view) = self.view.take() {
                view.removeFromSuperview();
            }
            self.path = None;
        }

        fn sync_inner(
            &mut self,
            path: &str,
            bounds: Bounds<Pixels>,
            window: &mut Window,
        ) -> Result<PlatformVideoSnapshot, String> {
            let parent = root_ns_view(window)?;
            let frame = child_frame(parent, bounds);
            if self.path.as_deref() != Some(path) {
                self.detach();
                let mtm = MainThreadMarker::new()
                    .ok_or_else(|| "AVFoundation video surface must be created on the main thread".to_string())?;
                let path_string = NSString::from_str(path);
                let url = NSURL::fileURLWithPath(&path_string);
                let player = unsafe { AVPlayer::playerWithURL(&url, mtm) };
                let view = unsafe { AVPlayerView::initWithFrame(AVPlayerView::alloc(mtm), frame) };
                unsafe {
                    view.setControlsStyle(AVPlayerViewControlsStyle::Inline);
                    view.setPlayer(Some(&player));
                    parent.addSubview(&view);
                }
                self.path = Some(path.to_string());
                self.player = Some(player);
                self.view = Some(view);
            } else if let Some(view) = self.view.as_ref() {
                view.setFrame(frame);
            }

            Ok(PlatformVideoSnapshot {
                state: PlatformVideoState::Ready,
                position: Duration::ZERO,
                duration: None,
                backend: "AVFoundation",
                error: None,
            })
        }
    }

    impl Drop for SftpNativeVideoSurface {
        fn drop(&mut self) {
            self.detach();
        }
    }

    fn root_ns_view(window: &mut Window) -> Result<&NSView, String> {
        let handle = window
            .window_handle()
            .map_err(|_| "window handle is unavailable".to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("window is not an AppKit window".to_string());
        };
        // SAFETY: GPUI's AppKit raw window handle is documented as the root
        // NSView. The AVPlayerView is installed as a child and removed on close.
        Ok(unsafe { handle.ns_view.cast::<NSView>().as_ref() })
    }

    fn child_frame(parent: &NSView, bounds: Bounds<Pixels>) -> NSRect {
        let parent_bounds = parent.bounds();
        let x = bounds.origin.x.to_f64();
        let width = bounds.size.width.to_f64();
        let height = bounds.size.height.to_f64();
        let y_from_top = bounds.origin.y.to_f64();
        let y = parent_bounds.size.height - y_from_top - height;
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    #[derive(Default)]
    pub(crate) struct SftpNativeVideoSurface {
        snapshot: Option<PlatformVideoSnapshot>,
    }

    impl SftpNativeVideoSurface {
        pub(super) fn sync(
            &mut self,
            _path: &str,
            _bounds: Bounds<Pixels>,
            _window: &mut Window,
            _cx: &mut App,
        ) -> PlatformVideoSnapshot {
            let snapshot = PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: Duration::ZERO,
                duration: None,
                backend: platform_backend_name(),
                error: Some("native platform video backend is not linked on this platform yet".to_string()),
            };
            self.snapshot = Some(snapshot.clone());
            snapshot
        }

        #[allow(dead_code)]
        pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
            self.snapshot.clone().unwrap_or(PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: Duration::ZERO,
                duration: None,
                backend: platform_backend_name(),
                error: None,
            })
        }

        pub(super) fn detach(&mut self) {}
    }

    #[cfg(target_os = "windows")]
    fn platform_backend_name() -> &'static str {
        "Media Foundation"
    }

    #[cfg(target_os = "linux")]
    fn platform_backend_name() -> &'static str {
        "GStreamer"
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn platform_backend_name() -> &'static str {
        "platform video"
    }
}

pub(super) use imp::SftpNativeVideoSurface;
