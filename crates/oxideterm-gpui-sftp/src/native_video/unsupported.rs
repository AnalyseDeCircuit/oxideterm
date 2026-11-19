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
            error: Some(
                "native platform video backend is not linked on this platform yet".to_string(),
            ),
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
