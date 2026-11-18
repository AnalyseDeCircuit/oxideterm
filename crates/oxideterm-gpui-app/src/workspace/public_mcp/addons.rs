use std::path::Path;

use gpui::Context;
use oxideterm_plugin_registry as plugin_host;
use oxideterm_public_mcp::{DomainRequest, PublicToolCall, ToolEnvelope};
use oxideterm_public_mcp_host::{PublicMcpAddonPackage, public_addon_projection};
use serde_json::json;

use super::{WorkspaceApp, finish_serialized};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_addons_list(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let registry = self.plugin_entity.read(cx).registry_snapshot();
        self.public_mcp
            .handle_public_mcp_addons_list(request, &registry);
    }

    pub(super) fn handle_public_mcp_addons_install(
        &mut self,
        request: DomainRequest,
        package: PublicMcpAddonPackage,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::AddonsInstall(args) = &request.call else {
            return;
        };
        let expected_identity = args.expected_identity.clone();
        let settings_path = self.settings_store.path().to_path_buf();
        let cancellation = request.cancellation_token();
        if self.plugin_entity.read(cx).manager_operation_in_flight() {
            request.finish(ToolEnvelope::failed(
                "Another addon management operation is already running",
            ));
            return;
        }
        let mut retired_desktops = if args.replace_existing {
            self.remote_desktop.update(cx, |desktops, cx| {
                desktops.stop_plugins(Some(&expected_identity), cx)
            })
        } else {
            Vec::new()
        };
        if args.replace_existing && expected_identity == "com.oxideterm.terminal.mosh" {
            retired_desktops.push(self.mosh_plugin_sessions.stop());
        }
        let receiver = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_managed_package_install(
                settings_path.clone(),
                expected_identity.clone(),
                package.checksum,
                package.bytes,
                args.replace_existing,
                cancellation,
                retired_desktops,
            )
        });
        let Some(receiver) = receiver else {
            request.finish(ToolEnvelope::failed(
                "Another addon management operation is already running",
            ));
            return;
        };

        // The plugin entity owns the worker; this task only delivers its typed result.
        cx.spawn(async move |workspace, cx| {
            let result = receiver.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_addon_install(
                    request,
                    expected_identity,
                    &settings_path,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    fn finish_public_mcp_addon_install(
        &mut self,
        request: DomainRequest,
        expected_identity: String,
        settings_path: &Path,
        result: Result<
            Result<plugin_host::NativePluginUrlInstallResult, String>,
            tokio::sync::oneshot::error::RecvError,
        >,
        cx: &mut Context<Self>,
    ) {
        let cancelled = request.is_cancelled();
        let installed = matches!(&result, Ok(Ok(_)));
        if let Ok(Ok(install)) = &result {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.add_background_pack_images(&install.imported_backgrounds, cx);
            });
        }
        self.plugin_entity.update(cx, |plugins, cx| {
            plugins.finish_managed_package_install(settings_path, installed, cx);
        });
        self.process_native_plugin_install_queue(cx);
        if installed && !cancelled {
            self.bootstrap_native_plugin_runtime(cx);
        }
        let registry = self.plugin_entity.read(cx).registry_snapshot();
        self.public_mcp.finish_addon_install(
            request,
            &expected_identity,
            &registry,
            result,
            cancelled,
        );
        cx.notify();
    }

    pub(super) fn handle_public_mcp_addons_set_enabled(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::AddonsSetEnabled(args) = &request.call else {
            return;
        };
        let registry = self.plugin_entity.read(cx).registry_snapshot();
        let Some(plugin_id) =
            self.public_mcp
                .addon_id(&request.client_ref, &args.addon_ref, &registry)
        else {
            request.finish(ToolEnvelope::failed("The addon handle is unavailable"));
            return;
        };
        let result = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.set_plugin_enabled(&plugin_id, args.enabled, _cx)
        });
        if result.is_err() {
            request.finish(ToolEnvelope::failed("The addon state could not be changed"));
            return;
        }
        self.bootstrap_native_plugin_runtime(cx);
        let registry = self.plugin_entity.read(cx).registry_snapshot();
        let Some(plugin) = registry
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
        else {
            request.finish(ToolEnvelope::failed("The addon is no longer available"));
            return;
        };
        let addon = public_addon_projection(args.addon_ref.clone(), plugin);
        finish_serialized(request, json!({ "addon": addon }));
        cx.notify();
    }

    pub(super) fn handle_public_mcp_addons_remove(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::AddonsRemove(args) = &request.call else {
            return;
        };
        let registry = self.plugin_entity.read(cx).registry_snapshot();
        let Some(plugin_id) =
            self.public_mcp
                .addon_id(&request.client_ref, &args.addon_ref, &registry)
        else {
            request.finish(ToolEnvelope::failed("The addon handle is unavailable"));
            return;
        };
        let retain_settings = args.retain_settings.unwrap_or(true);
        if self.plugin_entity.read(cx).manager_operation_in_flight() {
            request.finish(ToolEnvelope::failed(
                "Another addon management operation is already running",
            ));
            return;
        }
        let addon_ref = args.addon_ref.clone();
        if self
            .plugin_entity
            .update(cx, |plugins, cx| {
                plugins.set_plugin_enabled(&plugin_id, false, cx)?;
                plugins.begin_remote_desktop_removal(&plugin_id);
                Ok::<_, String>(())
            })
            .is_err()
        {
            request.finish(ToolEnvelope::failed(
                "The addon could not be disabled before removal",
            ));
            return;
        }
        self.stop_acp_plugin(Some(&plugin_id), cx);
        let mut workers = self.remote_desktop.update(cx, |desktops, cx| {
            desktops.stop_plugins(Some(&plugin_id), cx)
        });
        if plugin_id == "com.oxideterm.terminal.mosh" {
            workers.push(self.mosh_plugin_sessions.stop());
        }
        let receiver = self.plugin_entity.update(cx, |plugins, cx| {
            plugins.start_plugin_uninstall(plugin_id, !retain_settings, workers, cx)
        });
        cx.spawn(async move |workspace, cx| {
            let result = receiver.await;
            let _ = workspace.update(cx, |workspace, cx| {
                if !matches!(result, Ok(Ok(()))) {
                    request.finish(ToolEnvelope::failed("The addon could not be removed"));
                    return;
                }
                workspace.bootstrap_native_plugin_runtime(cx);
                workspace
                    .public_mcp
                    .remove_addon_ref(&request.client_ref, &addon_ref);
                finish_serialized(
                    request,
                    json!({ "removed": true, "settings_retained": retain_settings }),
                );
                cx.notify();
            });
        })
        .detach();
    }
}
