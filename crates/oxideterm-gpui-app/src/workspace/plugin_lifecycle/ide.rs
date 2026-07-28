// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_gpui_ide::IdePluginSnapshot;

use super::WorkspaceApp;
pub(super) use oxideterm_plugin_host_api::ide::{
    native_plugin_ide_active_file_path, native_plugin_ide_file_map, native_plugin_ide_response,
    native_plugin_ide_snapshot_value,
};

pub(super) fn native_plugin_ide_workspace_snapshot(
    workspace: &WorkspaceApp,
    cx: &mut Context<WorkspaceApp>,
) -> Option<IdePluginSnapshot> {
    workspace
        .ide_workspace
        .read(cx)
        .plugin_snapshot(workspace.main_window_tabs.active_tab_id, cx)
}
