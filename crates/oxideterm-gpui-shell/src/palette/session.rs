//! Window-local interaction state. Provider tasks are cancelled when a picker closes or reopens.
use super::{
    CommandPaletteView, PaletteContent, PaletteState, ShellPaletteRequest,
    model::{PaletteItem, RankedItem, resolve_command_palette_selected_index},
};
use gpui::{Context, EventEmitter, Task, UniformListScrollHandle};
use oxideterm_gpui_ui::motion::{ExitPhase, ExitPresence};
use std::{ops::Range, time::Duration};
pub trait PaletteProvider: 'static {
    fn cancel_pending(&mut self);
}
impl PaletteProvider for () {
    fn cancel_pending(&mut self) {}
}
pub struct PaletteClosed;
pub struct PaletteExecution<A> {
    pub id: String,
    pub action: A,
}
pub struct PaletteModel<P: PaletteProvider> {
    pub provider: P,
    open: bool,
    interaction: PaletteState,
    pub focus: crate::overlays::PortalFocus,
    presence: ExitPresence,
    exit_task: Option<Task<()>>,
    motion_generation: usize,
}
impl<P: PaletteProvider> EventEmitter<PaletteClosed> for PaletteModel<P> {}
impl<P: PaletteProvider> EventEmitter<ShellPaletteRequest> for PaletteModel<P> {}
impl<P: PaletteProvider> PaletteModel<P> {
    pub fn new(provider: impl Into<P>) -> Self {
        Self {
            provider: provider.into(),
            open: false,
            interaction: PaletteState::default(),
            focus: Default::default(),
            presence: ExitPresence::visible(),
            exit_task: None,
            motion_generation: 0,
        }
    }
    pub fn view(&self) -> CommandPaletteView {
        CommandPaletteView {
            phase: self.presence.phase(),
            motion_generation: self.motion_generation,
            raw_query: self.interaction.raw_query.clone(),
            mode: self.interaction.mode,
            scroll_handle: self.interaction.scroll_handle.clone(),
            error: self.interaction.error.clone(),
            content: self.interaction.content,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn content(&self) -> PaletteContent {
        self.interaction.content
    }
    pub fn query(&self) -> &str {
        &self.interaction.raw_query
    }

    pub fn reveal_selection<A>(&self, items: &[super::model::RankedItem<A>]) {
        use super::model::*;
        use oxideterm_gpui_ui::virtual_list::*;
        if let Some(row) = self
            .selected_index()
            .and_then(|i| command_palette_scroll_child_index(items, i))
        {
            scroll_tauri_virtual_list_to_index(
                self.scroll_handle(),
                row,
                TauriVirtualListSpec::new(
                    gpui::px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT),
                    COMMAND_PALETTE_VIRTUAL_OVERSCAN,
                ),
                TauriVirtualScrollAlign::Center,
            );
        }
    }
    pub fn selected_index(&self) -> Option<usize> {
        self.interaction.selected_index
    }

    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.interaction.scroll_handle
    }

    pub fn open_content(&mut self, content: PaletteContent, cx: &mut Context<Self>) {
        self.provider.cancel_pending();
        self.exit_task = None;
        self.presence.reopen();
        // Keep the mounted springs when reopening during their exit.
        if !self.open {
            self.motion_generation = self.motion_generation.wrapping_add(1);
        }
        self.open = true;

        self.interaction = PaletteState {
            content,
            ..Default::default()
        };
        cx.notify();
    }
    pub fn is_closing(&self) -> bool {
        self.presence.phase() == ExitPhase::Exiting
    }

    pub fn begin_close(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let Some(generation) = self.presence.begin_exit() else {
            return;
        };
        self.provider.cancel_pending();
        if delay.is_zero() {
            self.close(cx);
            return;
        }
        // The entity keeps its existing view until fade-out ends; reopening cancels this task.
        let timer = cx.background_executor().timer(delay);
        self.exit_task = Some(cx.spawn(async move |weak, cx| {
            timer.await;
            let _ = weak.update(cx, |this, cx| {
                if this.presence.finish_exit(generation) {
                    this.close(cx);
                }
            });
        }));
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.exit_task = None;
        self.provider.cancel_pending();
        self.open = false;
        cx.emit(PaletteClosed);
        self.interaction = PaletteState::default();
        cx.notify();
    }

    pub fn push_query_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if self.is_closing() || text.is_empty() {
            return false;
        }
        self.interaction.push_query_text(text);
        cx.emit(ShellPaletteRequest::QueryChanged(self.query().to_owned()));
        cx.notify();
        true
    }

    pub fn replace_query_utf16(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if self.is_closing() {
            return;
        }
        self.interaction
            .replace_query_utf16(replacement_range, text);
        cx.emit(ShellPaletteRequest::QueryChanged(self.query().to_owned()));
        cx.notify();
    }

    pub fn set_selected_index(&mut self, index: Option<usize>, cx: &mut Context<Self>) -> bool {
        if self.is_closing() || self.interaction.selected_index == index {
            return false;
        }
        self.interaction.selected_index = index;
        cx.notify();
        true
    }

    pub fn selected_item_id<'a, A>(&self, items: &'a [RankedItem<A>]) -> Option<&'a str> {
        let index = resolve_command_palette_selected_index(items, self.selected_index())?;
        Some(&items[index].item.id)
    }

    pub fn restore_item_selection<A>(
        &mut self,
        id: &str,
        items: &[RankedItem<A>],
        cx: &mut Context<Self>,
    ) {
        let index = items
            .iter()
            .position(|row| !row.item.disabled && row.item.id == id);
        self.set_selected_index(index, cx);
    }

    pub fn take_item_action<A: Clone>(
        &mut self,
        item: &PaletteItem<A>,
        close_delay: Duration,
        cx: &mut Context<Self>,
    ) -> Option<PaletteExecution<A>> {
        if !self.open || self.is_closing() || item.disabled {
            return None;
        }
        let execution = PaletteExecution {
            id: item.id.clone(),
            action: item.action.clone(),
        };
        self.begin_close(close_delay, cx);
        Some(execution)
    }

    pub fn set_provider_error(&mut self, error: Option<String>) {
        self.interaction.error = error;
    }
    pub fn set_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.set_provider_error(Some(error));
        cx.notify();
    }
}
