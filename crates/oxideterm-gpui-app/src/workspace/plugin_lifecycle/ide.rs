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
    let active_surface = workspace
        .active_tab_id
        .and_then(|tab_id| workspace.ide_tab_surfaces.get(&tab_id))
        .and_then(|surface| surface.read(cx).plugin_snapshot());
    active_surface.or_else(|| {
        workspace
            .ide_tab_surfaces
            .values()
            .find_map(|surface| surface.read(cx).plugin_snapshot())
    })
}
