use std::{ops::Range, rc::Rc};

use crate::FileManagerInput;
use gpui::{AnyElement, AnyView, App, MouseDownEvent, MouseMoveEvent, Rgba, SharedString, Window};
use oxideterm_gpui_markdown::{MarkdownMermaidZoomHandler, MarkdownOptions};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputViewport};
use oxideterm_i18n::I18n;
use oxideterm_preview::{PlatformVideoSnapshot, PlatformVideoState};
use oxideterm_theme::ThemeTokens;

#[derive(Clone)]
pub struct FileManagerBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<FileManagerEnvironment>>,
    pub dispatch: Rc<dyn Fn(FileManagerRequest, &mut App)>,
    pub pointer: Rc<dyn Fn(FileManagerPointer, &mut Window, &mut App)>,
    pub anchor: Rc<dyn Fn(TextInputAnchor)>,
    pub selectable: Rc<dyn Fn(FileManagerSelectableText, &mut App) -> AnyElement>,
    pub document: Rc<dyn Fn(&str, &str, ThemeTokens, I18n, &mut App) -> Option<AnyView>>,
    pub mermaid_zoom: Option<MarkdownMermaidZoomHandler>,
}

#[derive(Clone)]
pub struct FileManagerVideo {
    pub render: Rc<dyn Fn(String, AnyElement) -> AnyElement>,
    pub snapshot: Rc<dyn Fn() -> PlatformVideoSnapshot>,
    pub detach: Rc<dyn Fn()>,
}
impl Default for FileManagerVideo {
    fn default() -> Self {
        Self {
            render: Rc::new(|_, fallback| fallback),
            snapshot: Rc::new(|| PlatformVideoSnapshot {
                state: PlatformVideoState::Unavailable,
                position: std::time::Duration::ZERO,
                duration: None,
                backend: "",
                error: None,
            }),
            detach: Rc::new(|| {}),
        }
    }
}

#[derive(Clone)]
pub struct FileManagerEnvironment {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub background: bool,
    pub modal_background: bool,
    pub chrome_divider: Rgba,
    pub ui_font_family: SharedString,
    pub mono_font_family: SharedString,
    pub mono_font_size: f32,
    pub editor: FileManagerEditorFonts,
    pub markdown: MarkdownOptions,
    pub caret_visible: bool,
    pub ime: [FileManagerImeFrame; 3],
    pub bookmarks_width: i64,
}

#[derive(Clone)]
pub struct FileManagerEditorFonts {
    pub editor_font_family: String,
    pub editor_font_weight: f32,
    pub editor_font_fallback: Option<String>,
    pub editor_font_size: f32,
    pub editor_line_height: f32,
    pub word_wrap: bool,
    pub background_active: bool,
}

#[derive(Clone, Default)]
pub struct FileManagerImeFrame {
    pub anchor: Option<TextInputAnchor>,
    pub selection: Option<Range<usize>>,
    pub marked: Option<String>,
    pub active_offset: Option<usize>,
    pub viewport: TextInputViewport,
}

#[derive(Clone, Copy)]
pub enum FileManagerNoticeKind {
    Error,
    Success,
    Default,
}

pub enum FileManagerRequest {
    ClearIme,
    ClearImeSelection,
    Notice {
        title: String,
        description: Option<String>,
        kind: FileManagerNoticeKind,
    },
    Tooltip {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    ClearTooltip(String),
    SaveBookmarksWidth(i64),
    WindowInteractionChanged,
}

pub enum FileManagerPointer {
    Focus,
    TextDown(FileManagerInput, MouseDownEvent),
    TextMove(MouseMoveEvent),
    DismissTransient,
    OpenTerminal(String),
}

pub struct FileManagerSelectableText {
    pub id: u64,
    pub text: SharedString,
    pub color: u32,
    pub row: Option<(u64, usize, Option<SharedString>)>,
}
