use crate::{
    AiChatMessage, AiChatRole, AiChatState, AiConversation, current_terminal_context_system_message,
};

impl AiChatState {
    pub fn create_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now_ms: i64,
        profile_id: Option<String>,
    ) -> String {
        let session_metadata = {
            let mut metadata = serde_json::Map::new();
            metadata.insert("conversationId".to_string(), serde_json::json!(&id));
            metadata.insert("origin".to_string(), serde_json::json!("sidebar"));
            if let Some(profile_id) = profile_id.as_ref() {
                metadata.insert("profileId".to_string(), serde_json::json!(profile_id));
            }
            Some(serde_json::Value::Object(metadata))
        };
        let title = title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "New Chat".to_string());
        let conversation = AiConversation {
            id: id.clone(),
            title,
            messages: Vec::new(),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            origin: "sidebar".to_string(),
            profile_id,
            message_count: 0,
            session_id: None,
            session_metadata,
            messages_loaded: true,
        };
        self.conversations.insert(0, conversation);
        self.active_conversation_id = Some(id.clone());
        id
    }

    pub fn active_conversation(&self) -> Option<&AiConversation> {
        let active_id = self.active_conversation_id.as_deref()?;
        self.conversations
            .iter()
            .find(|conversation| conversation.id == active_id)
    }

    pub fn active_conversation_mut(&mut self) -> Option<&mut AiConversation> {
        let active_id = self.active_conversation_id.clone()?;
        self.conversations
            .iter_mut()
            .find(|conversation| conversation.id == active_id)
    }

    pub fn set_active_conversation(&mut self, id: String) {
        if self
            .conversations
            .iter()
            .any(|conversation| conversation.id == id)
        {
            self.active_conversation_id = Some(id);
        }
    }

    pub fn delete_conversation(&mut self, id: &str) {
        self.conversations
            .retain(|conversation| conversation.id != id);
        if self.active_conversation_id.as_deref() == Some(id) {
            self.active_conversation_id = self
                .conversations
                .first()
                .map(|conversation| conversation.id.clone());
        }
    }

    pub fn clear_conversations(&mut self) {
        self.conversations.clear();
        self.active_conversation_id = None;
    }

    pub fn rename_conversation(&mut self, id: &str, title: String, now_ms: i64) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)
        {
            conversation.title = title.to_string();
            conversation.updated_at_ms = now_ms;
        }
    }

    pub fn ensure_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now_ms: i64,
        profile_id: Option<String>,
    ) -> String {
        self.active_conversation_id
            .clone()
            .unwrap_or_else(|| self.create_conversation(id, title, now_ms, profile_id))
    }

    pub fn add_message(&mut self, conversation_id: &str, message: AiChatMessage) {
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            conversation.updated_at_ms = message.timestamp_ms;
            conversation.messages.push(message);
            conversation.message_count = conversation.messages.len();
        }
    }

    pub fn update_message(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        update: impl FnOnce(&mut AiChatMessage),
    ) {
        if let Some(message) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
            })
        {
            update(message);
        }
    }
}

pub fn generate_chat_title(first_message: &str) -> String {
    let cleaned = first_message.replace('\n', " ").trim().to_string();
    let mut chars = cleaned.chars();
    let title = chars.by_ref().take(30).collect::<String>();
    if chars.next().is_some() {
        format!("{title}...")
    } else {
        title
    }
}

pub fn apply_chat_request_overrides(
    history: &mut Vec<AiChatMessage>,
    request_content: Option<String>,
    task_system_prompt: Option<String>,
) {
    if let Some(request_content) = request_content
        && let Some(message) = history
            .iter_mut()
            .rev()
            .find(|message| message.role == crate::AiChatRole::User)
    {
        message.content = request_content;
    }
    let current_context = history
        .iter()
        .rev()
        .find(|message| message.role == AiChatRole::User)
        .and_then(|message| message.context.as_deref())
        .map(str::trim)
        .filter(|context| !context.is_empty())
        .map(current_terminal_context_system_message);
    let mut system_messages = Vec::new();
    if let Some(task_system_prompt) = task_system_prompt {
        system_messages.push(AiChatMessage {
            id: "task-mode".to_string(),
            role: crate::AiChatRole::System,
            content: task_system_prompt,
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        });
    }
    if let Some(current_context) = current_context {
        system_messages.push(AiChatMessage {
            id: "current-terminal-context".to_string(),
            role: AiChatRole::System,
            content: current_context,
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        });
    }
    history.splice(0..0, system_messages);
}
