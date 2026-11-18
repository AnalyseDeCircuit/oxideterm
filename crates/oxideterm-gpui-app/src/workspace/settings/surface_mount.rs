use super::*;
use oxideterm_gpui_settings_view::surface::{
    SettingsSurfaceBindings, SettingsSurfaceFrame, SettingsSurfaceRequest, SettingsSurfaceView,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_settings_surface(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<SettingsSurfaceView> {
        let owner = cx.entity().downgrade();
        let bindings = SettingsSurfaceBindings {
            frame: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(SettingsSurfaceFrame {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        has_background: app.settings_background_active(),
                        ui_font: settings_ui_font_family(
                            &app.settings_store.settings().appearance.ui_font_family,
                        ),
                        counts: app.settings_dynamic_section_counts(cx),
                        signatures: app.settings_section_list_signatures(cx),
                        divider: app.workspace_chrome_divider(),
                        groups: SettingsNavigationLayout::from_persisted_groups(
                            &app.settings_store.settings().settings_navigation.groups,
                        )
                        .groups()
                        .to_vec(),
                    })
                }
            }),
            dispatch: Rc::new({
                let owner = owner.clone();
                move |request, window, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        app.handle_settings_surface_request(request, window, cx)
                    });
                }
            }),
            knowledge_available: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    owner
                        .upgrade()
                        .is_some_and(|owner| owner.read(cx).knowledge_has_selected_collection(cx))
                }
            }),
            search_field: Rc::new({
                let owner = owner.clone();
                let dispatch = Self::settings_window_dispatch_for(owner.clone());
                move |value, focused, cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(app.settings_components().navigation_search_field(
                        app.settings_ime_frame(SettingsInput::SettingsSearch, value, focused, cx),
                        dispatch.clone(),
                    ))
                }
            }),
            // Unmigrated sections still use the host's virtual row factory.
            section: Rc::new({
                let owner = owner.clone();
                move |tab, section, cx| {
                    if tab == SettingsTab::Privilege {
                        let owner = owner.upgrade()?;
                        return Some(owner.read(cx).render_settings_credential_section(section, owner, cx));
                    }
                    if tab == SettingsTab::Network {
                        let owner = owner.upgrade()?;
                        let app = owner.read(cx);
                        return Some(if section == SettingsSectionId::NetworkPublicMcp {
                            app.render_mcp_settings_page(owner, cx)
                        } else {
                            app.render_settings_proxy_section(section, owner, cx)
                        });
                    }
                    if tab == SettingsTab::Connections
                        && !matches!(section, SettingsSectionId::ConnectionsReconnect | SettingsSectionId::ConnectionsKnownHosts)
                    {
                        let owner = owner.upgrade()?;
                        return Some(owner.read(cx).render_connections_page_section(section, owner, cx));
                    }
                    owner
                        .update(cx, |app, cx| {
                            if tab == SettingsTab::Ai {
                                app.render_settings_ai_page_section(section, cx)
                            } else {
                                app.render_settings_tab_section(tab, section, cx)
                            }
                        })
                        .ok()
                }
            }),
            scrolled: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let _ = owner.update(cx, |app, cx| {
                        app.pause_settings_caret_blink_during_scroll(cx);
                        if app.open_settings_select.is_some() {
                            app.close_settings_select();
                            app.clear_settings_select_anchors();
                            cx.notify();
                        }
                    });
                }
            }),
            keybinding_note: Rc::new(move |note, color, cx| {
                owner
                    .update(cx, |app, cx| {
                        app.render_selectable_text_scoped(
                            "settings-keybindings-note",
                            "keybindings",
                            note,
                            color,
                            cx,
                        )
                    })
                    .ok()
            }),
        };
        cx.new(|cx| SettingsSurfaceView::new(settings.clone(), bindings, cx))
    }

    fn handle_settings_surface_request(
        &mut self,
        request: SettingsSurfaceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            SettingsSurfaceRequest::ToggleSearch => self.toggle_settings_search(window, cx),
            SettingsSurfaceRequest::ClearSearch => {
                self.settings_workspace
                    .update(cx, |settings, cx| settings.clear_settings_search_query(cx));
                self.clear_ime_selection();
                self.show_active_input_caret(cx);
            }
            SettingsSurfaceRequest::SearchResult(result) => {
                self.activate_settings_search_result(result, cx)
            }
            SettingsSurfaceRequest::OpenNavigationEditor => {
                self.open_settings_navigation_editor(cx)
            }
            SettingsSurfaceRequest::SelectedTab(tab) => {
                let had_select = self.open_settings_select.is_some();
                self.close_settings_select();
                self.focused_settings_input = None;
                self.settings_slider_drag = None;
                self.clear_ime_selection();
                if tab == SettingsTab::General {
                    self.refresh_cli_companion_status(cx);
                    #[cfg(not(target_os = "macos"))]
                    self.refresh_launch_at_login_status(cx);
                }
                if tab == SettingsTab::Portable {
                    self.refresh_portable_settings_snapshot(true, cx);
                }
                if had_select {
                    cx.notify();
                }
            }
            SettingsSurfaceRequest::Tooltip(text, Some(position)) => {
                self.queue_workspace_tooltip(
                    text.clone(),
                    text,
                    f32::from(position.x) + 12.0,
                    f32::from(position.y) + 16.0,
                    cx,
                );
            }
            SettingsSurfaceRequest::Tooltip(text, None) => self.clear_workspace_tooltip(&text, cx),
        }
    }
}
