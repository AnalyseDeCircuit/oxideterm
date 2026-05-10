impl IdeSurface {
    fn render_modal_overlay(&self, dialog: impl IntoElement) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(dialog_backdrop_color())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element()
    }

    fn ide_bg(&self, color: u32, fallback_alpha: u32) -> gpui::Rgba {
        if self.runtime_settings.background_active {
            // Tauri `[data-bg-active]` remaps theme backgrounds to 40% alpha.
            rgba((color << 8) | IDE_BG_ACTIVE_THEME_ALPHA)
        } else {
            rgba((color << 8) | fallback_alpha)
        }
    }

    fn ide_editor_content_bg(&self, color: u32) -> gpui::Rgba {
        if self.runtime_settings.background_active {
            // Tauri IDE leaves CodeMirror's scroller transparent when the tab
            // background is active; the tab strip/status/tree keep the 40% tint.
            rgba((color << 8) | 0x00)
        } else {
            rgb(color)
        }
    }

    fn icon(&self, path: &'static str, size: f32, color: u32) -> AnyElement {
        svg()
            .path(path)
            .size(px(size))
            .text_color(rgb(color))
            .into_any_element()
    }
}
