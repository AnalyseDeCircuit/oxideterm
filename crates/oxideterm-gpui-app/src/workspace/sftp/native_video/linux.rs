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
    failed_path: Option<(String, String)>,
    xlib: Option<xlib::Xlib>,
    display: Option<*mut xlib::Display>,
    child: Option<xlib::Window>,
    playbin: Option<gst::Element>,
    snapshot: Option<PlatformVideoSnapshot>,
    owns_display: bool,
}

impl SftpNativeVideoSurface {
    pub(in crate::workspace::sftp) fn sync(
        &mut self,
        path: &str,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut App,
    ) -> PlatformVideoSnapshot {
        if let Some((failed_path, error)) = &self.failed_path {
            if failed_path == path {
                return self
                    .snapshot
                    .clone()
                    .unwrap_or_else(|| PlatformVideoSnapshot {
                        state: PlatformVideoState::Error,
                        position: Duration::ZERO,
                        duration: None,
                        backend: "GStreamer",
                        error: Some(error.clone()),
                    });
            }
            self.failed_path = None;
        }

        let result = self.sync_inner(path, bounds, window);
        match result {
            Ok(snapshot) => {
                self.failed_path = None;
                self.snapshot = Some(snapshot.clone());
                snapshot
            }
            Err(error) => {
                self.detach();
                self.failed_path = Some((path.to_string(), error.clone()));
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

    pub(in crate::workspace::sftp) fn snapshot(&self) -> PlatformVideoSnapshot {
        self.snapshot.clone().unwrap_or(PlatformVideoSnapshot {
            state: PlatformVideoState::Unavailable,
            position: Duration::ZERO,
            duration: None,
            backend: "GStreamer",
            error: None,
        })
    }

    pub(in crate::workspace::sftp) fn detach(&mut self) {
        if let Some(playbin) = self.playbin.take() {
            let _ = playbin.set_state(gst::State::Null);
        }
        if let (Some(xlib), Some(display)) = (self.xlib.as_ref(), self.display.take()) {
            unsafe {
                if let Some(child) = self.child.take() {
                    (xlib.XDestroyWindow)(display, child);
                    (xlib.XFlush)(display);
                }
                if self.owns_display {
                    (xlib.XCloseDisplay)(display);
                }
            }
        }
        self.child = None;
        self.owns_display = false;
        self.path = None;
        self.failed_path = None;
    }

    fn sync_inner(
        &mut self,
        path: &str,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Result<PlatformVideoSnapshot, String> {
        let rect = child_rect(bounds, window.scale_factor());
        if self.path.as_deref() != Some(path) {
            self.detach();
            gst::init().map_err(|error| format!("failed to initialize GStreamer: {error}"))?;
            let xlib =
                xlib::Xlib::open().map_err(|error| format!("failed to load Xlib: {error}"))?;
            let (display, parent, owns_display) = root_x11_window(window, &xlib)?;

            // Use an RAII guard to roll back resources if any step in playbin
            // creation or state transition fails, preventing leaks of X connections
            // and child windows across frames.
            struct SetupGuard<'a> {
                xlib: &'a xlib::Xlib,
                display: *mut xlib::Display,
                owns_display: bool,
                child: Option<xlib::Window>,
                playbin: Option<gst::Element>,
                committed: bool,
            }

            impl<'a> SetupGuard<'a> {
                fn commit(mut self) {
                    self.committed = true;
                }
            }

            impl<'a> Drop for SetupGuard<'a> {
                fn drop(&mut self) {
                    if !self.committed {
                        if let Some(playbin) = self.playbin.take() {
                            let _ = playbin.set_state(gst::State::Null);
                        }
                        unsafe {
                            if let Some(child) = self.child.take() {
                                (self.xlib.XDestroyWindow)(self.display, child);
                                (self.xlib.XFlush)(self.display);
                            }
                            if self.owns_display {
                                (self.xlib.XCloseDisplay)(self.display);
                            }
                        }
                    }
                }
            }

            let mut guard = SetupGuard {
                xlib: &xlib,
                display,
                owns_display,
                child: None,
                playbin: None,
                committed: false,
            };

            let child = create_child_xwindow(&xlib, display, parent, rect)?;
            guard.child = Some(child);

            let playbin = create_gstreamer_playbin(path, child)?;
            guard.playbin = Some(playbin.clone());

            playbin
                .set_state(gst::State::Playing)
                .map_err(|error| format!("failed to start GStreamer playback: {error:?}"))?;

            guard.commit();

            self.path = Some(path.to_string());
            self.display = Some(display);
            self.child = Some(child);
            self.xlib = Some(xlib);
            self.playbin = Some(playbin);
            self.owns_display = owns_display;
        } else if let (Some(xlib), Some(display), Some(child)) =
            (self.xlib.as_ref(), self.display, self.child)
        {
            move_child_xwindow(xlib, display, child, rect);
        }

        Ok(self
            .drain_bus_snapshot()
            .unwrap_or_else(|| self.current_snapshot(None)))
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

    fn current_snapshot(&self, error: Option<String>) -> PlatformVideoSnapshot {
        PlatformVideoSnapshot {
            state: if error.is_some() {
                PlatformVideoState::Error
            } else {
                self.platform_state()
            },
            position: Duration::ZERO,
            duration: None,
            backend: "GStreamer",
            error,
        }
    }

    fn drain_bus_snapshot(&self) -> Option<PlatformVideoSnapshot> {
        let bus = self.playbin.as_ref()?.bus()?;
        let mut latest = None;
        // GStreamer posts EOS and decode errors asynchronously. Poll the bus
        // from GPUI's video frame tick so the preview state follows the real
        // native pipeline instead of being inferred from the UI shell.
        for message in bus.iter() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    latest = Some(PlatformVideoSnapshot {
                        state: PlatformVideoState::Ended,
                        position: Duration::ZERO,
                        duration: None,
                        backend: "GStreamer",
                        error: None,
                    });
                }
                gst::MessageView::Error(error) => {
                    let mut detail = error.error().to_string();
                    if let Some(debug) = error.debug() {
                        detail.push_str(": ");
                        detail.push_str(debug.as_str());
                    }
                    latest = Some(self.current_snapshot(Some(detail)));
                }
                gst::MessageView::StateChanged(state) => {
                    latest = Some(PlatformVideoSnapshot {
                        state: gstreamer_state(state.current()),
                        position: Duration::ZERO,
                        duration: None,
                        backend: "GStreamer",
                        error: None,
                    });
                }
                _ => {}
            }
        }
        latest
    }
}

impl Drop for SftpNativeVideoSurface {
    fn drop(&mut self) {
        self.detach();
    }
}

fn root_x11_window(
    window: &mut Window,
    xlib: &xlib::Xlib,
) -> Result<(*mut xlib::Display, xlib::Window, bool), String> {
    let window_handle = window
        .window_handle()
        .map_err(|_| "window handle is unavailable".to_string())?;
    let display_handle = window
        .display_handle()
        .map_err(|_| "display handle is unavailable".to_string())?;
    let parent = match window_handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get() as xlib::Window,
        RawWindowHandle::Xlib(handle) => handle.window,
        RawWindowHandle::Wayland(_) => {
            return Err("Linux native video currently requires an X11 GPUI window; Wayland subsurface support is not wired yet".to_string());
        }
        _ => return Err("Linux native video currently requires an X11 window".to_string()),
    };
    let (display, owns_display) = match display_handle.as_raw() {
        RawDisplayHandle::Xlib(handle) => {
            if let Some(display) = handle.display {
                (display.as_ptr() as *mut xlib::Display, false)
            } else {
                let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
                if display.is_null() {
                    return Err("X11 display pointer is unavailable".to_string());
                }
                (display, true)
            }
        }
        RawDisplayHandle::Xcb(_) => {
            let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
            if display.is_null() {
                return Err("X11 display pointer is unavailable".to_string());
            }
            (display, true)
        }
        RawDisplayHandle::Wayland(_) => {
            return Err("Linux native video currently requires an X11 display; Wayland subsurface support is not wired yet".to_string());
        }
        _ => return Err("Linux native video currently requires an X11 display".to_string()),
    };
    Ok((display, parent, owns_display))
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
) -> Result<xlib::Window, String> {
    let (x, y, width, height) = rect;
    unsafe {
        let screen = (xlib.XDefaultScreen)(display);
        let depth = (xlib.XDefaultDepth)(display, screen);
        let visual = (xlib.XDefaultVisual)(display, screen);
        let colormap = (xlib.XDefaultColormap)(display, screen);
        let mut attributes = std::mem::zeroed::<xlib::XSetWindowAttributes>();
        attributes.colormap = colormap;
        attributes.border_pixel = 0;
        let valuemask = xlib::CWBorderPixel | xlib::CWColormap;
        let child = (xlib.XCreateWindow)(
            display,
            parent,
            x,
            y,
            width,
            height,
            0,
            depth,
            xlib::InputOutput as u32,
            visual,
            valuemask,
            &mut attributes,
        );
        if child == 0 {
            return Err("failed to create child X11 window".to_string());
        }
        (xlib.XMapRaised)(display, child);
        (xlib.XFlush)(display);
        Ok(child)
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

fn create_gstreamer_playbin(path: &str, child: xlib::Window) -> Result<gst::Element, String> {
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

fn gstreamer_state(state: gst::State) -> PlatformVideoState {
    match state {
        gst::State::Playing => PlatformVideoState::Playing,
        gst::State::Paused => PlatformVideoState::Paused,
        gst::State::Ready => PlatformVideoState::Ready,
        gst::State::Null => PlatformVideoState::Unavailable,
        _ => PlatformVideoState::Ready,
    }
}
