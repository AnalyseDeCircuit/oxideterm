use gpui::UniformListScrollHandle;
use oxideterm_gpui_ui::motion::ExitPhase;
use oxideterm_workspace::{CommandPaletteMode as PaletteMode, parse_command_palette_query};
use std::ops::Range;
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PaletteContent {
    Commands,
    TabScopes,
    QuickBarCategories,
    BackgroundTasks,
}

/// Immutable palette state consumed by the window renderer.
#[derive(Clone)]
pub struct CommandPaletteView {
    pub phase: ExitPhase,
    pub motion_generation: usize,
    pub raw_query: String,
    pub mode: PaletteMode,
    pub scroll_handle: UniformListScrollHandle,
    pub error: Option<String>,
    pub content: PaletteContent,
}
#[derive(Clone)]
pub struct PaletteState {
    pub content: PaletteContent,
    pub raw_query: String,
    pub mode: PaletteMode,
    pub selected_index: Option<usize>,
    pub scroll_handle: UniformListScrollHandle,
    pub error: Option<String>,
}
impl Default for PaletteState {
    fn default() -> Self {
        Self {
            content: PaletteContent::Commands,
            raw_query: String::new(),
            mode: PaletteMode::All,
            selected_index: Some(0),
            scroll_handle: UniformListScrollHandle::new(),
            error: None,
        }
    }
}
impl PaletteState {
    pub fn replace_query_utf16(&mut self, range: Option<Range<usize>>, text: &str) {
        oxideterm_editor_core::utf16::replace_utf16(&mut self.raw_query, range, text);
        self.finish_query_change();
    }
    pub fn push_query_text(&mut self, text: &str) {
        self.raw_query.push_str(text);
        self.finish_query_change();
    }
    pub fn pop_query(&mut self) -> bool {
        if self.raw_query.pop().is_none() {
            return false;
        }
        self.finish_query_change();
        true
    }
    fn finish_query_change(&mut self) {
        let (mode, _) = parse_command_palette_query(&self.raw_query);
        self.mode = if self.content != PaletteContent::Commands {
            PaletteMode::All
        } else {
            mode
        };
        self.selected_index = Some(0);
        self.scroll_handle = UniformListScrollHandle::new();
        self.error = None;
    }
}
