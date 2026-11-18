use super::*;
use oxideterm_gpui_settings_view::{
    components::SettingsWindowRequest,
    credentials_page::{CredentialPage, CredentialRequest},
};

impl WorkspaceApp {
    pub(in crate::workspace) fn open_privilege_credentials_settings(
        &mut self,
        scope_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Privilege, cx);
        });
        if let Some(scope_id) = scope_id {
            self.set_settings_privilege_scope(scope_id, cx);
        }
        self.open_settings(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn settings_privilege_credentials_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_settings_credential_section(section, cx.entity(), cx)
    }

    pub(super) fn render_settings_credential_section(
        &self,
        section: SettingsSectionId,
        owner: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let owner_weak = owner.downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner_weak.update(cx, |app, cx| app.handle_credential_request(request, cx));
        });
        let scroll = |key: &str| self.selectable_text_scroll_handle(key);
        let pattern_input = |input, value: &str, placeholder, cx: &App| {
            self.settings_privilege_patterns_field(input, value, placeholder, owner.clone(), cx)
        };
        self.with_settings_controls_for(owner.clone(), cx, |controls, cx| {
            let page = CredentialPage {
                controls,
                store: &self.connection_store,
                state: &self.settings_workspace,
                scroll: &scroll,
                pattern_input: &pattern_input,
                dispatch,
            };
            match section {
                SettingsSectionId::PrivilegeCredentials => page.credentials(cx),
                SettingsSectionId::PrivilegeTotp => page.totp(cx),
                _ => div().into_any_element(),
            }
        })
    }

    fn handle_credential_request(&mut self, request: CredentialRequest, cx: &mut Context<Self>) {
        match request {
            CredentialRequest::SelectScope(id) => self.select_settings_privilege_scope(id, cx),
            CredentialRequest::BeginPrivilege => self.begin_new_settings_privilege_credential(cx),
            CredentialRequest::EditPrivilege(scope, id) => {
                if let Ok(credentials) = self.connection_store.list_privilege_credentials(&scope)
                    && let Some(credential) = credentials
                        .into_iter()
                        .find(|credential| credential.id == id)
                {
                    self.edit_settings_privilege_credential(scope, credential, cx);
                }
            }
            CredentialRequest::DeletePrivilege(scope, id) => {
                self.delete_settings_privilege_credential(scope, id, cx)
            }
            CredentialRequest::CancelPrivilege => {
                self.reset_settings_privilege_credential_draft(cx)
            }
            CredentialRequest::SavePrivilege => self.save_settings_privilege_credential(cx),
            CredentialRequest::EditTotp(id) => {
                let credential = id
                    .as_deref()
                    .and_then(|id| {
                        self.connection_store
                            .totp_credentials()
                            .iter()
                            .find(|item| item.id == id)
                    })
                    .cloned();
                if id.is_none() || credential.is_some() {
                    self.edit_totp_credential(credential, cx);
                }
            }
            CredentialRequest::DeleteTotp(id) => self.delete_totp_credential_from_settings(id, cx),
            CredentialRequest::CancelTotp => self.cancel_totp_draft(cx),
            CredentialRequest::SaveTotp => self.save_totp_draft(cx),
        }
    }

    pub(in crate::workspace) fn settings_privilege_prompt_type_label(
        &self,
        prompt_type: PrivilegePromptType,
    ) -> String {
        oxideterm_gpui_settings_view::credentials_page::settings_privilege_prompt_type_label(
            &self.i18n,
            prompt_type,
        )
    }

    pub(in crate::workspace) fn settings_privilege_response_mode_label(
        &self,
        response_mode: PrivilegeResponseMode,
    ) -> String {
        oxideterm_gpui_settings_view::credentials_page::settings_privilege_response_mode_label(
            &self.i18n,
            response_mode,
        )
    }

    fn settings_privilege_patterns_field(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder_value: String,
        owner: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let focused = self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            == Some(input);
        let target = WorkspaceImeTarget::Settings(input);
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let placeholder = value.is_empty();
        let visible_value = if placeholder {
            placeholder_value
        } else {
            value.to_string()
        };
        let mut textarea = div()
            .w_full()
            .min_h(px(80.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .text_color(rgb(theme.text));
        textarea = self.render_settings_multiline_textarea_lines(
            textarea,
            target,
            &visible_value,
            placeholder,
            line_height,
            cx,
        );
        if let Some(marked) = self.marked_text_for_target(target, cx) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let dispatch = Self::settings_window_dispatch_for(owner.downgrade());
        let select = dispatch.clone();
        let textarea = textarea
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                select(
                    SettingsWindowRequest::BeginInputSelection(input, event.clone()),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                dispatch(
                    SettingsWindowRequest::DragInputSelection(event.clone()),
                    window,
                    cx,
                )
            });
        let anchors = self.text_input_anchors.clone();
        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _, _| {
            anchors.update(anchor)
        });
        self.settings_multiline_input_viewport(target, control.into_any_element())
    }

    fn settings_privilege_active_scope_id(&self, selected: Option<&str>) -> String {
        selected
            .filter(|id| {
                *id == SSH_COMMON_PRIVILEGE_SCOPE_ID
                    || *id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID
                    || self
                        .connection_store
                        .connections()
                        .iter()
                        .any(|connection| connection.id == *id)
            })
            .unwrap_or(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
            .to_owned()
    }

    pub(in crate::workspace) fn set_settings_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_privilege_scope(scope_id, cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn select_settings_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        let selected_scope = self
            .settings_workspace
            .read(cx)
            .privilege_layout_flags()
            .0
            .map(str::to_string);
        if self.settings_privilege_active_scope_id(selected_scope.as_deref()) != scope_id {
            self.set_settings_privilege_scope(scope_id, cx);
        }
    }

    pub(in crate::workspace) fn reset_settings_privilege_credential_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.reset_privilege_draft(cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn begin_new_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn edit_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.edit_privilege_credential(scope_id, credential, cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn save_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selected_scope = self
            .settings_workspace
            .read(cx)
            .privilege_layout_flags()
            .0
            .map(str::to_string);
        let scope_id = self.settings_privilege_active_scope_id(selected_scope.as_deref());
        let snapshot = self.settings_workspace.read(cx).privilege_snapshot();
        if scope_id == SSH_COMMON_PRIVILEGE_SCOPE_ID
            && !snapshot.require_click_to_send
            && snapshot.host_patterns.trim().is_empty()
        {
            let error = self
                .i18n
                .t("sessionManager.privilege_credentials.auto_requires_host_patterns");
            self.settings_workspace.update(cx, |settings, cx| {
                settings.finish_privilege_save(scope_id, Some(error), cx);
            });
            self.close_settings_select();
            return;
        }
        if snapshot.response_mode.uses_totp() && snapshot.totp_credential_id.is_none() {
            let error = self
                .i18n
                .t("sessionManager.privilege_credentials.two_stage_requires_totp");
            self.settings_workspace.update(cx, |settings, cx| {
                settings.finish_privilege_save(scope_id, Some(error), cx);
            });
            self.close_settings_select();
            return;
        }
        let Some(request) = self.settings_workspace.update(cx, |settings, _cx| {
            settings.take_privilege_save_request(scope_id.clone())
        }) else {
            return;
        };
        match self.connection_store.save_privilege_credential(request) {
            Ok(_) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_save(scope_id, None, cx);
                });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_save(scope_id, Some(error.to_string()), cx);
                });
            }
        }
        self.close_settings_select();
    }

    pub(in crate::workspace) fn delete_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential_id: String,
        cx: &mut Context<Self>,
    ) {
        match self
            .connection_store
            .delete_privilege_credential(&scope_id, &credential_id)
        {
            Ok(_) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_delete(scope_id, &credential_id, None, cx);
                });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_delete(
                        scope_id,
                        &credential_id,
                        Some(error.to_string()),
                        cx,
                    );
                });
            }
        }
        self.close_settings_select();
    }
}
