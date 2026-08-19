// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformVideoSnapshot {
    pub state: PlatformVideoState,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub backend: &'static str,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformVideoState {
    Unavailable,
    Ready,
    Playing,
    Paused,
    Ended,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoPreviewCommand {
    PlayPause,
    Seek(Duration),
    Stop,
}

pub trait PlatformVideoBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn load(&mut self, path: &Path) -> Result<PlatformVideoSnapshot, String>;
    fn command(&mut self, command: VideoPreviewCommand) -> Result<PlatformVideoSnapshot, String>;
    fn snapshot(&self) -> PlatformVideoSnapshot;
}

#[derive(Clone, Debug, Default)]
pub struct UnsupportedPlatformVideoBackend;

impl PlatformVideoBackend for UnsupportedPlatformVideoBackend {
    fn backend_name(&self) -> &'static str {
        platform_backend_name()
    }

    fn is_available(&self) -> bool {
        false
    }

    fn load(&mut self, _path: &Path) -> Result<PlatformVideoSnapshot, String> {
        Ok(self.snapshot())
    }

    fn command(&mut self, _command: VideoPreviewCommand) -> Result<PlatformVideoSnapshot, String> {
        Ok(self.snapshot())
    }

    fn snapshot(&self) -> PlatformVideoSnapshot {
        PlatformVideoSnapshot {
            state: PlatformVideoState::Unavailable,
            position: Duration::ZERO,
            duration: None,
            backend: self.backend_name(),
            error: Some("native platform video backend is not linked in this build".to_string()),
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_backend_name() -> &'static str {
    "AVFoundation"
}

#[cfg(target_os = "windows")]
fn platform_backend_name() -> &'static str {
    "Media Foundation"
}

#[cfg(target_os = "linux")]
fn platform_backend_name() -> &'static str {
    "GStreamer"
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_backend_name() -> &'static str {
    "platform video"
}
