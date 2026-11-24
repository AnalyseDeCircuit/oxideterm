use super::{AiPendingChatStream, AiWorkspaceEntity};
use gpui::{App, Context};

use oxideterm_ai::AiImageAttachment;

#[cfg(not(target_family = "wasm"))]
pub enum AiImageSource {
    File(std::path::PathBuf),
    Clipboard(Vec<u8>),
}

pub(crate) fn ai_image_model_key(provider: &str, endpoint: &str, model: &str) -> String {
    serde_json::json!([provider, endpoint, model]).to_string()
}

#[cfg(not(target_family = "wasm"))]
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
    #[cfg(not(target_family = "wasm"))]
    pub fn select_chat_images(
        &mut self,
        prompt: String,
        import: impl FnOnce(Vec<std::path::PathBuf>, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(prompt.into()),
        });
        let conversation = self.conversation_state().active_conversation_id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let paths = receiver.await;
            let _ = weak.update(cx, |ai, cx| {
                ai.set_chat_image_import(None);
                if ai.conversation_state().active_conversation_id != conversation {
                    return;
                }
                if let Ok(Ok(Some(paths))) = paths {
                    cx.defer(move |cx| import(paths, cx));
                }
                cx.notify();
            });
        });
        self.set_chat_image_import(Some(task));
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn import_chat_images(
        &mut self,
        sources: Vec<AiImageSource>,
        validate: impl FnOnce(&App) -> Result<(), String> + 'static,
        notify: impl FnOnce(AiImageImportNotice, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        let runtime = self.task_runtime().clone();
        self.import_prepared_chat_images(
            sources.len(),
            move || {
                let work = runtime.spawn_blocking(move || {
                    sources
                        .into_iter()
                        .map(prepare_ai_image)
                        .collect::<anyhow::Result<Vec<_>>>()
                });
                async move { work.await.ok()?.ok() }
            },
            validate,
            notify,
            cx,
        )
    }

    pub fn import_prepared_chat_images<
        F: std::future::Future<Output = Option<Vec<AiImageAttachment>>> + 'static,
    >(
        &mut self,
        count: usize,
        prepare: impl FnOnce() -> F,
        validate: impl FnOnce(&App) -> Result<(), String> + 'static,
        notify: impl FnOnce(AiImageImportNotice, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if count
            > oxideterm_ai::AI_IMAGE_MAX_ATTACHMENTS.saturating_sub(self.chat_ui().images.len())
        {
            return false;
        }
        let conversation = self.conversation_state().active_conversation_id.clone();
        let work = prepare();
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
                        Some(images)
                            if images.len()
                                <= oxideterm_ai::AI_IMAGE_MAX_ATTACHMENTS
                                    .saturating_sub(ai.chat_ui().images.len()) =>
                        {
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

#[cfg(not(target_family = "wasm"))]
impl AiImageSource {
    pub fn from_clipboard(item: &gpui::ClipboardItem) -> Option<Vec<Self>> {
        item.entries().iter().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => Some(vec![Self::Clipboard(image.bytes.clone())]),
            gpui::ClipboardEntry::ExternalPaths(paths) => {
                Some(paths.paths().iter().cloned().map(Self::File).collect())
            }
            _ => None,
        })
    }
}

pub fn check_chat_image_model(
    config: &oxideterm_ai::AiChatStreamConfig,
    i18n: &oxideterm_i18n::I18n,
) -> Result<(), String> {
    if config.execution_backend == oxideterm_ai::AiExecutionBackend::Acp
        || oxideterm_ai::model_image_input(&config.provider_type, &config.model, config.image_input)
            != Some(false)
    {
        Ok(())
    } else {
        Err(i18n.t("ai.images.model_unavailable"))
    }
}
