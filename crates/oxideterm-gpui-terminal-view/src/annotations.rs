//! Per-frame annotations supplied by the host, independent of the terminal transport.

use std::{ops::Range, sync::Arc, time::Duration};

use gpui::RenderImage;
use oxideterm_terminal_emulator::TerminalImageSnapshot;
use oxideterm_terminal_semantic::SemanticLineRole;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImagePlayback {
    pub frame: usize,
    pub next: Option<Duration>,
}

#[derive(Clone)]
pub struct TerminalRenderedImage {
    pub snapshot: TerminalImageSnapshot,
    pub render_image: Option<Arc<RenderImage>>,
    pub playback: ImagePlayback,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TransientCommandHighlight {
    pub command_id: Arc<str>,
    pub query: Arc<str>,
    pub case_sensitive: bool,
    pub output_start_global_line: usize,
    pub output_end_global_line: Option<usize>,
}

/// A versioned command snapshot for painting. Session provenance and command text stay with the host.
#[derive(Clone, Debug)]
pub struct CommandMarkAnnotation {
    pub command_id: String,
    pub start_line: usize,
    pub command_line: usize,
    pub command_line_clipped: bool,
    pub output_start_line: usize,
    pub has_output_start: bool,
    pub output_role: SemanticLineRole,
    pub foldable_output: Option<Range<usize>>,
    pub end_line: Option<usize>,
    pub is_closed: bool,
    pub stale: bool,
    pub exit_code: Option<i32>,
    pub finished_at: Option<u64>,
}
