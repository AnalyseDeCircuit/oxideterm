use super::*;
use oxideterm_connections::{PluginConnectionProfile, SecretString};
use oxideterm_gpui_ui::{
    modal::{modal_backdrop, overlay_content_boundary},
    scroll::ScrollableElement,
};
use std::collections::BTreeMap;

mod runtime;

#[derive(Default)]
pub(in crate::workspace) struct PluginConnections {
    pub form: Option<PluginConnectionForm>,
    pub sessions: HashMap<String, PluginConnectionSession>,
}

#[derive(Clone)]
pub(in crate::workspace) struct PluginConnectionForm {
    pub profile: PluginConnectionProfile,
    definition: Option<plugin_host::NativePluginConnectionDef>,
    error: bool,
}

pub(in crate::workspace) struct PluginConnectionSession {
    pub plugin_id: String,
    profile_id: String,
    definition: Option<plugin_host::NativePluginConnectionDef>,
    pub schema: Option<plugin_host::NativePluginDeclarativeUiSchema>,
    started: bool,
    cancel: tokio_util::sync::CancellationToken,
    pub status: &'static str,
    terminal: Option<SharedTerminalSession>,
    pub tab: TabId,
}

impl PluginConnections {
    pub(in crate::workspace) fn instance_for_page(&self, tab: TabId) -> Option<&str> {
        self.sessions
            .iter()
            .find(|(_, session)| session.tab == tab && session.terminal.is_none())
            .map(|(instance, _)| instance.as_str())
    }

    pub(in crate::workspace) fn instance_for_terminal(
        &self,
        terminal: &SharedTerminalSession,
    ) -> Option<&str> {
        // Terminal tab IDs change during split/merge; the owned session remains stable.
        self.sessions
            .iter()
            .find(|(_, session)| {
                session
                    .terminal
                    .as_ref()
                    .is_some_and(|owned| Arc::ptr_eq(owned, terminal))
            })
            .map(|(instance, _)| instance.as_str())
    }

    pub fn stop_plugin(&mut self, plugin_id: &str) {
        for session in self
            .sessions
            .values_mut()
            .filter(|session| session.plugin_id == plugin_id)
        {
            session.cancel.cancel();
            session.status = "unavailable";
            session.schema = None;
            if let Some(terminal) = &session.terminal {
                terminal.lock().shutdown();
            }
        }
    }
}

impl Drop for PluginConnectionSession {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(terminal) = self.terminal.take() {
            terminal.lock().shutdown();
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn plugin_connection_types(
        &self,
        cx: &App,
    ) -> Vec<(String, plugin_host::NativePluginConnectionDef)> {
        self.plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .filter(|plugin| {
                matches!(
                    plugin.state,
                    plugin_host::NativePluginState::ReadyProcess
                        | plugin_host::NativePluginState::Loading
                        | plugin_host::NativePluginState::Active
                ) && matches!(
                    plugin.runtime_plan,
                    plugin_host::NativePluginRuntimePlan::Process { .. }
                ) && !plugin_host::native_plugin_requires_permission_review(
                    &plugin.manifest,
                    &plugin.runtime_plan,
                    &plugin.config,
                )
            })
            .flat_map(|plugin| {
                plugin
                    .manifest
                    .contributes
                    .as_ref()
                    .and_then(|c| c.connections.as_ref())
                    .into_iter()
                    .flatten()
                    .map(|definition| (plugin.manifest.id.clone(), definition.clone()))
            })
            .collect()
    }

    pub(in crate::workspace) fn open_plugin_connection_form(
        &mut self,
        plugin_id: &str,
        type_id: &str,
        saved: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let definition = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .and_then(|plugin| {
                plugin
                    .manifest
                    .contributes
                    .as_ref()?
                    .connections
                    .as_ref()?
                    .iter()
                    .find(|definition| definition.id == type_id)
            })
            .cloned();
        if saved.is_none()
            && !self
                .plugin_connection_types(cx)
                .iter()
                .any(|(plugin, definition)| plugin == plugin_id && definition.id == type_id)
        {
            return;
        }
        let now = chrono::Utc::now();
        let profile = saved
            .and_then(|id| {
                self.connection_store
                    .plugin_profiles()
                    .iter()
                    .find(|p| p.id == id)
                    .cloned()
            })
            .unwrap_or_else(|| PluginConnectionProfile {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: plugin_id.into(),
                type_id: type_id.into(),
                name: definition
                    .as_ref()
                    .map(|d| d.title.clone())
                    .unwrap_or_default(),
                group: None,
                config: BTreeMap::new(),
                credential_refs: BTreeMap::new(),
                created_at: now,
                updated_at: now,
                last_used_at: None,
            });
        self.prepare_modal_interaction_boundary(cx);
        self.close_plugin_connection_form(cx);
        self.update_connection_form_state(cx, |state| state.form = None);
        self.plugin_entity.update(cx, |plugins, _| {
            plugins.connections.form = Some(PluginConnectionForm {
                profile,
                definition,
                error: false,
            });
        });
        window.focus(&self.focus_handle, cx);
        self.needs_active_pane_focus = false;
        cx.notify();
    }

    pub(in crate::workspace) fn edit_plugin_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(profile) = self
            .connection_store
            .plugin_profiles()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        {
            self.open_plugin_connection_form(
                &profile.plugin_id,
                &profile.type_id,
                Some(profile.id),
                window,
                cx,
            );
        }
    }

    pub(in crate::workspace) fn close_plugin_connection_form(&mut self, cx: &mut Context<Self>) {
        self.plugin_entity.update(cx, |plugins, _| {
            if let Some(form) = plugins.connections.form.take() {
                plugins.ui_state_mut().remove_surface(
                    &form.profile.plugin_id,
                    "connectionForm",
                    &form.profile.id,
                );
            }
        });
        self.clear_ime_selection();
        cx.notify();
    }

    pub(in crate::workspace) fn update_plugin_connection_form_value(
        &mut self,
        context: &plugin_ui::NativePluginUiControlContext,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.update_plugin_ui_state(cx, |ui| ui.set_context_value(context, value));
    }

    fn save_plugin_connection_form(&mut self, connect: bool, cx: &mut Context<Self>) {
        let Some(form) = self.plugin_entity.read(cx).connections.form.clone() else {
            return;
        };
        let mut profile = form.profile;
        let (mut values, secrets) = self
            .plugin_ui_state(cx)
            .connection_values(&profile.plugin_id, &profile.id);
        profile.name = values
            .remove("__profile/name")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        profile.group = values
            .remove("__profile/group")
            .and_then(|v| v.as_str().map(str::to_string));
        if values
            .remove("__profile/clearCredentials")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            profile
                .credential_refs
                .values_mut()
                .for_each(|reference| *reference = None);
        }
        profile.config.extend(values);
        for key in secrets.keys() {
            profile.config.remove(key);
        }
        let secrets = secrets
            .into_iter()
            .map(|(field, value)| (field, SecretString::from(value.to_string())))
            .collect();
        let id = profile.id.clone();
        if self
            .connection_store
            .save_plugin_profile_credentials(profile, secrets)
            .is_err()
        {
            self.plugin_entity.update(cx, |plugins, _| {
                if let Some(form) = &mut plugins.connections.form {
                    form.error = true;
                }
            });
            cx.notify();
            return;
        }
        self.close_plugin_connection_form(cx);
        if connect {
            self.open_plugin_connection(&id, cx);
        }
        self.queue_cloud_sync_dirty_refresh(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_plugin_connection_form(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.plugin_entity.read(cx).connections.form.clone()?;
        let available = self
            .plugin_connection_types(cx)
            .iter()
            .any(|(plugin, definition)| {
                plugin == &form.profile.plugin_id && definition.id == form.profile.type_id
            });
        let mut schema = form
            .definition
            .as_ref()
            .map(|d| d.form.clone())
            .unwrap_or_else(|| serde_json::from_value(serde_json::json!({})).expect("empty form"));
        fn fill(
            controls: &mut [plugin_host::NativePluginDeclarativeUiControl],
            section: &str,
            profile: &PluginConnectionProfile,
            password_hint: &str,
        ) {
            for control in controls {
                if control.kind != "password"
                    && let Some(value) = control
                        .id
                        .as_ref()
                        .and_then(|id| profile.config.get(&format!("{section}/{id}")))
                {
                    control.value = Some(value.clone());
                }
                if control.kind == "password" {
                    control.value = None;
                    control.placeholder = Some(password_hint.to_string());
                }
                fill(&mut control.children, section, profile, password_hint);
            }
        }
        plugin_ui::localize_native_plugin_schema(&mut schema, self.i18n.locale());
        let hint = self.i18n.t("plugin.connections.password_unchanged");
        fill(&mut schema.controls, "root", &form.profile, &hint);
        for section in &mut schema.sections {
            fill(&mut section.controls, &section.id, &form.profile, &hint);
        }
        let metadata: plugin_host::NativePluginDeclarativeUiSection = serde_json::from_value(serde_json::json!({
            "id":"__profile", "controls":[
                {"kind":"text","id":"name","label":self.i18n.t("plugin.connections.name"),"value":form.profile.name},
                {"kind":"text","id":"group","label":self.i18n.t("plugin.connections.group"),"value":form.profile.group.clone().unwrap_or_default()},
                {"kind":"checkbox","id":"clearCredentials","label":self.i18n.t("plugin.connections.clear_credentials"),"value":false}
            ]
        })).ok()?;
        schema.sections.insert(0, metadata);
        if !schema.controls.is_empty() {
            // Keep the host identity fields first without changing plugin field keys.
            schema.sections.insert(
                1,
                plugin_host::NativePluginDeclarativeUiSection {
                    id: "root".into(),
                    title: None,
                    description: None,
                    controls: std::mem::take(&mut schema.controls),
                },
            );
        }
        let body = self.render_native_plugin_declarative_schema(
            &form.profile.plugin_id,
            "connectionForm",
            &form.profile.id,
            &schema,
            cx,
        );
        let mut footer = oxideterm_gpui_ui::modal::modal_footer(&self.tokens);
        for (key, action) in [("cancel", 0), ("save", 1), ("save_connect", 2)] {
            if action == 2 && !available {
                continue;
            }
            footer = footer.child(
                oxideterm_gpui_ui::button::button(
                    &self.tokens,
                    self.i18n.t(&format!("plugin.connections.{key}")),
                    if action == 2 || (action == 1 && !available) {
                        oxideterm_gpui_ui::button::ButtonTone::Primary
                    } else {
                        oxideterm_gpui_ui::button::ButtonTone::Secondary
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if action == 0 {
                            this.close_plugin_connection_form(cx);
                        } else {
                            this.save_plugin_connection_form(action == 2, cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        let panel = oxideterm_gpui_ui::modal::modal_container(&self.tokens)
            .flex()
            .flex_col()
            .w(px(
                640.0_f32.min(f32::from(window.viewport_size().width) - 32.0)
            ))
            .max_h(px(
                700.0_f32.min(f32::from(window.viewport_size().height) - 32.0)
            ))
            .child(oxideterm_gpui_ui::modal::modal_header(
                &self.tokens,
                form.definition
                    .as_ref()
                    .map(|d| d.title.clone())
                    .unwrap_or_else(|| form.profile.name.clone()),
                form.profile.plugin_id.clone(),
            ))
            .child(
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(body),
            )
            .when(!available, |panel| {
                panel.child(
                    div()
                        .px_4()
                        .child(self.i18n.t("plugin.connections.unavailable")),
                )
            })
            .when(form.error, |panel| {
                panel.child(
                    div()
                        .text_color(rgb(self.tokens.ui.error))
                        .child(self.i18n.t("plugin.connections.failed")),
                )
            })
            .child(footer);
        Some(
            modal_backdrop(oxideterm_gpui_ui::modal::backdrop_color(
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
            ))
            .child(overlay_content_boundary(panel))
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_plugin_connection_surface(
        &mut self,
        instance: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((plugin, schema, status)) = self
            .plugin_entity
            .read(cx)
            .connections
            .sessions
            .get(instance)
            .map(|session| {
                (
                    session.plugin_id.clone(),
                    session.schema.clone(),
                    session.status,
                )
            })
        else {
            return div().into_any_element();
        };
        let instance_id = instance.to_string();
        let mut surface = div().size_full().flex().flex_col().p_3().gap_2();
        if matches!(status, "connecting" | "connected") {
            surface = surface.child(
                oxideterm_gpui_ui::button::button(
                    &self.tokens,
                    self.i18n.t("plugin.connections.disconnect"),
                    oxideterm_gpui_ui::button::ButtonTone::Secondary,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.disconnect_plugin_connection(&instance_id, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if let Some(mut schema) = schema.filter(|_| status == "connected") {
            plugin_ui::localize_native_plugin_schema(&mut schema, self.i18n.locale());
            surface = surface.child(div().flex_1().min_h_0().overflow_y_scrollbar().child(
                self.render_native_plugin_declarative_schema(&plugin, "tab", instance, &schema, cx),
            ));
        } else {
            surface = surface.child(self.i18n.t(&format!("plugin.connections.{status}")));
        }
        surface.into_any_element()
    }
}
