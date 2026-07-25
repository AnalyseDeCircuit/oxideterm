// Settings adapters stay in the app because the domain crate does not depend on UI settings.
pub(in crate::workspace) fn ai_tool_use_policy_from_settings(
    settings: &oxideterm_settings::AiToolUseSettings,
) -> AiToolUsePolicy {
    tool_policy_from_parts(
        settings.enabled,
        settings
            .auto_approve_tools
            .iter()
            .filter_map(|(key, value)| value.as_bool().map(|enabled| (key.clone(), enabled))),
        settings.disabled_tools.clone(),
        settings.max_rounds,
        settings.max_calls_per_round,
    )
}
