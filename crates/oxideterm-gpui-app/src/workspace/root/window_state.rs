// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::super::*;
use oxideterm_settings::{WindowGeometry, WindowUiState};

const WINDOW_STATE_SAVE_DELAY: Duration = Duration::from_millis(300);

impl WorkspaceApp {
    pub(in crate::workspace) fn capture_main_window_state(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // Minimize events can report synthetic positions or restore bounds.
        // Neither represents a user-selected normal window placement.
        if window.is_minimized() {
            return;
        }

        let fullscreen = window.is_fullscreen();
        let maximized = !fullscreen && window.is_maximized();
        let normal_bounds = if fullscreen || maximized {
            self.settings_store.settings().window_ui.normal_bounds
        } else {
            let Some(geometry) = geometry_from_window(window) else {
                return;
            };
            Some(geometry)
        };
        let current = self.settings_store.settings().window_ui.clone();
        let next = WindowUiState {
            normal_bounds,
            maximized,
            fullscreen,
            extra: current.extra.clone(),
        };
        if current == next {
            self.pending_window_ui_state = None;
            self.window_state_save_task = None;
            return;
        }
        if self.pending_window_ui_state.as_ref() == Some(&next) {
            return;
        }

        self.pending_window_ui_state = Some(next);
        self.window_state_save_task = Some(cx.spawn(async move |weak, cx| {
            Timer::after(WINDOW_STATE_SAVE_DELAY).await;
            let _ = weak.update(cx, |this, cx| {
                this.window_state_save_task = None;
                this.commit_main_window_state(cx);
            });
        }));
    }

    pub(in crate::workspace) fn flush_main_window_state(&mut self, cx: &mut App) {
        self.window_state_save_task = None;
        self.commit_main_window_state(cx);
    }

    fn commit_main_window_state(&mut self, cx: &mut App) {
        let Some(state) = self.pending_window_ui_state.take() else {
            return;
        };
        if self.settings_store.settings().window_ui != state {
            self.settings_store.settings_mut().window_ui = state;
            self.persist_main_window_state(cx);
        }
    }

    fn persist_main_window_state(&mut self, cx: &mut App) {
        if self.settings_store.save().is_ok() {
            // Keep the external watcher aligned with this Entity-owned write.
            self.settings_workspace.update(cx, |settings, _cx| {
                settings.acknowledge_external_store_state()
            });
        }
    }
}

fn geometry_from_window(window: &Window) -> Option<WindowGeometry> {
    let bounds = window.window_bounds().get_bounds();
    let x = f32::from(bounds.origin.x);
    let y = f32::from(bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }

    Some(WindowGeometry {
        x: x.round() as i64,
        y: y.round() as i64,
        width: width.round() as i64,
        height: height.round() as i64,
    })
}
