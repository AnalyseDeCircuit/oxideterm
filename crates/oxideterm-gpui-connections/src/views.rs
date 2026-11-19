use std::{cell::RefCell, ops::Range, rc::Rc};

use gpui::{
    AnyElement, App, AppContext, CursorStyle, Entity, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, Window, prelude::*,
};
use oxideterm_gpui_ui::text_input::{
    TextInputAnchor, TextInputAnchorId, TextInputView, TextInputViewport, text_input_anchor_probe,
    text_input_with_viewport,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use zeroize::Zeroizing;

use crate::form_state::NewConnectionSubmitAction;
use crate::launch::SshConnectionIntent;
use crate::session_manager::{
    BatchWindowAction, CatalogAction, OxideDialogAction, SessionManagerInput, SessionManagerState,
    SessionManagerViewBindings,
    catalog_view::{CatalogMount, CatalogView},
    oxide_dialogs::{ManagerOverlay, OxideDialogs},
};
use crate::{
    NewConnectionField, NewConnectionForm, entity::ConnectionFlowEntity,
    form_state::connection_field_is_selected,
};

pub mod algorithms;
mod authentication;
mod delivery;
pub use delivery::{SftpConnected, SftpConnectedEndpoint};
mod form;
mod ftp;
pub use ftp::ConnectedFtp;
#[cfg(test)]
mod layout_tests;
mod select;
use authentication::AuthenticationView;
pub use form::{
    ConnectionFormEnvironment, FormKeyAction, FormPluginType, FormTotpCredential,
    NEW_CONNECTION_TYPE_SIDEBAR_WIDTH, TunnelFormEnvironment, TunnelKind,
};
use form::{ConnectionFormView, FormMount};
pub use select::select_anchor_id;

#[derive(Clone, Copy)]
pub enum ConnectionsInput {
    Form(NewConnectionField),
    KeyboardInteractive(usize),
}

pub enum ConnectionsWindowRequest {
    VerifiedSsh {
        config: oxideterm_ssh::SshConfig,
        title: String,
        intent: SshConnectionIntent,
    },
    PreflightFailed {
        intent: SshConnectionIntent,
        message: String,
        inline: bool,
    },
    AdvanceProxyChain,
    AbortProxyChain(String),
    ConnectProxyNode {
        node_id: oxideterm_ssh::NodeId,
        changed: bool,
    },
    FinishProxyChain {
        plan: oxideterm_ssh::NativeSessionTreeConnectPlan,
        terminal: Option<crate::proxy_launch::ReadyProxyTerminal>,
        report: Option<String>,
    },
    FocusHostKey,
    FocusAuthentication,
    AdoptSftp(SftpConnected),
    AdoptFtp(ConnectedFtp),
    ConnectionStatus(Option<String>),
    GetSecurityKeyProvider(
        tokio::sync::oneshot::Sender<
            Result<std::sync::Arc<oxideterm_ssh::SecurityKeyProvider>, String>,
        >,
    ),
    PasswordPrompt {
        node_id: Option<oxideterm_ssh::NodeId>,
        prompt: oxideterm_ssh::SshPasswordPrompt,
        response_tx: tokio::sync::oneshot::Sender<
            Result<oxideterm_ssh::SshPasswordResponse, oxideterm_ssh::SshPromptError>,
        >,
    },
    SaveAuthenticatedPassword {
        target: crate::saved_auth::SavedPasswordTarget,
        password: Zeroizing<String>,
    },
    SaveConfirmedAuthentication {
        node_id: oxideterm_ssh::NodeId,
        connection_id: String,
        configured_credentials_confirmed: bool,
    },
    FocusForm,
    ToggleCatalogSidebarSearch,
    FocusSessionInput(SessionManagerInput, MouseDownEvent),
    Oxide(OxideDialogAction),
    Catalog(CatalogAction),
    Batch(BatchWindowAction),
    FocusFormSelect,
    FocusInput(ConnectionsInput, MouseDownEvent),
    DragInputSelection(MouseMoveEvent),
    AcceptHostKey {
        persist: bool,
    },
    CancelHostKey,
    SubmitAuthentication,
    CancelAuthentication,
    ClearComposition,
    ShowCaret,
    CloseForm,
    SubmitForm(NewConnectionSubmitAction),
    TestForm(SshConnectionIntent),
    OpenTunnel,
    LeaveTunnel,
    CloseTunnel,
    SaveTunnel {
        start: bool,
    },
    SetTunnelKind(TunnelKind),
    ToggleTunnelStartup,
    SelectTunnelHost(String),
    OpenPluginForm {
        plugin: String,
        type_id: String,
    },
    OpenMoshPlugin,
    OpenRemoteDesktopPlugin(oxideterm_remote_desktop::RemoteDesktopProtocol),
    ConnectNextHop {
        parent: oxideterm_ssh::NodeId,
        connection_id: String,
    },
    RevealSavedPassword,
    QueueTooltip {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    ClearTooltip(String),
}

pub struct ConnectionImeFrame {
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    // The platform composition may contain a pending secret character.
    pub marked_text: Option<Zeroizing<String>>,
    pub marked_range: Option<Range<usize>>,
    pub notes_with_composition: Option<String>,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

pub type ConnectionsDispatch = Rc<dyn Fn(ConnectionsWindowRequest, Option<&mut Window>, &mut App)>;
pub type ConnectionsEnvironmentSource = Rc<dyn Fn(&App) -> Option<ConnectionsEnvironment>>;
pub type ConnectionImeSource = Rc<dyn Fn(ConnectionsInput, &App) -> Option<ConnectionImeFrame>>;
pub type ConnectionsSelectableText = Rc<dyn Fn(&str, &str, bool, u32, &mut App) -> AnyElement>;

pub struct ConnectionsEnvironment {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
}

#[derive(Clone)]
pub struct ConnectionsViewBindings {
    pub environment: ConnectionsEnvironmentSource,
    pub ime: ConnectionImeSource,
    pub selectable_text: ConnectionsSelectableText,
    pub dispatch: ConnectionsDispatch,
    pub form_environment: Rc<dyn Fn(&App) -> Option<ConnectionFormEnvironment>>,
    pub form_selectable_text: Rc<dyn Fn(&str, (&str, &str), String, u32, &mut App) -> AnyElement>,
    pub manager: SessionManagerViewBindings,
}

pub fn connection_text_input(
    tokens: &ThemeTokens,
    dispatch: &ConnectionsDispatch,
    target: ConnectionsInput,
    frame: ConnectionImeFrame,
    view: TextInputView<'_>,
) -> AnyElement {
    let input = text_input_with_viewport(
        tokens,
        TextInputView {
            caret_visible: frame.caret_visible,
            selected_range: frame.selected_range,
            marked_text: frame.marked_text.as_ref().map(|text| text.as_str()),
            ..view
        },
        &frame.viewport,
        frame.active_offset,
    );
    bind_connection_input(
        input,
        target,
        frame.anchor_id,
        frame.update_anchor,
        dispatch,
    )
}

fn bind_connection_input(
    input: gpui::Div,
    target: ConnectionsInput,
    anchor_id: TextInputAnchorId,
    update_anchor: Rc<dyn Fn(TextInputAnchor)>,
    dispatch: &ConnectionsDispatch,
) -> AnyElement {
    let focus = dispatch.clone();
    let drag = dispatch.clone();
    let id = match target {
        ConnectionsInput::Form(field) => ("connection-field", field as usize),
        ConnectionsInput::KeyboardInteractive(index) => ("kbi-prompt", index),
    };
    let input = match target {
        ConnectionsInput::Form(_) => input.cursor(CursorStyle::IBeam),
        ConnectionsInput::KeyboardInteractive(_) => input,
    };
    text_input_anchor_probe(
        anchor_id,
        input
            .id(id)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    focus(
                        ConnectionsWindowRequest::FocusInput(target, event.clone()),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                drag(
                    ConnectionsWindowRequest::DragInputSelection(event.clone()),
                    Some(window),
                    cx,
                );
            }),
        move |anchor, _, _| update_anchor(anchor),
    )
    .into_any_element()
}

pub struct ConnectionsViews {
    pub flow: Entity<ConnectionFlowEntity>,
    pub session_manager: Entity<SessionManagerState>,
    oxide_dialogs: Entity<OxideDialogs>,
    batch_dialog: Entity<OxideDialogs>,
    batch_menus: Entity<OxideDialogs>,
    session_manager_catalog: Entity<CatalogView>,
    session_manager_sidebar_rows: Entity<CatalogView>,
    session_manager_dialogs: Entity<CatalogView>,
    session_manager_sidebar_search: Entity<CatalogView>,
    session_manager_sidebar_search_button: Entity<CatalogView>,
    authentication: Entity<AuthenticationView>,
    form_body: Entity<ConnectionFormView>,
    form_overlays: Entity<ConnectionFormView>,
    dispatch: ConnectionsDispatch,
}

impl ConnectionsViews {
    pub fn new(
        bindings: ConnectionsViewBindings,
        environment: ConnectionsEnvironment,
        runtime: tokio::runtime::Handle,
        cx: &mut App,
    ) -> Self {
        let flow = cx.new(ConnectionFlowEntity::new);
        let session_manager = cx.new(SessionManagerState::new);
        let dispatch = bindings.dispatch.clone();
        let form_body = cx.new(|cx| {
            ConnectionFormView::new(
                flow.clone(),
                &bindings,
                &environment,
                runtime.clone(),
                FormMount::Body,
                cx,
            )
        });
        let form_overlays = cx.new(|cx| {
            ConnectionFormView::new(
                flow.clone(),
                &bindings,
                &environment,
                runtime.clone(),
                FormMount::Overlays,
                cx,
            )
        });
        let authentication =
            cx.new(|cx| AuthenticationView::new(flow.clone(), bindings.clone(), cx));
        let mut new_catalog = |mount| {
            cx.new(|cx| {
                CatalogView::new(
                    mount,
                    session_manager.clone(),
                    bindings.environment.clone(),
                    &bindings.manager,
                    dispatch.clone(),
                    cx,
                )
            })
        };
        let session_manager_catalog = new_catalog(CatalogMount::Main);
        let session_manager_sidebar_rows = new_catalog(CatalogMount::Rows);
        let session_manager_dialogs = new_catalog(CatalogMount::Dialogs);
        let session_manager_sidebar_search = new_catalog(CatalogMount::SidebarSearch);
        let session_manager_sidebar_search_button = new_catalog(CatalogMount::SidebarSearchButton);
        let batch_anchors = Rc::new(RefCell::new(Default::default()));
        let mut new_overlay = |mount| {
            cx.new(|cx| {
                OxideDialogs::new(
                    mount,
                    session_manager.clone(),
                    bindings.environment.clone(),
                    bindings.manager.clone(),
                    dispatch.clone(),
                    cx,
                    batch_anchors.clone(),
                )
            })
        };
        let oxide_dialogs = new_overlay(ManagerOverlay::Oxide);
        let batch_dialog = new_overlay(ManagerOverlay::Batch);
        let batch_menus = new_overlay(ManagerOverlay::BatchMenus);
        Self {
            flow,
            session_manager,
            oxide_dialogs,
            batch_dialog,
            batch_menus,
            session_manager_catalog,
            session_manager_sidebar_rows,
            session_manager_dialogs,
            session_manager_sidebar_search,
            session_manager_sidebar_search_button,
            authentication,
            form_body,
            form_overlays,
            dispatch,
        }
    }

    pub fn text_input(
        &self,
        tokens: &ThemeTokens,
        form: Option<&NewConnectionForm>,
        field: NewConnectionField,
        frame: ConnectionImeFrame,
        value: &str,
        placeholder: String,
        secret: bool,
    ) -> AnyElement {
        connection_text_input(
            tokens,
            &self.dispatch,
            ConnectionsInput::Form(field),
            frame,
            TextInputView {
                value,
                placeholder,
                focused: form.is_some_and(|form| form.field_focused && form.focused_field == field),
                selected_all: form.is_some_and(|form| connection_field_is_selected(form, field)),
                secret,
                caret_visible: false,
                selected_range: None,
                marked_text: None,
            },
        )
    }

    pub fn authentication(&self) -> AnyElement {
        self.authentication.clone().into_any_element()
    }

    pub fn form_body(&self) -> AnyElement {
        self.form_body.clone().into_any_element()
    }

    pub fn oxide_dialogs(&self) -> AnyElement {
        self.oxide_dialogs.clone().into_any_element()
    }

    pub fn batch_dialog(&self) -> AnyElement {
        self.batch_dialog.clone().into_any_element()
    }

    pub fn batch_menus(&self) -> AnyElement {
        self.batch_menus.clone().into_any_element()
    }

    pub fn session_manager_catalog(&self) -> AnyElement {
        self.session_manager_catalog.clone().into_any_element()
    }

    pub fn session_manager_sidebar_rows(&self) -> AnyElement {
        self.session_manager_sidebar_rows.clone().into_any_element()
    }

    pub fn session_manager_dialogs(&self) -> AnyElement {
        self.session_manager_dialogs.clone().into_any_element()
    }

    pub fn session_manager_sidebar_search_button(&self) -> AnyElement {
        self.session_manager_sidebar_search_button.clone().into_any_element()
    }

    pub fn session_manager_sidebar_search(&self) -> AnyElement {
        self.session_manager_sidebar_search
            .clone()
            .into_any_element()
    }

    pub fn notify_manager_input(&self, input: SessionManagerInput, cx: &mut App) {
        match input {
            SessionManagerInput::Search => {
                cx.notify(self.session_manager_catalog.entity_id());
                cx.notify(self.session_manager_sidebar_search.entity_id());
            }
            SessionManagerInput::GroupName => cx.notify(self.session_manager_dialogs.entity_id()),
            SessionManagerInput::BatchPassword
            | SessionManagerInput::BatchKeyPath
            | SessionManagerInput::BatchTemplateName => {
                cx.notify(self.batch_dialog.entity_id());
                cx.notify(self.batch_menus.entity_id());
            }
            _ => cx.notify(self.oxide_dialogs.entity_id()),
        }
    }

    pub fn form_overlays(&self) -> AnyElement {
        self.form_overlays.clone().into_any_element()
    }

    pub fn refresh_serial_ports(&self, cx: &mut App) {
        self.form_body
            .update(cx, |view, cx| view.refresh_serial_ports(cx));
    }

    pub fn refresh_ble_adapters(&self, cx: &mut App) {
        self.form_body
            .update(cx, |view, cx| view.refresh_ble_adapters(cx));
    }

    pub fn finish_jump_form(&self, commit: bool, tokens: &ThemeTokens, cx: &mut App) {
        let delay = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.flow.update(cx, |flow, cx| {
            flow.begin_jump_server_form_exit(commit, delay, cx)
        });
    }

    pub fn form_scroll(&self, id: &str, cx: &App) -> gpui::ScrollHandle {
        self.form_body.read(cx).selectable_text_scroll_handle(id)
    }

    pub fn notify_form_input(&self, cx: &mut App) {
        cx.notify(self.form_body.entity_id());
        cx.notify(self.form_overlays.entity_id());
    }

    pub fn ssh_algorithms<'a>(
        &'a self,
        tokens: &'a ThemeTokens,
        i18n: &'a I18n,
    ) -> algorithms::SshAlgorithmEditor<'a> {
        algorithms::SshAlgorithmEditor::new(&self.flow, tokens, i18n, &self.dispatch)
    }

    pub fn notify_input(&self, cx: &mut App) {
        self.authentication.update(cx, |_, cx| cx.notify());
    }
}
