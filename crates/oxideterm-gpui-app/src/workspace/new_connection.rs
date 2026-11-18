mod ble_flow;
use oxideterm_gpui_connections::{entity, form_state};
mod form_view;
mod ftp;
mod host_key_dialog;
mod kbi_dialog;
mod password_prompt;
mod ssh_flow;

use std::rc::Rc;

use gpui::{WeakEntity, div, prelude::*};
use oxideterm_forwarding::ForwardType;
use oxideterm_gpui_connections::session_manager::{
    ManagerTextRole, OxideDialogAction, OxideDialogEnvironment, SessionManagerViewBindings,
};
use oxideterm_gpui_connections::views::{
    ConnectionFormEnvironment, ConnectionsEnvironment, ConnectionsInput, ConnectionsViewBindings,
    ConnectionsWindowRequest, FormPluginType, TunnelFormEnvironment, TunnelKind,
};

use super::{WorkspaceApp, ime::WorkspaceImeTarget, selectable_text::selectable_text_id};

pub(super) use ble_flow::{
    BleConnectionWorkerResult, BleOpenRequest, BlePendingOpen, BleStandaloneTarget, BleTraceTarget,
};
pub(super) use entity::{ConnectionFlowEvent, NativeProxyConnectRun};
pub(super) use form_state::{
    CONNECTION_NOTES_LINE_HEIGHT, CONNECTION_NOTES_VERTICAL_PADDING,
    NewConnectionField, NewConnectionForm, NewConnectionSelect,
    NewConnectionSubmitAction, NewConnectionTransport,
    SavedConnectionPromptAction,
    form_from_mosh_profile, form_from_remote_desktop_profile,
    terminal_serial_flow_from_profile,
    terminal_serial_parity_from_profile, terminal_serial_runtime_options_from_ble_profile,
    terminal_serial_runtime_options_from_profile, terminal_telnet_line_endings_from_profile,
};
pub(super) use oxideterm_gpui_connections::ConnectionFormState;
pub(in crate::workspace) use password_prompt::SavedAuthSaveTarget;
pub(super) use ssh_flow::{
    MoshConnectionOptions, NativeSshPromptHandler, SshConnectionIntent, SshConnectionWorkerResult,
    SshTerminalConnectionOptions, mosh_options_from_profile,
};

pub(super) use oxideterm_gpui_connections::preflight::ssh_connection_display_title;

impl WorkspaceApp {
    pub(in crate::workspace) fn connections_view_bindings(
        owner: WeakEntity<Self>,
    ) -> ConnectionsViewBindings {
        let environment = owner.clone();
        let ime = owner.clone();
        let text = owner.clone();
        let form = owner.clone();
        let form_text = owner.clone();
        let oxide_environment = owner.clone();
        let catalog_environment = owner.clone();
        let manager_ime = owner.clone();
        let manager_text = owner.clone();
        ConnectionsViewBindings {
            manager: SessionManagerViewBindings {
                catalog: Rc::new(move |include_items, cx| Some(catalog_environment.upgrade()?.read(cx).session_manager_catalog_environment(include_items, cx))),
                oxide_environment: Rc::new(move |cx| {
                    let owner = oxide_environment.upgrade()?;
                    let app = owner.read(cx);
                    let store = &app.connection_store;
                    Some(OxideDialogEnvironment::from_store(store))
                }),
                ime: Rc::new(move |input, cx| Some(manager_ime.upgrade()?.read(cx)
                    .connection_ime_frame_for_target(WorkspaceImeTarget::SessionManager(input), cx))),
                text: Rc::new(move |role, scope, id, value, color, alpha, cx| {
                    use crate::workspace::selectable_text::SelectableTextRole;
                    let role = match role {
                        ManagerTextRole::PlainDocument => SelectableTextRole::PlainDocument,
                        ManagerTextRole::RowSafe => SelectableTextRole::RowSafe,
                        ManagerTextRole::NonSelectable => SelectableTextRole::NonSelectable,
                    };
                    manager_text.update(cx, |app, cx| app.render_display_text_with_role_and_alpha(role, scope, id, value, color, alpha, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                }),
            },
            environment: Rc::new(move |cx| {
                let owner = environment.upgrade()?;
                let app = owner.read(cx);
                Some(ConnectionsEnvironment {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                })
            }),
            ime: Rc::new(move |input, cx| {
                let owner = ime.upgrade()?;
                Some(owner.read(cx).connections_ime_frame(input, cx))
            }),
            selectable_text: Rc::new(move |label, value, is_label, color, cx| {
                text.update(cx, |app, cx| {
                    if is_label {
                        app.render_selectable_text_scoped(
                            "host-key-label",
                            (label, value),
                            label.to_owned(),
                            color,
                            cx,
                        )
                    } else {
                        app.render_selectable_text(
                            selectable_text_id("host-key-value", (label, value)),
                            value.to_owned(),
                            color,
                            cx,
                        )
                    }
                })
                .unwrap_or_else(|_| div().into_any_element())
            }),
            form_environment: Rc::new(move |cx| {
                let owner = form.upgrade()?;
                Some(owner.read(cx).connection_form_environment(cx))
            }),
            form_selectable_text: Rc::new(move |scope, key, value, color, cx| {
                form_text
                    .update(cx, |app, cx| {
                        app.render_selectable_text_scoped(scope, key, value, color, cx)
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            }),
            dispatch: Rc::new(move |request, window, cx| {
                let _ = owner.update(cx, |app, cx| app.dispatch_connections_request(request, window, cx));
            }),
        }
    }

    pub(in crate::workspace) fn dispatch_connections_request(
        &mut self, request: ConnectionsWindowRequest, window: Option<&mut gpui::Window>, cx: &mut gpui::Context<Self>,
    ) {
        match request {
            ConnectionsWindowRequest::Catalog(action) => {
                self.apply_session_manager_catalog_action(action, window, cx);
                return;
            }
            ConnectionsWindowRequest::Batch(action) => {
                self.apply_batch_window_action(action, window, cx);
                return;
            }
            ConnectionsWindowRequest::Oxide(action) => {
                match action {
                    OxideDialogAction::PickImportFile => self.select_oxide_import_file(cx),
                    OxideDialogAction::PreviewImport => self.preview_oxide_import_dialog(cx),
                    OxideDialogAction::ApplyImport => self.apply_oxide_import_dialog(cx),
                    OxideDialogAction::RefreshExport => self.refresh_oxide_export_preflight(cx),
                    OxideDialogAction::Export => self.export_oxide_dialog(cx),
                }
                return;
            }
            ConnectionsWindowRequest::AdoptFtp(result) => {
                self.adopt_ftp_connection(result, cx);
                return;
            }
            ConnectionsWindowRequest::ClearComposition => {
                self.ime_marked_text = None;
                return;
            }
            ConnectionsWindowRequest::ShowCaret => {
                self.show_active_input_caret(cx);
                return;
            }
            ConnectionsWindowRequest::SelectTunnelHost(id) => {
                if let Some(form) = self.ssh_tunnels.form.as_mut() {
                    form.host_id = Some(id);
                    self.connections.notify_form_input(cx);
                }
                return;
            }
            ConnectionsWindowRequest::SetTunnelKind(kind) => {
                if let Some(form) = self.ssh_tunnels.form.as_mut() {
                    form.kind = match kind {
                        TunnelKind::Local => ForwardType::Local,
                        TunnelKind::Remote => ForwardType::Remote,
                        TunnelKind::Dynamic => ForwardType::Dynamic,
                    };
                    self.connections.notify_form_input(cx);
                }
                return;
            }
            ConnectionsWindowRequest::ToggleTunnelStartup => {
                if let Some(form) = self.ssh_tunnels.form.as_mut() {
                    form.startup = !form.startup;
                    self.connections.notify_form_input(cx);
                }
                return;
            }
            ConnectionsWindowRequest::RevealSavedPassword => {
                self.reveal_edit_saved_password(cx);
                return;
            }
            ConnectionsWindowRequest::QueueTooltip { id, label, x, y } => {
                self.queue_workspace_tooltip(id, label, x, y, cx);
                return;
            }
            ConnectionsWindowRequest::ClearTooltip(id) => {
                self.clear_workspace_tooltip(&id, cx);
                return;
            }
            _ => {}
        }
        let Some(window) = window else {
            return;
        };
        match request {
            ConnectionsWindowRequest::VerifiedSsh { config, title, intent } => self.continue_verified_ssh_flow(config, title, intent, window, cx),
            ConnectionsWindowRequest::PreflightFailed { intent, message, inline } => {
                if matches!(&intent, SshConnectionIntent::Mosh(_)) && self.plugin_entity.read(cx).mosh_executable().is_none() {
                    self.show_required_plugin_notice("com.oxideterm.terminal.mosh", "Mosh", cx);
                }
                self.fail_public_mcp_mosh_open_for_intent(&intent, message.clone());
                if !inline {
                    self.set_connection_status(Some(message), cx);
                }
            }
            ConnectionsWindowRequest::AdvanceProxyChain => self.continue_active_proxy_session_tree_connect(window, cx),
            ConnectionsWindowRequest::ConnectProxyNode { node_id, changed } => {
                if changed {
                    self.persist_session_tree_snapshot();
                }
                if !self.ensure_node_connection_started_without_ancestors(&node_id, cx) {
                    self.cancel_active_proxy_connect_run(cx);
                    self.report_proxy_session_tree_error(format!("failed to start SSH node {}", node_id.0), cx);
                }
            }
            ConnectionsWindowRequest::FinishProxyChain { plan, terminal, report } => {
                self.finish_proxy_session_tree_connect(plan, terminal, report, window, cx);
            }
            ConnectionsWindowRequest::AbortProxyChain(message) => {
                self.cancel_active_proxy_connect_run(cx);
                self.report_proxy_session_tree_error(message, cx);
            }
            ConnectionsWindowRequest::FocusHostKey => {
                self.prepare_modal_interaction_boundary(cx);
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            ConnectionsWindowRequest::FocusAuthentication => {
                self.prepare_modal_interaction_boundary(cx);
                self.show_active_input_caret(cx);
                self.needs_active_pane_focus = false;
                window.focus(&self.focus_handle, cx);
                cx.notify();
            }
            ConnectionsWindowRequest::AdoptSftp(sftp) => self.adopt_connected_sftp(sftp, cx),
            ConnectionsWindowRequest::ConnectionStatus(status) => self.set_connection_status(status, cx),
            ConnectionsWindowRequest::GetSecurityKeyProvider(response) => {
                let provider = self.plugin_entity.update(cx, |plugins, _| plugins.security_key_provider());
                let _ = response.send(provider.ok_or_else(|| self.i18n.t("ssh.fido.errors.unavailable")));
            }
            ConnectionsWindowRequest::PasswordPrompt { node_id, prompt, response_tx } => self.open_password_prompt(node_id, prompt, response_tx, window, cx),
            ConnectionsWindowRequest::SaveAuthenticatedPassword { target, password } => self.save_authenticated_prompt_password(target, password, cx),
            ConnectionsWindowRequest::SaveConfirmedAuthentication { node_id, connection_id, configured_credentials_confirmed } => {
                self.save_confirmed_connection_auth(node_id, connection_id, configured_credentials_confirmed, cx);
            }
            ConnectionsWindowRequest::ToggleCatalogSidebarSearch => {
            self.prepare_modal_interaction_boundary(cx);
            if self.connections.session_manager.read(cx).sidebar_search_open {
                self.close_session_manager_sidebar_search(cx);
            } else {
                self.begin_disclosure_motion("session-manager-search".into(), true, cx);
                self.clear_ime_selection();
                self.selected_ime_target = Some(WorkspaceImeTarget::SessionManager(
                    super::SessionManagerInput::Search,
                ));
                self.connections.session_manager.update(cx, super::session_manager::SessionManagerState::open_sidebar_search);
                window.focus(&self.focus_handle, cx);
                self.show_active_input_caret(cx);
            }
            cx.stop_propagation();
            cx.notify();
            }
            ConnectionsWindowRequest::FocusForm => window.focus(&self.focus_handle, cx),
            ConnectionsWindowRequest::FocusSessionInput(input, event) => {
                self.connections.session_manager.update(cx, |manager, cx| {
                    manager.focused_input = Some(input);
                    cx.notify();
                });
                self.needs_active_pane_focus = false;
                self.ime_marked_text = None;
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(WorkspaceImeTarget::SessionManager(input), &event, window, cx);
            }
            ConnectionsWindowRequest::FocusFormSelect => {
                self.ime_marked_text = None;
                window.focus(&self.focus_handle, cx);
            }
            ConnectionsWindowRequest::FocusInput(input, event) => {
                let target = match input {
                    ConnectionsInput::Form(field) => {
                        self.connections.flow.update(cx, |flow, cx| {
                            flow.focus_form_input(field, cx);
                        });
                        WorkspaceImeTarget::NewConnection(field)
                    }
                    ConnectionsInput::KeyboardInteractive(index) => {
                        self.connections.flow.update(cx, |flow, cx| {
                            flow.focus_keyboard_interactive_prompt(index, cx);
                        });
                        WorkspaceImeTarget::KeyboardInteractive(index)
                    }
                };
                self.ime_marked_text = None;
                self.show_active_input_caret(cx);
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(target, &event, window, cx);
            }
            ConnectionsWindowRequest::DragInputSelection(event) => {
                self.update_ime_selection_drag_from_mouse_move(&event, window, cx);
            }
            ConnectionsWindowRequest::AcceptHostKey { persist } => {
                self.accept_host_key_challenge(persist, window, cx);
            }
            ConnectionsWindowRequest::CancelHostKey => {
                self.cancel_host_key_challenge(cx)
            }
            ConnectionsWindowRequest::SubmitAuthentication => {
                self.submit_keyboard_interactive_challenge(window, cx);
            }
            ConnectionsWindowRequest::CancelAuthentication => {
                self.cancel_keyboard_interactive_challenge(cx);
            }
            ConnectionsWindowRequest::CloseForm => {
                self.close_new_connection_form(window, cx)
            }
            ConnectionsWindowRequest::SubmitForm(action) => {
                self.submit_new_connection_form_with_action(action, window, cx)
            }
            ConnectionsWindowRequest::TestForm(intent) => {
                self.start_new_connection_flow(intent, window, cx)
            }
            ConnectionsWindowRequest::OpenTunnel => {
                self.open_ssh_tunnel_form(None, window, cx)
            }
            ConnectionsWindowRequest::LeaveTunnel => {
                self.open_new_connection_form(window, cx)
            }
            ConnectionsWindowRequest::CloseTunnel => self.close_ssh_tunnel_form(window, cx),
            ConnectionsWindowRequest::SaveTunnel { start } => self.save_ssh_tunnel(start, window, cx),
            ConnectionsWindowRequest::OpenPluginForm { plugin, type_id } => {
                self.ssh_tunnels.form = None;
                self.open_plugin_connection_form(&plugin, &type_id, None, window, cx);
            }
            ConnectionsWindowRequest::OpenMoshPlugin => {
                self.open_mosh_plugin(window, cx)
            }
            ConnectionsWindowRequest::OpenRemoteDesktopPlugin(protocol) => {
                self.open_remote_desktop_plugin(protocol, window, cx)
            }
            ConnectionsWindowRequest::ConnectNextHop {
                parent,
                connection_id,
            } => {
                self.connect_saved_connection_as_next_hop(
                    parent,
                    connection_id,
                    window,
                    cx,
                );
            }
            _ => unreachable!("application-only request was already dispatched"),
        }
    }

    fn connection_form_environment(&self, cx: &gpui::App) -> ConnectionFormEnvironment {
        use oxideterm_remote_desktop::RemoteDesktopProtocol;

        let settings = self.settings_store.settings();
        let plugins = self.plugin_entity.read(cx);
        ConnectionFormEnvironment {
            node_titles: self
                .ssh_nodes
                .iter()
                .map(|(id, node)| (id.clone(), node.title.clone()))
                .collect(),
            tunnel: self.ssh_tunnels.form.as_ref().map(|form| TunnelFormEnvironment {
                host_id: form.host_id.clone(),
                editing: form.saved.is_some(),
                startup: form.startup,
                kind: match form.kind {
                    ForwardType::Local => TunnelKind::Local,
                    ForwardType::Remote => TunnelKind::Remote,
                    ForwardType::Dynamic => TunnelKind::Dynamic,
                },
            }),
            local_shells: self.effective_local_shells_for_settings(settings),
            plugin_types: self
                .plugin_connection_types(cx)
                .into_iter()
                .map(|(plugin, t)| {
                    (
                        plugin,
                        FormPluginType {
                            id: t.id,
                            title: t.title,
                            icon: t.icon,
                        },
                    )
                })
                .collect(),
            installed_plugins: plugins
                .registry()
                .plugins()
                .iter()
                .map(|plugin| plugin.manifest.id.clone())
                .collect(),
            mosh_available: plugins.mosh_executable().is_some(),
            rdp_available: plugins.remote_desktop_plugin_enabled(RemoteDesktopProtocol::Rdp),
            vnc_available: plugins.remote_desktop_plugin_enabled(RemoteDesktopProtocol::Vnc),
            panel_glass: self.settings_background_active(),
            ..ConnectionFormEnvironment::from_store(&self.connection_store, settings)
        }
    }

    fn reveal_edit_saved_password(&self, cx: &mut gpui::Context<Self>) {
        self.connections.flow.update(cx, |flow, cx| flow.reveal_saved_password(&self.connection_store, cx));
    }
}
