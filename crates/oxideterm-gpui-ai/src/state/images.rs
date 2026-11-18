use super::{AiPendingChatStream, AiWorkspaceEntity};
use gpui::{App, Context};

use oxideterm_ai::AiImageAttachment;

pub enum AiImageSource {
    File(std::path::PathBuf),
    Clipboard(Vec<u8>),
}

pub(crate) fn ai_image_model_key(provider: &str, endpoint: &str, model: &str) -> String {
    serde_json::json!([provider, endpoint, model]).to_string()
}

fn prepare_ai_image(source: AiImageSource) -> anyhow::Result<AiImageAttachment> {
    use std::io::Read;
    let (name, bytes) = match source {
        AiImageSource::File(path) => {
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(oxideterm_ai::AI_IMAGE_MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                bytes,
            )
        }
        AiImageSource::Clipboard(bytes) => (String::new(), bytes),
    };
    oxideterm_ai::prepare_image_attachment(name, bytes)
}

pub enum AiImageImportNotice {
    Unsupported(String),
    Invalid,
}

impl AiWorkspaceEntity {
    pub fn import_chat_images(
        &mut self,
        sources: Vec<AiImageSource>,
        validate: impl FnOnce(&App) -> Result<(), String> + 'static,
        notify: impl FnOnce(AiImageImportNotice, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if sources.len() + self.chat_ui().images.len() > oxideterm_ai::AI_IMAGE_MAX_ATTACHMENTS {
            return false;
        }
        let conversation = self.conversation_state().active_conversation_id.clone();
        let work = self.task_runtime.spawn_blocking(move || {
            sources
                .into_iter()
                .map(prepare_ai_image)
                .collect::<anyhow::Result<Vec<_>>>()
        });
        let task = cx.spawn(async move |weak, cx| {
            let result = work.await;
            let validation = cx.update(|cx| {
                let owner = weak.upgrade()?;
                (owner.read(cx).conversation_state().active_conversation_id == conversation)
                    .then(|| validate(cx))
            });
            let _ = weak.update(cx, |ai, cx| {
                ai.set_chat_image_import(None);
                if ai.conversation_state().active_conversation_id != conversation {
                    return;
                }
                let Some(validation) = validation else { return };
                let notice = match validation {
                    Err(error) => Some(AiImageImportNotice::Unsupported(error)),
                    Ok(()) => match result {
                        Ok(Ok(images)) => {
                            ai.chat_images_mut().extend(images);
                            None
                        }
                        _ => Some(AiImageImportNotice::Invalid),
                    },
                };
                if let Some(notice) = notice {
                    cx.defer(move |cx| notify(notice, cx));
                }
                cx.notify();
            });
        });
        self.set_chat_image_import(Some(task));
        cx.notify();
        true
    }

    pub fn consume_text_only_image_request(&mut self, id: &str) -> Option<AiPendingChatStream> {
        let mut pending = self.pending_image_requests.remove(id)?;
        pending.config.omit_history_images = true;
        let key = ai_image_model_key(
            pending.config.provider_id.as_deref().unwrap_or_default(),
            &pending.config.base_url,
            &pending.config.model,
        );
        if let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({}));
            if let Some(fields) = metadata.as_object_mut() {
                let models = fields
                    .entry("textOnlyImageModels")
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(models) = models.as_object_mut() {
                    models.insert(key, serde_json::json!(true));
                }
            }
        }
        self.persist_chat_state();
        Some(pending)
    }
}
