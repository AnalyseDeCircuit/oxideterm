use super::*;
use oxideterm_gpui_cloud_sync::panel::CloudSyncPanelView;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_cloud_sync_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::CloudSync)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::CloudSync,
                    title: self.i18n.t("plugin.cloud_sync.panel_title"),
                    title_source: TabTitleSource::I18nKey("plugin.cloud_sync.panel_title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        self.persist_sidebar_settings(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_cloud_sync_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_sidebar_empty(
            &self.tokens,
            Self::render_lucide_icon(
                LucideIcon::Cloud,
                self.tokens.metrics.empty_sidebar_icon_size,
                rgb(theme.text_muted),
            ),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-sidebar-empty",
                "title",
                self.i18n.t("plugin.cloud_sync.panel_title"),
                theme.text_muted,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-sidebar-empty",
                "description",
                self.i18n.t("plugin.cloud_sync.native_description"),
                theme.text_muted,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn render_cloud_sync_surface(&self) -> AnyElement {
        self.cloud_sync_panel.clone().into_any_element()
    }

    pub(in crate::workspace) fn mount_cloud_sync_panel(
        sync: &Entity<CloudSyncWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<CloudSyncPanelView> {
        let owner = cx.weak_entity();
        let for_frame = owner.clone();
        cx.new(|cx| {
            CloudSyncPanelView::new(
                sync,
                move |cx| {
                    for_frame
                        .update(cx, |app, cx| {
                            let anchor =
                                app.cloud_sync.read(cx).view.open_select.and_then(|select| {
                                    app.select_anchors
                                        .get(&cloud_sync_select_anchor_id(select))
                                        .copied()
                                });
                            (app.cloud_sync_page_renderer(cx), anchor)
                        })
                        .ok()
                },
                move |window, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        app.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                    });
                },
                cx,
            )
        })
    }

    fn cloud_sync_sources(&self) -> CloudSyncSources<'_> {
        CloudSyncSources {
            cloud_sync: &self.cloud_sync,
            connection_store: &self.connection_store,
            settings_store: &self.settings_store,
            forwarding_registry: self.forwarding_service.registry(),
        }
    }

    pub(in crate::workspace) fn invalidate_cloud_sync_snapshot_caches(&self, cx: &App) {
        self.cloud_sync_sources()
            .invalidate_cloud_sync_snapshot_caches(cx);
    }

    pub(super) fn cloud_sync_page_renderer(&self, cx: &mut Context<Self>) -> CloudSyncPageRenderer {
        let focused_input = self.focused_settings_input.filter(|input| {
            cloud_sync_form_input_value_ref(&self.cloud_sync.read(cx).view.form, *input).is_some()
        });
        let input = focused_input
            .map(|input| {
                let target = WorkspaceImeTarget::Settings(input);
                let mask = super::super::ime::secret_ime_proxy;
                CloudSyncInputRenderProjection {
                    focused_input: Some(input),
                    active_value: Some(if input.is_secret() {
                        // The page receives only UTF-16-length geometry for an active secret.
                        mask(&self.settings_input_draft)
                    } else {
                        self.settings_input_draft.clone()
                    }),
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    active_offset: self.ime_active_offset_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx).map(|text| {
                        if input.is_secret() {
                            mask(text)
                        } else {
                            text.to_owned()
                        }
                    }),
                    caret_visible: self.input_caret.visible(),
                }
            })
            .unwrap_or_default();
        let diff = self.cloud_sync_sources().diff_render_data(
            self.ai.state.read(cx).key_store(),
            &self.i18n,
            cx,
        );
        let active_tab = self.cloud_sync.read(cx).view.active_tab;
        let tab_transition_active = self.segmented_control_user_transition_active(
            selection_motion::CLOUD_SYNC_SWITCHER_ID,
            cloud_sync_tab_index(active_tab),
        );
        // I18n clones share the catalog Arc; no locale table or secret draft is copied.
        let anchors = self.text_input_anchors.clone();
        let text = self.selectable_text_render_state(cx);
        CloudSyncPageRenderer {
            cloud_sync: self.cloud_sync.clone(),
            input_anchors: std::rc::Rc::new(move |input| {
                let id = WorkspaceImeTarget::Settings(input).anchor_id();
                (id, anchors.viewport(id))
            }),
            render: std::rc::Rc::new(CloudSyncListRenderProjection {
                tokens: self.tokens,
                i18n: self.i18n.clone(),
                selectable_text: std::rc::Rc::new(move |role, id, value, color, _| {
                    let role = match role {
                        CloudSyncTextRole::PlainDocument => SelectableTextRole::PlainDocument,
                        CloudSyncTextRole::NonSelectable => SelectableTextRole::NonSelectable,
                    };
                    text.render_display_text_with_id_in_group(
                        role,
                        super::super::selectable_text::selectable_document_group_id(),
                        id,
                        0,
                        value,
                        color,
                    )
                }),
                has_background: self.background_surface_active("cloud_sync"),
                input,
                diff,
                mono_font_family: settings_mono_font_family(self.settings_store.settings()),
                tab_transition_active,
            }),
        }
    }
}
