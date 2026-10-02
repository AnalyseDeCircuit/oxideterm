use futures_util::future::BoxFuture;
use tokio::sync::mpsc;

use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

/// The agent loop owns the request future, including its credentials and network stream.
/// Dropping a cancelled loop must not leave an independently spawned model request alive.
pub struct AgentModelRequest {
    request: Option<BoxFuture<'static, ()>>,
    events: mpsc::UnboundedReceiver<AiStreamEvent>,
}

impl AgentModelRequest {
    pub fn start(config: AiChatStreamConfig, history: Vec<AiChatMessage>) -> Self {
        let (sender, events) = mpsc::unbounded_channel();
        let history = crate::sanitize_api_messages_for_provider(history);
        Self {
            request: Some(Box::pin(crate::stream_chat_completion(
                config, history, sender,
            ))),
            events,
        }
    }

    pub async fn next_event(&mut self) -> Option<AiStreamEvent> {
        if let Some(request) = self.request.as_mut() {
            tokio::select! {
                event = self.events.recv() => return event,
                () = request => self.request = None,
            }
        }
        // Providers can finish after enqueueing several events. Deliver those before EOF.
        self.events.recv().await
    }
}
