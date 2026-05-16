use super::actions::classify_command_risk;
use super::quick_commands::match_quick_command_host_pattern;
use super::*;
use oxideterm_ai::infer_ai_cwd;
use oxideterm_sftp::{FileType as RemotePathFileType, ListFilter, SortOrder};

include!("completion/types.rs");
include!("completion/engine.rs");
include!("completion/history_provider.rs");
include!("completion/quick_command_provider.rs");
include!("completion/path_provider.rs");
include!("completion/render.rs");
include!("completion/common.rs");
include!("completion/tokenizer.rs");
include!("completion/fig_provider.rs");
include!("completion/fig_specs.rs");
