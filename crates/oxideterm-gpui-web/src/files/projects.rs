use super::*;
use oxideterm_gpui_ide::IdeSurfaceEvent;

#[derive(Default)]
pub(crate) struct Projects {
    tabs: HashMap<TabId, IdeProject>,
    picker: Option<Task<()>>,
}

enum Source {
    Node(String, Files),
    Device(super::device::DeviceFolder),
}

impl Source {
    fn labels(&self, i: &I18n) -> IdeLabels {
        let mut labels = IdeLabels::localized(i);
        if matches!(self, Self::Device(_)) {
            labels.conflict_desc = i.t("ide.device_file_changed");
            labels.remote_version = i.t("ide.device_file_version");
            labels.reload_remote = i.t("ide.device_file_reload");
            labels.overwrite = i.t("ide.device_file_overwrite");
            labels.select_folder_desc = i.t("ide.select_folder");
        }
        labels
    }
}

struct IdeProject {
    source: Source,
    surface: Entity<IdeSurface>,
    modal: Entity<IdeModalLayer>,
    _events: Subscription,
}

pub(crate) fn runtime_settings(s: &PersistedSettings) -> IdeRuntimeSettings {
    let ide = &s.ide;
    let terminal = &s.terminal;
    let primary = match ide.font_family {
        Some(f) => f.terminal_family_name(&ide.custom_font_family),
        None => terminal
            .font_family
            .terminal_family_name(&terminal.custom_font_family),
    };
    let cjk = ide
        .cjk_font_family
        .as_deref()
        .unwrap_or(&terminal.cjk_font_family)
        .trim();
    IdeRuntimeSettings {
        editor_font_fallback: oxideterm_gpui_ui::css_font_family_head(if cjk.is_empty() {
            &primary
        } else {
            cjk
        })
        .map(|family| family.to_string()),
        editor_font_family: primary,
        editor_font_weight: ide
            .font_weight
            .unwrap_or(terminal.font_weight)
            .clamp(100, 900) as f32,
        editor_font_size: ide.font_size.unwrap_or(terminal.font_size).clamp(8, 32) as f32,
        editor_line_height: ide
            .line_height
            .unwrap_or_else(|| terminal.resolved_ide_line_height())
            .clamp(0.8, 3.0) as f32,
        auto_save: ide.auto_save,
        word_wrap: ide.word_wrap,
        background_active: false,
    }
}

impl Projects {
    pub(crate) fn project_nodes(&self) -> impl Iterator<Item = &str> {
        self.tabs.keys().filter_map(|id| self.project_node(*id))
    }

    pub(crate) fn project_count(&self) -> usize {
        self.tabs.len()
    }

    pub(crate) fn project_node(&self, tab: TabId) -> Option<&str> {
        match &self.tabs.get(&tab)?.source {
            Source::Node(id, _) => Some(id),
            Source::Device(_) => None,
        }
    }

    pub(crate) fn page(&self, id: TabId) -> AnyElement {
        let p = &self.tabs[&id];
        div()
            .relative()
            .size_full()
            .child(p.surface.clone())
            .child(p.modal.clone())
            .into_any_element()
    }

    pub(crate) fn refresh_drive(&self, drive: &Files, cx: &mut App) {
        for p in self.tabs.values() {
            if matches!(&p.source, Source::Node(_,f) if Rc::ptr_eq(f,drive)) {
                p.surface
                    .update(cx, |v, cx| v.refresh_project_tree_root(cx));
            }
        }
    }

    pub(crate) fn appearance(
        &self,
        t: ThemeTokens,
        i: &I18n,
        runtime: IdeRuntimeSettings,
        cx: &mut App,
    ) {
        for p in self.tabs.values() {
            p.surface.update(cx, |v, cx| {
                v.set_visual_and_runtime_settings(t, runtime.clone(), cx);
                v.set_labels(p.source.labels(i), cx);
            });
        }
    }

    pub(crate) fn dirty_pages(
        &self,
        closing: &[TabId],
        layout: &crate::host::BrowserTabs,
        cx: &mut App,
    ) -> Vec<String> {
        self.tabs
            .iter()
            .filter_map(|(id, p)| {
                if !closing.contains(id) && !closing.contains(&layout.container_tab_id(*id)) {
                    return None;
                }
                p.surface
                    .update(cx, |s, cx| s.has_unsaved_changes(cx))
                    .then(|| layout.tab_by_id(*id).map(|t| t.title.clone()))
                    .flatten()
            })
            .collect()
    }

    pub(crate) fn release(&mut self, id: TabId) {
        self.tabs.remove(&id);
    }
    pub(crate) fn sync_project_mounts(&self, layout: &crate::host::BrowserTabs, cx: &mut App) {
        for (id, p) in &self.tabs {
            let mount = if layout.surface_is_visible(*id) {
                IdeSurfaceMount::MainWindow
            } else {
                IdeSurfaceMount::Hidden
            };
            if p.surface.read(cx).mount() != mount {
                p.surface.update(cx, |s, cx| s.set_mount(mount, cx));
            }
        }
    }
}

impl Workspace {
    pub(crate) fn focused_file_node(&self, cx: &App) -> Option<String> {
        let files = self.files.as_ref()?;
        if let Some(s) = self.active_session().and_then(|s| self.sessions.get(&s)) {
            return (!s.is_ble() && s.node != "local").then(|| s.node.clone());
        }
        if let Some(t) = self.focused_tab() {
            if t.kind == TabKind::Ide {
                return self.projects.project_node(t.id).map(str::to_owned);
            }
            if let Some(page) = files.sftp_pages.get(&t.id) {
                let target = &page.read(cx).target;
                return target.profile.is_none().then(|| target.id.clone());
            }
        }
        let mut nodes = self
            .sessions
            .values()
            .filter(|s| !s.is_ble() && s.node != "local")
            .map(|s| &s.node);
        let first = nodes.next()?;
        nodes.all(|n| n == first).then(|| first.clone())
    }

    pub(crate) fn open_node_ide(&mut self, node: &str, w: &mut Window, cx: &mut Context<Self>) {
        self.open_node_ide_at(node, None, w, cx);
    }

    pub(crate) fn open_node_ide_at(
        &mut self,
        node: &str,
        path: Option<&str>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(files) = self.files.as_ref() else {
            return;
        };
        if let Some((&id, p)) = self
            .projects
            .tabs
            .iter()
            .find(|(_, p)| matches!(&p.source,Source::Node(n,_) if n==node))
        {
            if let Some(path) = path {
                p.surface.update(cx, |s, cx| {
                    s.open_remote_folder_picker_for_node(node, path, cx)
                });
            }
            w.focus(&p.surface.read(cx).focus_handle(cx), cx);
            self.activate_page(id, cx);
            self.changed(cx);
            return;
        }
        let drive = files.drives.files(Drive::Node(node.into()));
        let name = self
            .catalog_data
            .items
            .iter()
            .find(|r| r.id() == node)
            .map_or(node, |r| &r.name)
            .to_owned();
        let id = self.layout.alloc_tab_id();
        self.mount_project(
            id,
            Source::Node(node.into(), drive),
            name,
            path.unwrap_or(REMOTE),
            w,
            cx,
        );
    }

    pub(crate) fn pick_device_project(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.projects.picker.is_some() || !super::device::can_pick_directory() {
            return;
        }
        let choice = super::device::pick_directory();
        self.projects.picker = Some(cx.spawn_in(w, async move |owner, cx| {
            let folder = super::device::picked(choice).await;
            let known = owner
                .read_with(cx, |h, _| {
                    h.projects
                        .tabs
                        .iter()
                        .filter_map(|(&id, p)| match &p.source {
                            Source::Device(f) => Some((id, f.clone())),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut existing = None;
            if let Ok(Some(f)) = &folder {
                for (id, other) in known {
                    if f.same_entry(&other).await {
                        existing = Some(id);
                        break;
                    }
                }
            }
            let _ = owner.update_in(cx, |h, w, cx| {
                h.projects.picker = None;
                let folder = match folder {
                    Ok(Some(f)) => f,
                    Ok(None) => return,
                    Err(key) => {
                        h.notice(key, cx);
                        return;
                    }
                };
                if let Some(id) = existing
                    && let Some(p) = h.projects.tabs.get(&id)
                {
                    w.focus(&p.surface.read(cx).focus_handle(cx), cx);
                    h.activate_page(id, cx);
                    h.changed(cx);
                } else {
                    let id = h.layout.alloc_tab_id();
                    let name = folder.name.clone();
                    h.mount_project(id, Source::Device(folder), name, "/", w, cx);
                }
            });
        }));
    }

    fn mount_project(
        &mut self,
        id: TabId,
        source: Source,
        name: String,
        path: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = runtime_settings(self.settings.read(cx).settings(cx));
        let surface = cx.new(|cx| {
            let host: Rc<dyn oxideterm_gpui_ide::portable::PortableIdeHost> = match &source {
                Source::Node(_, drive) => Rc::new(MemoryFs::new(drive.clone())),
                Source::Device(folder) => Rc::new(folder.clone()),
            };
            let mut s = IdeSurface::new(host, self.tokens, source.labels(&self.i18n), settings, cx);
            s.set_mount(IdeSurfaceMount::MainWindow, cx);
            match &source {
                Source::Node(node, _) => s.open_remote_folder_picker_for_node(node, path, cx),
                // The IDE uses this opaque project ID for buffer identity, never for SSH routing.
                Source::Device(_) => {
                    s.open_remote_project(format!("device-folder:{}", id.0), path, cx)
                }
            }
            s
        });
        let modal = cx.new(|cx| IdeModalLayer::new(surface.clone(), cx));
        w.focus(&surface.read(cx).focus_handle(cx), cx);
        let events = cx.subscribe(&surface, move |h, _, event, cx| {
            if matches!(event, IdeSurfaceEvent::TransientFolderPickerCancelled) {
                h.close(id, cx);
            }
        });
        self.projects.tabs.insert(
            id,
            IdeProject {
                source,
                surface,
                modal,
                _events: events,
            },
        );
        self.layout.insert_and_select_main_tab(Tab {
            id,
            kind: TabKind::Ide,
            title: format!("{} · {name}", self.i18n.t("settings_view.tabs.ide")),
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        self.sync_active_scope(cx);
        self.changed(cx);
    }
}
