//! Mount a [`TerminalView`], feed it output, and subscribe to [`TerminalViewEvent`]
//! to route input and terminal replies back to the same peer.

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
mod view;

pub use annotations::{
    CommandMarkAnnotation, ImagePlayback, TerminalRenderedImage, TransientCommandHighlight,
};
pub use element::{TerminalElement, TerminalElementLayout, TerminalLayoutCache};
pub use host::{TerminalCanvasHost, TerminalElementInput};
pub use metrics::TerminalMetrics;
pub use theme::TerminalUiTheme;
pub use timestamps::TerminalRowTimestampStore;
pub use view::{TerminalView, TerminalViewEvent};

#[cfg(test)]
mod tests;
