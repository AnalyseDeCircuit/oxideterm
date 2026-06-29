use std::{
    io,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use oxideterm_remote_desktop::{RemoteDesktopHelperEvent, write_event_line};

const FRAME_COALESCE_WINDOW: Duration = Duration::from_millis(16);

#[derive(Clone)]
pub(crate) struct SharedEventWriter {
    stdout: Arc<Mutex<io::Stdout>>,
    queue: Arc<(Mutex<EventWriterQueue>, Condvar)>,
}

#[derive(Default)]
struct EventWriterQueue {
    frame: Option<RemoteDesktopHelperEvent>,
}

impl SharedEventWriter {
    pub(crate) fn stdio() -> Self {
        let writer = Self {
            stdout: Arc::new(Mutex::new(io::stdout())),
            queue: Arc::new((Mutex::new(EventWriterQueue::default()), Condvar::new())),
        };
        writer.start_stdout_thread();
        writer
    }

    #[cfg(test)]
    pub(crate) fn inert_for_tests() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(io::stdout())),
            queue: Arc::new((Mutex::new(EventWriterQueue::default()), Condvar::new())),
        }
    }

    pub(crate) fn send(&self, event: RemoteDesktopHelperEvent) -> Result<(), String> {
        let (queue, wake) = &*self.queue;
        let mut queue = queue
            .lock()
            .map_err(|_| "RDP event queue lock is poisoned.".to_string())?;
        if is_frame_event(&event) {
            if let Some(frame) = queue.frame.as_mut() {
                merge_frame_event(frame, event);
            } else {
                queue.frame = Some(event);
            }
            wake.notify_one();
            return Ok(());
        }
        let pending_frame = queue.frame.take();
        drop(queue);

        // Control events are written synchronously so short-lived failures are
        // not lost when the helper exits immediately after reporting them.
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| "RDP stdout writer lock is poisoned.".to_string())?;
        if let Some(frame) = pending_frame {
            write_event_line(&mut *stdout, &frame)
                .map_err(|error| format!("RDP event write failed: {error}"))?;
        }
        write_event_line(&mut *stdout, &event)
            .map_err(|error| format!("RDP event write failed: {error}"))?;
        Ok(())
    }

    fn start_stdout_thread(&self) {
        let queue = self.queue.clone();
        let stdout = self.stdout.clone();
        thread::Builder::new()
            .name("oxideterm-rdp-event-writer".to_string())
            .spawn(move || {
                while let Some(event) = next_frame_for_stdout(&queue) {
                    let Ok(mut stdout) = stdout.lock() else {
                        break;
                    };
                    if write_event_line(&mut *stdout, &event).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to start RDP event writer");
    }
}

fn next_frame_for_stdout(
    queue: &Arc<(Mutex<EventWriterQueue>, Condvar)>,
) -> Option<RemoteDesktopHelperEvent> {
    let (queue_lock, wake) = &**queue;
    let mut queue = queue_lock.lock().ok()?;
    loop {
        if queue.frame.is_none() {
            queue = wake.wait(queue).ok()?;
            continue;
        }

        // Keep only one pending frame and give fast bursts one refresh tick to
        // merge into a smaller stdout write workload.
        let deadline = Instant::now() + FRAME_COALESCE_WINDOW;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next_queue, timeout) = wake.wait_timeout(queue, remaining).ok()?;
            queue = next_queue;
            if timeout.timed_out() {
                break;
            }
        }
        if let Some(frame) = queue.frame.take() {
            return Some(frame);
        }
    }
}

fn is_frame_event(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(
        event,
        RemoteDesktopHelperEvent::Frame { .. } | RemoteDesktopHelperEvent::FrameUpdate { .. }
    )
}

fn merge_frame_event(existing: &mut RemoteDesktopHelperEvent, incoming: RemoteDesktopHelperEvent) {
    match existing {
        RemoteDesktopHelperEvent::Frame { frame } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if !frame.apply_update(&update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate { update };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        RemoteDesktopHelperEvent::FrameUpdate { update } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate {
                update: incoming_update,
            } => {
                if !update.merge(&incoming_update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate {
                        update: incoming_update,
                    };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        slot => {
            *slot = incoming;
        }
    }
}

pub(crate) fn send_event(
    writer: &SharedEventWriter,
    event: RemoteDesktopHelperEvent,
) -> Result<(), String> {
    writer.send(event)
}
