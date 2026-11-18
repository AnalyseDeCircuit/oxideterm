use gpui::{App, Context, PathPromptOptions};
use oxideterm_connections::ConnectionImportSource;
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::ThemeTarget;
use std::{
    future::Future,
    path::{Path, PathBuf},
};

use super::SettingsWorkspaceEntity;

pub fn pick_paths(
    cx: &App,
    prompt: String,
    directories: bool,
    multiple: bool,
) -> impl Future<Output = Option<Vec<PathBuf>>> + use<> {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: !directories,
        directories,
        multiple,
        prompt: Some(prompt.into()),
    });
    async move { receiver.await.ok()?.ok()? }
}

pub fn pick_path(
    cx: &App,
    prompt: String,
    directories: bool,
) -> impl Future<Output = Option<PathBuf>> + use<> {
    let selection = pick_paths(cx, prompt, directories, false);
    async move { selection.await?.into_iter().next() }
}

impl SettingsWorkspaceEntity {
    pub fn pick_data_directory(&mut self, i18n: &I18n, cx: &mut Context<Self>) {
        let selection = pick_path(
            cx,
            i18n.t("settings_view.general.select_data_directory"),
            true,
        );
        self.start_data_directory_picker(selection, cx);
    }

    pub fn pick_managed_key_file(&mut self, i18n: &I18n, cx: &mut Context<Self>) {
        let selection = pick_path(
            cx,
            i18n.t("modals.managed_key.import_file.browse_title"),
            false,
        );
        self.start_managed_key_file_picker(
            async move {
                let path = selection.await?;
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Managed SSH Key");
                Some((path.display().to_string(), name.to_owned()))
            },
            cx,
        );
    }

    pub fn pick_connection_import(
        &mut self,
        i18n: &I18n,
        directories: bool,
        cx: &mut Context<Self>,
    ) {
        let multiple =
            !directories && self.connection_import_source() != ConnectionImportSource::Termius;
        let prompt = i18n.t(if directories {
            "settings_view.connections.importers.choose_directory"
        } else {
            "settings_view.connections.importers.choose_files"
        });
        let selection = pick_paths(cx, prompt, directories, multiple);
        self.start_connection_import_path_picker(
            async move {
                let paths = selection.await?;
                (!paths.is_empty()).then(|| {
                    paths
                        .into_iter()
                        .map(|path| path.display().to_string())
                        .collect()
                })
            },
            cx,
        );
    }

    pub fn pick_theme_import(
        &mut self,
        target: ThemeTarget,
        i18n: &I18n,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let selection = pick_path(cx, i18n.t("settings_view.appearance.theme_import"), false);
        self.start_theme_import(target, self.editing_scheme(), selection, runtime, cx);
    }

    pub fn editing_scheme(&self) -> Option<bool> {
        Some(self.appearance_edit_dark.unwrap_or(false))
    }

    pub fn edit_background_gallery(
        &mut self,
        operation: crate::background_page::BackgroundGalleryEdit,
        settings: &PersistedSettings,
        settings_path: &Path,
        runtime: tokio::runtime::Handle,
        is_bundled: fn(&Path, &Path) -> bool,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        use crate::background_page::BackgroundGalleryEdit::*;
        let path = settings_path.to_owned();
        let current = self
            .background_settings_for_controls(settings)
            .terminal
            .background_image;
        let scheme = self.editing_scheme();
        match operation {
            Pick => {
                let selection = pick_paths(
                    cx,
                    i18n.t("settings_view.terminal.bg_select_title"),
                    false,
                    true,
                );
                self.start_background_image_import(
                    selection,
                    path,
                    current.map(PathBuf::from),
                    scheme,
                    runtime,
                    cx,
                );
            }
            Remove(image) => {
                self.remove_background_image(path, image, current, scheme, runtime, is_bundled, cx);
            }
            Clear => {
                self.clear_background_image_gallery(path, current, scheme, runtime, is_bundled, cx);
            }
        }
    }
}
