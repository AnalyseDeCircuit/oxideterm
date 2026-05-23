use super::actions::classify_command_risk;
use super::ime::WorkspaceImeTarget;
use super::*;
use crate::assets::LucideIcon;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const QUICK_COMMANDS_FILENAME: &str = "quick-commands.json";
const QUICK_COMMANDS_SCHEMA_VERSION: u32 = 1;
const MAX_QUICK_COMMANDS_FILE_BYTES: u64 = 512 * 1024;
const MAX_CATEGORIES: usize = 100;
const MAX_COMMANDS: usize = 1000;
const MAX_ID_LEN: usize = 128;
const MAX_NAME_LEN: usize = 160;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_HOST_PATTERN_LEN: usize = 256;
static QUICK_COMMAND_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum QuickCommandIcon {
    Terminal,
    Server,
    Folder,
    Docker,
    Zap,
}

impl QuickCommandIcon {
    fn as_source_id(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Server => "server",
            Self::Folder => "folder",
            Self::Docker => "docker",
            Self::Zap => "zap",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandCategory {
    pub id: String,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_pattern: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandsSnapshot {
    pub version: u32,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub updated_at: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QuickCommandImportStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandDraft {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub host_pattern: String,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandCategoryDraft {
    pub id: Option<String>,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandsState {
    path: PathBuf,
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

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct QuickCommandsImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

include!("quick_commands_store.rs");
include!("quick_commands_view.rs");
include!("quick_commands_buttons.rs");
