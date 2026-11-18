#[cfg(not(target_os = "macos"))]
use std::sync::Arc;

#[cfg(not(target_os = "macos"))]
use gpui::Context;
use gpui::{AnyElement, App, div, prelude::*};
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView,
    confirm::confirm_dialog_with_focus_motion,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

#[cfg(not(target_os = "macos"))]
use crate::entity::LaunchAtLoginError;
use crate::entity::{DataDirectoryConfirm, SettingsWorkspaceEntity};

impl SettingsWorkspaceEntity {
    #[cfg(not(target_os = "macos"))]
    pub fn refresh_launch_at_login(
        &mut self,
        runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) {
        self.start_launch_at_login_operation(
            async move {
                let result = runtime
                    .spawn_blocking(oxideterm_gpui_platform::autostart::is_enabled)
                    .await
                    .map_err(|error| LaunchAtLoginError::TaskFailed(error.to_string().into()))?;
                result
                    .map_err(|error| LaunchAtLoginError::OperationFailed(error.to_string().into()))
            },
            cx,
        );
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_launch_at_login(
        &mut self,
        enabled: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) {
        if self.launch_at_login_snapshot().pending {
            return;
        }
        self.start_launch_at_login_operation(
            async move {
                let result = runtime
                    .spawn_blocking(move || {
                        oxideterm_gpui_platform::autostart::set_enabled(enabled)?;
                        let actual = oxideterm_gpui_platform::autostart::is_enabled()?;
                        if actual != enabled {
                            return Err(std::io::Error::other(
                                "the operating system did not retain the startup setting",
                            ));
                        }
                        Ok(actual)
                    })
                    .await
                    .map_err(|error| LaunchAtLoginError::TaskFailed(error.to_string().into()))?;
                result.map_err(|error| {
                    if error.kind() == std::io::ErrorKind::PermissionDenied {
                        LaunchAtLoginError::ApprovalRequired
                    } else {
                        LaunchAtLoginError::OperationFailed(error.to_string().into())
                    }
                })
            },
            cx,
        );
    }
}

pub fn render_data_directory_confirm(
    settings: &SettingsWorkspaceEntity,
    tokens: &ThemeTokens,
    i18n: &I18n,
    focus: Option<ConfirmDialogAction>,
    finish: impl Fn(bool, &mut App) + Clone + 'static,
) -> Option<AnyElement> {
    let (title_key, description) = match settings.data_directory_confirm()? {
        DataDirectoryConfirm::Conflict { files_found, .. } => (
            "settings_view.general.data_directory_conflict",
            i18n.t("settings_view.general.data_directory_conflict_detail")
                .replace("{{files}}", &files_found.join(", ")),
        ),
        DataDirectoryConfirm::Reset => (
            "settings_view.general.reset_data_directory",
            i18n.t("settings_view.general.reset_data_directory_confirm"),
        ),
    };
    Some(confirm_dialog_with_focus_motion(
        tokens,
        "settings-data-directory-confirm-motion",
        settings.data_directory_confirm_phase(),
        ConfirmDialogView {
            variant: ConfirmDialogVariant::Default,
            title: div().child(i18n.t(title_key)).into_any_element(),
            description: Some(div().child(description).into_any_element()),
            cancel_label: div()
                .child(i18n.t("common.actions.cancel"))
                .into_any_element(),
            confirm_label: div()
                .child(i18n.t("common.actions.confirm"))
                .into_any_element(),
        },
        focus,
        {
            let finish = finish.clone();
            move |_, _, cx| {
                finish(false, cx);
                cx.stop_propagation();
            }
        },
        move |_, _, cx| {
            finish(true, cx);
            cx.stop_propagation();
        },
    ))
}
