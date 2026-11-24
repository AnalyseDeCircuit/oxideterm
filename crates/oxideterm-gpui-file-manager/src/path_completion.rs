use gpui::ScrollHandle;
use oxideterm_local_files::{PathCompletionCandidate, PathCompletionRequest};

#[derive(Default)]
pub struct PathCompletionState {
    request: Option<PathCompletionRequest>,
    loaded_parent: Option<String>,
    loading_parent: Option<String>,
    entries: Vec<PathCompletionCandidate>,
    suggestions: Vec<PathCompletionCandidate>,
    selected_index: Option<usize>,
    generation: u64,
    pub scroll_handle: ScrollHandle,
}

impl PathCompletionState {
    /// Updates the query and returns one directory load when cached entries cannot satisfy it.
    pub fn request(&mut self, request: PathCompletionRequest) -> Option<(u64, String)> {
        let parent_path = request.parent_path.clone();
        self.request = Some(request);
        self.selected_index = None;

        if self.loaded_parent.as_deref() == Some(parent_path.as_str()) {
            self.rebuild_suggestions();
            return None;
        }
        self.suggestions.clear();
        if self.loading_parent.as_deref() == Some(parent_path.as_str()) {
            return None;
        }

        self.generation = self.generation.wrapping_add(1);
        self.loading_parent = Some(parent_path.clone());
        self.loaded_parent = None;
        self.entries.clear();
        Some((self.generation, parent_path))
    }

    pub fn apply_entries(
        &mut self,
        generation: u64,
        parent_path: &str,
        entries: Vec<PathCompletionCandidate>,
    ) -> bool {
        if self.generation != generation || self.loading_parent.as_deref() != Some(parent_path) {
            return false;
        }
        self.loading_parent = None;
        self.loaded_parent = Some(parent_path.to_string());
        self.entries = entries;
        if self
            .request
            .as_ref()
            .map(|request| request.parent_path.as_str())
            == Some(parent_path)
        {
            self.rebuild_suggestions();
        }
        true
    }

    pub fn dismiss(&mut self) {
        // Advancing the generation makes every in-flight remote result stale.
        self.generation = self.generation.wrapping_add(1);
        self.request = None;
        self.loaded_parent = None;
        self.loading_parent = None;
        self.entries.clear();
        self.suggestions.clear();
        self.selected_index = None;
        self.scroll_handle = ScrollHandle::new();
    }

    pub fn is_visible(&self) -> bool {
        !self.suggestions.is_empty()
    }

    pub fn suggestions(&self) -> &[PathCompletionCandidate] {
        &self.suggestions
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn acceptance_index(&self, key: &str) -> Option<usize> {
        match key {
            // Suggestions arriving asynchronously must not change Enter's target.
            "enter" => self.selected_index,
            "tab" if self.is_visible() => Some(self.selected_index.unwrap_or(0)),
            _ => None,
        }
    }

    pub fn candidate(&self, index: usize) -> Option<&PathCompletionCandidate> {
        self.suggestions.get(index)
    }

    pub fn parent_path(&self) -> Option<&str> {
        self.request
            .as_ref()
            .map(|request| request.parent_path.as_str())
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        let max_index = self.suggestions.len().saturating_sub(1) as isize;
        let index = match self.selected_index {
            Some(index) => (index as isize + delta).clamp(0, max_index) as usize,
            None if delta < 0 => max_index as usize,
            None => 0,
        };
        self.selected_index = Some(index);
        // Keep keyboard navigation and the popup viewport owned by the same state.
        self.scroll_handle.scroll_to_item(index);
        true
    }

    fn rebuild_suggestions(&mut self) {
        let Some(request) = self.request.as_ref() else {
            self.suggestions.clear();
            return;
        };
        self.suggestions = request.matching_candidates(&self.entries);
        self.scroll_handle = ScrollHandle::new();
        self.selected_index = self
            .selected_index
            .filter(|index| *index < self.suggestions.len());
    }
}

use gpui::{
    AnchoredPositionMode, AnyElement, App, Bounds, Corner, MouseButton, Pixels, anchored, deferred,
    div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_theme::ThemeTokens;

const PATH_COMPLETION_VISIBLE_ROWS: usize = 8;
const PATH_COMPLETION_ROW_HEIGHT: f32 = 28.0;
const PATH_COMPLETION_POPUP_GAP: f32 = 4.0;
const PATH_COMPLETION_BG_ALPHA: u32 = 0xf2;
const PATH_COMPLETION_HOVER_ALPHA: u32 = 0x99;

fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

pub fn render_completion_popup(
    id: &'static str,
    tokens: &ThemeTokens,
    bounds: Bounds<Pixels>,
    selected_index: Option<usize>,
    scroll_handle: ScrollHandle,
    suggestions: &[PathCompletionCandidate],
    accept: impl Fn(usize, &mut App) + Clone + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    let mut popup = div()
        .id(id)
        .w(bounds.size.width)
        .max_h(px(
            PATH_COMPLETION_ROW_HEIGHT * PATH_COMPLETION_VISIBLE_ROWS as f32
        ))
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .track_scroll(&scroll_handle)
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgba((theme.bg_elevated << 8) | PATH_COMPLETION_BG_ALPHA))
        .shadow_lg()
        // Deferred drawing alone does not prevent the file list behind the popup from hit-testing.
        .occlude();

    for (index, candidate) in suggestions.iter().enumerate() {
        let label = if candidate.is_directory {
            format!("{}/", candidate.name)
        } else {
            candidate.name.clone()
        };
        popup = popup.child(
            div()
                .h(px(PATH_COMPLETION_ROW_HEIGHT))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .bg(if Some(index) == selected_index {
                    rgba((theme.bg_hover << 8) | PATH_COMPLETION_HOVER_ALPHA)
                } else {
                    rgba(theme.bg_hover << 8)
                })
                .hover(move |row| row.bg(rgba((theme.bg_hover << 8) | PATH_COMPLETION_HOVER_ALPHA)))
                .child(render_lucide_icon(
                    if candidate.is_directory {
                        LucideIcon::Folder
                    } else {
                        LucideIcon::File
                    },
                    13.0,
                    rgb(if candidate.is_directory {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
                ))
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(rgb(theme.text))
                        .child(label),
                )
                .on_mouse_down(MouseButton::Left, {
                    let accept = accept.clone();
                    move |_, _, cx| {
                        accept(index, cx);
                        cx.stop_propagation();
                    }
                }),
        );
    }

    let popup = oxideterm_gpui_ui::motion::popover_zoom_enter(
        tokens,
        "path-completion-popup",
        div().child(oxideterm_gpui_ui::modal::overlay_content_boundary(popup)),
        -4.0,
        gpui::point(0.5, 0.0),
    );
    deferred(
        anchored()
            .anchor(Corner::TopLeft)
            .position(bounds.bottom_left())
            .offset(gpui::point(px(0.0), px(PATH_COMPLETION_POPUP_GAP)))
            .position_mode(AnchoredPositionMode::Window)
            .child(popup),
    )
    .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
    .into_any_element()
}
