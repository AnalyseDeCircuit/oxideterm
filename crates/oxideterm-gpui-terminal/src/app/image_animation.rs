use oxideterm_terminal::TerminalImageData;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImagePlayback {
    pub frame: usize,
    pub next: Option<Duration>,
}

pub(super) struct ImageClock {
    source: Arc<TerminalImageData>,
    ends: Vec<(u128, usize)>,
    position: u128,
    loops: u128,
    updated: Instant,
    suspended: bool,
    finished: bool,
    held_frame: Option<usize>,
}

impl ImageClock {
    pub(super) fn new(source: Arc<TerminalImageData>, now: Instant) -> Self {
        let held_frame = (!source.animation.running).then_some(source.animation.current_frame);
        let mut clock = Self {
            source,
            ends: Vec::new(),
            position: 0,
            loops: 0,
            updated: now,
            suspended: false,
            finished: false,
            held_frame,
        };
        clock.rebuild();
        clock.position = clock.frame_start(clock.source.animation.current_frame);
        clock
    }

    fn rebuild(&mut self) {
        self.ends.clear();
        let mut end = 0;
        for (index, frame) in self.source.frames.iter().enumerate() {
            let delay = if frame.gapless {
                0
            } else {
                u128::from(frame.delay_ms_numerator) * 1_000_000
                    / u128::from(frame.delay_ms_denominator.max(1))
            };
            if delay != 0 {
                end += delay;
                self.ends.push((end, index));
            }
        }
    }

    fn frame_start(&self, frame: usize) -> u128 {
        self.ends
            .iter()
            .take_while(|(_, index)| *index < frame)
            .last()
            .map_or(0, |(end, _)| *end)
    }

    fn total(&self) -> u128 {
        self.ends.last().map_or(0, |(end, _)| *end)
    }

    fn advance(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.updated).as_nanos();
        self.updated = now;
        let total = self.total();
        if self.suspended || !self.source.animation.running || self.finished || total == 0 {
            return;
        }
        if self.source.animation.loading {
            self.position = self.position.saturating_add(elapsed).min(total);
        } else {
            let position = self.position.saturating_add(elapsed);
            self.loops = self.loops.saturating_add(position / total);
            if self
                .source
                .animation
                .loop_limit
                .is_some_and(|limit| self.loops >= u128::from(limit))
            {
                self.position = total;
                self.finished = true;
            } else {
                self.position = position % total;
            }
        }
    }

    fn current_frame(&self) -> usize {
        if let Some(frame) = self.held_frame {
            return frame.min(self.source.frames.len().saturating_sub(1));
        }
        let index = self.ends.partition_point(|(end, _)| *end <= self.position);
        self.ends.get(index).or_else(|| self.ends.last()).map_or(
            self.source
                .animation
                .current_frame
                .min(self.source.frames.len().saturating_sub(1)),
            |(_, frame)| *frame,
        )
    }

    pub(super) fn update(&mut self, source: Arc<TerminalImageData>, now: Instant) {
        if Arc::ptr_eq(&source, &self.source) {
            return;
        }
        if source.frames.is_empty() && !self.source.frames.is_empty() {
            let suspended = self.suspended;
            *self = Self::new(source, now);
            self.suspended = suspended;
            return;
        }
        self.advance(now);
        let frame = self.current_frame();
        let offset = self.position.saturating_sub(self.frame_start(frame));
        let waiting = self.source.animation.loading && self.position >= self.total();
        let old_total = self.total();
        let appended = source.frames.len() > self.source.frames.len();
        let serial_changed =
            source.animation.control_serial != self.source.animation.control_serial;
        let was_running = self.source.animation.running;
        let identity = self.source.frames.get(frame).map(|f| &f.rgba);
        let next_frame = identity
            .and_then(|pixels| {
                if source
                    .frames
                    .get(frame)
                    .is_some_and(|f| Arc::ptr_eq(pixels, &f.rgba))
                {
                    Some(frame)
                } else {
                    source
                        .frames
                        .iter()
                        .position(|f| Arc::ptr_eq(pixels, &f.rgba))
                }
            })
            .unwrap_or(frame.min(source.frames.len().saturating_sub(1)));
        self.source = source;
        self.rebuild();
        self.position = if waiting && appended {
            old_total
        } else {
            self.frame_start(next_frame)
                .saturating_add(offset)
                .min(self.total())
        };
        self.held_frame = (!self.source.animation.running).then_some(next_frame);
        if serial_changed {
            if self.source.animation.reset_loops {
                self.loops = 0;
                self.finished = false;
            }
            if let Some(seek) = self.source.animation.seek {
                self.position = self.frame_start(seek);
                self.finished = false;
                self.held_frame = (!self.source.animation.running).then_some(seek);
            }
            if !was_running && self.source.animation.running {
                self.finished = false;
            }
        }
    }

    pub(super) fn suspend(&mut self, now: Instant) {
        self.advance(now);
        self.suspended = true;
    }
    pub(super) fn resume(&mut self, now: Instant) {
        if self.suspended {
            self.updated = now;
            self.suspended = false;
        }
    }

    pub(super) fn sample(&mut self, now: Instant) -> ImagePlayback {
        self.advance(now);
        let frame = self.current_frame();
        let index = self.ends.partition_point(|(end, _)| *end <= self.position);
        let next = if self.suspended
            || self.finished
            || !self.source.animation.running
            || self.ends.len() < 2
            || index == self.ends.len()
        {
            None
        } else {
            Some(Duration::from_nanos(
                (self.ends[index].0 - self.position).min(u128::from(u64::MAX)) as u64,
            ))
        };
        ImagePlayback { frame, next }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_terminal::{
        TerminalImageAnimationState, TerminalImageFrame, TerminalImageId, TerminalImageProtocol,
    };

    #[test]
    fn loading_appends_gapless_frames_and_manual_seeks_use_the_requested_frame() {
        let now = Instant::now();
        let mut image = Arc::new(TerminalImageData {
            memory: None,
            id: TerminalImageId(1),
            protocol: TerminalImageProtocol::Kitty,
            version: 1,
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255].into(),
            frames: [0, 100, 100]
                .into_iter()
                .map(|delay| TerminalImageFrame {
                    memory: None,
                    rgba: vec![delay as u8, 0, 0, 255].into(),
                    delay_ms_numerator: delay,
                    delay_ms_denominator: 1,
                    gapless: delay == 0,
                })
                .collect(),
            animation: TerminalImageAnimationState {
                running: true,
                loading: true,
                ..Default::default()
            },
            name: None,
        });
        let mut clock = ImageClock::new(image.clone(), now);
        assert_eq!(
            clock.sample(now),
            ImagePlayback {
                frame: 1,
                next: Some(Duration::from_millis(100))
            }
        );
        assert_eq!(
            clock.sample(now + Duration::from_millis(100)),
            ImagePlayback {
                frame: 2,
                next: Some(Duration::from_millis(100))
            }
        );
        assert_eq!(
            clock.sample(now + Duration::from_millis(500)),
            ImagePlayback {
                frame: 2,
                next: None
            }
        );
        Arc::make_mut(&mut image).frames.push(TerminalImageFrame {
            memory: None,
            rgba: vec![0, 255, 0, 255].into(),
            delay_ms_numerator: 100,
            delay_ms_denominator: 1,
            gapless: false,
        });
        clock.update(image.clone(), now + Duration::from_millis(500));
        assert_eq!(
            clock.sample(now + Duration::from_millis(550)),
            ImagePlayback {
                frame: 3,
                next: Some(Duration::from_millis(50))
            }
        );
        Arc::make_mut(&mut image).frames[1].rgba = vec![0, 0, 255, 255].into();
        clock.update(image.clone(), now + Duration::from_millis(550));
        assert_eq!(
            clock.sample(now + Duration::from_millis(575)),
            ImagePlayback {
                frame: 3,
                next: Some(Duration::from_millis(25))
            }
        );
        let state = &mut Arc::make_mut(&mut image).animation;
        state.running = false;
        state.control_serial += 1;
        state.seek = Some(0);
        clock.update(image, now + Duration::from_millis(575));
        assert_eq!(
            clock.sample(now + Duration::from_secs(2)),
            ImagePlayback {
                frame: 0,
                next: None
            }
        );
    }

    #[test]
    fn clock_preserves_stop_seek_loading_and_hidden_progress() {
        let now = Instant::now();
        let mut image = Arc::new(TerminalImageData {
            memory: None,
            id: TerminalImageId(1),
            protocol: TerminalImageProtocol::Kitty,
            version: 1,
            width: 1,
            height: 1,
            rgba: vec![0; 4].into(),
            frames: [100, 200]
                .into_iter()
                .map(|delay| TerminalImageFrame {
                    memory: None,
                    rgba: vec![0; 4].into(),
                    delay_ms_numerator: delay,
                    delay_ms_denominator: 1,
                    gapless: false,
                })
                .collect(),
            animation: TerminalImageAnimationState {
                running: true,
                ..Default::default()
            },
            name: None,
        });
        let mut clock = ImageClock::new(image.clone(), now);
        assert_eq!(
            clock.sample(now + Duration::from_millis(150)),
            ImagePlayback {
                frame: 1,
                next: Some(Duration::from_millis(150))
            }
        );
        let control = &mut Arc::make_mut(&mut image).animation;
        control.running = false;
        control.control_serial += 1;
        control.reset_loops = true;
        clock.update(image.clone(), now + Duration::from_millis(150));
        assert_eq!(
            clock.sample(now + Duration::from_secs(5)),
            ImagePlayback {
                frame: 1,
                next: None
            }
        );
        let control = &mut Arc::make_mut(&mut image).animation;
        control.running = true;
        control.control_serial += 1;
        control.reset_loops = false;
        clock.update(image.clone(), now + Duration::from_secs(5));
        clock.suspend(now + Duration::from_millis(5050));
        clock.resume(now + Duration::from_secs(20));
        assert_eq!(
            clock.sample(now + Duration::from_secs(20)),
            ImagePlayback {
                frame: 1,
                next: Some(Duration::from_millis(100))
            }
        );
        let control = &mut Arc::make_mut(&mut image).animation;
        control.control_serial += 1;
        control.seek = Some(0);
        control.loop_limit = Some(1);
        clock.update(image.clone(), now + Duration::from_secs(20));
        assert_eq!(
            clock.sample(now + Duration::from_millis(20300)),
            ImagePlayback {
                frame: 1,
                next: None
            }
        );
    }
}
