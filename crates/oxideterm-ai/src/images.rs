//! Image attachments retain their identity independently from a provider request.

use crate::{AiChatMessage, AiChatMessageMetadata};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const AI_IMAGE_MAX_BYTES: usize = 10 * 1024 * 1024;
pub const AI_IMAGE_MAX_PIXELS: u64 = 40_000_000;
pub const AI_IMAGE_MAX_ATTACHMENTS: usize = 8;
pub const AI_IMAGE_REQUEST_BYTES: usize = 12 * 1024 * 1024;

/// Normalizes uploaded and tool-returned pixels through the same bounded decoder.
pub fn prepare_image_attachment(name: String, bytes: Vec<u8>) -> anyhow::Result<AiImageAttachment> {
    use image::ImageDecoder;
    use std::io::Cursor;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= AI_IMAGE_MAX_BYTES,
        "image size"
    );
    let bytes = zeroize::Zeroizing::new(bytes);
    let mut reader =
        image::ImageReader::new(Cursor::new(bytes.as_slice())).with_guessed_format()?;
    anyhow::ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ),
        "image format"
    );
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    anyhow::ensure!(
        u64::from(width) * u64::from(height) <= AI_IMAGE_MAX_PIXELS,
        "image dimensions"
    );
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    // Normalize orientation and remove file metadata before an explicit send.
    if decoded.width().max(decoded.height()) > 2000 {
        decoded = decoded.resize(2000, 2000, image::imageops::FilterType::Lanczos3);
    }
    let mut full = Cursor::new(Vec::new());
    decoded.write_to(&mut full, image::ImageFormat::Png)?;
    let mut mime = "image/png";
    if full.get_ref().len() > 3 * 1024 * 1024 {
        full = Cursor::new(Vec::new());
        let mut rgba = decoded.to_rgba8();
        for pixel in rgba.pixels_mut() {
            let alpha = u16::from(pixel.0[3]);
            for channel in &mut pixel.0[..3] {
                *channel = ((u16::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
        let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut full, 90).encode_image(&rgb)?;
        mime = "image/jpeg";
    }
    anyhow::ensure!(full.get_ref().len() <= 3 * 1024 * 1024, "image size");
    let mut thumb = Cursor::new(Vec::new());
    decoded
        .thumbnail(160, 160)
        .write_to(&mut thumb, image::ImageFormat::Png)?;
    AiImageAttachment::new(
        name,
        mime.into(),
        decoded.width(),
        decoded.height(),
        full.get_ref(),
        thumb.get_ref(),
    )
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiImageAttachment {
    #[serde(rename = "_oxideImage")]
    pub(crate) image_marker: bool,
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub thumbnail: Arc<str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Arc<str>>,
}

impl std::fmt::Debug for AiImageAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiImageAttachment")
            .field("id", &self.id)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl AiImageAttachment {
    pub fn new(
        name: String,
        mime_type: String,
        width: u32,
        height: u32,
        bytes: &[u8],
        thumbnail: &[u8],
    ) -> anyhow::Result<Self> {
        use base64::Engine;
        anyhow::ensure!(
            width > 0 && height > 0 && u64::from(width) * u64::from(height) <= AI_IMAGE_MAX_PIXELS,
            "ai_image_dimensions"
        );
        anyhow::ensure!(
            !bytes.is_empty() && bytes.len() <= AI_IMAGE_MAX_BYTES && thumbnail.len() <= 256 * 1024,
            "ai_image_size"
        );
        anyhow::ensure!(
            matches!(
                mime_type.as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            ),
            "ai_image_format"
        );
        let encode = |bytes: &[u8]| {
            Arc::<str>::from(base64::engine::general_purpose::STANDARD.encode(bytes))
        };
        Ok(Self {
            image_marker: true,
            id: uuid::Uuid::new_v4().to_string(),
            name,
            mime_type,
            width,
            height,
            bytes: bytes.len(),
            thumbnail: encode(thumbnail),
            data: Some(encode(bytes)),
        })
    }

    pub fn preview_only(&self) -> Self {
        let mut image = self.clone();
        image.data = None;
        image
    }

    pub fn data_url(&self) -> Option<String> {
        self.data
            .as_ref()
            .map(|data| format!("data:{};base64,{data}", self.mime_type))
    }

    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.width > 0
                && self.height > 0
                && u64::from(self.width) * u64::from(self.height) <= AI_IMAGE_MAX_PIXELS,
            "ai_image_dimensions"
        );
        anyhow::ensure!(
            self.bytes > 0 && self.bytes <= AI_IMAGE_MAX_BYTES && self.thumbnail.len() <= 350_000,
            "ai_image_size"
        );
        anyhow::ensure!(
            matches!(
                self.mime_type.as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            ),
            "ai_image_format"
        );
        Ok(())
    }
}

pub fn message_images(message: &AiChatMessage) -> &[AiImageAttachment] {
    message
        .metadata
        .as_ref()
        .map_or(&[], |metadata| &metadata.images)
}

pub fn set_message_images(message: &mut AiChatMessage, images: Vec<AiImageAttachment>) {
    if images.is_empty() {
        return;
    }
    message
        .metadata
        .get_or_insert_with(|| AiChatMessageMetadata {
            kind: "attachments".into(),
            original_count: None,
            compacted_at_ms: None,
            original_messages: None,
            original_ref: None,
            original_user_count: None,
            images: Vec::new(),
        })
        .images = images;
}

pub(crate) fn validate_image_request<'a>(
    images: impl Iterator<Item = &'a AiImageAttachment>,
) -> anyhow::Result<()> {
    let mut bytes = 0usize;
    for image in images {
        image.validate()?;
        anyhow::ensure!(image.data.is_some(), "ai_image_missing");
        bytes = bytes.saturating_add(image.bytes);
    }
    anyhow::ensure!(bytes <= AI_IMAGE_REQUEST_BYTES, "ai_image_request_limit");
    Ok(())
}

pub(crate) fn image_message_content(message: &AiChatMessage, protocol: &str) -> serde_json::Value {
    image_content_parts(&message.content, message_images(message), protocol)
}

pub(crate) fn image_content_parts(
    text: &str,
    images: &[AiImageAttachment],
    protocol: &str,
) -> serde_json::Value {
    use serde_json::json;
    let mut parts = Vec::new();
    if !text.is_empty() || images.is_empty() {
        parts.push(match protocol {
            "gemini" => json!({"text":text}),
            "responses" => json!({"type":"input_text", "text":text}),
            _ => json!({"type":"text", "text":text}),
        });
    }
    for image in images {
        let data = image.data.as_deref().unwrap_or_default();
        parts.push(match protocol {
            "anthropic" => json!({"type":"image", "source":{"type":"base64", "media_type":image.mime_type, "data":data}}),
            "gemini" => json!({"inlineData":{"mimeType":image.mime_type,"data":data}}),
            "responses" => json!({"type":"input_image","image_url":image.data_url(),"detail":"auto"}),
            _ => json!({"type":"image_url","image_url":{"url":image.data_url()}}),
        });
    }
    serde_json::Value::Array(parts)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiImageInput {
    #[default]
    Auto,
    Enabled,
    Disabled,
}

/// Explicit configuration applies to the endpoint's model, not the provider brand.
/// None means unknown, not unsupported; callers must not reject it as text-only.
pub fn model_image_input(provider: &str, model: &str, setting: AiImageInput) -> Option<bool> {
    match setting {
        AiImageInput::Enabled => return Some(true),
        AiImageInput::Disabled => return Some(false),
        _ => {}
    }
    let model = model.to_ascii_lowercase();
    if model.contains("audio")
        || model.contains("realtime")
        || model.contains("embedding")
        || model.contains("transcribe")
        || model.contains("image")
    {
        return None;
    }
    match provider {
        "openai"
            if model.starts_with("o3-mini")
                || model.starts_with("o1-mini")
                || model.starts_with("o1-preview") =>
        {
            Some(false)
        }
        "openai"
            if ["gpt-4o", "gpt-4.1", "gpt-5", "gpt-6", "o1", "o3", "o4-mini"]
                .iter()
                .any(|prefix| model.starts_with(prefix)) =>
        {
            Some(true)
        }
        "anthropic"
            if model.starts_with("claude-3")
                || model.starts_with("claude-sonnet-4")
                || model.starts_with("claude-opus-4")
                || model.starts_with("claude-haiku-4") =>
        {
            Some(true)
        }
        "gemini" if model.starts_with("gemini-") => Some(true),
        "openai" if model.starts_with("gpt-3.5") => Some(false),
        "deepseek"
            if matches!(
                model.as_str(),
                "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp"
            ) =>
        {
            Some(true)
        }
        "deepseek"
            if matches!(
                model.as_str(),
                "deepseek-chat" | "deepseek-reasoner" | "deepseek-v4-pro"
            ) =>
        {
            Some(false)
        }
        "kimi" if model == "kimi-k2.5" => Some(true),
        "mimo"
            if matches!(
                model.as_str(),
                "mimo-v2.6-pro" | "mimo-v2.6-flash" | "mimo-v2.6-pro-ultraspeed" | "mimo-v2.5"
            ) =>
        {
            Some(true)
        }
        "glm"
            if matches!(
                model.as_str(),
                "glm-4.6v" | "glm-4.6v-flash" | "glm-4.6v-flashx"
            ) =>
        {
            Some(true)
        }
        "xai"
            if model == "grok-4"
                || model.starts_with("grok-4-")
                || model.starts_with("grok-4.")
                || model.starts_with("grok-2-vision-") =>
        {
            Some(true)
        }
        "ollama"
            if matches!(
                model.split(':').next(),
                Some("qwen3-vl" | "qwen2.5vl" | "llava" | "llama3.2-vision")
            ) =>
        {
            Some(true)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiImageTokenEstimate {
    pub tokens: usize,
    pub known_rule: bool,
}

/// Estimates default-detail inputs from the providers' image-token documentation.
/// Custom endpoints and unspecified sizing rules remain provisional; request usage is authoritative.
/// https://developers.openai.com/api/docs/guides/images-vision#calculating-costs
/// https://platform.claude.com/docs/en/build-with-claude/vision#resolution-and-token-cost
pub fn estimate_image_tokens(
    provider: &str,
    model: &str,
    width: u32,
    height: u32,
) -> AiImageTokenEstimate {
    let (mut w, mut h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let model = model.to_ascii_lowercase();
    let family = |name: &str| {
        model == name
            || model
                .strip_prefix(name)
                .is_some_and(|s| s.starts_with("-20"))
    };
    let tile = if family("gpt-4o-mini") {
        Some((2833, 5667))
    } else if family("gpt-4o") || family("gpt-4.1") {
        Some((85, 170))
    } else if family("gpt-5") || family("gpt-5.1") {
        Some((70, 140))
    } else if family("o1") || family("o1-pro") || family("o3") {
        Some((75, 150))
    } else {
        None
    };
    let patches = if family("gpt-6-astra")
        || family("gpt-5.6-sol")
        || family("gpt-5.6-terra")
        || family("gpt-5.6-luna")
    {
        Some((65535.0, 30000, 1.2))
    } else if family("gpt-5.5") {
        Some((6000.0, 10000, 1.2))
    } else if family("gpt-5.4") || family("gpt-5.4-mini") || family("gpt-5.4-nano") {
        Some((2048.0, 2500, 1.2))
    } else if family("gpt-5.2") {
        Some((2048.0, 6144, 1.2))
    } else if family("gpt-4.1-mini") {
        Some((2048.0, 6144, 1.62))
    } else {
        None
    };
    let (tokens, known_rule) = if provider == "mimo"
        && model_image_input(provider, &model, AiImageInput::Auto) == Some(true)
    {
        // MiMo rounds to merged 32-pixel patches, with an 8 Ki–8 Mi pixel range.
        // https://mimo.mi.com/docs/en-US/quick-start/usage-guide/multimodal-understanding/image-understanding
        let (mut rounded_w, mut rounded_h) = (
            (w / 32.0).round_ties_even() * 32.0,
            (h / 32.0).round_ties_even() * 32.0,
        );
        if rounded_w * rounded_h > 8_388_608.0 {
            let scale = (w * h / 8_388_608.0).sqrt();
            rounded_w = (w / scale / 32.0).floor() * 32.0;
            rounded_h = (h / scale / 32.0).floor() * 32.0;
        } else if rounded_w * rounded_h < 8192.0 {
            let scale = (8192.0 / (w * h)).sqrt();
            rounded_w = (w * scale / 32.0).ceil() * 32.0;
            rounded_h = (h * scale / 32.0).ceil() * 32.0;
        }
        ((rounded_w * rounded_h / 1024.0) as usize, true)
    } else if let Some((edge, cap, multiplier)) = patches {
        let scale = (edge / w.max(h)).min(1.0);
        w = (w * scale).floor().max(1.0);
        h = (h * scale).floor().max(1.0);
        if (w / 32.0).ceil() as usize * (h / 32.0).ceil() as usize > cap {
            let scale = ((32.0 * 32.0 * cap as f64) / (w * h)).sqrt();
            let adjust = ((w * scale / 32.0).floor() / (w * scale / 32.0))
                .min((h * scale / 32.0).floor() / (h * scale / 32.0));
            w = (w * scale * adjust).floor().max(1.0);
            h = (h * scale * adjust).floor().max(1.0);
        }
        (
            ((w / 32.0).ceil() * (h / 32.0).ceil() * multiplier).ceil() as usize,
            provider == "openai",
        )
    } else if let Some((base, per_tile)) = tile {
        let scale = (2048.0 / w.max(h)).min(1.0);
        w *= scale;
        h *= scale;
        let scale = (768.0 / w.min(h)).min(1.0);
        w *= scale;
        h *= scale;
        (
            base + per_tile
                * (w.floor() / 512.0).ceil() as usize
                * (h.floor() / 512.0).ceil() as usize,
            provider == "openai",
        )
    } else if provider == "anthropic"
        && (model.starts_with("claude-sonnet-4")
            || model.starts_with("claude-opus-4")
            || model.starts_with("claude-haiku-4"))
    {
        // Reserve the documented patch cap when server resizing is needed.
        let raw = width.div_ceil(28) as usize * height.div_ceil(28) as usize;
        let high = model
            .split('-')
            .skip_while(|part| *part != "4")
            .nth(1)
            .and_then(|part| part.parse::<u8>().ok())
            .is_some_and(|minor| minor >= 7);
        let (cap, edge) = if high { (4784, 2576) } else { (1568, 1568) };
        // This is a budget upper estimate; server padding can use fewer patches.
        (raw.min(cap), raw <= cap && width.max(height) <= edge)
    } else if provider == "gemini" && model.starts_with("gemini-") {
        (
            258 * width.div_ceil(768) as usize * height.div_ceil(768) as usize,
            false,
        )
    } else {
        // An unknown tokenizer still consumes budget. The UI labels this reserve as uncertain.
        (
            width.div_ceil(28) as usize * height.div_ceil(28) as usize,
            false,
        )
    };
    AiImageTokenEstimate {
        tokens: tokens.max(1),
        known_rule,
    }
}

pub(crate) fn message_image_tokens(message: &AiChatMessage, provider: &str) -> usize {
    message_images(message)
        .iter()
        .map(|image| {
            estimate_image_tokens(
                provider,
                message.model.as_deref().unwrap_or_default(),
                image.width,
                image.height,
            )
            .tokens
        })
        .sum()
}

/// Apply a request's frozen policy to a copy; persisted history retains its images.
pub fn prepare_image_history(messages: &mut [AiChatMessage], config: &crate::AiChatStreamConfig) {
    for message in messages {
        if message_images(message).is_empty() {
            continue;
        }
        if config.omit_history_images && message.role != crate::AiChatRole::Tool {
            let images = std::mem::take(&mut message.metadata.as_mut().unwrap().images);
            for (index, _) in images.iter().enumerate() {
                message.content.push_str(&format!(
                    "\n[Image {} omitted: text-only conversation mode.]",
                    index + 1
                ));
            }
            if let Some(providers) = message
                .turn
                .as_mut()
                .and_then(|turn| turn.get_mut("providerParts"))
                .and_then(serde_json::Value::as_object_mut)
            {
                for rounds in providers
                    .values_mut()
                    .filter_map(serde_json::Value::as_array_mut)
                {
                    for round in rounds {
                        for result in round
                            .get_mut("results")
                            .and_then(serde_json::Value::as_array_mut)
                            .into_iter()
                            .flatten()
                        {
                            if result
                                .as_object_mut()
                                .is_some_and(|object| object.remove("imageIds").is_some())
                            {
                                let text = result["output"].as_str().unwrap_or_default();
                                result["output"] = serde_json::json!(format!(
                                    "{text}\n[Tool images omitted: text-only conversation mode.]"
                                ));
                            }
                        }
                    }
                }
            }
        } else {
            message.model = Some(config.model.clone());
        }
    }
}
