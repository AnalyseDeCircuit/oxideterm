use gpui::{Context, EventEmitter, Instant, Task};
use std::time::Duration;

pub const FORWARDS_PORT_SCAN_INTERVAL: Duration = Duration::from_secs(12);
const STATS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const TICK_INTERVAL: Duration = Duration::from_secs(1);

pub struct ForwardsSamplingDue;

#[derive(Default)]
pub struct ForwardsSampler {
    timer: Option<Task<()>>,
    last_stats_refresh: Option<Instant>,
}

impl EventEmitter<ForwardsSamplingDue> for ForwardsSampler {}

impl ForwardsSampler {
    pub fn visible(&self) -> bool {
        self.timer.is_some()
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) -> bool {
        if self.visible() == visible {
            return false;
        }
        self.last_stats_refresh = None;
        if visible {
            let executor = cx.background_executor().clone();
            self.timer = Some(cx.spawn(async move |entity, cx| {
                loop {
                    executor.timer(TICK_INTERVAL).await;
                    if entity
                        .update(cx, |_, cx| cx.emit(ForwardsSamplingDue))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        } else {
            // Stop sampling only. Transport and tunnel tasks belong to the host.
            self.timer.take();
        }
        true
    }

    pub fn stats_due(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last_stats_refresh
            .is_some_and(|last| now.duration_since(last) < STATS_REFRESH_INTERVAL)
        {
            return false;
        }
        self.last_stats_refresh = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn hidden_sampler_cancels_ticks_and_visible_mount_resumes(cx: &mut TestAppContext) {
        let sampler = cx.new(|_| ForwardsSampler::default());
        let ticks = Rc::new(Cell::new(0));
        let observed = ticks.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&sampler, move |_, _: &ForwardsSamplingDue, _| {
                observed.set(observed.get() + 1);
            })
        });
        sampler.update(cx, |sampler, cx| {
            sampler.set_visible(true, cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(TICK_INTERVAL);
        cx.run_until_parked();
        assert_eq!(ticks.get(), 1);

        sampler.update(cx, |sampler, cx| {
            sampler.set_visible(false, cx);
        });
        cx.executor().advance_clock(TICK_INTERVAL * 2);
        cx.run_until_parked();
        assert_eq!(ticks.get(), 1);

        sampler.update(cx, |sampler, cx| {
            sampler.set_visible(true, cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(TICK_INTERVAL);
        cx.run_until_parked();
        assert_eq!(ticks.get(), 2);
    }
}
