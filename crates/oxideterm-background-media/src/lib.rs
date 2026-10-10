//! Bounded, request-driven background decoding independent of the UI runtime.

mod budget;
mod decoder;
mod gif_metadata;
mod pixels;
mod poster;
mod types;
mod video;
mod worker;

pub use budget::{MemoryBudget, PixelLease, release_image_bytes, try_reserve_image_bytes};
pub use pixels::output_dimensions;
pub use poster::{decode_poster, is_animated_media};
pub use types::{
    BackgroundFit, LoopCount, MediaError, MediaFrame, MediaInfo, OutputParams, PlaybackLimits,
};
pub use video::{NativeVideoDevice, NativeVideoFrame};
pub use worker::{MediaReply, MediaStream};
