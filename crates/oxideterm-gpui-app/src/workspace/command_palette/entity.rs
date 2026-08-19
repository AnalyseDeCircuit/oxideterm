// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, ops::Range, sync::Arc};

use gpui::{Context, Task, UniformListScrollHandle};
use oxideterm_connections::{SshConfigHost, list_ssh_config_hosts};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_workspace::{CommandPaletteMode as PaletteMode, parse_command_palette_query};

use super::{PaletteExecution, PaletteItem};

/// Immutable palette state consumed by the window renderer.
#[derive(Clone)]
pub(super) struct CommandPaletteView {
    pub(super) raw_query: String,
    pub(super) mode: PaletteMode,
    pub(super) scroll_handle: UniformListScrollHandle,
    pub(super) error: Option<String>,
}

/// Owns command-palette interaction state and SSH-config discovery lifetime.
pub(in crate::workspace) struct CommandPaletteEntity {
    open: bool,
    raw_query: String,
    mode: PaletteMode,
    selected_index: usize,
    scroll_handle: UniformListScrollHandle,
    ssh_config_hosts: Arc<[SshConfigHost]>,
    ssh_config_hosts_loading: bool,
    error: Option<String>,
    load_generation: u64,
    load_task: Option<Task<()>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl CommandPaletteEntity {
    pub(in crate::workspace) fn new(runtime: Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            open: false,
            raw_query: String::new(),
            mode: PaletteMode::All,
            selected_index: 0,
            scroll_handle: UniformListScrollHandle::new(),
            ssh_config_hosts: Arc::default(),
            ssh_config_hosts_loading: false,
            error: None,
            load_generation: 0,
            load_task: None,
            runtime,
        }
    }

    pub(super) fn view(&self) -> CommandPaletteView {
        CommandPaletteView {
            raw_query: self.raw_query.clone(),
            mode: self.mode,
            scroll_handle: self.scroll_handle.clone(),
            error: self.error.clone(),
        }
    }

    pub(in crate::workspace) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::workspace) fn query(&self) -> &str {
        &self.raw_query
    }

    pub(in crate::workspace) fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub(in crate::workspace) fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    pub(in crate::workspace) fn ssh_config_hosts(&self) -> Arc<[SshConfigHost]> {
        self.ssh_config_hosts.clone()
    }

    pub(in crate::workspace) fn open(
        &mut self,
        auto_load_hosts: bool,
        existing_names: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.open = true;
        self.reset_interaction();
        if auto_load_hosts {
            self.start_ssh_config_load(existing_names, cx);
        } else {
            self.invalidate_load();
            self.ssh_config_hosts = Arc::default();
            self.ssh_config_hosts_loading = false;
        }
        cx.notify();
    }

    pub(in crate::workspace) fn close(&mut self, cx: &mut Context<Self>) {
        self.invalidate_load();
        self.open = false;
        self.reset_interaction();
        self.ssh_config_hosts_loading = false;
        cx.notify();
    }

    pub(in crate::workspace) fn reload_ssh_config_hosts(
        &mut self,
        auto_load_hosts: bool,
        existing_names: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        if auto_load_hosts {
            self.start_ssh_config_load(existing_names, cx);
        } else {
            self.invalidate_load();
            self.ssh_config_hosts = Arc::default();
            self.ssh_config_hosts_loading = false;
            self.error = None;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn push_query_text(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
        self.raw_query.push_str(text);
        self.finish_query_change();
        cx.notify();
        true
    }

    pub(in crate::workspace) fn pop_query(&mut self, cx: &mut Context<Self>) -> bool {
        if self.raw_query.pop().is_none() {
            return false;
        }
        self.finish_query_change();
        cx.notify();
        true
    }

    pub(in crate::workspace) fn replace_query_utf16(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        replace_utf16(&mut self.raw_query, replacement_range, text);
        self.finish_query_change();
        cx.notify();
    }

    pub(in crate::workspace) fn move_selection_forward(
        &mut self,
        item_count: usize,
        step: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if item_count == 0 {
            return false;
        }
        let next = self.selected_index.saturating_add(step).min(item_count - 1);
        self.set_selected_index(next, cx)
    }

    pub(in crate::workspace) fn move_selection_backward(
        &mut self,
        item_count: usize,
        step: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if item_count == 0 {
            return false;
        }
        let next = self.selected_index.min(item_count - 1).saturating_sub(step);
        self.set_selected_index(next, cx)
    }

    pub(in crate::workspace) fn select_first(&mut self, cx: &mut Context<Self>) -> bool {
        self.set_selected_index(0, cx)
    }

    pub(in crate::workspace) fn select_last(
        &mut self,
        item_count: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if item_count == 0 {
            return false;
        }
        self.set_selected_index(item_count - 1, cx)
    }

    pub(in crate::workspace) fn set_selected_index(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.selected_index == index {
            return false;
        }
        self.selected_index = index;
        cx.notify();
        true
    }

    pub(super) fn take_selected_action(
        &mut self,
        items: &[PaletteItem],
        cx: &mut Context<Self>,
    ) -> Option<PaletteExecution> {
        let item = items.get(self.selected_index)?;
        self.take_item_action(item, cx)
    }

    pub(super) fn take_item_action(
        &mut self,
        item: &PaletteItem,
        cx: &mut Context<Self>,
    ) -> Option<PaletteExecution> {
        if !self.open || item.disabled {
            return None;
        }
        let execution = PaletteExecution {
            id: item.id.clone(),
            action: item.action.clone(),
        };
        self.invalidate_load();
        self.open = false;
        self.reset_interaction();
        self.ssh_config_hosts_loading = false;
        cx.notify();
        Some(execution)
    }

    pub(in crate::workspace) fn set_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }

    fn reset_interaction(&mut self) {
        self.raw_query.clear();
        self.mode = PaletteMode::All;
        self.selected_index = 0;
        self.scroll_handle = UniformListScrollHandle::new();
        self.error = None;
    }

    fn finish_query_change(&mut self) {
        let (mode, _) = parse_command_palette_query(&self.raw_query);
        self.mode = mode;
        self.selected_index = 0;
        self.scroll_handle = UniformListScrollHandle::new();
        self.error = None;
    }

    fn invalidate_load(&mut self) {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.load_task.take();
    }

    fn begin_load_generation(&mut self) -> u64 {
        self.invalidate_load();
        self.ssh_config_hosts_loading = true;
        self.error = None;
        self.load_generation
    }

    fn start_ssh_config_load(&mut self, existing_names: HashSet<String>, cx: &mut Context<Self>) {
        let generation = self.begin_load_generation();
        let runtime = self.runtime.clone();
        self.load_task = Some(cx.spawn(async move |entity, cx| {
            let result = runtime
                .spawn_blocking(move || list_ssh_config_hosts(&existing_names))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = entity.update(cx, |entity, cx| {
                if entity.apply_ssh_config_load(generation, result) {
                    cx.notify();
                }
            });
        }));
    }

    fn apply_ssh_config_load(
        &mut self,
        generation: u64,
        result: Result<Vec<SshConfigHost>, String>,
    ) -> bool {
        if generation != self.load_generation {
            return false;
        }
        self.load_task = None;
        self.ssh_config_hosts_loading = false;
        match result {
            Ok(hosts) => {
                self.ssh_config_hosts = hosts.into();
                self.error = None;
            }
            Err(error) => {
                self.ssh_config_hosts = Arc::default();
                self.error = Some(error);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::{
        rc::Rc,
        sync::atomic::{AtomicBool, Ordering},
    };

    use gpui::{AppContext, TestAppContext};

    use super::super::{PaletteAction, PaletteSection};
    use super::*;
    use crate::workspace::LucideIcon;

    fn test_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(tokio::runtime::Runtime::new().expect("test runtime"))
    }

    fn palette_item(disabled: bool) -> PaletteItem {
        PaletteItem {
            id: "cmd:test".to_string(),
            label: "Test".to_string(),
            section: PaletteSection::Commands,
            icon: LucideIcon::Search,
            detail: None,
            shortcut: None,
            value: "test".to_string(),
            action: PaletteAction::ReloadWindow,
            disabled,
        }
    }

    #[gpui::test]
    fn selection_stays_within_item_bounds(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            assert!(!entity.move_selection_forward(0, 1, cx));
            assert!(entity.move_selection_forward(3, 8, cx));
            assert_eq!(entity.selected_index(), 2);
            assert!(entity.move_selection_backward(3, 8, cx));
            assert_eq!(entity.selected_index(), 0);
            assert!(entity.select_last(3, cx));
            assert_eq!(entity.selected_index(), 2);
        });
    }

    #[gpui::test]
    fn query_change_resets_selection_mode_error_and_scroll(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            entity.set_selected_index(3, cx);
            entity.set_error("stale".to_string(), cx);
            let previous_scroll = entity.scroll_handle().clone();

            entity.push_query_text("> command", cx);
            let view = entity.view();
            assert_eq!(view.mode, PaletteMode::Commands);
            assert_eq!(entity.selected_index(), 0);
            assert!(view.error.is_none());
            assert!(!Rc::ptr_eq(&view.scroll_handle.0, &previous_scroll.0));
        });
    }

    #[gpui::test]
    fn stale_load_generation_is_rejected(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, _cx| {
            let stale = entity.begin_load_generation();
            let current = entity.begin_load_generation();

            assert!(!entity.apply_ssh_config_load(stale, Ok(Vec::new())));
            assert!(entity.ssh_config_hosts_loading);
            assert!(entity.apply_ssh_config_load(current, Ok(Vec::new())));
            assert!(!entity.ssh_config_hosts_loading);
        });
    }

    #[gpui::test]
    fn close_and_reopen_reject_stale_completion(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            let stale = entity.begin_load_generation();
            entity.close(cx);
            entity.open(false, HashSet::new(), cx);

            assert!(!entity.apply_ssh_config_load(stale, Err("stale".to_string())));
            assert!(entity.view().error.is_none());
        });
    }

    struct DropSignal(Arc<AtomicBool>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[gpui::test]
    fn entity_release_cancels_retained_worker(cx: &mut TestAppContext) {
        let dropped = Arc::new(AtomicBool::new(false));
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            let dropped = dropped.clone();
            entity.load_task = Some(cx.spawn(async move |_, _| {
                let _signal = DropSignal(dropped);
                std::future::pending::<()>().await;
            }));
        });
        cx.run_until_parked();

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(dropped.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn disabled_item_produces_no_action(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            assert!(entity.take_item_action(&palette_item(true), cx).is_none());
            assert!(entity.is_open());
        });
    }

    #[gpui::test]
    fn enabled_item_is_taken_exactly_once(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            let item = palette_item(false);
            let execution = entity
                .take_item_action(&item, cx)
                .expect("enabled palette action");
            assert_eq!(execution.id, "cmd:test");
            assert!(matches!(execution.action, PaletteAction::ReloadWindow));
            assert!(entity.take_item_action(&item, cx).is_none());
        });
    }
}
