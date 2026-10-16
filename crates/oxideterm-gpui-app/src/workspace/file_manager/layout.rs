use super::*;
use crate::workspace::detached_tab_window::DetachedTabWindow;

const BOOKMARKS_MAX_WIDTH_RATIO: f32 = 0.45;
const FILE_LIST_MIN_WIDTH: f32 = 280.0;

#[derive(Clone, Copy)]
pub(super) struct FileManagerBookmarksResize {
    pub(super) window_id: gpui::WindowId,
    start_x: Pixels,
    start_width: f32,
    width: f32,
}

pub(super) fn bookmarks_width(preferred: f32, surface_width: f32) -> f32 {
    let max = (surface_width - FILE_LIST_MIN_WIDTH)
        .max(0.0)
        .min(surface_width * BOOKMARKS_MAX_WIDTH_RATIO)
        .min(FILE_MANAGER_SIDEBAR_MAX_WIDTH as f32);
    preferred.clamp((FILE_MANAGER_SIDEBAR_MIN_WIDTH as f32).min(max), max)
}

impl FileManagerState {
    pub(in crate::workspace::file_manager) fn cancel_bookmarks_resize(&mut self) -> bool {
        let Some(drag) = self.bookmarks_resize.take() else {
            return false;
        };
        self.bookmarks_resize_activation = None;
        self.bookmarks_resize_close = None;
        self.bookmarks_motion.settle(drag.start_width);
        true
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::file_manager) fn file_manager_layout_width(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        self.file_manager
            .read(cx)
            .bookmarks_surface_size
            .filter(|(id, _)| *id == window.window_handle().window_id())
            .map(|(_, size)| f32::from(size.width))
            .unwrap_or_else(|| f32::from(window.viewport_size().width))
    }

    pub(in crate::workspace::file_manager) fn file_manager_bookmarks_width(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        let preferred = self
            .file_manager
            .read(cx)
            .bookmarks_resize
            .filter(|drag| drag.window_id == window.window_handle().window_id())
            .map(|drag| drag.width)
            .unwrap_or(
                self.settings_store
                    .settings()
                    .sidebar_ui
                    .file_manager_sidebar_width as f32,
            );
        bookmarks_width(preferred, self.file_manager_layout_width(window, cx))
    }

    pub(in crate::workspace) fn file_manager_bookmarks_resize_active(
        &self,
        window_id: gpui::WindowId,
        cx: &App,
    ) -> bool {
        self.file_manager
            .read(cx)
            .bookmarks_resize
            .is_some_and(|drag| drag.window_id == window_id)
    }

    pub(in crate::workspace::file_manager) fn start_file_manager_bookmarks_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_workspace_tooltip("file-manager-bookmarks-resize", cx);
        let window_id = window.window_handle().window_id();
        let width = self.file_manager_bookmarks_width(window, cx);
        let activation = cx.observe_window_activation(window, move |this, window, cx| {
            if !window.is_window_active() {
                this.cancel_file_manager_bookmarks_resize(window_id, cx);
            }
        });
        let weak = cx.weak_entity();
        let closed = window_shell::observe_window_close(window, cx, move |id, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.cancel_file_manager_bookmarks_resize(id, cx)
            });
        });
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.bookmarks_resize = Some(FileManagerBookmarksResize {
                window_id,
                start_x: event.position.x,
                start_width: width,
                width,
            });
            file_manager.bookmarks_motion.settle(width);
            file_manager.bookmarks_resize_activation = Some(activation);
            file_manager.bookmarks_resize_close = Some(closed);
            cx.notify();
        });
        let focus = window
            .root::<DetachedTabWindow>()
            .flatten()
            .map(|root| root.read(cx).focus_handle(cx))
            .unwrap_or_else(|| self.focus_handle.clone());
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn update_file_manager_bookmarks_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self
            .file_manager
            .read(cx)
            .bookmarks_resize
            .filter(|drag| drag.window_id == window.window_handle().window_id())
        else {
            return;
        };
        if !event.dragging() {
            self.finish_file_manager_bookmarks_resize(window.window_handle().window_id(), cx);
            return;
        }
        let width = bookmarks_width(
            drag.start_width + f32::from(event.position.x - drag.start_x),
            self.file_manager_layout_width(window, cx),
        );
        self.file_manager.update(cx, |file_manager, cx| {
            if let Some(active) = file_manager.bookmarks_resize.as_mut()
                && active.width != width
            {
                active.width = width;
                file_manager.bookmarks_motion.settle(width);
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn cancel_file_manager_bookmarks_resize(
        &mut self,
        window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.file_manager_bookmarks_resize_active(window_id, cx) {
            return false;
        }
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.cancel_bookmarks_resize();
            cx.notify();
        });
        cx.notify();
        true
    }

    pub(in crate::workspace) fn finish_file_manager_bookmarks_resize(
        &mut self,
        window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.file_manager_bookmarks_resize_active(window_id, cx) {
            return false;
        }
        let drag = self.file_manager.update(cx, |file_manager, cx| {
            let drag = file_manager.bookmarks_resize.take().unwrap();
            file_manager.bookmarks_resize_activation = None;
            file_manager.bookmarks_resize_close = None;
            cx.notify();
            drag
        });
        if drag.width != drag.start_width {
            self.save_file_manager_bookmarks_width(drag.width, cx);
        }
        cx.notify();
        true
    }

    pub(in crate::workspace::file_manager) fn reset_file_manager_bookmarks_width(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_file_manager_bookmarks_resize(window.window_handle().window_id(), cx);
        self.save_file_manager_bookmarks_width(FILE_MANAGER_SIDEBAR_DEFAULT_WIDTH as f32, cx);
    }

    fn save_file_manager_bookmarks_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let previous = self
            .settings_store
            .settings()
            .sidebar_ui
            .file_manager_sidebar_width;
        let width = (width.round() as i64).clamp(
            FILE_MANAGER_SIDEBAR_MIN_WIDTH,
            FILE_MANAGER_SIDEBAR_MAX_WIDTH,
        );
        if previous == width {
            return;
        }
        self.settings_store
            .settings_mut()
            .sidebar_ui
            .file_manager_sidebar_width = width;
        match self.settings_store.save() {
            Ok(_) => self.settings_workspace.update(cx, |settings, _cx| {
                settings.acknowledge_external_store_state();
            }),
            Err(error) => {
                self.settings_store
                    .settings_mut()
                    .sidebar_ui
                    .file_manager_sidebar_width = previous;
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.error"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                    cx,
                );
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_width_leaves_room_for_files_without_changing_preference() {
        for (preferred, surface, expected) in [
            (250.0, 1000.0, 250.0),
            (999.0, 1000.0, 450.0),
            (10.0, 1000.0, 160.0),
            (400.0, 400.0, 120.0),
            (400.0, 200.0, 0.0),
            (400.0, 1000.0, 400.0),
        ] {
            assert_eq!(
                bookmarks_width(preferred, surface),
                expected,
                "{preferred}/{surface}"
            );
        }
    }
}
