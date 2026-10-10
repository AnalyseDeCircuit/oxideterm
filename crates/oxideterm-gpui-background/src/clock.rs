use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct PlaybackClock {
    position: Duration,
    resumed: Option<Instant>,
}

impl PlaybackClock {
    pub fn position(&self, now: Instant) -> Duration {
        self.position.saturating_add(
            self.resumed
                .map_or(Duration::ZERO, |start| now.saturating_duration_since(start)),
        )
    }

    pub fn set_running(&mut self, running: bool, now: Instant) {
        match (running, self.resumed) {
            (true, None) => self.resumed = Some(now),
            (false, Some(_)) => {
                self.position = self.position(now);
                self.resumed = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_time_does_not_advance_the_media_position() {
        let start = Instant::now();
        let mut clock = PlaybackClock::default();
        clock.set_running(true, start);
        clock.set_running(false, start + Duration::from_millis(120));
        assert_eq!(
            clock.position(start + Duration::from_secs(5)),
            Duration::from_millis(120)
        );
        clock.set_running(true, start + Duration::from_secs(5));
        assert_eq!(
            clock.position(start + Duration::from_millis(5080)),
            Duration::from_millis(200)
        );
    }
}
