use crate::PlaybackLimits;
use std::time::{Duration, Instant};

/// Runtime quality belongs to the player; user preferences remain unchanged.
pub(crate) struct AutomaticQuality {
    level: usize,
    average_seconds: f64,
    samples: u32,
    changed: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_load_changes_only_automatic_settings_and_recovers_slowly() {
        let mut quality = AutomaticQuality::default();
        let start = quality.changed;
        let automatic = PlaybackLimits::default();
        let manual = PlaybackLimits {
            max_width: Some(1920),
            max_height: Some(1080),
            max_fps: Some(60),
        };
        quality.observe(
            Duration::from_millis(80),
            Duration::from_millis(40),
            30,
            start,
        );
        assert_eq!(quality.limits(automatic, true).max_width, Some(1280));
        for _ in 0..12 {
            quality.observe(
                Duration::from_millis(80),
                Duration::from_millis(40),
                30,
                start + Duration::from_secs(3),
            );
        }
        let reduced = quality.limits(automatic, true);
        assert_eq!(
            (reduced.max_width, reduced.max_height, reduced.max_fps),
            (Some(960), Some(960), Some(24))
        );
        assert_eq!(quality.limits(manual, true), manual);
        for _ in 0..120 {
            quality.observe(
                Duration::from_millis(1),
                Duration::from_millis(40),
                24,
                start + Duration::from_secs(12),
            );
        }
        let restored = quality.limits(automatic, true);
        assert_eq!(
            (restored.max_width, restored.max_fps),
            (Some(1280), Some(30))
        );
    }
}

impl Default for AutomaticQuality {
    fn default() -> Self {
        Self {
            level: 0,
            average_seconds: 0.0,
            samples: 0,
            changed: Instant::now(),
        }
    }
}

impl AutomaticQuality {
    pub fn limits(&self, manual: PlaybackLimits, software: bool) -> PlaybackLimits {
        let scale = 0.75_f64.powi(self.level as i32);
        // Leave room for both display textures, an upload, and a previous size during transitions.
        let edge = if software { 1280 } else { 1920 };
        let automatic_edge = ((edge as f64 * scale).round() as u32).max(320);
        PlaybackLimits {
            max_width: manual.max_width.or(Some(automatic_edge)),
            max_height: manual.max_height.or(Some(automatic_edge)),
            max_fps: manual.max_fps.or(Some([30, 24, 20, 15, 10][self.level])),
        }
    }

    pub fn observe(&mut self, work: Duration, frame_duration: Duration, fps: u32, now: Instant) {
        if work.is_zero() || frame_duration.is_zero() {
            return;
        }
        let seconds = work.as_secs_f64();
        self.average_seconds = if self.samples == 0 {
            seconds
        } else {
            self.average_seconds * 0.9 + seconds * 0.1
        };
        self.samples = self.samples.saturating_add(1);
        // Leave CPU time for terminal input and rendering; avoid reacting to a single slow frame.
        let budget = frame_duration.as_secs_f64().max(1.0 / fps as f64) * 0.6;
        let elapsed = now.saturating_duration_since(self.changed);
        if self.samples >= 12
            && seconds > budget
            && self.average_seconds > budget
            && elapsed >= Duration::from_secs(2)
            && self.level < 4
        {
            self.level += 1;
            self.changed = now;
            self.samples = 0;
        } else if self.samples >= 60
            && self.average_seconds < budget * 0.4
            && elapsed >= Duration::from_secs(8)
            && self.level > 0
        {
            self.level -= 1;
            self.changed = now;
            self.samples = 0;
        }
    }
}
