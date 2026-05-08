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
            MFP_EVENT_TYPE_MEDIAITEM_SET, MFP_EVENT_TYPE_PAUSE, MFP_EVENT_TYPE_PLAY,
            MFP_EVENT_TYPE_PLAYBACK_ENDED, MFP_EVENT_TYPE_STOP, MFP_MEDIAPLAYER_STATE_EMPTY,
            MFP_MEDIAPLAYER_STATE_PAUSED, MFP_MEDIAPLAYER_STATE_PLAYING,
            MFP_MEDIAPLAYER_STATE_STOPPED, MFP_OPTION_NONE, MFPCreateMediaPlayer,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SWP_NOACTIVATE, SWP_NOZORDER, SetFocus, SetWindowPos,
            WINDOW_EX_STYLE, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                uri.push(*byte as char)
            }
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
            || header.eEventType == MFP_EVENT_TYPE_MEDIAITEM_CREATED
            || header.eEventType == MFP_EVENT_TYPE_MEDIAITEM_SET
        {
            media_player_state(header.eState)
        } else if header.eEventType == MFP_EVENT_TYPE_PLAYBACK_ENDED {
            PlatformVideoState::Ended
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
