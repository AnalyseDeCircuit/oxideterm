use std::rc::Rc;

use gpui::{AnyElement, App};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

mod dialog;
mod host_key;
mod keyboard_interactive;
pub(crate) mod line_edit;
mod select;

pub use crate::session_manager::{PortableCatalogData, PortableCatalogEvent, PortableCatalogView};
pub use host_key::{HostKeyDecision, HostKeyFrame, HostKeyPresentation, PortableHostKeyDialog};
pub use keyboard_interactive::{
    KeyboardInteractiveDecision, KeyboardInteractiveFrame, KeyboardInteractivePrompt,
    PortableKeyboardInteractiveDialog,
};
pub use select::{NewConnectionSelect, select_anchor_id};

/// Render a host-key label (true) or its value (false) with the host's text selection.
pub type ConnectionsSelectableText = Rc<dyn Fn(&str, &str, bool, u32, &mut App) -> AnyElement>;

#[derive(Clone)]
pub struct ConnectionsEnvironment {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
}

#[cfg(not(target_family = "wasm"))]
mod desktop;
mod form_input;
#[cfg(not(target_family = "wasm"))]
pub use desktop::*;
pub use form_input::{
    ConnectionImeFrame, ConnectionImeSource, ConnectionsEnvironmentSource, ConnectionsInput,
};
use form_input::{FormAction, FormDispatch, bind_connection_input, form_text_input};

pub mod algorithms;
#[cfg(not(target_family = "wasm"))]
mod authentication;
#[cfg(not(target_family = "wasm"))]
mod delivery;
#[cfg(not(target_family = "wasm"))]
pub use delivery::{SftpConnected, SftpConnectedEndpoint};
mod form;
pub mod portable_form;
pub use form::{
    ConnectionFormEnvironment, FormKeyAction, FormPluginType, FormTotpCredential,
    NEW_CONNECTION_TYPE_SIDEBAR_WIDTH, TunnelFormEnvironment, TunnelKind,
};
pub use portable_form::{
    PortableConnectionDraft, PortableConnectionFormEnvironment, PortableConnectionFormEvent,
    PortableConnectionFormView,
};
#[cfg(not(target_family = "wasm"))]
mod ftp;
#[cfg(not(target_family = "wasm"))]
pub use ftp::ConnectedFtp;
