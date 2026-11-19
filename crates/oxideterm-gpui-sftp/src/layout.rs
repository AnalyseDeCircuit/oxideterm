// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Size;

pub(crate) const SFTP_ROOT_PADDING: f32 = 8.0;
const PANE_SPLIT_MIN: f32 = 0.2;
const PANE_SPLIT_MAX: f32 = 0.8;
const QUEUE_MIN_HEIGHT: f32 = 96.0;
const QUEUE_MAX_VIEWPORT_RATIO: f32 = 0.65;

impl SftpWorkspaceEntity {
    pub(crate) fn start_pane_resize(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.pane_resize_drag = Some(SftpPaneResizeDrag {
            start_cursor_x: x,
            start_ratio: self.pane_split_ratio,
        });
        cx.notify();
    }

    pub(crate) fn start_queue_resize(
        &mut self,
        y: Pixels,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let current_height = self.queue_height_for_viewport(viewport);
        self.queue_height = current_height;
        self.queue_resize_drag = Some(SftpQueueResizeDrag {
            start_cursor_y: y,
            start_height: current_height,
        });
        cx.notify();
    }

    pub(crate) fn reset_pane_split(&mut self, cx: &mut Context<Self>) -> bool {
        let ended = self.pane_resize_drag.take().is_some();
        let changed = (self.pane_split_ratio - SFTP_PANE_SPLIT_DEFAULT_RATIO).abs() >= f32::EPSILON;
        if changed || ended {
            self.pane_split_ratio = SFTP_PANE_SPLIT_DEFAULT_RATIO;
            cx.notify();
        }
        ended
    }

    pub(crate) fn reset_queue_height(
        &mut self,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let ended = self.queue_resize_drag.take().is_some();
        let default_height = self.clamp_queue_height(SFTP_QUEUE_DEFAULT_HEIGHT, viewport);
        if (self.queue_height - default_height).abs() >= f32::EPSILON || ended {
            self.queue_height = default_height;
            cx.notify();
        }
        ended
    }

    pub fn update_resize(
        &mut self,
        position: Point<Pixels>,
        viewport: Size<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !dragging {
            return self.finish_resize(cx);
        }
        if let Some(drag) = self.pane_resize_drag {
            // The split ratio is applied inside the SFTP root padding.
            let width = (f32::from(self.surface_size.unwrap_or(viewport).width)
                - SFTP_ROOT_PADDING * 2.0)
                .max(1.0);
            let ratio = (drag.start_ratio + f32::from(position.x - drag.start_cursor_x) / width)
                .clamp(PANE_SPLIT_MIN, PANE_SPLIT_MAX);
            if (ratio - self.pane_split_ratio).abs() >= f32::EPSILON {
                self.pane_split_ratio = ratio;
                cx.notify();
            }
        }
        if let Some(drag) = self.queue_resize_drag {
            let height = self.clamp_queue_height(
                drag.start_height - f32::from(position.y - drag.start_cursor_y),
                viewport,
            );
            if (height - self.queue_height).abs() >= f32::EPSILON {
                self.queue_height = height;
                cx.notify();
            }
        }
        false
    }

    pub fn finish_resize(&mut self, cx: &mut Context<Self>) -> bool {
        let pane_ended = self.pane_resize_drag.take().is_some();
        let queue_ended = self.queue_resize_drag.take().is_some();
        if pane_ended || queue_ended {
            cx.notify();
        }
        pane_ended || queue_ended
    }

    pub(crate) fn queue_height_for_viewport(&self, viewport: Size<Pixels>) -> f32 {
        self.clamp_queue_height(self.queue_height, viewport)
    }

    fn clamp_queue_height(&self, height: f32, viewport: Size<Pixels>) -> f32 {
        let height_limit = f32::from(self.surface_size.unwrap_or(viewport).height);
        let max = (height_limit * QUEUE_MAX_VIEWPORT_RATIO).max(QUEUE_MIN_HEIGHT);
        height.clamp(QUEUE_MIN_HEIGHT, max)
    }
}
