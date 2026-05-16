use gpui::{Pixels, px};

use crate::terminal_ui::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollbarDrag {
    pub(super) thumb_offset_y: Pixels,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollbarGeometry {
    pub(super) x: Pixels,
    pub(super) y: Pixels,
    pub(super) top: Pixels,
    pub(super) height: Pixels,
    pub(super) track_height: Pixels,
}

impl ScrollbarGeometry {
    pub(super) fn contains_thumb(&self, position: gpui::Point<Pixels>) -> bool {
        position.x >= self.x
            && position.x <= self.x + px(SCROLLBAR_WIDTH)
            && position.y >= self.y + self.top
            && position.y <= self.y + self.top + self.height
    }

    pub(super) fn contains_track(&self, position: gpui::Point<Pixels>) -> bool {
        position.x >= self.x - px(SCROLLBAR_GAP)
            && position.x <= self.x + px(SCROLLBAR_WIDTH) + px(SCROLLBAR_GAP)
            && position.y >= self.y
            && position.y <= self.y + self.track_height
    }
}
