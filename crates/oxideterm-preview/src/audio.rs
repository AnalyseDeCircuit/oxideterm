// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioPreviewSnapshot {
    pub state: AudioPreviewState,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioPreviewState {
    Stopped,
    Playing,
    Paused,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioPreviewCommand {
    PlayPause,
    Seek(Duration),
    Stop,
}

pub trait AudioPreviewBackend: Send + Sync {
    fn load(&mut self, path: &Path) -> Result<AudioPreviewSnapshot, String>;
    fn command(&mut self, command: AudioPreviewCommand) -> Result<AudioPreviewSnapshot, String>;
    fn snapshot(&self) -> AudioPreviewSnapshot;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryAudioPreviewBackend {
    snapshot: Option<AudioPreviewSnapshot>,
}

#[derive(Clone, Debug, Default)]
pub struct UnsupportedAudioPreviewBackend;

impl AudioPreviewBackend for MemoryAudioPreviewBackend {
    fn load(&mut self, _path: &Path) -> Result<AudioPreviewSnapshot, String> {
        let snapshot = AudioPreviewSnapshot {
            state: AudioPreviewState::Paused,
            position: Duration::ZERO,
            duration: None,
            error: None,
        };
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn command(&mut self, command: AudioPreviewCommand) -> Result<AudioPreviewSnapshot, String> {
        let mut snapshot = self.snapshot();
        match command {
            AudioPreviewCommand::PlayPause => {
                snapshot.state = match snapshot.state {
                    AudioPreviewState::Playing => AudioPreviewState::Paused,
                    AudioPreviewState::Paused | AudioPreviewState::Stopped => {
                        AudioPreviewState::Playing
                    }
                    AudioPreviewState::Error => AudioPreviewState::Error,
                };
            }
            AudioPreviewCommand::Seek(position) => snapshot.position = position,
            AudioPreviewCommand::Stop => {
                snapshot.state = AudioPreviewState::Stopped;
                snapshot.position = Duration::ZERO;
            }
        }
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn snapshot(&self) -> AudioPreviewSnapshot {
        self.snapshot.clone().unwrap_or(AudioPreviewSnapshot {
            state: AudioPreviewState::Stopped,
            position: Duration::ZERO,
            duration: None,
            error: None,
        })
    }
}

impl AudioPreviewBackend for UnsupportedAudioPreviewBackend {
    fn load(&mut self, _path: &Path) -> Result<AudioPreviewSnapshot, String> {
        Ok(self.snapshot())
    }

    fn command(&mut self, _command: AudioPreviewCommand) -> Result<AudioPreviewSnapshot, String> {
        Ok(self.snapshot())
    }

    fn snapshot(&self) -> AudioPreviewSnapshot {
        AudioPreviewSnapshot {
            state: AudioPreviewState::Error,
            position: Duration::ZERO,
            duration: None,
            error: Some("native audio output backend is not linked in this build".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn memory_audio_backend_tracks_play_pause_and_seek() {
        let mut backend = MemoryAudioPreviewBackend::default();
        backend.load(Path::new("sound.mp3")).unwrap();
        assert_eq!(backend.snapshot().state, AudioPreviewState::Paused);

        backend.command(AudioPreviewCommand::PlayPause).unwrap();
        assert_eq!(backend.snapshot().state, AudioPreviewState::Playing);

        backend
            .command(AudioPreviewCommand::Seek(Duration::from_secs(12)))
            .unwrap();
        assert_eq!(backend.snapshot().position, Duration::from_secs(12));

        backend.command(AudioPreviewCommand::PlayPause).unwrap();
        assert_eq!(backend.snapshot().state, AudioPreviewState::Paused);
    }

    #[test]
    fn audio_backend_types_are_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MemoryAudioPreviewBackend>();
        assert_send_sync::<UnsupportedAudioPreviewBackend>();
        assert_send_sync::<AudioPreviewSnapshot>();
    }
}
