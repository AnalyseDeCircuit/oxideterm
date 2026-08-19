pub(super) use oxideterm_quick_commands::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandCategory, QuickCommandCategoryDraft,
    QuickCommandDraft, QuickCommandIcon, QuickCommandImportResult, QuickCommandImportStrategy,
    QuickCommandsSnapshot, default_quick_command_categories, default_quick_commands, now_ms,
};
use std::{cell::RefCell, collections::HashMap, path::Path, path::PathBuf};

use gpui::{ListAlignment, ListState, px};
use oxideterm_gpui_ui::text_input::TextInputViewport;
use zeroize::Zeroizing;

use super::{
    QUICK_COMMAND_LIST_ESTIMATED_HEIGHT, QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT,
    QUICK_COMMAND_LIST_OVERSCAN, TauriVirtualListSpec, VirtualListSignatureCache,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum QuickCommandInput {
    Search,
    CommandName,
    CommandText,
    CommandDescription,
    CommandHostPattern,
    CategoryName,
}

impl QuickCommandInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::CommandName => 2,
            Self::CommandText => 3,
            Self::CommandDescription => 4,
            Self::CommandHostPattern => 5,
            Self::CategoryName => 6,
        }
    }
}

fn quick_command_icon_source_id(icon: QuickCommandIcon) -> &'static str {
    match icon {
        QuickCommandIcon::Terminal => "terminal",
        QuickCommandIcon::Server => "server",
        QuickCommandIcon::Folder => "folder",
        QuickCommandIcon::Docker => "docker",
        QuickCommandIcon::Zap => "zap",
    }
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandsState {
    settings_path: PathBuf,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub active_category: String,
    pub query: String,
    pub focused_input: Option<QuickCommandInput>,
    // Browser popovers keep one active option for keyboard navigation without
    // stealing the row click target; store the stable command id instead of a
    // transient index so filtering and category changes cannot select a stale row.
    pub highlighted_command: Option<String>,
    pub command_editor: Option<QuickCommandDraft>,
    pub category_editor: Option<QuickCommandCategoryDraft>,
    pub last_persist_error: Option<String>,
}

/// Owns the quick-command store and its complete terminal-surface lifecycle.
pub(in crate::workspace) struct TerminalQuickCommandsState {
    pub(super) store: QuickCommandsState,
    pub(super) open: bool,
    pub(super) pinned: bool,
    pub(super) pending_command: Option<Zeroizing<String>>,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) input_viewports: RefCell<HashMap<QuickCommandInput, TextInputViewport>>,
}

impl TerminalQuickCommandsState {
    pub(in crate::workspace) fn load(settings_path: &Path) -> Self {
        Self {
            store: QuickCommandsState::load(settings_path),
            open: false,
            pinned: false,
            pending_command: None,
            // User-defined command sets are unbounded, so the surface owns a
            // virtual list instead of rebuilding every row on root repaint.
            list_state: ListState::new(
                QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
                    QUICK_COMMAND_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Each editor field keeps browser-like horizontal position across redraws.
            input_viewports: RefCell::new(HashMap::new()),
        }
    }

    pub(super) fn input_viewport(&self, input: QuickCommandInput) -> TextInputViewport {
        self.input_viewports
            .borrow_mut()
            .entry(input)
            .or_default()
            .clone()
    }
}

#[path = "quick_commands_buttons.rs"]
mod buttons;
#[path = "quick_commands_store.rs"]
mod store;
#[path = "quick_commands_view.rs"]
mod view;

pub(in crate::workspace) use oxideterm_quick_commands::match_quick_command_host_pattern;
