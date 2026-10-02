//! Host API permission gates for native plugin runtime effects.
//!
//! Runtime bridges may transport host calls, but this module owns the policy
//! check that decides whether a plugin is allowed to issue a namespaced method.

use super::*;
use crate::capabilities::*;

pub(super) fn install_process_host_call_handler(
    runtime: &mut NativeProcessPluginRuntime,
    plugin_id: String,
    permissions: PluginPermissionSet,
    resolver: NativeHostApiResolver,
) {
    runtime.set_host_call_handler(Box::new(move |mut call| {
        if !host_api_allowed(&permissions, &call.namespace, &call.method) {
            let request_id = std::mem::take(&mut call.request_id);
            if call.sensitivity().is_sensitive() {
                // Rejected sensitive calls still own process-provided JSON and
                // must clear it before returning the permission error.
                call.zeroize_args();
            }
            return Some(PluginResponse::error(
                request_id,
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

pub(super) fn validate_outbound_message_permissions(
    messages: &[PluginOutboundMessage],
    permissions: &PluginPermissionSet,
) -> Result<(), PluginError> {
    for message in messages {
        match message {
            PluginOutboundMessage::RegisterContribution { registration } => {
                validate_registration_permissions(registration, permissions)?;
            }
            PluginOutboundMessage::EmitEvent { .. }
                if !capability_allowed(permissions, NATIVE_PLUGIN_CAPABILITY_EVENTS_EMIT) =>
            {
                return Err(capability_error(NATIVE_PLUGIN_CAPABILITY_EVENTS_EMIT));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_registration_permissions(
    registration: &PluginRegistration,
    permissions: &PluginPermissionSet,
) -> Result<(), PluginError> {
    match registration.kind {
        PluginRegistrationKind::EventSubscription => {
            let event = oxideterm_plugin_registry::native_plugin_runtime_subscription_event(
                &registration.metadata,
                &registration.plugin_id,
            )
            .map_err(|error| PluginError::protocol("invalid_event_subscription", error))?;
            if let Some(capability) = event_subscription_capability(&event)
                && !capability_allowed(permissions, capability)
            {
                return Err(capability_error(capability));
            }
        }
        PluginRegistrationKind::TerminalInputInterceptor => {
            require_capabilities(
                permissions,
                &[
                    NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ,
                    NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE,
                ],
            )?;
        }
        PluginRegistrationKind::TerminalOutputProcessor => {
            require_capabilities(
                permissions,
                &[NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ],
            )?;
        }
        PluginRegistrationKind::Tab
        | PluginRegistrationKind::SidebarPanel
        | PluginRegistrationKind::ActivityBarItem => {
            require_capabilities(permissions, &[NATIVE_PLUGIN_CAPABILITY_UI_WRITE])?;
        }
        _ => {}
    }
    Ok(())
}

fn event_subscription_capability(event: &str) -> Option<&'static str> {
    use oxideterm_plugin_registry::{
        NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT, NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
        NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
        NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT, NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
        NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT, NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
        NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT, NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
        NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
    };

    match event {
        NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT => {
            Some(NATIVE_PLUGIN_CAPABILITY_APP_SETTINGS_READ)
        }
        NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT | NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT => {
            Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ)
        }
        NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT => {
            Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ)
        }
        NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT
        | NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT
        | NATIVE_PLUGIN_TRANSFER_ERROR_EVENT => Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT
        | NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT
        | NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT => Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        _ => None,
    }
}

fn require_capabilities(
    permissions: &PluginPermissionSet,
    capabilities: &[&'static str],
) -> Result<(), PluginError> {
    for capability in capabilities {
        if !capability_allowed(permissions, capability) {
            return Err(capability_error(capability));
        }
    }
    Ok(())
}

fn capability_allowed(permissions: &PluginPermissionSet, capability: &str) -> bool {
    permissions
        .capabilities
        .iter()
        .any(|allowed| allowed == capability)
}

fn capability_error(capability: &str) -> PluginError {
    PluginError::protocol(
        "plugin_capability_not_allowed",
        format!("Native plugin capability \"{capability}\" is not allowed"),
    )
}

fn host_api_allowed(permissions: &PluginPermissionSet, namespace: &str, method: &str) -> bool {
    let exact = format!("{namespace}.{method}");
    let namespace_wildcard = format!("{namespace}.*");
    permissions
        .allowed_host_apis
        .iter()
        .any(|allowed| allowed == &exact || allowed == &namespace_wildcard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_require_their_declared_capabilities() {
        use PluginRegistrationKind::*;

        for (scenario, kind, metadata, capabilities, expected_error) in [
            (
                "baseline event",
                EventSubscription,
                serde_json::json!({ "event": "app.themeChanged" }),
                vec![],
                None,
            ),
            (
                "IDE event denied",
                EventSubscription,
                serde_json::json!({ "event": "ide.fileOpen" }),
                vec![],
                Some("plugin_capability_not_allowed"),
            ),
            (
                "IDE event allowed",
                EventSubscription,
                serde_json::json!({ "event": "ide.fileOpen" }),
                vec![NATIVE_PLUGIN_CAPABILITY_IDE_READ],
                None,
            ),
            (
                "interceptor read only",
                TerminalInputInterceptor,
                Value::Null,
                vec![NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ],
                Some("plugin_capability_not_allowed"),
            ),
            (
                "interceptor read and write",
                TerminalInputInterceptor,
                Value::Null,
                vec![
                    NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ,
                    NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE,
                ],
                None,
            ),
            (
                "tab denied",
                Tab,
                Value::Null,
                vec![],
                Some("plugin_capability_not_allowed"),
            ),
            (
                "tab allowed",
                Tab,
                Value::Null,
                vec![NATIVE_PLUGIN_CAPABILITY_UI_WRITE],
                None,
            ),
            (
                "sidebar denied",
                SidebarPanel,
                Value::Null,
                vec![],
                Some("plugin_capability_not_allowed"),
            ),
            (
                "sidebar allowed",
                SidebarPanel,
                Value::Null,
                vec![NATIVE_PLUGIN_CAPABILITY_UI_WRITE],
                None,
            ),
            (
                "activity bar denied",
                ActivityBarItem,
                Value::Null,
                vec![],
                Some("plugin_capability_not_allowed"),
            ),
            (
                "activity bar allowed",
                ActivityBarItem,
                Value::Null,
                vec![NATIVE_PLUGIN_CAPABILITY_UI_WRITE],
                None,
            ),
        ] {
            let message = PluginOutboundMessage::RegisterContribution {
                registration: PluginRegistration {
                    registration_id: scenario.to_string(),
                    plugin_id: "com.example.permissions".to_string(),
                    kind,
                    metadata,
                },
            };
            let result = validate_outbound_message_permissions(
                &[message],
                &PluginPermissionSet {
                    capabilities: capabilities.into_iter().map(str::to_string).collect(),
                    allowed_host_apis: Vec::new(),
                },
            );
            assert_eq!(
                result.map_err(|error| error.code),
                expected_error.map_or(Ok(()), |code| Err(code.to_string())),
                "{scenario}"
            );
        }
    }
}
