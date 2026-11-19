use std::{hash::Hash, rc::Rc};

use gpui::App;
use oxideterm_ai::AcpClientEvent;
use serde_json::json;
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use super::AiConversationController;

pub struct AcpFileReviewBindings {
    pub preview: fn(&str, &str) -> Zeroizing<String>,
    pub frame: Rc<crate::stream::AiChatStreamFrameSource>,
    pub request_attention: Rc<dyn Fn(&str, &str, &mut App)>,
}

impl<Node: Clone + Eq + Hash + 'static, Terminal: Clone + 'static>
    AiConversationController<Node, Terminal>
{
    pub(super) fn review_acp_file_write(
        &self,
        agent_id: &str,
        generation: u64,
        conversation_id: &str,
        assistant_id: &str,
        event: AcpClientEvent,
        bindings: AcpFileReviewBindings,
        cx: &mut App,
    ) {
        let AcpClientEvent::WriteTextFile {
            mut request,
            response_tx,
        } = event
        else {
            unreachable!("the ACP router supplies a file write event");
        };
        let Some((root, policy)) = self
            .acp
            .read(cx)
            .session_context(agent_id, &request.session_id.to_string())
        else {
            let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                "ACP session context is unavailable",
            )));
            return;
        };
        if !policy.fs_write_text_file {
            let _ = response_tx.send(Err(oxideterm_ai::acp_method_not_found(
                "fs/write_text_file",
            )));
            return;
        }
        let path = request.path;
        let proposed = Zeroizing::new(std::mem::take(&mut request.content));
        let call = oxideterm_ai::next_acp_file_review_id();
        let conversation_id = conversation_id.to_owned();
        let assistant_id = assistant_id.to_owned();
        let acp = self.acp.downgrade();
        self.state.update(cx, |_, cx| {
            cx.spawn(async move |state, cx| {
                let target = match oxideterm_ai::resolve_acp_write_text_file_target(&root, &path).await {
                    Ok(target) => target,
                    Err(error) => {
                        let _ = response_tx.send(Err(error));
                        return;
                    }
                };
                let existing = match tokio::fs::read_to_string(&target).await {
                    Ok(content) => Zeroizing::new(content),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Zeroizing::new(String::new()),
                    Err(_) => {
                        let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error("ACP file review could not read the target")));
                        return;
                    }
                };
                let preview = (bindings.preview)(&existing, &proposed);
                let (choice_tx, choice_rx) = oneshot::channel();
                let label = path.display().to_string();
                let arguments = json!({ "path": label }).to_string();
                let registered = cx.update(|cx| {
                    let Some(state) = state.upgrade() else { return false };
                    let Some(frame) = (bindings.frame)(cx) else { return false };
                    if acp.update(cx, |acp, _| acp.register_file_write_preview(call.clone(), preview)).is_err() {
                        return false;
                    }
                    if state.read(cx).is_chat_stream_generation(generation) {
                        (bindings.request_attention)(&conversation_id, &assistant_id, cx);
                    }
                    state.update(cx, |ai, cx| {
                        ai.apply_ai_tool_status(
                            generation, &conversation_id, &assistant_id, &call, "write_file", &arguments,
                            "pending_user_approval", Some(json!({
                                "acpFileWriteReview": true,
                                "acpPermissionOptions": [
                                    {"id": "allow_once", "name": frame.i18n.t("ai.tool_use.approve"), "kind": "allow_once"},
                                    {"id": "reject_once", "name": frame.i18n.t("ai.tool_use.reject"), "kind": "reject_once"},
                                ],
                            })), Some("write".to_owned()), Some(label.clone()), false, None, None, None,
                            |data| frame.diagnostic(data), cx,
                        );
                        ai.register_acp_permission_choice(generation, call.clone(), choice_tx)
                    })
                });
                if !registered {
                    let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error("ACP file review is no longer active")));
                    return;
                }
                let approved = choice_rx.await.ok().flatten().as_deref().is_some_and(|option| option.starts_with("allow_"));
                let response = if approved {
                    match oxideterm_ai::resolve_acp_write_text_file_target(&root, &path).await {
                        Ok(target) => oxideterm_ai::write_acp_validated_text_file(&target, &proposed).await,
                        Err(error) => Err(error),
                    }
                } else {
                    Err(oxideterm_ai::acp_internal_error("ACP file write was declined by the user"))
                };
                let succeeded = response.is_ok();
                let _ = response_tx.send(response);
                cx.update(|cx| {
                    let _ = acp.update(cx, |acp, _| acp.remove_file_write_preview(&call));
                    let Some(frame) = (bindings.frame)(cx) else { return };
                    let _ = state.update(cx, |ai, cx| ai.apply_ai_tool_status(
                        generation, &conversation_id, &assistant_id, &call, "write_file", &arguments,
                        if succeeded { "completed" } else if approved { "error" } else { "rejected" },
                        Some(json!({ "ok": succeeded })), Some("write".to_owned()), Some(label), false, None, None, None,
                        |data| frame.diagnostic(data), cx,
                    ));
                });
            }).detach();
        });
    }
}
