use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use oxideterm_remote_desktop::{RemoteDesktopHelperEvent, write_event_line};

const FRAME_QUIET_COALESCE_WINDOW: Duration = Duration::from_millis(4);
const FRAME_MAX_COALESCE_WINDOW: Duration = Duration::from_millis(16);
const FRAME_RECOVERY_THRESHOLD: usize = 24;

#[derive(Clone)]
pub(crate) struct SharedEventWriter {
    stdout: Arc<Mutex<io::Stdout>>,
    queue: Arc<(Mutex<EventWriterQueue>, Condvar)>,
}

#[derive(Default)]
struct EventWriterQueue {
    frames: VecDeque<RemoteDesktopHelperEvent>,
    needs_frame_recovery: bool,
    frame_recovery_notified: bool,
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
            push_frame_event(&mut queue, event);
            wake.notify_one();
            return Ok(());
        }
        let pending_frames = queue.frames.drain(..).collect::<Vec<_>>();
        drop(queue);

        // Control events are written synchronously so short-lived failures are
        // not lost when the helper exits immediately after reporting them.
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| "RDP stdout writer lock is poisoned.".to_string())?;
        for frame in pending_frames {
            write_event_line(&mut *stdout, &frame)
                .map_err(|error| format!("RDP event write failed: {error}"))?;
        }
        write_event_line(&mut *stdout, &event)
            .map_err(|error| format!("RDP event write failed: {error}"))?;
        Ok(())
    }

    pub(crate) fn take_frame_recovery_request(&self) -> Result<bool, String> {
        let (queue, _) = &*self.queue;
        let mut queue = queue
            .lock()
            .map_err(|_| "RDP event queue lock is poisoned.".to_string())?;
        let requested = queue.needs_frame_recovery && !queue.frame_recovery_notified;
        if requested {
            queue.frame_recovery_notified = true;
        }
        Ok(requested)
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
        if queue.frames.is_empty() {
            queue = wake.wait(queue).ok()?;
            continue;
        }

        // Sparse updates should not wait a whole refresh tick, but a burst can
        // still use the full coalescing window to collapse dirty rectangles.
        let start = Instant::now();
        let max_deadline = start + FRAME_MAX_COALESCE_WINDOW;
        let mut quiet_deadline = start + FRAME_QUIET_COALESCE_WINDOW;
        loop {
            let now = Instant::now();
            if now >= quiet_deadline || now >= max_deadline {
                break;
            }
            let remaining = quiet_deadline
                .min(max_deadline)
                .saturating_duration_since(now);
            let (next_queue, timeout) = wake.wait_timeout(queue, remaining).ok()?;
            queue = next_queue;
            if timeout.timed_out() {
                break;
            }
            quiet_deadline = Instant::now() + FRAME_QUIET_COALESCE_WINDOW;
        }
        if let Some(frame) = queue.frames.pop_front() {
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

fn push_frame_event(queue: &mut EventWriterQueue, event: RemoteDesktopHelperEvent) {
    if matches!(event, RemoteDesktopHelperEvent::Frame { .. }) {
        queue.frames.clear();
        queue.frames.push_back(event);
        queue.needs_frame_recovery = false;
        queue.frame_recovery_notified = false;
        return;
    }

    if queue.needs_frame_recovery {
        return;
    }

    if let Some(existing) = queue.frames.back_mut() {
        if let Err(incoming) = try_merge_frame_event(existing, event) {
            queue.frames.push_back(incoming);
        }
    } else {
        queue.frames.push_back(event);
    }

    if queue.frames.len() > FRAME_RECOVERY_THRESHOLD {
        // Sparse dirty updates are only useful while the downstream UI can
        // consume them in order. Once the writer falls too far behind, discard
        // the stale delta chain and ask the RDP session for a new base frame.
        queue.frames.clear();
        queue.needs_frame_recovery = true;
        queue.frame_recovery_notified = false;
    }
}

fn try_merge_frame_event(
    existing: &mut RemoteDesktopHelperEvent,
    incoming: RemoteDesktopHelperEvent,
) -> Result<(), RemoteDesktopHelperEvent> {
    match existing {
        RemoteDesktopHelperEvent::Frame { frame } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if !frame.apply_update(&update) {
                    return Err(RemoteDesktopHelperEvent::FrameUpdate { update });
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
                    return Err(RemoteDesktopHelperEvent::FrameUpdate {
                        update: incoming_update,
                    });
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
    Ok(())
}

pub(crate) fn send_event(
    writer: &SharedEventWriter,
    event: RemoteDesktopHelperEvent,
) -> Result<(), String> {
    writer.send(event)
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::{
        RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate, RemoteDesktopRect,
        RemoteDesktopSize,
    };

    use super::*;

    fn dirty_update_at(x: u32) -> RemoteDesktopHelperEvent {
        RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                RemoteDesktopSize {
                    width: 128,
                    height: 1,
                },
                RemoteDesktopRect::new(x, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![x as u8, 0, 0, 0xff],
            ),
        }
    }

    #[test]
    fn sparse_dirty_backlog_requests_base_frame_recovery() {
        let writer = SharedEventWriter::inert_for_tests();

        for index in 0..=FRAME_RECOVERY_THRESHOLD {
            writer
                .send(dirty_update_at((index as u32) * 2))
                .expect("dirty update should enqueue");
        }

        assert!(writer.take_frame_recovery_request().unwrap());
        assert!(!writer.take_frame_recovery_request().unwrap());
    }

    #[test]
    fn base_frame_clears_writer_recovery_state() {
        let writer = SharedEventWriter::inert_for_tests();
        for index in 0..=FRAME_RECOVERY_THRESHOLD {
            writer
                .send(dirty_update_at((index as u32) * 2))
                .expect("dirty update should enqueue");
        }

        writer
            .send(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    RemoteDesktopSize {
                        width: 1,
                        height: 1,
                    },
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0, 0, 0, 0xff],
                ),
            })
            .expect("base frame should enqueue");

        assert!(!writer.take_frame_recovery_request().unwrap());
    }

    #[test]
    fn dirty_updates_are_dropped_while_writer_waits_for_base_frame() {
        let writer = SharedEventWriter::inert_for_tests();
        for index in 0..=FRAME_RECOVERY_THRESHOLD {
            writer
                .send(dirty_update_at((index as u32) * 2))
                .expect("dirty update should enqueue");
        }
        assert!(writer.take_frame_recovery_request().unwrap());

        writer
            .send(dirty_update_at(99))
            .expect("dirty update should be accepted and dropped");

        let (queue, _) = &*writer.queue;
        let queue = queue.lock().unwrap();
        assert!(queue.frames.is_empty());
        assert!(queue.needs_frame_recovery);
    }
}
