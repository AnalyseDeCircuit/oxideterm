use std::{future::Future, path::PathBuf, pin::Pin};

use super::*;

pub(super) fn cloud_sync_file_dialog(
    cx: &mut App,
    exporting: bool,
    i18n: &I18n,
) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>> {
    if exporting {
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let picker = cx.prompt_for_new_path(
            &directory,
            Some(&format!(
                "oxideterm-{}.oxide",
                chrono::Utc::now().format("%Y%m%d-%H%M%S")
            )),
        );
        Box::pin(async move {
            picker
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        })
    } else {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(i18n.t("plugin.cloud_sync.actions.import_local").into()),
        });
        Box::pin(async move {
            Ok(picker
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?
                .and_then(|paths| paths.into_iter().next()))
        })
    }
}
