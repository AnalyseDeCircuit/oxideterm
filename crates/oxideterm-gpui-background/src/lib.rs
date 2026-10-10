//! Shared background layout, static cache and native-window playback ownership.

mod cache;
mod clock;
#[cfg(target_os = "linux")]
mod dma_buf;
mod effect;
mod gpu_budget;
mod layer;
mod player;
mod poster;
mod preview;
mod quality;
mod scene;
pub use scene::{CameraMotion, CameraPreferences, ScenePreferences, note_input_activity};

pub use cache::{BackgroundImageRenderCache, BackgroundImageTargetSize, background_display_target};
pub use effect::{
    GeneratedEffectKind, GeneratedEffectPreferences, MAX_EFFECT_DENSITY, MIN_EFFECT_DENSITY,
};
pub use layer::{background_image_layer, background_object_fit};
pub use oxideterm_background_media::{BackgroundFit, PlaybackLimits};
pub use player::{background_layer, is_streaming_source};
pub use poster::poster_source;
pub use preview::BackgroundPreview;

#[derive(Clone, Copy)]
pub enum BackgroundFailure {
    Unsupported,
    Decode,
    ResourceExhausted,
    InvalidOutput,
}

#[derive(Clone)]
pub struct BackgroundPreferences {
    pub scene: ScenePreferences,
    pub path: std::path::PathBuf,
    pub opacity: f32,
    pub blur: f32,
    pub fit: BackgroundFit,
    pub alignment: (f32, f32),
    pub effect: Option<GeneratedEffectPreferences>,
    pub readability: Option<ReadingOverlay>,
    pub limits: PlaybackLimits,
    pub on_failure: Option<std::sync::Arc<dyn Fn(BackgroundFailure) + Send + Sync>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadingOverlay {
    pub color: u32,
    pub opacity: f32,
}

impl std::fmt::Debug for BackgroundPreferences {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundPreferences")
            .field("path", &self.path)
            .field("opacity", &self.opacity)
            .field("blur", &self.blur)
            .field("fit", &self.fit)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
