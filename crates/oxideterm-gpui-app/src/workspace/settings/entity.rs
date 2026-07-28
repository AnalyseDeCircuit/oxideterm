use std::sync::Arc;

use gpui::{Context, EventEmitter, Task};
use oxideterm_connections::PrivilegeCredentialKind;
use oxideterm_gpui_settings_view::SettingsInput;
use zeroize::Zeroizing;

use super::update::NativeUpdateRuntime;
use super::{
    CliCompanionStatus, PortableSettingsAction, PortableSettingsDialog, SettingsManagedKeyDialog,
};

/// Non-secret result produced by the portable runtime status worker.
pub(in crate::workspace) struct PortableStatusRefresh {
    pub(in crate::workspace) status:
        Result<oxideterm_portable_runtime::PortableStatusSnapshot, String>,
    pub(in crate::workspace) exportable_secret_count: usize,
}

/// Read-only projection used after releasing the settings Entity borrow.
#[derive(Clone)]
pub(in crate::workspace) struct PortableStatusSnapshot {
    pub(in crate::workspace) status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) exportable_secret_count: Option<usize>,
    pub(in crate::workspace) refresh_pending: bool,
}

pub(in crate::workspace) struct PortablePasswordDialogSnapshot {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) pending: bool,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) current_password: Zeroizing<String>,
    pub(in crate::workspace) new_password: Zeroizing<String>,
    pub(in crate::workspace) confirm_password: Zeroizing<String>,
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
}

/// Copies only the active dialog payload; secret frame copies remain zeroizing.
pub(in crate::workspace) enum ManagedKeyDialogSnapshot {
    ImportFile {
        file_path: String,
        file_name: String,
        file_passphrase: Zeroizing<String>,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Paste {
        name: String,
        private_key: Zeroizing<String>,
        passphrase: Zeroizing<String>,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Rename {
        name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Delete {
        key: oxideterm_connections::ManagedSshKeyInfo,
        usage: oxideterm_connections::ManagedSshKeyUsage,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
}

pub(in crate::workspace) struct NetworkProxyPasswordSnapshot {
    pub(in crate::workspace) password: Zeroizing<String>,
    pub(in crate::workspace) password_status: Option<String>,
}

pub(in crate::workspace) struct NetworkProxyTestSnapshot {
    pub(in crate::workspace) test_host: String,
    pub(in crate::workspace) test_port: String,
    pub(in crate::workspace) test_pending: bool,
    pub(in crate::workspace) test_result: Option<Result<u128, String>>,
}

/// Editable privilege credential state with a zeroizing secret owner.
pub(in crate::workspace) struct PrivilegeCredentialDraft {
    pub(super) credential_id: Option<String>,
    pub(super) label: String,
    pub(super) kind: PrivilegeCredentialKind,
    pub(super) username_hint: String,
    pub(super) prompt_patterns: String,
    pub(super) secret: Zeroizing<String>,
    pub(super) enabled: bool,
}

impl Default for PrivilegeCredentialDraft {
    fn default() -> Self {
        Self {
            credential_id: None,
            label: String::new(),
            kind: PrivilegeCredentialKind::SudoPassword,
            username_hint: String::new(),
            prompt_patterns: String::new(),
            secret: Zeroizing::new(String::new()),
            enabled: true,
        }
    }
}

pub(in crate::workspace) struct PrivilegeCredentialSnapshot {
    pub(in crate::workspace) credential_id: Option<String>,
    pub(in crate::workspace) label: String,
    pub(in crate::workspace) kind: PrivilegeCredentialKind,
    pub(in crate::workspace) username_hint: String,
    pub(in crate::workspace) prompt_patterns: String,
    pub(in crate::workspace) enabled: bool,
    pub(in crate::workspace) error: Option<String>,
}

#[derive(Clone)]
pub(in crate::workspace) struct CliCompanionSnapshot {
    pub(in crate::workspace) status: Option<CliCompanionStatus>,
    pub(in crate::workspace) loading: bool,
    pub(in crate::workspace) error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum CliCompanionOperation {
    Refresh,
    Install,
    Uninstall,
    UninstallLegacy,
    Migrate,
}

/// Owns settings work that must complete independently from root rendering.
pub(in crate::workspace) struct SettingsWorkspaceEntity {
    portable_status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    portable_status_error: Option<String>,
    portable_exportable_secret_count: Option<usize>,
    portable_refresh_pending: bool,
    portable_refresh_task: Option<Task<()>>,
    pub(super) portable_dialog: Option<PortableSettingsDialog>,
    pub(super) portable_action_pending: Option<PortableSettingsAction>,
    pub(super) portable_action_error: Option<String>,
    pub(super) portable_current_password: Zeroizing<String>,
    pub(super) portable_new_password: Zeroizing<String>,
    pub(super) portable_confirm_password: Zeroizing<String>,
    pub(super) settings_focused_input: Option<SettingsInput>,
    pub(super) portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) portable_dialog_exit_task: Option<Task<()>>,
    pub(super) portable_action_task: Option<Task<()>>,
    pub(super) managed_key_dialog: Option<SettingsManagedKeyDialog>,
    pub(super) managed_key_status: Option<String>,
    pub(super) managed_key_file_path: String,
    pub(super) managed_key_file_name: String,
    pub(super) managed_key_file_passphrase: Zeroizing<String>,
    pub(super) managed_key_paste_name: String,
    pub(super) managed_key_paste_private_key: Zeroizing<String>,
    pub(super) managed_key_paste_passphrase: Zeroizing<String>,
    pub(super) managed_key_rename_name: String,
    pub(super) managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) managed_key_dialog_exit_task: Option<Task<()>>,
    pub(super) network_proxy_password: Zeroizing<String>,
    pub(super) network_proxy_password_status: Option<String>,
    pub(super) network_proxy_test_host: String,
    pub(super) network_proxy_test_port: String,
    pub(super) network_proxy_test_pending: bool,
    pub(super) network_proxy_test_result: Option<Result<u128, String>>,
    pub(super) network_proxy_test_task: Option<Task<()>>,
    pub(super) network_proxy_test_abort: Option<tokio::task::AbortHandle>,
    pub(super) privilege_draft: PrivilegeCredentialDraft,
    pub(super) privilege_error: Option<String>,
    pub(super) privilege_editor_open: bool,
    pub(super) privilege_scope_id: Option<String>,
    pub(super) cli_companion_status: Option<CliCompanionStatus>,
    pub(super) cli_companion_loading: bool,
    pub(super) cli_companion_error: Option<String>,
    pub(super) cli_companion_task: Option<Task<()>>,
    pub(super) native_update: NativeUpdateRuntime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SettingsWorkspaceToast {
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SettingsWorkspaceEvent {
    ResetNativeUpdateOverlay,
    ShowNativeUpdateNotification,
    ShowNativeUpdateToast(SettingsWorkspaceToast),
    RequestAutomaticNativeUpdateCheck,
    RequestQuitAfterNativeUpdate,
    PortablePasswordChangeFinished {
        success: bool,
    },
    CliCompanionFinished {
        operation: CliCompanionOperation,
        success: bool,
    },
}

impl EventEmitter<SettingsWorkspaceEvent> for SettingsWorkspaceEntity {}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            portable_status: None,
            portable_status_error: None,
            portable_exportable_secret_count: None,
            portable_refresh_pending: false,
            portable_refresh_task: None,
            portable_dialog: None,
            portable_action_pending: None,
            portable_action_error: None,
            portable_current_password: Zeroizing::new(String::new()),
            portable_new_password: Zeroizing::new(String::new()),
            portable_confirm_password: Zeroizing::new(String::new()),
            settings_focused_input: None,
            portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            portable_dialog_exit_task: None,
            portable_action_task: None,
            managed_key_dialog: None,
            managed_key_status: None,
            managed_key_file_path: String::new(),
            managed_key_file_name: String::new(),
            managed_key_file_passphrase: Zeroizing::new(String::new()),
            managed_key_paste_name: String::new(),
            managed_key_paste_private_key: Zeroizing::new(String::new()),
            managed_key_paste_passphrase: Zeroizing::new(String::new()),
            managed_key_rename_name: String::new(),
            managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            managed_key_dialog_exit_task: None,
            network_proxy_password: Zeroizing::new(String::new()),
            network_proxy_password_status: None,
            network_proxy_test_host: String::new(),
            network_proxy_test_port: "22".to_string(),
            network_proxy_test_pending: false,
            network_proxy_test_result: None,
            network_proxy_test_task: None,
            network_proxy_test_abort: None,
            privilege_draft: PrivilegeCredentialDraft::default(),
            privilege_error: None,
            privilege_editor_open: false,
            privilege_scope_id: None,
            cli_companion_status: None,
            cli_companion_loading: false,
            cli_companion_error: None,
            cli_companion_task: None,
            native_update: NativeUpdateRuntime::new(cx),
        }
    }

    pub(in crate::workspace) fn portable_status_snapshot(&self) -> PortableStatusSnapshot {
        PortableStatusSnapshot {
            status: self.portable_status.clone(),
            error: self.portable_status_error.clone(),
            exportable_secret_count: self.portable_exportable_secret_count,
            refresh_pending: self.portable_refresh_pending,
        }
    }

    pub(in crate::workspace) fn portable_mode(&self) -> Option<bool> {
        self.portable_status
            .as_ref()
            .map(|status| status.is_portable)
    }

    pub(in crate::workspace) fn start_portable_status_refresh(
        &mut self,
        force: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        worker: impl FnOnce() -> PortableStatusRefresh + Send + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_refresh_pending {
            return false;
        }
        if !force
            && (self.portable_status.is_some() || self.portable_status_error.is_some())
            && self.portable_exportable_secret_count.is_some()
        {
            return false;
        }

        self.portable_refresh_pending = true;
        self.portable_refresh_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime.spawn_blocking(worker).await;
            let _ = settings.update(cx, |settings, cx| {
                settings
                    .finish_portable_status_refresh(result.map_err(|error| error.to_string()), cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_portable_status_refresh(
        &mut self,
        result: Result<PortableStatusRefresh, String>,
        cx: &mut Context<Self>,
    ) {
        self.portable_refresh_task = None;
        self.portable_refresh_pending = false;
        match result {
            Ok(PortableStatusRefresh {
                status: Ok(status),
                exportable_secret_count,
            }) => {
                self.portable_status = Some(status);
                self.portable_status_error = None;
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Ok(PortableStatusRefresh {
                status: Err(error),
                exportable_secret_count,
            }) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Err(error) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn invalidate_portable_status(&mut self, cx: &mut Context<Self>) {
        self.portable_status = None;
        self.portable_status_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn settings_entity_focused_input(&self) -> Option<SettingsInput> {
        self.settings_focused_input
    }

    pub(in crate::workspace) fn settings_entity_input_value(
        &self,
        input: SettingsInput,
    ) -> Option<&str> {
        match input {
            SettingsInput::PortableCurrentPassword => Some(&self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&self.portable_confirm_password),
            SettingsInput::ManagedKeyFilePath => Some(&self.managed_key_file_path),
            SettingsInput::ManagedKeyFileName => Some(&self.managed_key_file_name),
            SettingsInput::ManagedKeyFilePassphrase => Some(&self.managed_key_file_passphrase),
            SettingsInput::ManagedKeyPasteName => Some(&self.managed_key_paste_name),
            SettingsInput::ManagedKeyPastePrivateKey => Some(&self.managed_key_paste_private_key),
            SettingsInput::ManagedKeyPastePassphrase => Some(&self.managed_key_paste_passphrase),
            SettingsInput::ManagedKeyRenameName => Some(&self.managed_key_rename_name),
            SettingsInput::NetworkProxyPassword => Some(&self.network_proxy_password),
            SettingsInput::NetworkProxyTestHost => Some(&self.network_proxy_test_host),
            SettingsInput::NetworkProxyTestPort => Some(&self.network_proxy_test_port),
            SettingsInput::LocalPrivilegeLabel => Some(&self.privilege_draft.label),
            SettingsInput::LocalPrivilegeUsernameHint => Some(&self.privilege_draft.username_hint),
            SettingsInput::LocalPrivilegeSecret => Some(&self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&self.privilege_draft.prompt_patterns)
            }
            _ => None,
        }
    }

    pub(in crate::workspace) fn focus_settings_entity_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        let portable_open = self.portable_dialog == Some(PortableSettingsDialog::ChangePassword);
        let can_focus = match input {
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => portable_open,
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase => matches!(
                self.managed_key_dialog,
                Some(SettingsManagedKeyDialog::ImportFile)
            ),
            SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase => {
                matches!(
                    self.managed_key_dialog,
                    Some(SettingsManagedKeyDialog::Paste)
                )
            }
            SettingsInput::ManagedKeyRenameName => matches!(
                self.managed_key_dialog,
                Some(SettingsManagedKeyDialog::Rename { .. })
            ),
            SettingsInput::NetworkProxyPassword
            | SettingsInput::NetworkProxyTestHost
            | SettingsInput::NetworkProxyTestPort => true,
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns => true,
            _ => false,
        };
        if !can_focus {
            return false;
        }
        self.settings_focused_input = Some(input);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn blur_settings_entity_input(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.settings_focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn replace_settings_entity_input(
        &mut self,
        input: SettingsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.settings_entity_input_mut(input) else {
            return false;
        };
        oxideterm_editor_core::utf16::replace_utf16(value, replacement_range, text);
        self.clear_settings_entity_input_error(input);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn pop_settings_entity_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.settings_entity_input_mut(input) else {
            return false;
        };
        value.pop();
        self.clear_settings_entity_input_error(input);
        cx.notify();
        true
    }

    fn clear_settings_entity_input_error(&mut self, input: SettingsInput) {
        match input {
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => self.portable_action_error = None,
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName => self.managed_key_status = None,
            SettingsInput::NetworkProxyPassword => self.network_proxy_password_status = None,
            SettingsInput::NetworkProxyTestHost | SettingsInput::NetworkProxyTestPort => {
                self.network_proxy_test_result = None;
            }
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns => self.privilege_error = None,
            _ => {}
        }
    }

    fn settings_entity_input_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        match input {
            SettingsInput::PortableCurrentPassword => Some(&mut self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&mut self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&mut self.portable_confirm_password),
            SettingsInput::ManagedKeyFilePath => Some(&mut self.managed_key_file_path),
            SettingsInput::ManagedKeyFileName => Some(&mut self.managed_key_file_name),
            SettingsInput::ManagedKeyFilePassphrase => Some(&mut self.managed_key_file_passphrase),
            SettingsInput::ManagedKeyPasteName => Some(&mut self.managed_key_paste_name),
            SettingsInput::ManagedKeyPastePrivateKey => {
                Some(&mut self.managed_key_paste_private_key)
            }
            SettingsInput::ManagedKeyPastePassphrase => {
                Some(&mut self.managed_key_paste_passphrase)
            }
            SettingsInput::ManagedKeyRenameName => Some(&mut self.managed_key_rename_name),
            SettingsInput::NetworkProxyPassword => Some(&mut self.network_proxy_password),
            SettingsInput::NetworkProxyTestHost => Some(&mut self.network_proxy_test_host),
            SettingsInput::NetworkProxyTestPort => Some(&mut self.network_proxy_test_port),
            SettingsInput::LocalPrivilegeLabel => Some(&mut self.privilege_draft.label),
            SettingsInput::LocalPrivilegeUsernameHint => {
                Some(&mut self.privilege_draft.username_hint)
            }
            SettingsInput::LocalPrivilegeSecret => Some(&mut self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&mut self.privilege_draft.prompt_patterns)
            }
            _ => None,
        }
    }
}

impl Drop for SettingsWorkspaceEntity {
    fn drop(&mut self) {
        if let Some(abort) = self.network_proxy_test_abort.take() {
            abort.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, TestAppContext};

    use super::SettingsWorkspaceEntity;

    #[gpui::test]
    fn portable_status_refresh_is_single_flight_and_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("test runtime"),
        );

        entity.update(cx, |entity, cx| {
            assert!(entity.start_portable_status_refresh(
                false,
                runtime,
                || super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                },
                cx,
            ));
            assert!(
                !entity.start_portable_status_refresh(
                    false,
                    Arc::new(
                        tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(1)
                            .enable_all()
                            .build()
                            .expect("second test runtime"),
                    ),
                    || unreachable!("single-flight worker"),
                    cx,
                )
            );
            entity.portable_refresh_task = None;
            entity.finish_portable_status_refresh(
                Ok(super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                }),
                cx,
            );
        });

        entity.update(cx, |entity, _cx| {
            let snapshot = entity.portable_status_snapshot();
            assert!(!snapshot.refresh_pending);
            assert_eq!(snapshot.error.as_deref(), Some("unavailable"));
            assert_eq!(snapshot.exportable_secret_count, Some(2));
        });
    }
}
