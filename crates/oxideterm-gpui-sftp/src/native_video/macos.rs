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
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                "AVFoundation video surface must be created on the main thread".to_string()
            })?;
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
