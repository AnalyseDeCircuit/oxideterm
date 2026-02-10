//! State persistence using redb + MessagePack (rmp-serde)
//! Handles session metadata, forward rules, and AI chat history persistence

pub mod ai_chat;
pub mod forwarding;
pub mod session;
pub mod store;

pub use ai_chat::{
    AiChatError, AiChatStats, AiChatStore, ContextSnapshot, ConversationMeta, FullConversation,
    PersistedMessage,
};
pub use forwarding::PersistedForward;
pub use session::{BufferConfig, PersistedSession, SessionPersistence};
pub use store::{StateError, StateStore};
