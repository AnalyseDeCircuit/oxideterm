use std::{cell::RefCell, rc::Rc};

use super::{ConnectedFtp, SftpConnected, TunnelKind};
use crate::{
    form_state::NewConnectionSubmitAction,
    launch::SshConnectionIntent,
    session_manager::{BatchWindowAction, CatalogAction, OxideDialogAction},
};
use gpui::{AnyElement, App, AppContext, Entity, IntoElement};
use gpui::{MouseDownEvent, MouseMoveEvent, Window};
use oxideterm_gpui_ui::text_input::TextInputView;
#[cfg(test)]
use oxideterm_gpui_ui::text_input::{TextInputAnchorId, TextInputViewport};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use zeroize::Zeroizing;

use crate::session_manager::{
    SessionManagerInput, SessionManagerState, SessionManagerViewBindings,
    catalog_view::{CatalogMount, CatalogView},
    oxide_dialogs::{ManagerOverlay, OxideDialogs},
};
use crate::{
    NewConnectionField, NewConnectionForm, entity::ConnectionFlowEntity,
    form_state::connection_field_is_selected,
};

use super::{
    ConnectionsEnvironment, ConnectionsSelectableText, algorithms,
    authentication::AuthenticationView,
    form::{ConnectionFormEnvironment, ConnectionFormView, FormMount},
    form_input::*,
};
#[cfg(test)]
#[path = "layout_tests.rs"]
mod layout_tests;

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

pub type ConnectionsDispatch = Rc<dyn Fn(ConnectionsWindowRequest, Option<&mut Window>, &mut App)>;

pub(super) fn form_dispatch(dispatch: &ConnectionsDispatch) -> FormDispatch {
    let dispatch = dispatch.clone();
    Rc::new(move |action, window, cx| {
        let request = match action {
            FormAction::FocusForm => ConnectionsWindowRequest::FocusForm,
            FormAction::FocusFormSelect => ConnectionsWindowRequest::FocusFormSelect,
            FormAction::FocusInput(a, b) => ConnectionsWindowRequest::FocusInput(a, b),
            FormAction::DragInputSelection(a) => ConnectionsWindowRequest::DragInputSelection(a),
            FormAction::ClearComposition => ConnectionsWindowRequest::ClearComposition,
            FormAction::ShowCaret => ConnectionsWindowRequest::ShowCaret,
            FormAction::CloseForm => ConnectionsWindowRequest::CloseForm,
            FormAction::SubmitForm(a) => ConnectionsWindowRequest::SubmitForm(a),
            FormAction::OpenTunnel => ConnectionsWindowRequest::OpenTunnel,
            FormAction::LeaveTunnel => ConnectionsWindowRequest::LeaveTunnel,
            FormAction::CloseTunnel => ConnectionsWindowRequest::CloseTunnel,
            FormAction::SaveTunnel { start } => ConnectionsWindowRequest::SaveTunnel { start },
            FormAction::SetTunnelKind(a) => ConnectionsWindowRequest::SetTunnelKind(a),
            FormAction::ToggleTunnelStartup => ConnectionsWindowRequest::ToggleTunnelStartup,
            FormAction::SelectTunnelHost(a) => ConnectionsWindowRequest::SelectTunnelHost(a),
            FormAction::OpenPluginForm { plugin, type_id } => {
                ConnectionsWindowRequest::OpenPluginForm { plugin, type_id }
            }
            FormAction::OpenMoshPlugin => ConnectionsWindowRequest::OpenMoshPlugin,
            FormAction::OpenRemoteDesktopPlugin(a) => {
                ConnectionsWindowRequest::OpenRemoteDesktopPlugin(a)
            }
            FormAction::ConnectNextHop {
                parent,
                connection_id,
            } => ConnectionsWindowRequest::ConnectNextHop {
                parent,
                connection_id,
            },
            FormAction::RevealSavedPassword => ConnectionsWindowRequest::RevealSavedPassword,
            FormAction::QueueTooltip { id, label, x, y } => {
                ConnectionsWindowRequest::QueueTooltip { id, label, x, y }
            }
            FormAction::ClearTooltip(a) => ConnectionsWindowRequest::ClearTooltip(a),
            FormAction::TestForm { standalone_sftp } => {
                ConnectionsWindowRequest::TestForm(if standalone_sftp {
                    SshConnectionIntent::TestStandaloneSftp
                } else {
                    SshConnectionIntent::Test
                })
            }
        };
        dispatch(request, window, cx);
    })
}

pub fn connection_text_input(
    tokens: &ThemeTokens,
    dispatch: &ConnectionsDispatch,
    target: ConnectionsInput,
    frame: ConnectionImeFrame,
    view: TextInputView<'_>,
) -> AnyElement {
    form_text_input(tokens, &form_dispatch(dispatch), target, frame, view)
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
    pub(super) dispatch: ConnectionsDispatch,
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
        self.session_manager_sidebar_search_button
            .clone()
            .into_any_element()
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
        algorithms::SshAlgorithmEditor::new(
            super::form::FormOwner::Desktop(self.flow.clone()),
            Rc::new(algorithms::FormAlgorithmCapabilities::desktop()),
            tokens,
            i18n,
            form_dispatch(&self.dispatch),
        )
    }

    pub fn notify_input(&self, cx: &mut App) {
        self.authentication.update(cx, |_, cx| cx.notify());
    }
}
