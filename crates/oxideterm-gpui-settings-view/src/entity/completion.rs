use std::path::Path;

use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    delete_custom_theme_from_settings, save_theme_editor_snapshot_to_settings,
};
use serde_json::{Map, Value};

use super::{
    BackgroundGalleryOperationResult, CliCompanionOperation, DataDirectoryOperationResult,
    KeybindingFileOperationResult, SettingsWorkspaceEntity, SettingsWorkspaceEvent,
    SettingsWorkspaceToast, ThemeEditorOperationResult, ThemeImportResult,
};
use crate::{components::SettingsEdit, native_update::NativeUpdateRenderState};

type ThemeSave = Box<dyn FnOnce(&mut PersistedSettings) -> Option<String>>;

pub enum SettingsCompletion {
    Edit(SettingsEdit),
    SaveTheme(ThemeSave),
    ApplyKeybindings(Map<String, Value>),
    Notice(String, SettingsWorkspaceToast),
    RefreshPortableStatus,
}

impl SettingsWorkspaceEntity {
    /// Drains only the results for the completed operation. The window applies persisted edits.
    pub fn take_completions(
        &mut self,
        event: SettingsWorkspaceEvent,
        settings_path: &Path,
        i18n: &I18n,
    ) -> Vec<SettingsCompletion> {
        use SettingsCompletion::*;
        use SettingsWorkspaceToast::{Error, Success};
        match event {
            SettingsWorkspaceEvent::DataDirectoryOperationReady => self
                .take_data_directory_results()
                .into_iter()
                .map(|result| match result {
                    DataDirectoryOperationResult::Changed => Notice(
                        i18n.t("settings_view.general.data_directory_changed"),
                        Success,
                    ),
                    DataDirectoryOperationResult::Reset => Notice(
                        i18n.t("settings_view.general.data_directory_reset"),
                        Success,
                    ),
                    DataDirectoryOperationResult::Failed(error) => Notice(error, Error),
                })
                .collect(),
            SettingsWorkspaceEvent::BackgroundBlurCommitReady(dark, value) => {
                vec![Edit(Box::new(move |settings| {
                    let mut style = settings.terminal.background_for_scheme(dark);
                    style.blur = value;
                    settings.terminal.set_background_for_scheme(dark, style);
                }))]
            }
            SettingsWorkspaceEvent::BackgroundGalleryOperationReady => {
                let results = self.take_background_gallery_results();
                results
                    .into_iter()
                    .map(|result| match result {
                        BackgroundGalleryOperationResult::Updated(dark, active_path) => {
                            let gallery = self.background_images_snapshot();
                            let settings_path = settings_path.to_owned();
                            Edit(Box::new(move |settings| {
                                let mut style = settings.terminal.background_for_scheme(dark);
                                style.image = active_path;
                                settings.terminal.set_background_for_scheme(dark, style);
                                // Gallery deletion must remove references in both system modes.
                                let keep = |path: &str| {
                                    !oxideterm_settings::is_managed_background_image(
                                        &settings_path,
                                        Path::new(path),
                                    ) || gallery.iter().any(|image| image == path)
                                };
                                if settings
                                    .terminal
                                    .background_image
                                    .as_deref()
                                    .is_some_and(|path| !keep(path))
                                {
                                    settings.terminal.background_image = None;
                                }
                                for style in [
                                    &mut settings.terminal.system_backgrounds.light,
                                    &mut settings.terminal.system_backgrounds.dark,
                                ]
                                .into_iter()
                                .flatten()
                                {
                                    if style.image.as_deref().is_some_and(|path| !keep(path)) {
                                        style.image = None;
                                    }
                                }
                            }))
                        }
                        BackgroundGalleryOperationResult::Failed => {
                            Notice(i18n.t("settings_view.terminal.bg_operation_failed"), Error)
                        }
                        BackgroundGalleryOperationResult::MediaFailed(failure) => {
                            let key = match failure {
                                oxideterm_gpui_background::BackgroundFailure::Unsupported => {
                                    "bg_unsupported"
                                }
                                oxideterm_gpui_background::BackgroundFailure::ResourceExhausted => {
                                    "bg_resource_exhausted"
                                }
                                _ => "bg_decode_failed",
                            };
                            Notice(i18n.t(&format!("settings_view.terminal.{key}")), Error)
                        }
                    })
                    .collect()
            }
            SettingsWorkspaceEvent::ThemeImportReady => self
                .take_theme_import_results()
                .into_iter()
                .flat_map(|result| match result {
                    ThemeImportResult::Imported {
                        target,
                        system_dark,
                        theme_id,
                        name,
                        value,
                    } => vec![
                        Edit(Box::new(move |settings| {
                            let selected = theme_id.clone();
                            settings.custom_themes.insert(theme_id, value);
                            target.apply_for_scheme(settings, system_dark, selected);
                        })),
                        Notice(
                            i18n.t("settings_view.appearance.theme_import_success")
                                .replace("{{name}}", &name),
                            Success,
                        ),
                    ],
                    ThemeImportResult::Failed(error) => vec![Notice(
                        i18n.t("settings_view.appearance.theme_import_error")
                            .replace("{{error}}", &error),
                        Error,
                    )],
                })
                .collect(),
            SettingsWorkspaceEvent::ThemeEditorOperationReady => self
                .take_theme_editor_results()
                .into_iter()
                .map(|result| match result {
                    ThemeEditorOperationResult::Save(editor) => {
                        let label = i18n.t("settings_view.appearance.theme_import_success");
                        SaveTheme(Box::new(move |settings| {
                            save_theme_editor_snapshot_to_settings(settings, &editor)
                                .map(|name| label.replace("{{name}}", &name))
                        }))
                    }
                    ThemeEditorOperationResult::Delete(editor) => Edit(Box::new(move |settings| {
                        if let Some(id) = editor.edit_theme_id.as_deref() {
                            delete_custom_theme_from_settings(
                                settings,
                                id,
                                oxideterm_theme::DEFAULT_THEME.id,
                            );
                        }
                    })),
                })
                .collect(),
            SettingsWorkspaceEvent::KeybindingFileOperationReady => self
                .take_keybinding_file_operation_results()
                .into_iter()
                .flat_map(|result| match result {
                    KeybindingFileOperationResult::Exported => vec![Notice(
                        i18n.t("settings_view.keybindings.export_success"),
                        Success,
                    )],
                    KeybindingFileOperationResult::ExportFailed => vec![Notice(
                        i18n.t("settings_view.keybindings.export_error"),
                        Error,
                    )],
                    KeybindingFileOperationResult::Imported { overrides } => vec![
                        ApplyKeybindings(overrides),
                        Notice(i18n.t("settings_view.keybindings.import_success"), Success),
                    ],
                    KeybindingFileOperationResult::ImportFailed => vec![Notice(
                        i18n.t("settings_view.keybindings.import_invalid"),
                        Error,
                    )],
                })
                .collect(),
            SettingsWorkspaceEvent::ShowNativeUpdateToast(variant) => {
                let scheduled = self.native_update_is_portable()
                    && matches!(
                        self.native_update_render_state(),
                        NativeUpdateRenderState::InstallFinished {
                            status: oxideterm_update::NativeInstallStatus::ReplacementScheduled,
                            ..
                        },
                    );
                let message = if scheduled {
                    Some(i18n.t("settings_view.help.replacement_scheduled"))
                } else {
                    self.native_update_message().map(str::to_owned)
                };
                message
                    .into_iter()
                    .map(|message| Notice(message, variant))
                    .collect()
            }
            SettingsWorkspaceEvent::PortablePasswordChangeFinished { success: true } => vec![
                Notice(
                    i18n.t("settings_view.general.portable_password_changed"),
                    Success,
                ),
                RefreshPortableStatus,
            ],
            SettingsWorkspaceEvent::PortablePasswordChangeFinished { success: false } => self
                .portable_action_error()
                .map(|error| Notice(error.to_owned(), Error))
                .into_iter()
                .collect(),
            SettingsWorkspaceEvent::CliCompanionFinished { operation, success } => {
                let key = match operation {
                    CliCompanionOperation::Refresh => return Vec::new(),
                    CliCompanionOperation::Install => "settings_view.general.cli_installed",
                    CliCompanionOperation::Uninstall => "settings_view.general.cli_uninstalled",
                    CliCompanionOperation::UninstallLegacy => "migration.cli_legacy_uninstalled",
                    CliCompanionOperation::Migrate => "migration.cli_migrated",
                };
                if success {
                    vec![Notice(i18n.t(key), Success)]
                } else {
                    self.cli_companion_error()
                        .map(|error| Notice(error.to_owned(), Error))
                        .into_iter()
                        .collect()
                }
            }
            _ => Vec::new(),
        }
    }
}
