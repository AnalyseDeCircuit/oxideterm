//! Native chat tool advertisements and RAG request arguments.

#[cfg(not(target_family = "wasm"))]
pub fn ai_stream_tool_definitions(
    tool_use_enabled: bool,
    skills_enabled: bool,
    web_enabled: bool,
    tool_policy: &crate::AiToolUsePolicy,
    mcp_registry: &crate::McpRegistry,
) -> Vec<crate::AiToolDefinition> {
    ai_stream_tool_definitions_with(
        tool_use_enabled,
        skills_enabled,
        web_enabled,
        tool_policy,
        || mcp_registry.tool_definitions(),
    )
}

pub fn ai_stream_tool_definitions_with(
    tool_use_enabled: bool,
    skills_enabled: bool,
    web_enabled: bool,
    tool_policy: &crate::AiToolUsePolicy,
    mcp: impl FnOnce() -> Vec<crate::AiToolDefinition>,
) -> Vec<crate::AiToolDefinition> {
    if !tool_use_enabled {
        return Vec::new();
    }
    let mut tools = crate::orchestrator_tool_definitions();
    if !web_enabled {
        tools.retain(|tool| !crate::web::is_web_tool(&tool.name));
    }
    if !skills_enabled {
        // The global Skills switch controls capability exposure, not only
        // whether a guessed tool call succeeds at execution time.
        tools.retain(|tool| tool.name != "load_skill" && tool.name != "read_skill_resource");
    }
    // Native does not ship Tauri's autonomous agent path yet. Expose MCP
    // resource/dynamic tools through chat as a native-only bridge so MCP
    // remains usable from the primary AI surface.
    tools.extend(mcp().into_iter().filter(|tool| {
        !tool_policy
            .disabled_tools
            .iter()
            .any(|name| name == &tool.name)
    }));
    tools
}

#[cfg(not(target_family = "wasm"))]
pub fn ai_active_tool_count(
    tool_use_enabled: bool,
    skills_enabled: bool,
    web_enabled: bool,
    tool_policy: &crate::AiToolUsePolicy,
    mcp_registry: &crate::McpRegistry,
) -> usize {
    // The compact status must reflect tools that policy permits, rather than
    // reusing an unrelated execution limit such as max tool rounds.
    ai_stream_tool_definitions(
        tool_use_enabled,
        skills_enabled,
        web_enabled,
        tool_policy,
        mcp_registry,
    )
    .into_iter()
    .filter(|tool| {
        !tool_policy
            .disabled_tools
            .iter()
            .any(|disabled| disabled == &tool.name)
    })
    .count()
}

pub fn ai_rag_query_arg(args: &serde_json::Value) -> &str {
    // Tauri uses `options.query ?? options.path ?? ''` for RAG reads and does
    // not trim the selected string before passing it to ragSearch.
    args.get("query")
        .or_else(|| args.get("path"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_chat_tool_definitions_include_mcp_bridge_tools_and_gate_web_search() {
        let registry = crate::McpRegistry::new(crate::AiProviderKeyStore::new());
        let policy = crate::AiToolUsePolicy {
            enabled: true,
            disabled_tools: vec![
                "list_mcp_resources".to_string(),
                "read_resource".to_string(),
            ],
            ..crate::AiToolUsePolicy::default()
        };

        let tools = ai_stream_tool_definitions(true, true, false, &policy, &registry);
        let names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"read_resource"));
        assert!(names.contains(&"read_mcp_resource"));
        assert!(!names.contains(&"list_mcp_resources"));
        assert!(!names.contains(&"search_web"));
        assert!(!names.contains(&"read_web_page"));

        let web_names = ai_stream_tool_definitions(true, true, true, &policy, &registry)
            .into_iter()
            .filter(|tool| crate::web::is_web_tool(&tool.name))
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert_eq!(web_names, ["search_web", "read_web_page"]);
        assert!(ai_stream_tool_definitions(false, true, true, &policy, &registry).is_empty());
    }
}
