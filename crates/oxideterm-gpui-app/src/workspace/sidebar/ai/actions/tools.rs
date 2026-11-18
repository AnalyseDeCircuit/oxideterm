use oxideterm_ai::tool_loop::{
    agent_chat_message, agent_tool_result, ai_direction_changed_result, ai_pending_dispatch,
    execute_ai_tool, preflight_ai_tool, resolve_ai_candidate_selection_if_needed,
    run_ai_chat_tool_loop, send_ai_stream_delivery, send_ai_tool_status,
};
#[cfg(test)]
use oxideterm_ai::tool_loop::{record_completed_ai_tool_call, send_ai_tool_status_with_payload};
use oxideterm_ai::tool_resources::{
    ai_app_surface_stable_resources, ai_application_action_result, ai_memory_entry_content,
    ai_memory_entry_json, ai_memory_entry_mut, ai_memory_expected_revision, ai_memory_expiry,
    ai_memory_id, ai_memory_kind_from_arguments, ai_memory_kind_label, ai_memory_now_ms,
    ai_memory_scope_from_arguments, ai_memory_scope_label, ai_memory_settings_json,
    ai_memory_trimmed_content, ai_normalized_memory_content, ai_stable_resource_ref_for_target,
    ai_transfer_path_looks_directory,
};
use oxideterm_ai::tool_resources::{
    ai_model_safe_runtime_text, ai_model_visible_settings_projection,
    ai_runtime_validation_public_code, ai_runtime_validation_recovery_message,
};
#[cfg(test)]
use oxideterm_ai::tool_result_protocol::parse_ai_tool_args;

include!("tools/types.rs");
include!("tools/acp_bridge.rs");
include!("tools/application.rs");
include!("tools/background.rs");
include!("tools/snapshot.rs");
include!("tools/local_command.rs");
include!("tools/agents.rs");
include!("tools/agent_resources.rs");
include!("tools/executor.rs");
include!("tools/web.rs");
include!("tools/capabilities.rs");
include!("tools/protocol.rs");
