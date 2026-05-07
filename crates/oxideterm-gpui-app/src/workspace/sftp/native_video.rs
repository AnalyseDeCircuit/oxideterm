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
                    let should_refresh = self.snapshot.as_ref() != Some(&snapshot);
                    self.snapshot = Some(snapshot.clone());
                    if should_refresh {
                        window.on_next_frame(|window, _cx| window.refresh());
                    }
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
                    view.setNextResponder(Some(parent));
                    parent.addSubview(&view);
                }
                self.path = Some(path.to_string());
                self.player = Some(player);
                self.view = Some(view);
            } else if let Some(view) = self.view.as_ref() {
                view.setFrame(frame);
            }
            keep_gpui_first_responder(parent);

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

    fn keep_gpui_first_responder(parent: &NSView) {
        if let Some(window) = parent.window() {
            // AVPlayerView is a real AppKit child view. Let it receive mouse
            // events for native controls, but keep keyboard focus on GPUI so
            // preview-level shortcuts such as Escape are not eaten by AppKit.
            window.makeFirstResponder(Some(parent));
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::{
        ffi::c_void,
        fmt::Write as _,
        iter::once,
        os::windows::ffi::OsStrExt,
        path::Path,
        sync::{Arc, Mutex},
    };

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::{
        Win32::{
            Foundation::{HWND, RECT},
            Media::MediaFoundation::{
                IMFPMediaPlayer, IMFPMediaPlayerCallback, IMFPMediaPlayerCallback_Impl,
                MFP_EVENT_HEADER, MFP_EVENT_TYPE_ERROR, MFP_EVENT_TYPE_MEDIAITEM_CREATED,
                MFP_EVENT_TYPE_MEDIAITEM_SET, MFP_EVENT_TYPE_PAUSE,
                MFP_EVENT_TYPE_PLAY, MFP_EVENT_TYPE_PLAYBACK_ENDED, MFP_EVENT_TYPE_STOP,
                MFP_OPTION_NONE, MFPCreateMediaPlayer, MFP_MEDIAPLAYER_STATE_EMPTY,
                MFP_MEDIAPLAYER_STATE_PAUSED, MFP_MEDIAPLAYER_STATE_PLAYING,
                MFP_MEDIAPLAYER_STATE_STOPPED,
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, SWP_NOACTIVATE, SWP_NOZORDER, SetFocus,
                SetWindowPos, WINDOW_EX_STYLE, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
                WS_VISIBLE,
            },
        },
        core::{PCWSTR, implement, w},
    };

    use super::*;

    #[derive(Default)]
    pub(crate) struct SftpNativeVideoSurface {
        path: Option<String>,
        hwnd: Option<HWND>,
        player: Option<IMFPMediaPlayer>,
        callback: Option<IMFPMediaPlayerCallback>,
        events: Option<MediaPlayerEventStore>,
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
                        backend: "Media Foundation",
                        error: Some(error),
                    };
                    let should_refresh = self.snapshot.as_ref() != Some(&snapshot);
                    self.snapshot = Some(snapshot.clone());
                    if should_refresh {
                        window.on_next_frame(|window, _cx| window.refresh());
                    }
                    snapshot
                }
            }
        }

        pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
            self.snapshot.clone().unwrap_or(PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: Duration::ZERO,
                duration: None,
                backend: "Media Foundation",
                error: None,
            })
        }

        pub(super) fn detach(&mut self) {
            if let Some(player) = self.player.take() {
                unsafe {
                    let _ = player.Stop();
                    let _ = player.Shutdown();
                }
            }
            self.callback = None;
            self.events = None;
            if let Some(hwnd) = self.hwnd.take() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            self.path = None;
        }

        fn sync_inner(
            &mut self,
            path: &str,
            bounds: Bounds<Pixels>,
            window: &mut Window,
        ) -> Result<PlatformVideoSnapshot, String> {
            let parent = root_hwnd(window)?;
            let rect = child_rect(bounds, window.scale_factor());
            if self.path.as_deref() != Some(path) {
                self.detach();
                let hwnd = create_child_hwnd(parent, rect)?;
                let url = wide_file_uri(path)?;
                let mut player = None;
                let events = MediaPlayerEventStore::default();
                let callback: IMFPMediaPlayerCallback = MediaPlayerCallback {
                    events: events.clone(),
                }
                .into();
                unsafe {
                    MFPCreateMediaPlayer(
                        PCWSTR(url.as_ptr()),
                        true,
                        MFP_OPTION_NONE,
                        &callback,
                        Some(hwnd),
                        Some(&mut player),
                    )
                    .map_err(|error| format!("failed to create Media Foundation player: {error}"))?;
                    SetFocus(parent);
                }
                self.path = Some(path.to_string());
                self.hwnd = Some(hwnd);
                self.player = player;
                self.callback = Some(callback);
                self.events = Some(events);
            } else if let Some(hwnd) = self.hwnd {
                move_child_hwnd(hwnd, rect)?;
                if let Some(player) = self.player.as_ref() {
                    unsafe {
                        let _ = player.UpdateVideo();
                    }
                }
            }

            Ok(PlatformVideoSnapshot {
                state: self.platform_state(),
                position: Duration::ZERO,
                duration: None,
                backend: "Media Foundation",
                error: self
                    .events
                    .as_ref()
                    .and_then(MediaPlayerEventStore::snapshot)
                    .and_then(|snapshot| snapshot.error),
            })
        }

        fn platform_state(&self) -> PlatformVideoState {
            if let Some(snapshot) = self
                .events
                .as_ref()
                .and_then(MediaPlayerEventStore::snapshot)
            {
                return snapshot.state;
            }
            let Some(player) = self.player.as_ref() else {
                return PlatformVideoState::Unavailable;
            };
            let state = unsafe { player.GetState() };
            match state {
                Ok(value) if value == MFP_MEDIAPLAYER_STATE_PLAYING => PlatformVideoState::Playing,
                Ok(value) if value == MFP_MEDIAPLAYER_STATE_PAUSED => PlatformVideoState::Paused,
                Ok(value) if value == MFP_MEDIAPLAYER_STATE_STOPPED => PlatformVideoState::Ready,
                Ok(value) if value == MFP_MEDIAPLAYER_STATE_EMPTY => PlatformVideoState::Unavailable,
                Ok(_) => PlatformVideoState::Ready,
                Err(_) => PlatformVideoState::Error,
            }
        }
    }

    impl Drop for SftpNativeVideoSurface {
        fn drop(&mut self) {
            self.detach();
        }
    }

    fn root_hwnd(window: &mut Window) -> Result<HWND, String> {
        let handle = window
            .window_handle()
            .map_err(|_| "window handle is unavailable".to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("window is not a Win32 window".to_string());
        };
        Ok(HWND(handle.hwnd.get() as *mut c_void))
    }

    fn child_rect(bounds: Bounds<Pixels>, scale_factor: f32) -> RECT {
        let x = bounds.origin.x.to_f64() * scale_factor as f64;
        let y = bounds.origin.y.to_f64() * scale_factor as f64;
        let width = bounds.size.width.to_f64() * scale_factor as f64;
        let height = bounds.size.height.to_f64() * scale_factor as f64;
        RECT {
            left: x.round() as i32,
            top: y.round() as i32,
            right: (x + width).round() as i32,
            bottom: (y + height).round() as i32,
        }
    }

    fn create_child_hwnd(parent: HWND, rect: RECT) -> Result<HWND, String> {
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                rect.left,
                rect.top,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                Some(parent),
                None,
                None,
                None,
            )
            .map_err(|error| format!("failed to create Media Foundation child window: {error}"))
        }
    }

    fn move_child_hwnd(hwnd: HWND, rect: RECT) -> Result<(), String> {
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|error| format!("failed to move Media Foundation child window: {error}"))
        }
    }

    fn wide_file_uri(path: &str) -> Result<Vec<u16>, String> {
        let path = Path::new(path)
            .canonicalize()
            .map_err(|error| format!("failed to resolve video path: {error}"))?;
        let uri = percent_encoded_file_uri(&path);
        Ok(std::ffi::OsStr::new(&uri)
            .encode_wide()
            .chain(once(0))
            .collect())
    }

    fn percent_encoded_file_uri(path: &Path) -> String {
        let mut uri = String::from("file:///");
        let normalized = path.to_string_lossy().replace('\\', "/");
        for byte in normalized.as_bytes() {
            match byte {
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
                | b':' => uri.push(*byte as char),
                _ => {
                    let _ = write!(uri, "%{byte:02X}");
                }
            }
        }
        uri
    }

    #[derive(Clone, Default)]
    struct MediaPlayerEventStore(Arc<Mutex<Option<PlatformVideoSnapshot>>>);

    impl MediaPlayerEventStore {
        fn snapshot(&self) -> Option<PlatformVideoSnapshot> {
            self.0.lock().ok().and_then(|snapshot| snapshot.clone())
        }

        fn record(&self, snapshot: PlatformVideoSnapshot) {
            if let Ok(mut current) = self.0.lock() {
                *current = Some(snapshot);
            }
        }
    }

    #[implement(IMFPMediaPlayerCallback)]
    struct MediaPlayerCallback {
        events: MediaPlayerEventStore,
    }

    #[allow(non_snake_case)]
    impl IMFPMediaPlayerCallback_Impl for MediaPlayerCallback_Impl {
        fn OnMediaPlayerEvent(&self, event_header: *const MFP_EVENT_HEADER) {
            if event_header.is_null() {
                return;
            }
            // MFPlay delivers these events on its own worker threads. Keep the
            // callback tiny and only copy the UI-facing state into the surface
            // owner; GPUI consumes it on the next layout/prepaint pass.
            let header = unsafe { &*event_header };
            let state = if header.eEventType == MFP_EVENT_TYPE_PLAY {
                PlatformVideoState::Playing
            } else if header.eEventType == MFP_EVENT_TYPE_PAUSE {
                PlatformVideoState::Paused
            } else if header.eEventType == MFP_EVENT_TYPE_STOP
                || header.eEventType == MFP_EVENT_TYPE_PLAYBACK_ENDED
                || header.eEventType == MFP_EVENT_TYPE_MEDIAITEM_CREATED
                || header.eEventType == MFP_EVENT_TYPE_MEDIAITEM_SET
            {
                media_player_state(header.eState)
            } else if header.eEventType == MFP_EVENT_TYPE_ERROR || header.hrEvent.is_err() {
                PlatformVideoState::Error
            } else {
                media_player_state(header.eState)
            };
            let error = if state == PlatformVideoState::Error {
                Some(format!(
                    "Media Foundation event {:?} failed: {:?}",
                    header.eEventType, header.hrEvent
                ))
            } else {
                None
            };
            self.events.record(PlatformVideoSnapshot {
                state,
                position: Duration::ZERO,
                duration: None,
                backend: "Media Foundation",
                error,
            });
        }
    }

    fn media_player_state(
        state: windows::Win32::Media::MediaFoundation::MFP_MEDIAPLAYER_STATE,
    ) -> PlatformVideoState {
        if state == MFP_MEDIAPLAYER_STATE_PLAYING {
            PlatformVideoState::Playing
        } else if state == MFP_MEDIAPLAYER_STATE_PAUSED {
            PlatformVideoState::Paused
        } else if state == MFP_MEDIAPLAYER_STATE_STOPPED {
            PlatformVideoState::Ready
        } else if state == MFP_MEDIAPLAYER_STATE_EMPTY {
            PlatformVideoState::Unavailable
        } else {
            PlatformVideoState::Ready
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_video as gst_video;
    use gstreamer_video::prelude::*;
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use x11_dl::xlib;

    use super::*;

    #[derive(Default)]
    pub(crate) struct SftpNativeVideoSurface {
        path: Option<String>,
        xlib: Option<xlib::Xlib>,
        display: Option<*mut xlib::Display>,
        child: Option<xlib::Window>,
        playbin: Option<gst::Element>,
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
                        backend: "GStreamer",
                        error: Some(error),
                    };
                    let should_refresh = self.snapshot.as_ref() != Some(&snapshot);
                    self.snapshot = Some(snapshot.clone());
                    if should_refresh {
                        window.on_next_frame(|window, _cx| window.refresh());
                    }
                    snapshot
                }
            }
        }

        pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
            self.snapshot.clone().unwrap_or(PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: Duration::ZERO,
                duration: None,
                backend: "GStreamer",
                error: None,
            })
        }

        pub(super) fn detach(&mut self) {
            if let Some(playbin) = self.playbin.take() {
                let _ = playbin.set_state(gst::State::Null);
            }
            if let (Some(xlib), Some(display), Some(child)) =
                (self.xlib.as_ref(), self.display, self.child.take())
            {
                unsafe {
                    (xlib.XDestroyWindow)(display, child);
                    (xlib.XFlush)(display);
                }
            }
            self.path = None;
        }

        fn sync_inner(
            &mut self,
            path: &str,
            bounds: Bounds<Pixels>,
            window: &mut Window,
        ) -> Result<PlatformVideoSnapshot, String> {
            let (display, parent) = root_x11_window(window)?;
            let rect = child_rect(bounds, window.scale_factor());
            if self.path.as_deref() != Some(path) {
                self.detach();
                gst::init().map_err(|error| format!("failed to initialize GStreamer: {error}"))?;
                let xlib = xlib::Xlib::open().map_err(|error| format!("failed to load Xlib: {error}"))?;
                let child = create_child_xwindow(&xlib, display, parent, rect);
                let playbin = create_gstreamer_playbin(path, child)?;
                playbin
                    .set_state(gst::State::Playing)
                    .map_err(|error| format!("failed to start GStreamer playback: {error:?}"))?;
                self.path = Some(path.to_string());
                self.display = Some(display);
                self.child = Some(child);
                self.xlib = Some(xlib);
                self.playbin = Some(playbin);
            } else if let (Some(xlib), Some(display), Some(child)) =
                (self.xlib.as_ref(), self.display, self.child)
            {
                move_child_xwindow(xlib, display, child, rect);
            }

            Ok(PlatformVideoSnapshot {
                state: self.platform_state(),
                position: Duration::ZERO,
                duration: None,
                backend: "GStreamer",
                error: None,
            })
        }

        fn platform_state(&self) -> PlatformVideoState {
            let Some(playbin) = self.playbin.as_ref() else {
                return PlatformVideoState::Unavailable;
            };
            match playbin.current_state() {
                gst::State::Playing => PlatformVideoState::Playing,
                gst::State::Paused => PlatformVideoState::Paused,
                gst::State::Ready => PlatformVideoState::Ready,
                gst::State::Null => PlatformVideoState::Unavailable,
                _ => PlatformVideoState::Ready,
            }
        }
    }

    impl Drop for SftpNativeVideoSurface {
        fn drop(&mut self) {
            self.detach();
        }
    }

    fn root_x11_window(window: &mut Window) -> Result<(*mut xlib::Display, xlib::Window), String> {
        let window_handle = window
            .window_handle()
            .map_err(|_| "window handle is unavailable".to_string())?;
        let display_handle = window
            .display_handle()
            .map_err(|_| "display handle is unavailable".to_string())?;
        let RawWindowHandle::Xlib(window_handle) = window_handle.as_raw() else {
            return Err("Linux native video currently requires an X11 GPUI window; Wayland subsurface support is not wired yet".to_string());
        };
        let RawDisplayHandle::Xlib(display_handle) = display_handle.as_raw() else {
            return Err("Linux native video currently requires an X11 display".to_string());
        };
        let display = display_handle
            .display
            .ok_or_else(|| "X11 display pointer is unavailable".to_string())?
            .as_ptr() as *mut xlib::Display;
        Ok((display, window_handle.window))
    }

    fn child_rect(bounds: Bounds<Pixels>, scale_factor: f32) -> (i32, i32, u32, u32) {
        let x = bounds.origin.x.to_f64() * scale_factor as f64;
        let y = bounds.origin.y.to_f64() * scale_factor as f64;
        let width = bounds.size.width.to_f64() * scale_factor as f64;
        let height = bounds.size.height.to_f64() * scale_factor as f64;
        (
            x.round() as i32,
            y.round() as i32,
            width.round().max(1.0) as u32,
            height.round().max(1.0) as u32,
        )
    }

    fn create_child_xwindow(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        parent: xlib::Window,
        rect: (i32, i32, u32, u32),
    ) -> xlib::Window {
        let (x, y, width, height) = rect;
        unsafe {
            let child = (xlib.XCreateSimpleWindow)(display, parent, x, y, width, height, 0, 0, 0);
            (xlib.XMapRaised)(display, child);
            (xlib.XFlush)(display);
            child
        }
    }

    fn move_child_xwindow(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        child: xlib::Window,
        rect: (i32, i32, u32, u32),
    ) {
        let (x, y, width, height) = rect;
        unsafe {
            (xlib.XMoveResizeWindow)(display, child, x, y, width, height);
            (xlib.XFlush)(display);
        }
    }

    fn create_gstreamer_playbin(
        path: &str,
        child: xlib::Window,
    ) -> Result<gst::Element, String> {
        let uri = gst::glib::filename_to_uri(path, None)
            .map_err(|error| format!("failed to build video file URI: {error}"))?;
        let sink = gst::ElementFactory::make("ximagesink")
            .property("force-aspect-ratio", true)
            .build()
            .map_err(|error| format!("failed to create GStreamer ximagesink: {error}"))?;
        let overlay = sink
            .clone()
            .dynamic_cast::<gst_video::VideoOverlay>()
            .map_err(|_| "GStreamer ximagesink does not implement VideoOverlay".to_string())?;
        unsafe {
            overlay.set_window_handle(child as usize);
        }
        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|error| format!("failed to create GStreamer playbin: {error}"))?;
        playbin.set_property("uri", uri.as_str());
        playbin.set_property("video-sink", &sink);
        Ok(playbin)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
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
