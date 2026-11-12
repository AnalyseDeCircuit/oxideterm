use crate::{
    NativePluginInfo, NativePluginRuntimePlan, NativePluginState,
    native_plugin_requires_permission_review,
};

pub const PLUGIN_ID_CONFLICT_ERROR_PREFIX: &str = "PLUGIN_ID_CONFLICT:";

pub fn native_plugin_conflict_id(error: &str) -> Option<String> {
    error
        .strip_prefix(PLUGIN_ID_CONFLICT_ERROR_PREFIX)
        .map(str::trim)
        .filter(|plugin_id| !plugin_id.is_empty())
        .map(str::to_string)
}

pub fn native_plugin_state_is_active_like(state: NativePluginState) -> bool {
    matches!(
        state,
        NativePluginState::Active
            | NativePluginState::ReadyManifestOnly
            | NativePluginState::ReadyWasm
            | NativePluginState::ReadyProcess
    )
}

pub fn native_plugin_state_is_error_like(state: NativePluginState) -> bool {
    matches!(
        state,
        NativePluginState::Error | NativePluginState::AutoDisabled
    )
}

/// The plugin can serve host features as a process runtime: loaded enough to
/// activate (ReadyProcess, or Loading/Active while its process is coming up),
/// planned as a process, and cleared of the permission-review gate. Toolbar
/// chips and connection entries share this availability predicate.
pub fn native_plugin_process_activatable(plugin: &NativePluginInfo) -> bool {
    matches!(
        plugin.state,
        NativePluginState::ReadyProcess | NativePluginState::Loading | NativePluginState::Active
    ) && matches!(plugin.runtime_plan, NativePluginRuntimePlan::Process { .. })
        && !native_plugin_requires_permission_review(
            &plugin.manifest,
            &plugin.runtime_plan,
            &plugin.config,
        )
}

pub fn native_plugin_error_has_code(error: &str, code: &str) -> bool {
    // Runtime errors keep the stable code at the start of the message so
    // consumers do not need to match localized explanatory text.
    let Some(rest) = error.trim_start().strip_prefix(code) else {
        return false;
    };
    rest.is_empty() || rest.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_errors_preserve_codes_and_conflict_identity() {
        assert_eq!(
            native_plugin_conflict_id("PLUGIN_ID_CONFLICT:com.example.demo").as_deref(),
            Some("com.example.demo")
        );
        assert!(native_plugin_conflict_id("checksum mismatch").is_none());
        assert!(native_plugin_error_has_code(
            " wasm_runtime_not_installed: install runtime",
            "wasm_runtime_not_installed"
        ));
        assert!(!native_plugin_error_has_code(
            "wasm_runtime_not_installed_extra",
            "wasm_runtime_not_installed"
        ));
    }
}
