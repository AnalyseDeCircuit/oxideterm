//! Mount the Studio as a GPUI entity and subscribe to its intents. The host alone
//! decides whether a validated submission becomes a saved profile or a live session.
use super::{
    ConnectionFormEnvironment, ConnectionImeSource, ConnectionsEnvironment, ConnectionsInput,
    FormAction, FormDispatch, FormKeyAction, TunnelFormEnvironment, TunnelKind,
    algorithms::FormAlgorithmCapabilities,
    form::{ConnectionFormView, FormMount},
};
use crate::{
    ConnectionFormState, NewConnectionField, NewConnectionForm,
    form_entity::ConnectionSelectAnchorStore,
    form_model::SerialPortInfo,
    form_state::{NewConnectionSubmitAction, NewConnectionTransport},
};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, Render, Window, div, prelude::*,
};
use std::{cell::RefCell, rc::Rc};

mod draft;
mod input;
pub use draft::*;
use input::FormInput;

/// Host-supplied presentation, saved-profile metadata, and available connection capabilities.
pub struct PortableConnectionFormEnvironment {
    pub theme: ConnectionsEnvironment,
    pub form: ConnectionFormEnvironment,
    pub algorithms: FormAlgorithmCapabilities,
    pub serial_ports: Vec<SerialPortInfo>,
    pub agent_available: Option<bool>,
    pub kerberos_available: Option<bool>,
}
impl PortableConnectionFormEnvironment {
    pub fn new(theme: ConnectionsEnvironment) -> Self {
        Self {
            theme,
            form: Default::default(),
            algorithms: Default::default(),
            serial_ports: Vec::new(),
            agent_available: None,
            kerberos_available: None,
        }
    }
}
pub enum PortableConnectionFormEvent {
    Save(PortableConnectionDraft),
    Connect(PortableConnectionDraft),
    SaveAndConnect(PortableConnectionDraft),
    Test(PortableConnectionDraft),
    RequestPathPicker(NewConnectionField),
    OpenPlugin {
        plugin: String,
        type_id: String,
    },
    OpenMoshPlugin,
    OpenRemoteDesktopPlugin(oxideterm_remote_desktop::RemoteDesktopProtocol),
    RevealSavedPassword(String),
    #[cfg(target_family = "wasm")]
    RequestBleScan,
    Cancel,
}
/// Mount with `cx.new` and subscribe to [`PortableConnectionFormEvent`] on the returned entity.
/// Each instance owns its draft, focus, selection, and overlays independently.
pub struct PortableConnectionFormView {
    state: Entity<ConnectionFormState>,
    body: Entity<ConnectionFormView>,
    overlays: Entity<ConnectionFormView>,
    focus: FocusHandle,
    input: FormInput,
    environment: Rc<RefCell<Rc<ConnectionFormEnvironment>>>,
    algorithms: Rc<FormAlgorithmCapabilities>,
    theme: ConnectionsEnvironment,
    tooltip: Option<(String, String, f32, f32)>,
}
impl EventEmitter<PortableConnectionFormEvent> for PortableConnectionFormView {}
impl Focusable for PortableConnectionFormView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl PortableConnectionFormView {
    pub fn new(
        transport: NewConnectionTransport,
        env: PortableConnectionFormEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_form(NewConnectionForm::for_transport(transport), env, cx)
    }
    pub fn from_form(
        mut form: NewConnectionForm,
        env: PortableConnectionFormEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        form.agent_available = env.agent_available;
        form.standalone_sftp_secondary.agent_available = env.agent_available;
        form.gssapi_credentials_available = env.kerberos_available;
        form.serial_ports = env.serial_ports.clone();
        let state = cx.new(|cx| {
            let mut state = ConnectionFormState::new(cx.focus_handle());
            state.replace_with_new_form(form);
            state
        });
        let environment = Rc::new(RefCell::new(Rc::new(env.form)));
        let algorithms = Rc::new(env.algorithms);
        let anchors = ConnectionSelectAnchorStore::default();
        let owner = cx.weak_entity();
        let dispatch: FormDispatch = Rc::new(move |request, window, cx| {
            let _ = owner.update(cx, |view, cx| view.dispatch(request, window, cx));
        });
        let owner = cx.weak_entity();
        let ime: ConnectionImeSource = Rc::new(move |input, cx| {
            let owner = owner.upgrade()?;
            let view = owner.read(cx);
            view.input.frame(input, view.state.read(cx))
        });
        let owner = cx.weak_entity();
        let picker = Rc::new(
            move |field: NewConnectionField, _: &oxideterm_i18n::I18n, cx: &mut App| {
                let _ = owner.update(cx, |_, cx| {
                    cx.emit(PortableConnectionFormEvent::RequestPathPicker(field))
                });
            },
        );
        let mut mount = |kind| {
            let owner = cx.weak_entity();
            cx.new(|cx| {
                ConnectionFormView::portable(
                    state.clone(),
                    anchors.clone(),
                    kind,
                    env.theme.clone(),
                    Rc::new(move |cx| Some(owner.upgrade()?.read(cx).theme.clone())),
                    environment.clone(),
                    algorithms.clone(),
                    env.serial_ports.clone(),
                    ime.clone(),
                    dispatch.clone(),
                    picker.clone(),
                    cx,
                )
            })
        };
        let body = mount(FormMount::Body);
        let overlays = mount(FormMount::Overlays);
        cx.observe(&state, |view, state, cx| {
            let active = state
                .read(cx)
                .form
                .as_ref()
                .filter(|form| form.field_focused)
                .map(|form| form.focused_field);
            view.input.leave_other_field(active);
        })
        .detach();
        Self {
            state,
            body,
            overlays,
            focus: cx.focus_handle(),
            input: FormInput::default(),
            environment,
            algorithms,
            theme: env.theme,
            tooltip: None,
        }
    }

    pub fn set_environment(&mut self, env: ConnectionsEnvironment, cx: &mut Context<Self>) {
        self.theme = env;
        self.refresh(cx);
    }

    pub fn begin_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let started = self.state.update(cx, |state, cx| {
            let started = state.presence.begin_exit().is_some();
            if started {
                cx.notify();
            }
            started
        });
        if started {
            self.refresh(cx);
        }
        started
    }

    #[cfg(target_family = "wasm")]
    pub fn set_web_ble_scanning(
        &mut self,
        scanning: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            if let Some(form) = state.form.as_mut() {
                form.ble.scanning = scanning;
                form.ble.scan_started = true;
                form.ble.inspect_error = error;
            }
            cx.notify();
        });
        self.refresh(cx);
    }

    #[cfg(target_family = "wasm")]
    pub fn set_web_ble_device(
        &mut self,
        id: String,
        name: String,
        services: Vec<crate::form_state::WebBleServiceSummary>,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            if let Some(form) = state.form.as_mut() {
                form.ble.scanning = false;
                form.ble.scan_started = true;
                form.ble.inspect_error = None;
                form.ble.web_device_id = Some(id);
                if form.ble.profile_name.trim().is_empty() {
                    form.ble.profile_name = name.clone();
                }
                form.ble.web_device_name = Some(name);
                let preferred = services
                    .iter()
                    .find(|s| (s.has_write || s.has_write_without_response) && s.has_notify)
                    .or_else(|| services.first())
                    .map(|s| s.uuid);
                form.ble.selected_service_uuid = preferred;
                form.ble.web_services = services;
                form.error = None;
            }
            cx.notify();
        });
        self.refresh(cx);
    }

    /// Edit a saved SSH record without fetching its protected password.
    pub fn edit_saved_ssh(
        id: String,
        form: NewConnectionForm,
        env: PortableConnectionFormEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self::from_form(form, env, cx);
        view.state.update(cx, |state, cx| {
            state.editing_saved_connection_id = Some(id);
            cx.notify();
        });
        view
    }
    pub fn duplicate_ssh(
        source_id: String,
        mut form: NewConnectionForm,
        env: PortableConnectionFormEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        form.focused_field = NewConnectionField::Name;
        form.field_focused = true;
        let view = Self::from_form(form, env, cx);
        view.state.update(cx, |state, cx| {
            state.duplicating_saved_connection_id = Some(source_id);
            cx.notify();
        });
        view
    }
    pub fn form<'a>(&self, cx: &'a App) -> Option<&'a NewConnectionForm> {
        self.state.read(cx).form.as_ref()
    }
    /// Deliver a reveal only to the record that requested it. Edits supersede a pending reveal.
    pub fn reveal_password(
        &mut self,
        id: &str,
        mut value: zeroize::Zeroizing<String>,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            if state.editing_saved_connection_id.as_deref() != Some(id) {
                return;
            }
            let Some(form) = state.form.as_mut() else {
                return;
            };
            if form.password_loaded || form.saved_password_keychain_id.is_none() {
                return;
            }
            *crate::form_state::password_draft_mut(form) = std::mem::take(&mut *value);
            form.password_from_store = true;
            form.password_visible = true;
            cx.notify();
        });
    }
    pub fn apply_path(&mut self, field: NewConnectionField, path: String, cx: &mut Context<Self>) {
        use NewConnectionField::*;
        self.state.update(cx, |s, cx| {
            let Some(form) = s.form.as_mut() else { return };
            let target = match field {
                KeyPath => &mut form.key_path,
                CertPath => &mut form.cert_path,
                StandaloneSftpSecondaryKeyPath => &mut form.standalone_sftp_secondary.key_path,
                StandaloneSftpSecondaryCertPath => &mut form.standalone_sftp_secondary.cert_path,
                JumpKeyPath | JumpCertPath => {
                    let Some(hop) = form.jump_server_form.as_mut() else {
                        return;
                    };
                    if field == JumpKeyPath {
                        &mut hop.key_path
                    } else {
                        &mut hop.cert_path
                    }
                }
                _ => return,
            };
            *target = path;
            form.error = None;
            cx.notify();
        });
    }

    fn submit(
        &mut self,
        event: fn(PortableConnectionDraft) -> PortableConnectionFormEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form(cx) else { return };
        if form.pending {
            return;
        }
        if self
            .environment
            .borrow()
            .browser
            .is_some_and(|caps| !caps.supports(form.transport))
        {
            return;
        }
        let saved_id = self.state.read(cx).editing_saved_connection_id.clone();
        match PortableConnectionDraft::snapshot(form, &self.environment.borrow(), saved_id) {
            Ok(draft) => cx.emit(event(draft)),
            Err(key) => {
                let message = self.theme.i18n.t(key);
                self.state.update(cx, |state, cx| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(message)
                    }
                    cx.notify();
                });
            }
        }
    }
    fn fresh_form(&self, cx: &App) -> NewConnectionForm {
        let mut form = NewConnectionForm::default();
        if let Some(previous) = self.form(cx) {
            form.agent_available = previous.agent_available;
            form.standalone_sftp_secondary.agent_available =
                previous.standalone_sftp_secondary.agent_available;
            form.gssapi_credentials_available = previous.gssapi_credentials_available;
            form.serial_ports = previous.serial_ports.clone();
        }
        form
    }

    fn dispatch(
        &mut self,
        action: FormAction,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        use FormAction::*;
        use PortableConnectionFormEvent as Event;
        if self.environment.borrow().browser.is_some()
            && matches!(
                &action,
                TestForm { .. }
                    | RevealSavedPassword
                    | OpenTunnel
                    | SaveTunnel { .. }
                    | OpenPluginForm { .. }
                    | OpenMoshPlugin
                    | OpenRemoteDesktopPlugin(_)
            )
        {
            return;
        }
        match action {
            FocusInput(ConnectionsInput::Form(field), event) => {
                self.state.update(cx, |s, cx| {
                    if let Some(f) = s.form.as_mut() {
                        f.focused_field = field;
                        f.field_focused = true;
                        f.selected_field = None;
                    }
                    cx.notify();
                });
                if let Some(w) = window {
                    self.input
                        .click(field, &event, self.state.read(cx), &self.theme.tokens, w);
                    w.focus(&self.focus, cx)
                }
            }
            DragInputSelection(event) if event.pressed_button == Some(gpui::MouseButton::Left) => {
                if let Some(w) = window {
                    self.input
                        .drag(event.position, self.state.read(cx), &self.theme.tokens, w);
                }
            }
            FocusForm | FocusFormSelect => {
                if let Some(w) = window {
                    w.focus(&self.focus, cx)
                }
            }
            ClearComposition => self.input.clear(),
            ShowCaret => {}
            SubmitForm(NewConnectionSubmitAction::Save) => self.submit(Event::Save, cx),
            SubmitForm(NewConnectionSubmitAction::Connect) => self.submit(Event::Connect, cx),
            SubmitForm(NewConnectionSubmitAction::SaveAndConnect) => {
                self.submit(Event::SaveAndConnect, cx)
            }
            TestForm { standalone_sftp } => {
                if self.form(cx).is_some_and(|f| {
                    (f.transport == NewConnectionTransport::StandaloneSftp) == standalone_sftp
                }) {
                    self.submit(Event::Test, cx);
                }
            }
            RevealSavedPassword => {
                if let Some(id) = self.state.read(cx).editing_saved_connection_id.clone() {
                    cx.emit(Event::RevealSavedPassword(id));
                }
            }
            #[cfg(target_family = "wasm")]
            RequestBleScan => {
                if self
                    .environment
                    .borrow()
                    .browser
                    .is_none_or(|caps| caps.bluetooth)
                {
                    cx.emit(Event::RequestBleScan);
                }
            }
            QueueTooltip { id, label, x, y } => self.tooltip = Some((id, label, x, y)),
            ClearTooltip(id) => {
                if self.tooltip.as_ref().is_some_and(|t| t.0 == id) {
                    self.tooltip = None;
                }
            }
            CloseForm | CloseTunnel => cx.emit(Event::Cancel),
            OpenTunnel => {
                self.input.clear();
                let mut draft = self.fresh_form(cx);
                draft.host = "127.0.0.1".into();
                draft.port.clear();
                draft.tunnel_target_host = "127.0.0.1".into();
                draft.advanced_connections_expanded = true;
                self.state.update(cx, |s, cx| {
                    s.replace_with_new_form(draft);
                    cx.notify();
                });
                Rc::make_mut(&mut *self.environment.borrow_mut()).tunnel =
                    Some(TunnelFormEnvironment {
                        host_id: None,
                        editing: false,
                        kind: TunnelKind::Local,
                        startup: false,
                    })
            }
            LeaveTunnel => {
                Rc::make_mut(&mut *self.environment.borrow_mut()).tunnel = None;
                self.input.clear();
                let draft = self.fresh_form(cx);
                self.state.update(cx, |s, cx| {
                    s.replace_with_new_form(draft);
                    cx.notify();
                });
            }
            SetTunnelKind(kind) => {
                if let Some(t) = &mut Rc::make_mut(&mut *self.environment.borrow_mut()).tunnel {
                    t.kind = kind
                }
            }
            SelectTunnelHost(id) => {
                if let Some(t) = &mut Rc::make_mut(&mut *self.environment.borrow_mut()).tunnel {
                    t.host_id = Some(id)
                }
            }
            ToggleTunnelStartup => {
                if let Some(t) = &mut Rc::make_mut(&mut *self.environment.borrow_mut()).tunnel {
                    t.startup = !t.startup
                }
            }
            SaveTunnel { start: false } => self.submit(Event::Save, cx),
            SaveTunnel { start: true } => self.submit(Event::SaveAndConnect, cx),
            OpenPluginForm { plugin, type_id } => cx.emit(Event::OpenPlugin { plugin, type_id }),
            OpenMoshPlugin => cx.emit(Event::OpenMoshPlugin),
            OpenRemoteDesktopPlugin(p) => cx.emit(Event::OpenRemoteDesktopPlugin(p)),
            _ => return,
        }
        self.refresh(cx);
    }
    fn refresh(&self, cx: &mut Context<Self>) {
        cx.notify();
        App::notify(cx, self.body.entity_id());
        App::notify(cx, self.overlays.entity_id());
    }
    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.marked.is_some() {
            return;
        }
        if self.input_key(e, window, cx) {
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        let key = e.keystroke.key.as_str();
        if !matches!(key, "escape" | "enter" | "tab" | "up" | "down" | "space") {
            return;
        }
        if self.active_field(cx).is_some() && key == "space" {
            return;
        }
        let fields = self.environment.borrow().tunnel.as_ref().map(|t| {
            use NewConnectionField::*;
            let mut fields = vec![Name, Host, Port];
            if t.kind != TunnelKind::Dynamic {
                fields.extend([TunnelTargetHost, TunnelTargetPort]);
            }
            fields
        }).or_else(|| {
            self.environment.borrow().browser?;
            let form = self.form(cx)?;
            use crate::form_state::{ConnectionFormDrawer as Drawer, NewConnectionField::*};
            Some(match (form.transport, form.active_drawer) {
                (NewConnectionTransport::Ssh, None) => vec![Name, Username, Host, Port],
                (NewConnectionTransport::StandaloneSftp, None) => vec![Name],
                (NewConnectionTransport::StandaloneSftp, Some(Drawer::StandaloneSftpPrimary)) => vec![Username, Host, Port, InitialRemotePath],
                (NewConnectionTransport::StandaloneSftp, Some(Drawer::StandaloneSftpSecondary)) => vec![StandaloneSftpSecondaryUsername, StandaloneSftpSecondaryHost, StandaloneSftpSecondaryPort, StandaloneSftpSecondaryInitialRemotePath],
                (NewConnectionTransport::Ssh | NewConnectionTransport::StandaloneSftp, Some(Drawer::Notes)) => vec![Color, IconBackgroundColor, Notes],
                (NewConnectionTransport::Ssh, Some(Drawer::SshSecurity)) => vec![],
                _ => return None,
            })
        });
        let result = self.state.update(cx, |s, cx| {
            if super::algorithms::handle_key(s, &self.algorithms, e, cx) {
                cx.notify();
                return FormKeyAction::Handled;
            }
            s.handle_key(e, true, fields.as_deref(), cx).0
        });
        match result {
            FormKeyAction::CloseForm => cx.emit(PortableConnectionFormEvent::Cancel),
            FormKeyAction::Submit => {
                let state = self.state.read(cx);
                let event = if self.environment.borrow().tunnel.is_some() {
                    PortableConnectionFormEvent::SaveAndConnect
                } else if state.editing_saved_connection_id.is_some()
                    || state.duplicating_saved_connection_id.is_some()
                    || state.form.as_ref().is_some_and(|f| {
                        f.remote_desktop_profile_id.is_some()
                            || f.mosh_profile_id.is_some()
                            || f.serial_profile_id.is_some()
                            || f.telnet_profile_id.is_some()
                            || f.ftp_profile_id.is_some()
                            || f.standalone_sftp_profile_id.is_some()
                            || f.local_profile_id.is_some()
                    })
                {
                    PortableConnectionFormEvent::Save
                } else if state
                    .form
                    .as_ref()
                    .is_some_and(|f| f.standalone_connection_id.is_some())
                {
                    PortableConnectionFormEvent::Connect
                } else {
                    PortableConnectionFormEvent::SaveAndConnect
                };
                self.submit(event, cx);
            }
            FormKeyAction::CloseJumpForm | FormKeyAction::AddJumpServer => {
                let commit = result == FormKeyAction::AddJumpServer;
                self.state.update(cx, |s, cx| {
                    if commit
                        && !s
                            .form
                            .as_ref()
                            .and_then(|f| f.jump_server_form.as_ref())
                            .is_some_and(|h| h.complete())
                    {
                        if let Some(f) = s.form.as_mut() {
                            f.error = Some(self.theme.i18n.t("ssh.form.proxy_jump_required"));
                        }
                    } else {
                        s.finish_jump_edit(commit);
                    }
                    cx.notify();
                });
            }
            FormKeyAction::NotHandled => return,
            _ => {}
        }
        self.input.clear();
        window.notify_text_input_changed(&self.focus);
        window.prevent_default();
        self.refresh(cx);
        cx.stop_propagation();
    }
}
impl Render for PortableConnectionFormView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let owner = cx.entity();
        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(self.body.clone())
            .child(self.overlays.clone())
            .children(self.tooltip.as_ref().map(|(_, label, x, y)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(gpui::point(gpui::px(*x), gpui::px(*y)))
                        .position_mode(gpui::AnchoredPositionMode::Window)
                        .child(oxideterm_gpui_ui::tooltip::tooltip_view(
                            self.theme.tokens,
                            label.clone(),
                            None,
                            cx,
                        )),
                )
            }))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, owner),
                            cx,
                        )
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}
