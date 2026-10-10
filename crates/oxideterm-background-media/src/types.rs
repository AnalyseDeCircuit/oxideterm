use crate::PixelLease;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackgroundFit {
    Cover,
    Contain,
    Fill,
    Tile,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PlaybackLimits {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_fps: Option<u32>,
}

impl PlaybackLimits {
    pub fn validate(self) -> Result<Self, MediaError> {
        if self.max_width.is_some_and(|n| !(1..=8192).contains(&n))
            || self.max_height.is_some_and(|n| !(1..=8192).contains(&n))
            || self.max_fps.is_some_and(|n| !(1..=240).contains(&n))
        {
            return Err(MediaError::InvalidOutput);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputParams {
    pub width: u32,
    pub height: u32,
    pub fit: BackgroundFit,
    pub blur: f32,
    pub limits: PlaybackLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopCount {
    Infinite,
    Finite(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub frame_count: Option<u32>,
    pub loops: LoopCount,
}

pub struct MediaFrame {
    pub generation: u64,
    pub sequence: u64,
    pub timestamp: Duration,
    pub duration: Duration,
    /// Worker time spent decoding and processing this reply, excluding waits for requests.
    pub processing_time: Duration,
    pub width: u32,
    pub height: u32,
    /// Top-down, tightly packed BGRA8 with straight alpha.
    pub pixels: Vec<u8>,
    /// Retained native video storage; when present, `pixels` is empty.
    pub native: Option<std::sync::Arc<crate::NativeVideoFrame>>,
    pub(crate) _lease: Option<PixelLease>,
    pub(crate) recycle: Option<std::sync::Weak<std::sync::Mutex<crate::pixels::RecycledPixels>>>,
}

impl MediaFrame {
    pub fn memory_budget(&self) -> crate::MemoryBudget {
        self._lease
            .as_ref()
            .expect("frame owns its admission")
            .budget()
    }
}

impl Drop for MediaFrame {
    fn drop(&mut self) {
        if !self.pixels.is_empty()
            && let Some(pool) = self.recycle.as_ref().and_then(std::sync::Weak::upgrade)
        {
            let mut pool = pool.lock().unwrap_or_else(|error| error.into_inner());
            // GPU submissions retain the frame, so recycling only happens after their reads finish.
            if self.pixels.len() == pool.bytes && pool.buffers.len() < 2 {
                pool.buffers.push((
                    std::mem::take(&mut self.pixels),
                    self._lease.take().expect("frame owns its admission"),
                ));
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("background decoding cancelled")]
    Cancelled,
    #[error("unsupported background format")]
    Unsupported,
    #[error("background memory allowance exceeded")]
    ResourceExhausted,
    #[error("invalid background output parameters")]
    InvalidOutput,
    #[error("background decoding failed: {0}")]
    Decode(String),
}

impl From<image::ImageError> for MediaError {
    fn from(error: image::ImageError) -> Self {
        Self::Decode(error.to_string())
    }
}

impl From<std::io::Error> for MediaError {
    fn from(error: std::io::Error) -> Self {
        Self::Decode(error.to_string())
    }
}

impl From<image_webp::DecodingError> for MediaError {
    fn from(error: image_webp::DecodingError) -> Self {
        Self::Decode(error.to_string())
    }
}
