use agent_client_protocol::JsonRpcRequest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroize;

pub type CursorResponseSender =
    tokio::sync::oneshot::Sender<Result<Value, agent_client_protocol::Error>>;

#[derive(Clone, Serialize, Deserialize, JsonRpcRequest)]
#[serde(rename_all = "camelCase")]
#[request(method = "cursor/ask_question", response = Value)]
pub struct CursorAskQuestion {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub questions: Vec<CursorQuestion>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<CursorQuestionOption>,
    #[serde(default)]
    pub allow_multiple: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CursorQuestionOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Serialize, Deserialize, JsonRpcRequest)]
#[serde(rename_all = "camelCase")]
#[request(method = "cursor/create_plan", response = Value)]
pub struct CursorCreatePlan {
    pub tool_call_id: String,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub plan: String,
}

#[derive(Clone)]
pub enum CursorRequest {
    Questions(CursorAskQuestion),
    Plan(CursorCreatePlan),
}

impl CursorRequest {
    pub fn tool_call_id(&self) -> &str {
        match self {
            Self::Questions(request) => &request.tool_call_id,
            Self::Plan(request) => &request.tool_call_id,
        }
    }

    pub fn method(&self) -> &'static str {
        match self {
            Self::Questions(_) => "cursor/ask_question",
            Self::Plan(_) => "cursor/create_plan",
        }
    }

    pub fn response(&self, accepted: bool, selections: &[Vec<String>]) -> Option<Value> {
        match self {
            Self::Plan(_) => {
                Some(json!({"outcome":{"outcome":if accepted {"accepted"} else {"rejected"}}}))
            }
            Self::Questions(request) if accepted => {
                if selections.len() != request.questions.len() {
                    return None;
                }
                let mut answers = Vec::new();
                for (question, selected) in request.questions.iter().zip(selections) {
                    if selected.is_empty()
                        || (!question.allow_multiple && selected.len() != 1)
                        || selected
                            .iter()
                            .any(|id| !question.options.iter().any(|option| &option.id == id))
                        || selected
                            .iter()
                            .collect::<std::collections::HashSet<_>>()
                            .len()
                            != selected.len()
                    {
                        return None;
                    }
                    answers.push(json!({"questionId":question.id,"selectedOptionIds":selected}));
                }
                Some(json!({"outcome":{"outcome":"answered","answers":answers}}))
            }
            Self::Questions(_) => Some(Self::cancelled_response()),
        }
    }

    pub fn cancelled_response() -> Value {
        json!({"outcome":{"outcome":"cancelled"}})
    }

    pub fn valid(&self) -> bool {
        if self.tool_call_id().is_empty() {
            return false;
        }
        match self {
            Self::Plan(request) => !request.plan.trim().is_empty(),
            Self::Questions(request) => {
                let mut ids = std::collections::HashSet::new();
                !request.questions.is_empty()
                    && request.questions.iter().all(|question| {
                        let mut option_ids = std::collections::HashSet::new();
                        !question.id.is_empty()
                            && ids.insert(&question.id)
                            && !question.prompt.trim().is_empty()
                            && !question.options.is_empty()
                            && question.options.iter().all(|option| {
                                !option.id.is_empty()
                                    && !option.label.trim().is_empty()
                                    && option_ids.insert(&option.id)
                            })
                    })
            }
        }
    }
}

// Requests may include private workspace text. SDK diagnostics expose only the type.
impl std::fmt::Debug for CursorAskQuestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CursorAskQuestion(<redacted>)")
    }
}
impl std::fmt::Debug for CursorCreatePlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CursorCreatePlan(<redacted>)")
    }
}
impl Drop for CursorAskQuestion {
    fn drop(&mut self) {
        self.tool_call_id.zeroize();
        self.title.zeroize();
    }
}
impl Drop for CursorQuestion {
    fn drop(&mut self) {
        self.id.zeroize();
        self.prompt.zeroize();
    }
}
impl Drop for CursorQuestionOption {
    fn drop(&mut self) {
        self.id.zeroize();
        self.label.zeroize();
    }
}
impl Drop for CursorCreatePlan {
    fn drop(&mut self) {
        self.tool_call_id.zeroize();
        self.name.zeroize();
        self.overview.zeroize();
        self.plan.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::*;
    use agent_client_protocol::{
        Agent,
        schema::v1::{
            AgentCapabilities, InitializeRequest, InitializeResponse, NewSessionRequest,
            NewSessionResponse, PromptRequest, PromptResponse, StopReason,
        },
    };

    #[tokio::test]
    async fn cursor_blocking_requests_receive_exact_answers_approval_and_cancellation() {
        let peer = Agent.builder()
            .on_receive_request(async |request: InitializeRequest, responder, _| {
                responder.respond(InitializeResponse::new(request.protocol_version).agent_capabilities(AgentCapabilities::new()))
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async |_: NewSessionRequest, responder, _| {
                responder.respond(NewSessionResponse::new("cursor-session"))
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async |_: PromptRequest, responder, connection| {
                let peer = connection.clone();
                connection.spawn(async move {
                    let question = json!({
                        "toolCallId":"question-call", "questions":[
                            {"id":"mode","prompt":"Which mode?","options":[{"id":"a","label":"Agent"},{"id":"p","label":"Plan"}]},
                            {"id":"features","prompt":"Which features?","options":[{"id":"x","label":"One"},{"id":"y","label":"Two"}],"allowMultiple":true}
                        ]
                    });
                    let answer = peer.send_request(agent_client_protocol::UntypedMessage::new("cursor/ask_question", question)?).block_task().await?;
                    assert_eq!(answer, json!({"outcome":{"outcome":"answered","answers":[
                        {"questionId":"mode","selectedOptionIds":["p"]},
                        {"questionId":"features","selectedOptionIds":["x","y"]}
                    ]}}));
                    for outcome in ["accepted", "rejected", "cancelled"] {
                        let plan = agent_client_protocol::UntypedMessage::new("cursor/create_plan", json!({"toolCallId":outcome,"plan":"Inspect then change.","todos":[]}))?;
                        assert_eq!(peer.send_request(plan).block_task().await?, json!({"outcome":{"outcome":outcome}}));
                    }
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                })?;
                Ok(())
            }, agent_client_protocol::on_receive_request!());
        let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            with_acp_agent_runtime_events(
                peer,
                "test".into(),
                AcpHostCapabilityPolicy::default(),
                event_tx,
                async move |runtime| {
                    let session = runtime
                        .start_session(NewSessionRequest::new(std::env::current_dir().unwrap()))
                        .await?;
                    let (result, ()) = tokio::join!(session.send_prompt("hello"), async {
                        for index in 0..4 {
                            let AcpClientEvent::CursorRequest {
                                request,
                                response_tx,
                            } = events.recv().await.unwrap()
                            else {
                                panic!("expected Cursor request")
                            };
                            assert!(request.valid());
                            let response = match index {
                                0 => request
                                    .response(
                                        true,
                                        &[vec!["p".into()], vec!["x".into(), "y".into()]],
                                    )
                                    .unwrap(),
                                1 => request.response(true, &[]).unwrap(),
                                2 => request.response(false, &[]).unwrap(),
                                _ => CursorRequest::cancelled_response(),
                            };
                            response_tx.send(Ok(response)).unwrap();
                        }
                    });
                    assert_eq!(result?.stop_reason, StopReason::EndTurn);
                    Ok(())
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
    }

    #[test]
    fn cursor_choices_reject_unknown_ids_and_private_text_is_redacted() {
        let question: CursorAskQuestion = serde_json::from_value(json!({"toolCallId":"q","title":"private-workspace-secret","questions":[
            {"id":"mode","prompt":"private-workspace-secret","options":[{"id":"a","label":"A"},{"id":"p","label":"P"}]}
        ]})).unwrap();
        assert!(!format!("{question:?}").contains("private-workspace-secret"));
        let request = CursorRequest::Questions(question);
        for selection in [vec![], vec!["unknown".into()], vec!["a".into(), "p".into()]] {
            assert!(request.response(true, &[selection]).is_none());
        }
        assert_eq!(
            request.response(false, &[]).unwrap(),
            json!({"outcome":{"outcome":"cancelled"}})
        );
        let plan: CursorCreatePlan =
            serde_json::from_value(json!({"toolCallId":"p","plan":"private-workspace-secret"}))
                .unwrap();
        assert!(!format!("{plan:?}").contains("private-workspace-secret"));
    }
}
