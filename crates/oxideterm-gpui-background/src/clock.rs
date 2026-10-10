use std::time::{Duration, Instant};

const INPUT_RESUME_DURATION: Duration = Duration::from_millis(400);

#[derive(Default)]
pub(crate) struct PlaybackClock {
    position: Duration,
    resumed: Option<Instant>,
    easing_elapsed: Option<Duration>,
}

impl PlaybackClock {
    pub fn position(&self, now: Instant) -> Duration {
        self.position
            .saturating_add(self.resumed.map_or(Duration::ZERO, |start| {
                let elapsed = now.saturating_duration_since(start);
                self.easing_elapsed.map_or(elapsed, |age| {
                    eased_position(age.saturating_add(elapsed)).saturating_sub(eased_position(age))
                })
            }))
    }

    pub fn set_running(&mut self, running: bool, now: Instant) {
        match (running, self.resumed) {
            (true, None) => self.resumed = Some(now),
            (false, Some(start)) => {
                self.position = self.position(now);
                if let Some(age) = self.easing_elapsed {
                    let age = age.saturating_add(now.saturating_duration_since(start));
                    self.easing_elapsed = (age < INPUT_RESUME_DURATION).then_some(age);
                }
                self.resumed = None;
            }
            _ => {}
        }
    }

    pub fn pause_for_input(&mut self, now: Instant) {
        self.set_running(false, now);
        self.easing_elapsed = Some(Duration::ZERO);
    }
}

fn eased_position(elapsed: Duration) -> Duration {
    if elapsed >= INPUT_RESUME_DURATION {
        return elapsed - INPUT_RESUME_DURATION / 2;
    }
    let duration = INPUT_RESUME_DURATION.as_secs_f64();
    let t = elapsed.as_secs_f64() / duration;
    // Integrating smoothstep velocity keeps both position and speed continuous at the join.
    Duration::from_secs_f64(duration * (t * t * t - 0.5 * t * t * t * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_resume_accelerates_without_jumping_or_restarting_on_each_frame() {
        let start = Instant::now();
        let mut clock = PlaybackClock::default();
        clock.set_running(true, start);
        clock.pause_for_input(start + Duration::from_millis(500));
        let resume = start + Duration::from_secs(2);
        assert_eq!(clock.position(resume), Duration::from_millis(500));
        clock.set_running(true, resume);
        // Integrated smoothstep velocity over a 400 ms ramp, followed by normal speed.
        for (millis, expected_micros) in [
            (0, 500_000),
            (100, 505_469),
            (200, 537_500),
            (400, 700_000),
            (500, 800_000),
        ] {
            let now = resume + Duration::from_millis(millis);
            clock.set_running(true, now);
            assert!(
                clock
                    .position(now)
                    .abs_diff(Duration::from_micros(expected_micros))
                    <= Duration::from_micros(1)
            );
        }
    }

    #[test]
    fn repaint_suspension_preserves_ramp_but_new_input_starts_a_fresh_ramp() {
        let start = Instant::now();
        let mut clock = PlaybackClock::default();
        clock.pause_for_input(start);
        clock.set_running(true, start);
        clock.set_running(false, start + Duration::from_millis(100));
        clock.set_running(true, start + Duration::from_millis(110));
        assert_eq!(
            clock.position(start + Duration::from_millis(210)),
            Duration::from_micros(37_500)
        );
        clock.pause_for_input(start + Duration::from_millis(210));
        clock.pause_for_input(start + Duration::from_secs(1));
        assert_eq!(
            clock.position(start + Duration::from_secs(2)),
            Duration::from_micros(37_500)
        );
        clock.set_running(true, start + Duration::from_secs(2));
        assert_eq!(
            clock.position(start + Duration::from_millis(2200)),
            Duration::from_millis(75)
        );
        assert_eq!(
            clock.position(start + Duration::from_millis(2400)),
            Duration::from_micros(237_500)
        );
    }

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
