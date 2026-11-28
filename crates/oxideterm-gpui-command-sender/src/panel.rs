//! Shared task strip, editor and controls for the expanded sender. Pane access and saved group resolution stay with the host.
mod controls;
mod tabs;
use crate::*;
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::{
    button::{
        ActionChipOptions, ActionChipTextTone, ButtonRadius, action_chip, action_chip_foreground,
    },
    icons::LucideIcon,
    scroll::ScrollableElement,
    typography::{MonospaceDatumOptions, MonospaceDatumTone, monospace_datum},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::PaneId;
use std::rc::Rc;

const TERMINAL_SENDER_CONTROL_HEIGHT: f32 = 26.0;
const TERMINAL_SENDER_PANEL_PADDING: f32 = 12.0;
const TERMINAL_SENDER_INTERVAL_STEP_LINE_MS: i64 = 100;
const TERMINAL_SENDER_INTERVAL_STEP_CHARACTER_MS: i64 = 20;
const TERMINAL_SENDER_TARGET_LIST_MIN_WIDTH: f32 = 160.0;
const TERMINAL_SENDER_TARGET_CONTEXT_MAX_WIDTH: f32 = 220.0;
const TERMINAL_SENDER_PROGRESS_HEIGHT: f32 = 2.0;

#[derive(PartialEq)]
pub struct SenderPanelFrame {
    pub tokens: ThemeTokens,
    pub mono_font: SharedString,
    pub divider: Rgba,
    pub has_background: bool,
    pub current: String,
    pub targets: Vec<(PaneId, String)>,
    pub groups: Vec<(uuid::Uuid, String)>,
}

pub enum SenderAction {
    Start(TerminalCommandSenderId),
    Group(TerminalCommandSenderId, uuid::Uuid),
    FocusEditor(TerminalCommandSenderId),
    FocusCompact,
    BlurHostInput,
}
type Dispatch = Rc<dyn Fn(SenderAction, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ExpandedSender {
    model: Entity<TerminalCommandSenderEntity>,
    frame: SenderPanelFrame,
    i18n: I18n,
    dispatch: Dispatch,
}

impl ExpandedSender {
    pub fn new(
        model: Entity<TerminalCommandSenderEntity>,
        frame: SenderPanelFrame,
        i18n: I18n,
        dispatch: impl Fn(SenderAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            model,
            frame,
            i18n,
            dispatch: Rc::new(dispatch),
        }
    }
}

struct ExpandedView {
    terminal_command_sender: Entity<TerminalCommandSenderEntity>,
    frame: SenderPanelFrame,
    i18n: I18n,
    dispatch: Dispatch,
    _changes: Subscription,
}

impl RenderOnce for ExpandedSender {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            model,
            frame,
            i18n,
            dispatch,
        } = self;
        let mut frame = Some(frame);
        let controls = w.use_keyed_state(
            ("sender-controls", model.entity_id().as_u64()),
            cx,
            |_, cx| {
                let changes = cx.observe(&model, |_, _, cx| cx.notify());
                ExpandedView {
                    terminal_command_sender: model.clone(),
                    frame: frame.take().unwrap(),
                    i18n: i18n.clone(),
                    dispatch: dispatch.clone(),
                    _changes: changes,
                }
            },
        );
        if let Some(frame) = frame {
            controls.update(cx, |v, cx| {
                v.dispatch = dispatch;
                if v.frame != frame || v.i18n.locale() != i18n.locale() {
                    v.frame = frame;
                    v.i18n = i18n;
                    cx.notify();
                }
            });
        }
        controls
    }
}
impl Render for ExpandedView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = self
            .terminal_command_sender
            .read(cx)
            .active_document_snapshot()
        else {
            return div().into_any_element();
        };
        let documents = self.terminal_command_sender.read(cx).document_snapshots();
        let theme = self.frame.tokens.ui;
        let has_background = self.frame.has_background;
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_terminal_command_sender_tabs(&documents, active.id, window, cx))
            .child({
                let active_id = active.id;
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .bg(rgba(
                        (theme.bg_sunken << 8) | if has_background { 0x6b } else { 0xff },
                    ))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(8.0))
                    .overflow_hidden()
                    .cursor_text()
                    .capture_any_mouse_down(cx.listener(
                        |this, event: &MouseDownEvent, window, cx| {
                            // The editor consumes the click before the workspace blur handler.
                            if event.button == MouseButton::Left {
                                (this.dispatch)(SenderAction::BlurHostInput, window, cx);
                            }
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            (this.dispatch)(SenderAction::FocusEditor(active_id), window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(active.editor.clone())
            })
            .child(self.render_terminal_command_sender_controls(&active, &self.frame.targets, cx))
            .into_any_element()
    }
}
impl ExpandedView {
    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }
}
