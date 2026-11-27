// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{Context, Task};
use oxideterm_connections::{SshConfigHost, list_ssh_config_hosts};
use oxideterm_gpui_shell::palette::{PaletteContent, PaletteModel, PaletteProvider};
use std::{collections::HashSet, sync::Arc};
pub(in crate::workspace) type CommandPaletteEntity = PaletteModel<DesktopPaletteProvider>;
pub(in crate::workspace) struct DesktopPaletteProvider {
    ssh_config_hosts: Arc<[SshConfigHost]>,
    ssh_config_hosts_loading: bool,

    load_generation: u64,
    load_task: Option<Task<()>>,
    runtime: Arc<tokio::runtime::Runtime>,
}
impl From<Arc<tokio::runtime::Runtime>> for DesktopPaletteProvider {
    fn from(runtime: Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            ssh_config_hosts: Arc::default(),
            ssh_config_hosts_loading: false,

            load_generation: 0,
            load_task: None,
            runtime,
        }
    }
}
impl PaletteProvider for DesktopPaletteProvider {
    fn cancel_pending(&mut self) {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.load_task = None;
        self.ssh_config_hosts_loading = false;
    }
}
pub(super) trait DesktopPalette: Sized {
    fn open(&mut self, auto_load: bool, names: HashSet<String>, cx: &mut Context<Self>);
    fn reload_ssh_config_hosts(
        &mut self,
        auto_load: bool,
        names: HashSet<String>,
        cx: &mut Context<Self>,
    );
    fn begin_load_generation(&mut self) -> u64;
    fn apply_ssh_config_load(
        &mut self,
        generation: u64,
        result: Result<Vec<SshConfigHost>, String>,
    ) -> bool;
    fn ssh_config_hosts(&self) -> Arc<[SshConfigHost]>;
}
impl DesktopPalette for CommandPaletteEntity {
    fn open(&mut self, auto_load: bool, names: HashSet<String>, cx: &mut Context<Self>) {
        self.open_content(PaletteContent::Commands, cx);
        self.reload_ssh_config_hosts(auto_load, names, cx);
    }
    fn ssh_config_hosts(&self) -> Arc<[SshConfigHost]> {
        self.provider.ssh_config_hosts.clone()
    }
    fn reload_ssh_config_hosts(
        &mut self,
        auto_load: bool,
        existing_names: HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        if !auto_load {
            self.provider.cancel_pending();
            self.provider.ssh_config_hosts = Arc::default();
            self.set_provider_error(None);
            cx.notify();
            return;
        }
        let generation = self.begin_load_generation();
        let runtime = self.provider.runtime.clone();
        self.provider.load_task = Some(cx.spawn(async move |entity, cx| {
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

    fn begin_load_generation(&mut self) -> u64 {
        self.provider.cancel_pending();
        self.provider.ssh_config_hosts_loading = true;
        self.set_provider_error(None);
        self.provider.load_generation
    }

    fn apply_ssh_config_load(
        &mut self,
        generation: u64,
        result: Result<Vec<SshConfigHost>, String>,
    ) -> bool {
        if generation != self.provider.load_generation {
            return false;
        }
        self.provider.load_task = None;
        self.provider.ssh_config_hosts_loading = false;
        match result {
            Ok(hosts) => {
                self.provider.ssh_config_hosts = hosts.into();
                self.set_provider_error(None);
            }
            Err(error) => {
                self.provider.ssh_config_hosts = Arc::default();
                self.set_provider_error(Some(error));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gpui::{AppContext, TestAppContext};

    use super::super::{PaletteAction, PaletteItem, PaletteSection, RankedItem};
    use super::*;
    use crate::workspace::LucideIcon;
    use oxideterm_gpui_shell::palette::{ShellPaletteRequest, key_request};
    use oxideterm_workspace::CommandPaletteMode as PaletteMode;
    use std::time::Duration;

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
            count: None,
            active: false,
            value: "test".to_string(),
            action: PaletteAction::ReloadWindow,
            disabled,
        }
    }

    #[gpui::test]
    fn query_change_resets_selection_mode_error_and_scroll(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            entity.set_selected_index(Some(3), cx);
            entity.set_error("stale".to_string(), cx);
            let previous_scroll = entity.scroll_handle().clone();

            entity.push_query_text("> command", cx);
            let view = entity.view();
            assert_eq!(view.mode, PaletteMode::Commands);
            assert_eq!(entity.selected_index(), Some(0));
            assert!(view.error.is_none());
            assert!(!Rc::ptr_eq(&view.scroll_handle.0, &previous_scroll.0));

            entity.open_content(PaletteContent::BackgroundTasks, cx);
            entity.push_query_text("@archive.tar", cx);
            assert_eq!(entity.query(), "@archive.tar");
            assert_eq!(entity.view().mode, PaletteMode::All);
            assert!((entity.content() == PaletteContent::BackgroundTasks));
            entity.open(false, HashSet::new(), cx);
            entity.push_query_text("@session", cx);
            assert_eq!(entity.view().mode, PaletteMode::Sessions);
            assert!(!(entity.content() == PaletteContent::BackgroundTasks));
        });
    }

    #[gpui::test]
    fn replaced_load_and_reopened_palette_reject_stale_completions(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            let stale = entity.begin_load_generation();
            let current = entity.begin_load_generation();

            assert!(!entity.apply_ssh_config_load(stale, Ok(Vec::new())));
            assert!(entity.provider.ssh_config_hosts_loading);
            assert!(entity.apply_ssh_config_load(current, Ok(Vec::new())));
            assert!(!entity.provider.ssh_config_hosts_loading);
            let stale = entity.begin_load_generation();
            entity.close(cx);
            entity.open(false, HashSet::new(), cx);

            assert!(!entity.apply_ssh_config_load(stale, Err("stale".to_string())));
            assert!(entity.view().error.is_none());
        });
    }

    #[gpui::test]
    fn animated_close_keeps_the_view_then_clears_it_and_reopen_cancels_close(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        let motion_generation = entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            entity.push_query_text("> terminal", cx);
            entity.begin_close(Duration::from_millis(200), cx);
            assert!(entity.is_open());
            assert!(entity.is_closing());
            entity.replace_query_utf16(None, "ignored", cx);
            assert_eq!(entity.query(), "> terminal");
            entity.view().motion_generation
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        entity.update(cx, |entity, cx| {
            assert_eq!(entity.query(), "> terminal");
            entity.open(false, HashSet::new(), cx);
            assert_eq!(entity.view().motion_generation, motion_generation);
            entity.push_query_text("new query", cx);
        });
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        entity.update(cx, |entity, cx| {
            assert!(entity.is_open());
            assert!(!entity.is_closing());
            assert_eq!(entity.query(), "new query");
            entity.begin_close(Duration::from_millis(200), cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        entity.update(cx, |entity, cx| {
            assert!(!entity.is_open());
            assert_eq!(entity.query(), "");
            entity.open(false, HashSet::new(), cx);
            assert_ne!(entity.view().motion_generation, motion_generation);
            entity.begin_close(Duration::ZERO, cx);
            assert!(!entity.is_open());
        });
    }

    #[gpui::test]
    fn discovery_refresh_preserves_identity_and_removed_selection_cannot_execute(
        cx: &mut TestAppContext,
    ) {
        let row = |id: &str| RankedItem {
            item: PaletteItem {
                id: id.into(),
                ..palette_item(false)
            },
            score: 1.0,
            highlights: Vec::new(),
        };
        let enter = gpui::Keystroke::parse("enter").unwrap();
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |palette, cx| {
            palette.open(false, HashSet::new(), cx);
            let previous = [row("west:gamma"), row("west:beta")];
            palette.set_selected_index(Some(1), cx);
            let selected = palette.selected_item_id(&previous).unwrap();
            // Sync can insert a saved connection without changing either discovery row.
            let refreshed = vec![row("saved:alpha"), row("west:gamma"), row("west:beta")];
            palette.restore_item_selection(selected, &refreshed, cx);
            assert!(matches!(
                key_request(&enter, palette.query(), palette.selected_index(), || refreshed),
                Some(ShellPaletteRequest::Execute(id)) if id == "west:beta"
            ));
            let replaced = vec![row("west:gamma"), row("east:beta")];
            palette.restore_item_selection(selected, &replaced, cx);
            assert!(
                key_request(&enter, palette.query(), palette.selected_index(), || {
                    replaced
                })
                .is_none()
            );
        });
    }

    #[gpui::test]
    fn only_enabled_items_execute_and_the_action_is_taken_once(cx: &mut TestAppContext) {
        let entity = cx.new(|_| CommandPaletteEntity::new(test_runtime()));
        entity.update(cx, |entity, cx| {
            entity.open(false, HashSet::new(), cx);
            assert!(
                entity
                    .take_item_action(&palette_item(true), Duration::ZERO, cx)
                    .is_none()
            );
            assert!(entity.is_open());
            let item = palette_item(false);
            let execution = entity
                .take_item_action(&item, Duration::from_millis(200), cx)
                .expect("enabled palette action");
            assert_eq!(execution.id, "cmd:test");
            assert!(entity.is_open());
            assert!(entity.is_closing());
            assert!(matches!(execution.action, PaletteAction::ReloadWindow));
            assert!(
                entity
                    .take_item_action(&item, Duration::from_millis(200), cx)
                    .is_none()
            );
        });
    }
}
