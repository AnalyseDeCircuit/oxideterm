//! Audit activity, session summaries, and offline recording playback.

use std::{path::PathBuf, rc::Rc};

use gpui::{App, Entity, Task, Window};
use oxideterm_gpui_terminal::TerminalPane;

mod surface;

pub use surface::{AuditFrame, AuditState, AuditView};

pub struct AuditBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<AuditFrame>>,
    pub is_showing: Rc<dyn Fn(&App) -> bool>,
    pub dispatch: Rc<dyn Fn(AuditWindowRequest, &mut App)>,
    pub save_export: Rc<dyn Fn(&str, &mut App) -> Task<Option<PathBuf>>>,
    pub playback: Rc<dyn Fn(&mut Window, &mut App) -> Option<Entity<TerminalPane>>>,
}

pub enum AuditWindowRequest {
    ClearWorkspaceTextFocus,
    PrepareFilter,
    StorageUnavailable,
    Notice {
        title: String,
        description: Option<String>,
        success: bool,
    },
}
