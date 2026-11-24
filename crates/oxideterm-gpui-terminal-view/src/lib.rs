//! GPUI terminal canvas, visual annotations, and input geometry for emulator snapshots.

mod annotations;
pub mod element;
mod highlight;
mod host;
pub mod input;
pub mod links;
pub mod metrics;
pub mod selection;
pub mod semantic;
pub mod style;
pub mod theme;
pub mod timestamps;

pub use annotations::{
    CommandMarkAnnotation, ImagePlayback, TerminalRenderedImage, TransientCommandHighlight,
};
pub use element::{TerminalElement, TerminalElementLayout, TerminalLayoutCache};
pub use host::{TerminalCanvasHost, TerminalElementInput};
pub use metrics::TerminalMetrics;
pub use theme::TerminalUiTheme;
pub use timestamps::TerminalRowTimestampStore;

#[cfg(test)]
mod tests;
