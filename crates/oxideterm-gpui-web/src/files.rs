mod ide;
mod manager;
mod manager_tabs;
pub(crate) use manager_tabs::DeviceManagers;
mod device;
mod embedded;
mod memory;
mod preview;
mod projects;
pub(crate) use device::can_pick_directory;
pub(crate) use projects::Projects;
pub(crate) use projects::runtime_settings as file_editor_settings;
mod sftp;
use crate::{
    demo::{DemoDrives, Drive, Files, LOCAL, REMOTE, SftpProfile, SftpTarget},
    host::Workspace,
};
use gpui::{prelude::*, *};
use memory::MemoryFs;
use oxideterm_gpui_file_manager::*;
use oxideterm_gpui_ide::{
    IdeLabels, IdeModalLayer, IdeRuntimeSettings, IdeSurface, IdeSurfaceMount,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::{Tab, TabId, TabKind, TabTitleSource};
use std::collections::HashMap;
use std::{path::Path, rc::Rc};

pub struct FileViews {
    manager: FileManagerViews,
    drives: DemoDrives,
    bookmarks: Entity<oxideterm_gpui_sftp::SftpBookmarks>,
    pub sftp_profiles: HashMap<String, SftpProfile>,
    pub sftp_pages: HashMap<TabId, Entity<sftp::Browser>>,
    embedded: embedded::EmbeddedSftp,
    recent_terminals: HashMap<String, oxideterm_workspace::TerminalSessionId>,
}
impl Workspace {
    pub(crate) fn open_file_manager(&mut self, cx: &mut Context<Self>) {
        if !self.demo {
            if let Some(id) = self
                .layout
                .tabs()
                .iter()
                .rev()
                .find(|tab| self.device_managers.tabs.contains_key(&tab.id))
                .map(|tab| tab.id)
            {
                self.activate_page(id, cx);
                self.changed(cx);
            } else {
                self.pick_manager_folder(cx);
            }
            return;
        }
        let path = self.active_session().and_then(|id| {
            let session = self.sessions.get(&id)?;
            (session.node == "local"
                && !session.is_ble()
                && self
                    .settings
                    .read(cx)
                    .settings(cx)
                    .terminal
                    .command_bar
                    .current_directory_awareness)
                .then(|| self.transport.as_ref()?.directory(id.0).map(str::to_owned))
                .flatten()
        });
        self.open_file_manager_at(path, cx);
    }

    pub(crate) fn open_file_manager_at(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        if let Some(files) = &self.files {
            files
                .manager
                .body
                .update(cx, |v, cx| v.open_location(path, cx));
            self.open_page(TabKind::FileManager, "sidebar.panels.files", cx);
        }
    }
    pub fn open_node_sftp(&mut self, id: &str, cx: &mut Context<Self>) {
        self.remember_file_terminal(cx);
        let profile = self
            .files
            .as_ref()
            .and_then(|f| f.sftp_profiles.get(id))
            .cloned();
        let target = SftpTarget {
            id: id.into(),
            profile,
        };
        if target.profile.is_some() || !self.demo || crate::ble::is_ble_node(id) {
            self.open_sftp_target(target, cx);
        } else {
            self.present_node_files(id, cx);
        }
    }

    pub fn open_sftp_target(&mut self, target: SftpTarget, cx: &mut Context<Self>) {
        let Some(f) = &mut self.files else {
            self.notice("remote_desktop.capability_unsupported", cx);
            return;
        };
        if crate::ble::is_ble_node(&target.id) {
            self.notice("remote_desktop.capability_unsupported", cx);
            return;
        }
        let node = target.id.clone();
        if let Some((&tab, _)) = f
            .sftp_pages
            .iter()
            .find(|(_, p)| p.read(cx).target == target)
        {
            self.activate_page(tab, cx);
        } else {
            let id = self.layout.alloc_tab_id();
            let title = self
                .catalog_data
                .items
                .iter()
                .find(|r| r.id() == target.id)
                .map(|r| r.name.clone())
                .or_else(|| target.profile.as_ref().map(|p| p.primary.label()))
                .unwrap_or_else(|| target.id.clone());
            let host = cx.weak_entity();
            let tab_title = format!("{} · {title}", self.i18n.t("sidebar.panels.sftp"));
            let page = cx.new(|cx| {
                sftp::Browser::new(
                    &f.drives,
                    target,
                    oxideterm_gpui_sftp::SftpSurfaceId::Tab(id),
                    title,
                    self.tokens,
                    self.i18n.clone(),
                    self.ui_font.clone(),
                    host,
                    f.bookmarks.clone(),
                    cx,
                )
            });
            f.sftp_pages.insert(id, page);
            self.layout.insert_and_select_main_tab(Tab {
                id,
                kind: TabKind::Sftp,
                title: tab_title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.sync_active_scope(cx);
        }
        self.record_connection_use(&node, cx);
        self.changed(cx);
    }
}

fn frame(t: ThemeTokens, i: &I18n, ui_font: &SharedString) -> FileManagerEnvironment {
    FileManagerEnvironment {
        tokens: t,
        i18n: i.clone(),
        background: false,
        modal_background: false,
        chrome_divider: rgb(t.ui.border),
        ui_font_family: ui_font.clone(),
        mono_font_family: "Lilex".into(),
        mono_font_size: 14.,
        editor: FileManagerEditorFonts {
            editor_font_family: "Lilex".into(),
            editor_font_weight: 400.,
            editor_font_fallback: None,
            editor_font_size: 14.,
            editor_line_height: 1.3,
            word_wrap: false,
            background_active: false,
        },
        markdown: Default::default(),
        caret_visible: true,
        ime: Default::default(),
        bookmarks_width: 180,
    }
}
impl FileViews {
    pub fn new(
        drives: DemoDrives,
        t: ThemeTokens,
        i: &I18n,
        ui_font: &SharedString,
        cx: &mut Context<Workspace>,
    ) -> Self {
        let fs = Rc::new(MemoryFs::new(drives.files(Drive::Node("local".into()))));
        let manager = manager_tabs::build(fs, None, t, i, ui_font, cx);
        Self {
            manager,
            drives,
            bookmarks: cx.new(|_| oxideterm_gpui_sftp::SftpBookmarks::in_memory()),
            sftp_profiles: HashMap::new(),
            sftp_pages: HashMap::new(),
            embedded: Default::default(),
            recent_terminals: HashMap::new(),
        }
    }
    pub(crate) fn cwd_entries(
        &self,
        node: &str,
        path: &str,
    ) -> Option<Vec<oxideterm_environment::CurrentDirectoryEntry>> {
        use oxideterm_environment::{CurrentDirectoryEntry, sort_current_directory_entries};
        let drive = self.drives.files(Drive::Node(node.into()));
        let fs = drive.borrow();
        let mut entries: Vec<_> = fs
            .list(path)
            .ok()?
            .into_iter()
            .filter_map(|(p, e)| {
                let name = Path::new(p).file_name()?.to_str()?;
                if e.data.is_none() {
                    CurrentDirectoryEntry::new(name, p)
                } else {
                    CurrentDirectoryEntry::new_file(name, p)
                }
            })
            .collect();
        sort_current_directory_entries(&mut entries);
        entries.truncate(oxideterm_gpui_command_sender::cwd::MAX_ENTRIES);
        Some(entries)
    }

    pub fn page(&self, tab: &Tab, _: &App) -> AnyElement {
        match tab.kind {
            TabKind::Sftp => self.sftp_pages[&tab.id].clone().into_any_element(),
            _ => div().into_any_element(),
        }
    }
    pub(crate) fn forget_file_terminal(
        &mut self,
        node: &str,
        id: oxideterm_workspace::TerminalSessionId,
        cx: &mut App,
    ) {
        if self.recent_terminals.get(node) == Some(&id) {
            self.recent_terminals.remove(node);
            self.refresh_terminal_directory(node, cx);
        }
    }
    pub(crate) fn refresh_terminal_directory(&self, node: &str, cx: &mut App) {
        for page in self.sftp_pages.values().chain(self.embedded.views.values()) {
            if page.read(cx).target.profile.is_none() && page.read(cx).target.id == node {
                App::notify(cx, page.entity_id());
            }
        }
    }
    pub fn refresh_targets(&self, drive: &Files, cx: &mut App) {
        for page in self.sftp_pages.values().chain(self.embedded.views.values()) {
            page.update(cx, |s, cx| s.reload_drive(drive, cx));
        }
        if Rc::ptr_eq(drive, &self.drives.files(Drive::Node("local".into()))) {
            self.manager
                .body
                .update(cx, |s, cx| s.open_location(None, cx));
        }
    }
    pub fn refresh(&self, t: ThemeTokens, i: &I18n, ui_font: &SharedString, cx: &mut App) {
        for page in self.sftp_pages.values().chain(self.embedded.views.values()) {
            page.update(cx, |s, cx| {
                s.tokens = t;
                s.ui_font = ui_font.clone();
                s.i18n = i.clone();
                cx.notify();
            });
        }
        App::notify(cx, self.manager.body.entity_id());
        App::notify(cx, self.manager.dialog.entity_id());
    }
}
