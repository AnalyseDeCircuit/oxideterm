//! A transport-independent terminal grid, parser, and snapshot renderer.

pub mod convert;
mod emulator;
pub mod graphics;
mod logical_context;
pub mod palette;
pub mod search;
pub mod selection;
pub mod size;
mod snapshot;

pub use alacritty_terminal::term::{ClipboardType, TermMode};
pub use emulator::{TerminalEmulator, TerminalEmulatorEvent, TerminalEmulatorEvents};
pub use graphics::TerminalGraphicsState;
pub use palette::TerminalPalette;
pub use search::{TerminalSearchMatch, TerminalSearchRange};
pub use selection::TerminalSelectionRange;
pub use size::TerminalSize;
pub use snapshot::{
    GraphicsOptions, KittyFileTransmissionControl, KittyFileTransmissionPrompt, TerminalAttrs,
    TerminalCell, TerminalColor, TerminalCursorShape, TerminalImageAnimationState,
    TerminalImageData, TerminalImageFrame, TerminalImageId, TerminalImageProtocol,
    TerminalImageSnapshot, TerminalLogicalContext, TerminalLogicalContextRow,
    TerminalLogicalMember, TerminalRow, TerminalSnapshot, TerminalSnapshotRowReuse,
    TerminalStyleOrigin, TerminalUnderlineStyle,
};
