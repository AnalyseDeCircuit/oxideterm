pub(super) use oxideterm_quick_commands::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandAvailability, QuickCommandCategory,
    QuickCommandCategoryDraft, QuickCommandConfirmationPolicy, QuickCommandIcon,
    QuickCommandImportResult, QuickCommandImportStrategy, QuickCommandParameter,
    QuickCommandParameterKind, QuickCommandTargetProtocol, QuickCommandsSnapshot,
    default_quick_command_categories, default_quick_commands, now_ms,
};
use std::{cell::RefCell, collections::HashMap, path::Path, path::PathBuf};

use gpui::{ListAlignment, ListState, px};
use oxideterm_gpui_ui::text_input::TextInputViewport;
use zeroize::Zeroizing;

use super::{
    QUICK_COMMAND_LIST_ESTIMATED_HEIGHT, QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT,
    QUICK_COMMAND_LIST_OVERSCAN, TauriVirtualListSpec, VirtualListSignatureCache,
};

pub(super) const QUICK_COMMAND_TEXTAREA_LINE_HEIGHT: f32 = 22.0;
pub(super) const QUICK_COMMAND_TEXTAREA_MIN_HEIGHT: f32 = 126.0;
pub(super) const QUICK_COMMAND_TEXTAREA_VERTICAL_PADDING: f32 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum QuickCommandInput {
    Search,
    CommandName,
    CommandText,
    CommandDescription,
    CommandHostPattern,
    ParameterName(usize),
    ParameterLabel(usize),
    ParameterDefault(usize),
    ParameterChoices(usize),
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
            Self::ParameterName(index) => 100 + index as u64 * 4,
            Self::ParameterLabel(index) => 101 + index as u64 * 4,
            Self::ParameterDefault(index) => 102 + index as u64 * 4,
            Self::ParameterChoices(index) => 103 + index as u64 * 4,
        }
    }
}

#[derive(Clone)]
pub(super) struct QuickCommandParameterEditorDraft {
    pub name: String,
    pub label: String,
    pub kind: QuickCommandParameterKind,
    pub default_value: String,
    pub choices: String,
    pub required: bool,
}

#[derive(Clone)]
pub(super) struct QuickCommandEditorDraft {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub host_patterns: String,
    pub parameters: Vec<QuickCommandParameterEditorDraft>,
    pub protocols: Vec<QuickCommandTargetProtocol>,
    pub confirmation: QuickCommandConfirmationPolicy,
    pub created_at: u64,
    pub sort_order: i64,
}

#[derive(Clone)]
pub(super) struct QuickCommandExecutionDraft {
    pub command: QuickCommand,
    // Runtime substitutions may contain credentials, so dialog ownership is
    // also the zeroization boundary for every entered value.
    pub parameter_values: Vec<Zeroizing<String>>,
}

#[derive(Clone)]
pub(super) struct QuickCommandCategoryDeletePrompt {
    pub id: String,
    pub name: String,
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

#[derive(Clone)]
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
    pub command_editor: Option<QuickCommandEditorDraft>,
    pub category_editor: Option<QuickCommandCategoryDraft>,
    pub last_persist_error: Option<String>,
}

/// Owns the quick-command store and its complete terminal-surface lifecycle.
pub(in crate::workspace) struct TerminalQuickCommandsState {
    pub(super) store: QuickCommandsState,
    pub(super) open: bool,
    pub(super) manager_open: bool,
    pub(super) pinned: bool,
    pub(super) pending_execution: Option<QuickCommandExecutionDraft>,
    pub(super) pending_category_delete: Option<QuickCommandCategoryDeletePrompt>,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) input_viewports: RefCell<HashMap<QuickCommandInput, TextInputViewport>>,
}

impl TerminalQuickCommandsState {
    pub(in crate::workspace) fn load(settings_path: &Path) -> Self {
        Self {
            store: QuickCommandsState::load(settings_path),
            open: false,
            manager_open: false,
            pinned: false,
            pending_execution: None,
            pending_category_delete: None,
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

pub(in crate::workspace) use view::quick_command_input_uses_monospace;
