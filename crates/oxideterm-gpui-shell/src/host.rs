use crate::{RailElements, RailSide, SidebarSection};
use gpui::{AnyElement, App, Context, Div, Pixels, Point, Rgba, Stateful, Window, prelude::*};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

pub enum ShellRequest {
    ExitZen,
    HoverZenStrip(bool),
    HoverZenPill(bool),
    Toggle(RailSide),
    Select(SidebarSection),
    BeginResize(RailSide, Pixels),
    DragResize(Pixels),
    EndResize,
    ShowToggleHint(RailSide, String, Point<Pixels>),
    HideToggleHint(RailSide),
}

pub trait ShellHost: Sized + 'static {
    fn request(&mut self, request: ShellRequest, window: &mut Window, cx: &mut Context<Self>);
}

pub struct ShellEnvironment {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub zen_mode: bool,
    pub chrome_locked: bool,
    pub chrome_background: Rgba,
    pub divider: Rgba,
    pub left_divider: Rgba,
}

/// The host supplies content and root event routing. Overlay children remain siblings
/// of the frame so absolute positioning and native hit testing use the window bounds.
pub struct ShellFrameSlots {
    pub environment: ShellEnvironment,
    pub root: Stateful<Div>,
    pub background: Option<AnyElement>,
    pub title_row: Option<AnyElement>,
    pub left_rail: Option<AnyElement>,
    pub left_panel: RailElements,
    pub tab_bar: Option<AnyElement>,
    pub center: AnyElement,
    pub right_panel: RailElements,
    pub right_rail: Option<AnyElement>,
    pub overlays: crate::overlays::OverlayStack,
}

impl ShellFrameSlots {
    pub fn at(mut self, priority: u16) -> Self {
        self.overlays.at(priority);
        self
    }
}
impl FluentBuilder for ShellFrameSlots {}
impl ParentElement for ShellFrameSlots {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.overlays.extend(elements);
    }
}

pub enum ShellContent {
    Frame(ShellFrameSlots),
    /// An exclusive window surface, such as the desktop lock screen.
    Exclusive(AnyElement),
}

pub(crate) type FrameSource = dyn Fn(&mut Window, &mut App) -> ShellContent;
