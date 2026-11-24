//! Local file-manager views and their workspace bindings.

mod bridge;
pub mod filesystem;
pub use filesystem::FileSystemBridge;
mod path_completion;
pub use path_completion::{PathCompletionState, render_completion_popup};
mod view;

pub use bridge::{
    FileManagerBindings, FileManagerEditorFonts, FileManagerEnvironment, FileManagerImeFrame,
    FileManagerNoticeKind, FileManagerPointer, FileManagerRequest, FileManagerSelectableText,
    FileManagerVideo,
};
pub use view::{
    FileManagerDialog, FileManagerInput, FileManagerState, FileManagerView, FileManagerViews,
};
