// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs::File, path::Path, time::Duration};

use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};

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

pub trait AudioPreviewBackend {
    fn load(&mut self, path: &Path) -> Result<AudioPreviewSnapshot, String>;
    fn command(&mut self, command: AudioPreviewCommand) -> Result<AudioPreviewSnapshot, String>;
    fn snapshot(&self) -> AudioPreviewSnapshot;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryAudioPreviewBackend {
    snapshot: Option<AudioPreviewSnapshot>,
}

#[derive(Default)]
pub struct RodioAudioPreviewBackend {
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    duration: Option<Duration>,
    last_error: Option<String>,
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

impl AudioPreviewBackend for RodioAudioPreviewBackend {
    fn load(&mut self, path: &Path) -> Result<AudioPreviewSnapshot, String> {
        self.stop_current();
        let result = self.load_inner(path);
        if let Err(error) = &result {
            self.last_error = Some(error.clone());
        }
        result.map(|_| self.snapshot())
    }

    fn command(&mut self, command: AudioPreviewCommand) -> Result<AudioPreviewSnapshot, String> {
        if command == AudioPreviewCommand::Stop {
            self.last_error = None;
            self.stop_current();
            return Ok(self.snapshot());
        }
        let Some(sink) = self.sink.as_ref() else {
            return Ok(self.snapshot());
        };
        match command {
            AudioPreviewCommand::PlayPause => {
                if sink.is_paused() {
                    sink.play();
                } else {
                    sink.pause();
                }
            }
            AudioPreviewCommand::Seek(position) => {
                sink.try_seek(position)
                    .map_err(|error| format!("failed to seek audio preview: {error}"))?;
            }
            AudioPreviewCommand::Stop => {}
        }
        Ok(self.snapshot())
    }

    fn snapshot(&self) -> AudioPreviewSnapshot {
        if let Some(error) = &self.last_error {
            return AudioPreviewSnapshot {
                state: AudioPreviewState::Error,
                position: Duration::ZERO,
                duration: self.duration,
                error: Some(error.clone()),
            };
        }
        let Some(sink) = self.sink.as_ref() else {
            return AudioPreviewSnapshot {
                state: AudioPreviewState::Stopped,
                position: Duration::ZERO,
                duration: self.duration,
                error: None,
            };
        };
        let state = if sink.is_paused() {
            AudioPreviewState::Paused
        } else {
            AudioPreviewState::Playing
        };
        AudioPreviewSnapshot {
            state,
            position: sink.get_pos(),
            duration: self.duration,
            error: None,
        }
    }
}

impl RodioAudioPreviewBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn load_inner(&mut self, path: &Path) -> Result<(), String> {
        let mut stream = OutputStreamBuilder::open_default_stream()
            .map_err(|error| format!("failed to open default audio output: {error}"))?;
        stream.log_on_drop(false);
        let sink = Sink::connect_new(stream.mixer());
        let file = File::open(path)
            .map_err(|error| format!("failed to open audio preview file: {error}"))?;
        let source = Decoder::try_from(file)
            .map_err(|error| format!("failed to decode audio preview file: {error}"))?;
        self.duration = source.total_duration();
        sink.append(source);
        // Tauri's <audio controls> loads media paused until the user presses play.
        sink.pause();
        self.stream = Some(stream);
        self.sink = Some(sink);
        self.last_error = None;
        Ok(())
    }

    fn stop_current(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.stream = None;
    }
}

impl Drop for RodioAudioPreviewBackend {
    fn drop(&mut self) {
        self.stop_current();
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
