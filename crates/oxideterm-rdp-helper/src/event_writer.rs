use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use oxideterm_remote_desktop::{RemoteDesktopHelperEvent, write_event_line};

const FRAME_QUIET_COALESCE_WINDOW: Duration = Duration::from_millis(2);
const FRAME_MAX_COALESCE_WINDOW: Duration = Duration::from_millis(8);

#[derive(Clone)]
pub(crate) struct SharedEventWriter {
    stdout: Arc<Mutex<io::Stdout>>,
    queue: Arc<(Mutex<EventWriterQueue>, Condvar)>,
}

#[derive(Default)]
struct EventWriterQueue {
    frames: VecDeque<RemoteDesktopHelperEvent>,
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
        return;
    }

    if let Some(existing) = queue.frames.back_mut() {
        if let Err(incoming) = try_merge_frame_event(existing, event) {
            queue.frames.push_back(incoming);
        }
    } else {
        queue.frames.push_back(event);
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
    fn sparse_dirty_backlog_stays_dirty_without_base_frame_recovery() {
        let writer = SharedEventWriter::inert_for_tests();
        let backlog_count = 32;

        for index in 0..backlog_count {
            writer
                .send(dirty_update_at((index as u32) * 2))
                .expect("dirty update should enqueue");
        }

        let (queue, _) = &*writer.queue;
        let queue = queue.lock().unwrap();
        assert_eq!(queue.frames.len(), backlog_count);
    }

    #[test]
    fn base_frame_supersedes_pending_dirty_queue() {
        let writer = SharedEventWriter::inert_for_tests();
        for index in 0..32 {
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

        let (queue, _) = &*writer.queue;
        let queue = queue.lock().unwrap();
        assert_eq!(queue.frames.len(), 1);
        assert!(matches!(
            queue.frames.front(),
            Some(RemoteDesktopHelperEvent::Frame { .. })
        ));
    }

    #[test]
    fn dirty_updates_continue_when_writer_has_sparse_backlog() {
        let writer = SharedEventWriter::inert_for_tests();
        let backlog_count = 32;
        for index in 0..backlog_count {
            writer
                .send(dirty_update_at((index as u32) * 2))
                .expect("dirty update should enqueue");
        }
        writer
            .send(dirty_update_at(99))
            .expect("dirty update should enqueue");

        let (queue, _) = &*writer.queue;
        let queue = queue.lock().unwrap();
        assert_eq!(queue.frames.len(), backlog_count + 1);
    }
}
