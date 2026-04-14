// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! State persistence using redb + MessagePack (rmp-serde)
//! Handles session metadata, forward rules, AI chat history and agent task persistence

pub mod agent_history;
pub mod ai_chat;
pub mod forwarding;
pub mod lazy_store;
pub mod lazy_state;
pub mod session;
pub mod store;

pub use agent_history::{AgentHistoryError, AgentHistoryStore};
pub use ai_chat::{
    AiChatError, AiChatStats, AiChatStore, ContextSnapshot, ConversationMeta, FullConversation,
    PersistedMessage, PersistedToolCall,
};
pub use forwarding::PersistedForward;
pub use lazy_store::LazyManagedStore;
pub use lazy_state::LazyStateStore;
pub use session::{BufferConfig, PersistedSession, SessionPersistence};
pub use store::{StateError, StateStore};
