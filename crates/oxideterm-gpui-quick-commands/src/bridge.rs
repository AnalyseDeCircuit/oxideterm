use std::{ops::Range, rc::Rc};

use gpui::{AnyElement, App, FocusHandle, MouseDownEvent, MouseMoveEvent, SharedString, Window};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{QuickCommand, QuickCommandTargetContext};
use oxideterm_theme::ThemeTokens;
use zeroize::Zeroizing;

use crate::{QuickCommandInput, TerminalQuickCommandsState};

type ReadState = dyn for<'a> Fn(&'a App) -> &'a TerminalQuickCommandsState;
type StateUpdate<'a> = dyn FnMut(&mut TerminalQuickCommandsState, &mut App) + 'a;
type UpdateState = dyn Fn(&mut App, &mut StateUpdate<'_>);

/// A single projection into the terminal-owned state keeps integrations and views on the same store.
#[derive(Clone)]
pub struct QuickCommandsAccess {
    read: Rc<ReadState>,
    update: Rc<UpdateState>,
}

impl QuickCommandsAccess {
    pub fn new<T: 'static>(
        owner: gpui::WeakEntity<T>,
        read: fn(&T) -> &TerminalQuickCommandsState,
        write: fn(&mut T) -> &mut TerminalQuickCommandsState,
    ) -> Self {
        let read_owner = owner.clone();
        Self {
            read: Rc::new(move |cx| {
                read(
                    read_owner
                        .upgrade()
                        .expect("mounted quick commands owner")
                        .read(cx),
                )
            }),
            update: Rc::new(move |cx, f| {
                owner
                    .update(cx, |owner, cx| f(write(owner), cx))
                    .expect("mounted quick commands owner");
            }),
        }
    }

    pub(crate) fn read<'a>(&self, cx: &'a App) -> &'a TerminalQuickCommandsState {
        (self.read)(cx)
    }

    pub(crate) fn update<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut TerminalQuickCommandsState, &mut App) -> R,
    ) -> R {
        let mut f = Some(f);
        let mut result = None;
        (self.update)(cx, &mut |state, cx| {
            result = Some(f.take().unwrap()(state, cx))
        });
        result.expect("quick commands update ran")
    }
}

#[derive(Clone)]
pub struct QuickCommandsBindings {
    pub frame: Rc<dyn Fn(&mut App) -> Option<QuickCommandsFrame>>,
    pub dispatch: Rc<dyn Fn(QuickCommandsRequest, Option<&mut Window>, &mut App)>,
    pub ime: Rc<dyn Fn(QuickCommandInput, &App) -> QuickCommandsIme>,
    pub anchor: Rc<dyn Fn(TextInputAnchor)>,
    pub selectable: Rc<dyn Fn(QuickCommandsSelectableText, &mut App) -> AnyElement>,
}

#[derive(Clone)]
pub struct QuickCommandsFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub ui_font: SharedString,
    pub mono_font: SharedString,
    pub focus: FocusHandle,
    pub enabled: bool,
    pub has_background: bool,
    pub caret_visible: bool,
    pub target: Option<QuickCommandTargetContext>,
    pub execution_targets: Vec<QuickCommandTargetContext>,
}

pub struct QuickCommandsIme {
    pub anchor_id: TextInputAnchorId,
    pub active_offset: Option<usize>,
    pub selection: Option<Range<usize>>,
    pub marked: Option<Zeroizing<String>>,
}

pub struct QuickCommandsSelectableText {
    pub id: u64,
    pub text: String,
    pub color: u32,
    pub row: Option<(u64, usize, f32, Option<SharedString>)>,
}

pub enum QuickCommandsRequest {
    WindowLayoutChanged,
    ClearIme,
    FocusInput(QuickCommandInput, MouseDownEvent),
    DragInput(MouseMoveEvent),
    OpenManager,
    FocusPane,
    Run(QuickCommand),
    ConfirmExecution,
    Insert(String),
    CheckQuickBarCategory,
    Tooltip {
        id: &'static str,
        label: String,
        x: f32,
        y: f32,
    },
    ClearTooltip(&'static str),
    Notice(QuickCommandsNotice),
}

pub struct QuickCommandsNotice {
    pub title: String,
    pub description: Option<String>,
    pub variant: QuickCommandsNoticeKind,
}

pub enum QuickCommandsNoticeKind {
    Success,
    Error,
}
