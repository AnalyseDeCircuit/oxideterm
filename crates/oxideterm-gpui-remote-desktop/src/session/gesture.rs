// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_public_mcp::calls::{
    DesktopButtonState::{Pressed, Released},
    DesktopGesture, DesktopInputEvent,
};

fn gesture_events(
    action: DesktopGesture,
    size: RemoteDesktopSize,
) -> Result<Vec<DesktopInputEvent>, String> {
    let point = |x, y| super::public_mcp::require_public_mcp_frame_point(size, x, y);
    let edge = |x, y, button, state| DesktopInputEvent::MouseButton {
        x,
        y,
        button,
        state,
    };
    Ok(match action {
        DesktopGesture::Click { x, y, button } => {
            point(x, y)?;
            vec![edge(x, y, button, Pressed), edge(x, y, button, Released)]
        }
        DesktopGesture::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
        } => {
            // Reject an invalid destination before pressing the button.
            point(from_x, from_y)?;
            point(to_x, to_y)?;
            let mut events = vec![edge(from_x, from_y, button, Pressed)];
            for i in 1..=8 {
                let interpolate =
                    |a: u32, b: u32| ((u64::from(a) * (8 - i) + u64::from(b) * i) / 8) as u32;
                events.push(DesktopInputEvent::MouseMove {
                    x: interpolate(from_x, to_x),
                    y: interpolate(from_y, to_y),
                });
            }
            events.push(edge(to_x, to_y, button, Released));
            events
        }
        DesktopGesture::KeyChord {
            code,
            ctrl,
            alt,
            shift,
            meta,
        } => [Pressed, Released]
            .map(|state| DesktopInputEvent::Key {
                code: code.clone(),
                text: None,
                ctrl,
                alt,
                shift,
                meta,
                state,
            })
            .into(),
    })
}

impl RemoteDesktopSessionEntity {
    pub(super) fn interrupt_gesture(&mut self) {
        if self.active_gesture.take().is_some() {
            self.release_inputs();
        }
    }

    pub fn run_public_mcp_gesture(
        &mut self,
        epoch: u64,
        action: DesktopGesture,
        cancelled: impl Fn() -> bool + 'static,
        cx: &mut Context<Self>,
    ) -> Result<Task<Result<crate::RemoteDesktopFrameSnapshot, String>>, String> {
        let size = self.input_frame_size(epoch)?;
        if self.state.frame_size() != Some(size)
            || u64::from(size.width) * u64::from(size.height)
                > oxideterm_public_mcp::calls::DESKTOP_MAX_FRAME_PIXELS
        {
            return Err("A complete desktop frame within the screenshot limit is required".into());
        }
        let events = gesture_events(action, size)?;
        if self.active_gesture.is_some() {
            return Err("A desktop gesture is already active".into());
        }
        let owner = Arc::new(());
        self.active_gesture = Some(owner.clone());
        Ok(cx.spawn(async move |session, cx| {
            // A preempted runner must never release keys belonging to the user or a newer gesture.
            let check_owner = |s: &mut Self| -> Result<(), String> {
                if !s
                    .active_gesture
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, &owner))
                {
                    return Err("Desktop gesture interrupted".into());
                }
                if cancelled() {
                    s.interrupt_gesture();
                    return Err("Desktop gesture cancelled".into());
                }
                Ok(())
            };
            for (i, event) in events.into_iter().enumerate() {
                if i != 0 {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                }
                session
                    .update(cx, |s, _| {
                        check_owner(s)?;
                        s.send_gesture_event(epoch, &event)
                            .inspect_err(|_| s.interrupt_gesture())
                    })
                    .map_err(|_| "Desktop session closed")??;
            }
            let baseline = session
                .update(cx, |s, _| s.state.snapshot().frame_generation)
                .map_err(|_| "Desktop session closed")?;
            // Static desktops may never repaint. Return their latest frame rather than timing out.
            let deadline = 8;
            for tick in 1..=deadline {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let frame = session
                    .update(cx, |s, _| {
                        check_owner(s)?;
                        s.input_frame_size(epoch)
                            .inspect_err(|_| s.interrupt_gesture())?;
                        if tick < deadline && s.state.snapshot().frame_generation == baseline {
                            return Ok(None);
                        }
                        s.active_gesture = None;
                        let frame = s
                            .public_mcp_frame_snapshot()
                            .filter(|f| f.graphics_epoch == epoch)
                            .ok_or_else(|| {
                                "Desktop input sent, but no matching framebuffer is available"
                                    .to_owned()
                            })?;
                        Ok::<_, String>(Some(frame))
                    })
                    .map_err(|_| "Desktop session closed")??;
                if let Some(frame) = frame {
                    return Ok(frame);
                }
            }
            unreachable!()
        }))
    }
}
