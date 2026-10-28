//! Display-sized raster decoding and owned-pixel transforms shared by image and background views.
mod jpeg;
mod metadata;
mod pixels;
mod png;
pub use metadata::ImageInfo;
pub use pixels::{resize_frame, rotate, to_bgra};
