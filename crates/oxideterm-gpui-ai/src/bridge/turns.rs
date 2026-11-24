use super::{HostTask, TurnSender};
use crate::{
    state::AiWorkspaceEntity,
    stream::{PortableStreamDelivery, PortableStreamEvent},
};

/// A producer can deliver only to the generation and assistant message that opened it.
#[derive(Clone)]
pub struct TurnOutput {
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    sender: TurnSender,
}

impl TurnOutput {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }
    pub fn assistant_id(&self) -> &str {
        &self.assistant_id
    }
    pub fn deliver(&self, event: PortableStreamEvent) -> bool {
        self.sender
            .send(PortableStreamDelivery {
                generation: self.generation,
                conversation_id: self.conversation_id.clone(),
                assistant_id: self.assistant_id.clone(),
                event,
            })
            .is_ok()
    }
}

/// The host owns provider/tool or ACP execution, including authorization and cancellation.
pub trait AiTurnHost {
    fn start(
        &self,
        config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
        output: TurnOutput,
    ) -> HostTask;
}

/// Live ACP controls address the host's thread, not a possibly stale transcript snapshot.
pub trait AiAcpSessionHost {
    fn session(&self, conversation: &str) -> Option<crate::state::AiAcpSessionState>;
    fn select_config(
        &self,
        conversation: &str,
        selection: oxideterm_ai::AcpSessionConfigSelection,
    ) -> bool;
    fn select_mode(&self, conversation: &str, mode: String) -> bool;
    fn cancel_prompt(&self, conversation: &str) -> bool;
}

pub struct LocalCommandRequest {
    pub conversation_id: String,
    pub assistant_id: String,
    pub generation: u64,
    pub call_id: String,
    pub command: zeroize::Zeroizing<String>,
    pub cwd: Option<String>,
    pub approved: bool,
}

pub trait AiLocalCommandHost {
    /// The execution owner validates policy and leases at dispatch. Dropping the future cancels observation and releases those leases.
    fn execute(
        &self,
        request: LocalCommandRequest,
    ) -> super::HostFuture<oxideterm_ai::AiExecutedToolResult>;
}

impl AiWorkspaceEntity {
    pub fn start_hosted_turn(
        &mut self,
        host: &dyn AiTurnHost,
        conversation_id: String,
        assistant_id: String,
        config: oxideterm_ai::AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
    ) -> u64 {
        let (generation, legacy_sender) =
            self.begin_chat_stream(conversation_id.clone(), assistant_id.clone());
        #[cfg(target_family = "wasm")]
        let sender = legacy_sender;
        #[cfg(not(target_family = "wasm"))]
        let sender = {
            drop(legacy_sender);
            self.host_stream_sender()
        };
        let task = host.start(
            config,
            messages,
            TurnOutput {
                generation,
                conversation_id,
                assistant_id,
                sender,
            },
        );
        self.set_host_stream_task(generation, task);
        generation
    }
}
