use oxideterm_ai::agent::{
    AgentError, AgentMessage, AgentModel, AgentResult, AgentRunRef, AgentSnapshot, AgentText,
};

pub trait AgentUpdates {
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + '_>>;
}

pub trait AiAgentHost {
    fn subscribe(&self) -> Box<dyn AgentUpdates>;
    fn snapshots(&self, conversation: &str) -> Vec<AgentSnapshot>;
    fn snapshot(&self, run: &AgentRunRef) -> Result<AgentSnapshot, AgentError>;
    fn communication(&self, run: &AgentRunRef) -> Vec<AgentMessage>;
    fn accepts_messages(&self, run: &AgentRunRef) -> bool;
    fn supplement(&self, run: &AgentRunRef, content: AgentText) -> Result<(), AgentError>;
    fn change_queued_model(&self, run: &AgentRunRef, model: AgentModel) -> Result<(), AgentError>;
    fn stop_child(&self, run: &AgentRunRef) -> Result<(), AgentError>;
    fn cancel_group(&self, parent: &AgentRunRef) -> Result<(), AgentError>;
    fn complete(&self, run: &AgentRunRef, result: AgentResult) -> Result<(), AgentError>;
    fn finish_group(&self, parent: &AgentRunRef) -> Result<(), AgentError>;
    fn remove_conversation(&self, conversation: &str);
}

#[cfg(not(target_family = "wasm"))]
impl AgentUpdates for tokio::sync::watch::Receiver<u64> {
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + '_>> {
        Box::pin(async move { self.changed().await.is_ok() })
    }
}

#[cfg(not(target_family = "wasm"))]
impl AiAgentHost for oxideterm_ai::agent::AgentRuntime {
    fn subscribe(&self) -> Box<dyn AgentUpdates> {
        Box::new(self.subscribe())
    }
    fn snapshots(&self, conversation: &str) -> Vec<AgentSnapshot> {
        self.snapshots(conversation)
    }
    fn snapshot(&self, run: &AgentRunRef) -> Result<AgentSnapshot, AgentError> {
        self.snapshot(run)
    }
    fn communication(&self, run: &AgentRunRef) -> Vec<AgentMessage> {
        self.communication(run)
    }
    fn accepts_messages(&self, run: &AgentRunRef) -> bool {
        self.accepts_messages(run)
    }
    fn supplement(&self, run: &AgentRunRef, content: AgentText) -> Result<(), AgentError> {
        self.send(
            run,
            run,
            oxideterm_ai::agent::AgentMessageKind::UserSupplement,
            content,
        )
        .map(|_| ())
    }
    fn change_queued_model(&self, run: &AgentRunRef, model: AgentModel) -> Result<(), AgentError> {
        self.change_queued_model(run, model)
    }
    fn stop_child(&self, run: &AgentRunRef) -> Result<(), AgentError> {
        self.stop(&self.parent_run(run)?, run)
    }
    fn cancel_group(&self, parent: &AgentRunRef) -> Result<(), AgentError> {
        self.cancel_group(parent)
    }
    fn complete(&self, run: &AgentRunRef, result: AgentResult) -> Result<(), AgentError> {
        self.complete(run, result)
    }
    fn finish_group(&self, parent: &AgentRunRef) -> Result<(), AgentError> {
        self.finish_group(parent)
    }
    fn remove_conversation(&self, conversation: &str) {
        self.remove_conversation(conversation)
    }
}
