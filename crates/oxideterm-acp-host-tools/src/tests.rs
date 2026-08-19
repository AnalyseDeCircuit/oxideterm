use serde_json::json;

use crate::AcpHostToolResponse;

#[test]
fn debug_output_redacts_tool_arguments_and_content() {
    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
    let call = crate::AcpHostToolCall::new(
        "call-1".to_string(),
        "run_command".to_string(),
        json!({ "command": "TOKEN=supersecret" }),
        response_tx,
    );
    assert!(!format!("{call:?}").contains("supersecret"));
    let response = AcpHostToolResponse::success("PASSWORD=supersecret");
    assert!(!format!("{response:?}").contains("supersecret"));
}
