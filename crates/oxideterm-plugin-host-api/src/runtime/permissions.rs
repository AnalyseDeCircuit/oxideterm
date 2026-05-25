//! Host API permission gates for native plugin runtime effects.
//!
//! Runtime bridges may transport host calls, but this module owns the policy
//! check that decides whether a plugin is allowed to issue a namespaced method.

use super::*;

pub(super) fn install_process_host_call_handler(
    runtime: &mut NativeProcessPluginRuntime,
    plugin_id: String,
    permissions: PluginPermissionSet,
    resolver: NativeHostApiResolver,
) {
    runtime.set_host_call_handler(Box::new(move |call| {
        if !host_api_allowed(&permissions, &call.namespace, &call.method) {
            return Some(PluginResponse::error(
                call.request_id,
                PluginError::protocol(
                    "host_api_not_allowed",
                    format!(
                        "Native plugin host call \"{}.{}\" is not allowed",
                        call.namespace, call.method
                    ),
                ),
            ));
        }
        resolver(plugin_id.clone(), permissions.clone(), call)
    }));
}

pub(super) fn validate_outbound_effect_permissions(
    effects: &[PluginOutboundEffect],
    permissions: &PluginPermissionSet,
) -> Result<(), PluginError> {
    for effect in effects {
        let PluginOutboundEffect::HostCall {
            namespace, method, ..
        } = effect
        else {
            continue;
        };
        if !host_api_allowed(permissions, namespace, method) {
            return Err(PluginError::protocol(
                "host_api_not_allowed",
                format!("Native plugin host call \"{namespace}.{method}\" is not allowed"),
            ));
        }
    }
    Ok(())
}

fn host_api_allowed(permissions: &PluginPermissionSet, namespace: &str, method: &str) -> bool {
    let exact = format!("{namespace}.{method}");
    let namespace_wildcard = format!("{namespace}.*");
    permissions
        .allowed_host_apis
        .iter()
        .any(|allowed| allowed == &exact || allowed == &namespace_wildcard)
}
