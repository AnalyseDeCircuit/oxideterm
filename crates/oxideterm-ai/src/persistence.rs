use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::AiChatState;

#[derive(Clone, Debug)]
pub struct AiChatPersistenceStore {
    path: PathBuf,
}

impl AiChatPersistenceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(path: impl Into<PathBuf>) -> Result<(Self, AiChatState)> {
        let store = Self::new(path);
        let state = store.load_state()?;
        Ok((store, state))
    }

    pub fn load_state(&self) -> Result<AiChatState> {
        if !self.path.exists() {
            return Ok(AiChatState::default());
        }
        let bytes = fs::read(&self.path).with_context(|| {
            format!(
                "failed to read AI chat persistence file {}",
                self.path.display()
            )
        })?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(AiChatState::default());
        }
        let mut state: AiChatState = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to parse AI chat persistence file {}",
                self.path.display()
            )
        })?;
        if state
            .active_conversation_id
            .as_ref()
            .is_some_and(|active_id| {
                !state
                    .conversations
                    .iter()
                    .any(|conversation| conversation.id == *active_id)
            })
        {
            state.active_conversation_id = state
                .conversations
                .first()
                .map(|conversation| conversation.id.clone());
        }
        Ok(state)
    }

    pub fn save_state(&self, state: &AiChatState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create AI chat persistence directory {}",
                    parent.display()
                )
            })?;
        }
        let bytes = serde_json::to_vec_pretty(state)
            .context("failed to serialize AI chat persistence state")?;
        let tmp_path = self.tmp_path();
        fs::write(&tmp_path, bytes).with_context(|| {
            format!(
                "failed to write AI chat persistence file {}",
                tmp_path.display()
            )
        })?;
        fs::rename(&tmp_path, &self.path).with_context(|| {
            format!(
                "failed to replace AI chat persistence file {}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    fn tmp_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ai_conversations.json");
        self.path.with_file_name(format!("{file_name}.tmp"))
    }
}
