use super::*;

#[derive(Default)]
pub(crate) struct DeviceManagers {
    pub(super) tabs: HashMap<TabId, (device::DeviceFolder, FileManagerViews)>,
    picker: Option<Task<()>>,
}

pub(super) fn build(
    fs: Rc<dyn FileSystemBridge>,
    tab: Option<TabId>,
    t: ThemeTokens,
    i: &I18n,
    ui_font: &SharedString,
    cx: &mut Context<Workspace>,
) -> FileManagerViews {
    let owner = cx.weak_entity();
    let read = owner.clone();
    let dispatch = owner.clone();
    FileManagerViews::with_bridge(
        Path::new(LOCAL),
        fs,
        Default::default(),
        FileManagerBindings {
            frame: Rc::new(move |cx| {
                read.read_with(cx, |h, _| frame(h.tokens, &h.i18n, &h.ui_font))
                    .ok()
            }),
            dispatch: Rc::new(move |req, cx| {
                let _ = dispatch.update(cx, |h, cx| match req {
                    FileManagerRequest::Notice {
                        title,
                        description,
                        kind,
                    } => {
                        use oxideterm_gpui_ui::toast::ToastVariant;
                        let variant = match kind {
                            FileManagerNoticeKind::Success => ToastVariant::Success,
                            FileManagerNoticeKind::Error => ToastVariant::Error,
                            FileManagerNoticeKind::Default => ToastVariant::Default,
                        };
                        let page = tab
                            .or_else(|| {
                                h.layout
                                    .tabs()
                                    .iter()
                                    .find(|t| {
                                        t.kind == TabKind::FileManager
                                            && !h.device_managers.tabs.contains_key(&t.id)
                                    })
                                    .map(|t| t.id)
                            })
                            .map(crate::host::NoticeTarget::Page);
                        h.record_notice(
                            oxideterm_notification_center::NotificationKind::Transfer,
                            variant,
                            title,
                            description,
                            page,
                            cx,
                        );
                    }
                    FileManagerRequest::WindowInteractionChanged => cx.notify(),
                    _ => {}
                });
            }),
            pointer: Rc::new(move |r, w, cx| {
                let _ = owner.update(cx, |h, cx| match r {
                    FileManagerPointer::BrowseDeviceFolder => h.pick_manager_folder(cx),
                    FileManagerPointer::OpenTerminal(path) if tab.is_none() => {
                        let before = h.active_session();
                        h.new_terminal("local".into(), None, w, cx);
                        if let Some(id) = h.active_session().filter(|id| Some(*id) != before)
                            && let Err(key) =
                                h.change_file_terminal_directory(id, "local", &path, w, cx)
                        {
                            h.notice(key, cx);
                        }
                    }
                    _ => w.focus(&h.focus, cx),
                });
            }),
            anchor: Rc::new(|_| {}),
            selectable: Rc::new(|v, _| {
                div()
                    .child(v.text)
                    .text_color(rgb(v.color))
                    .into_any_element()
            }),
            document: Rc::new(|_, _, _, _, _| None),
            mermaid_zoom: None,
        },
        frame(t, i, ui_font),
        cx,
    )
}

impl DeviceManagers {
    pub(crate) fn get(&self, id: TabId) -> Option<&FileManagerViews> {
        self.tabs.get(&id).map(|(_, v)| v)
    }
    pub(crate) fn release(&mut self, id: TabId) {
        self.tabs.remove(&id);
    }
    pub(crate) fn refresh(&self, cx: &mut App) {
        for (_, v) in self.tabs.values() {
            App::notify(cx, v.body.entity_id());
            App::notify(cx, v.dialog.entity_id());
        }
    }
}

impl Workspace {
    pub(crate) fn focused_manager(&self) -> Option<&FileManagerViews> {
        let tab = self
            .focused_tab()
            .filter(|t| t.kind == TabKind::FileManager)?;
        self.manager_for(tab.id)
    }

    pub(crate) fn manager_for(&self, id: TabId) -> Option<&FileManagerViews> {
        self.device_managers
            .get(id)
            .or_else(|| self.files.as_ref().map(|f| &f.manager))
    }

    pub(crate) fn pick_manager_folder(&mut self, cx: &mut Context<Self>) {
        if self.device_managers.picker.is_some() || !device::can_pick_directory() {
            return;
        }
        let choice = device::pick_directory();
        self.device_managers.picker = Some(cx.spawn(async move |owner, cx| {
            let choice = device::picked(choice).await;
            let mut existing = None;
            if let Ok(Some(root)) = &choice {
                let known = owner
                    .read_with(cx, |h, _| {
                        h.device_managers
                            .tabs
                            .iter()
                            .map(|(&id, (f, _))| (id, f.clone()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for (id, f) in known {
                    if root.same_entry(&f).await {
                        existing = Some(id);
                        break;
                    }
                }
            }
            let _ = owner.update(cx, |h, cx| {
                h.device_managers.picker = None;
                let root = match choice {
                    Ok(Some(f)) => f,
                    Ok(None) => return,
                    Err(key) => {
                        h.notice(key, cx);
                        return;
                    }
                };
                if let Some(id) = existing.filter(|id| h.device_managers.tabs.contains_key(id)) {
                    h.activate_page(id, cx);
                } else {
                    let id = h.layout.alloc_tab_id();
                    let views = build(
                        manager::bridge(root.clone()),
                        Some(id),
                        h.tokens,
                        &h.i18n,
                        &h.ui_font,
                        cx,
                    );
                    views.body.update(cx, |v, cx| v.open_location(None, cx));
                    let title = root.name.clone();
                    h.device_managers.tabs.insert(id, (root, views));
                    h.layout.insert_and_select_main_tab(Tab {
                        id,
                        kind: TabKind::FileManager,
                        title,
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    });
                }
                h.sync_active_scope(cx);
                h.changed(cx);
            });
        }));
    }
}
