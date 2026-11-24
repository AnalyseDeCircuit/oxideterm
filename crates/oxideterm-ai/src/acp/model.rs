use std::{collections::HashMap, fmt, path::PathBuf};

use zeroize::Zeroize;

pub struct AcpTerminalCreateSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub output_byte_limit: Option<usize>,
}

impl fmt::Debug for AcpTerminalCreateSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcpTerminalCreateSpec")
            .field("command", &"<redacted>")
            .field("args", &format_args!("<redacted:{}>", self.args.len()))
            .field("env", &format_args!("<redacted:{}>", self.env.len()))
            .field("cwd", &self.cwd)
            .field("output_byte_limit", &self.output_byte_limit)
            .finish()
    }
}

impl Drop for AcpTerminalCreateSpec {
    fn drop(&mut self) {
        // Agent-provided commands and environment values may contain secrets.
        self.command.zeroize();
        self.args.zeroize();
        for value in self.env.values_mut() {
            value.zeroize();
        }
        self.env.clear();
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AcpHostCapabilityPolicy {
    pub fs_read_text_file: bool,
    pub fs_write_text_file: bool,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpAuthMethodKind {
    Agent,
    Environment,
    Terminal,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpAuthMethod {
    pub method_id: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: AcpAuthMethodKind,
    pub environment_variables: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPromptSessionOutcome {
    pub session_id: String,
    pub session_metadata: Option<serde_json::Value>,
    pub session_config_options: Vec<AcpSessionConfigOption>,
    pub session_modes: Option<AcpSessionModeState>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigChoice {
    pub value_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigOption {
    pub config_id: String,
    pub name: String,
    pub category: Option<String>,
    pub current_value_id: String,
    pub choices: Vec<AcpSessionConfigChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigSelection {
    pub config_id: String,
    pub value_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionMode {
    pub mode_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionModeState {
    pub current_mode_id: String,
    pub available_modes: Vec<AcpSessionMode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcpLaunchModelHint {
    Automatic,
    Fixed(String),
}

/// Returns the first model selector because ACP defines option order as priority.
pub fn acp_model_config_option(
    options: &[AcpSessionConfigOption],
) -> Option<&AcpSessionConfigOption> {
    options.iter().find(|option| {
        option.category.as_deref() == Some("model")
            || (option.category.is_none()
                && (option.config_id.eq_ignore_ascii_case("model")
                    || option.name.eq_ignore_ascii_case("model")))
    })
}

/// Reads an explicit model flag without assuming that an ACP agent exposes model metadata.
pub fn acp_launch_model_hint(args: &[String]) -> Option<AcpLaunchModelHint> {
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        let value = if argument == "--model" {
            args.next().map(String::as_str)
        } else {
            argument.strip_prefix("--model=")
        };
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        return Some(if value.eq_ignore_ascii_case("auto") {
            AcpLaunchModelHint::Automatic
        } else {
            AcpLaunchModelHint::Fixed(value.to_string())
        });
    }
    None
}

/// Detects OxideTerm's native Claude Code adapter, which learns the resolved model from the
/// first prompt's stream instead of during ACP session creation.
pub fn acp_model_report_is_deferred_until_first_prompt(args: &[String]) -> bool {
    args.windows(2).any(|arguments| {
        arguments[0] == "--acp-adapter" && arguments[1].eq_ignore_ascii_case("claude-code")
    })
}

/// Detects OxideTerm's native Codex adapter, which publishes its model choices during
/// ACP session creation without requiring a user prompt.
pub fn acp_model_report_is_available_during_session_start(args: &[String]) -> bool {
    args.windows(2).any(|arguments| {
        arguments[0] == "--acp-adapter" && arguments[1].eq_ignore_ascii_case("codex")
    })
}

/// Resolves a stored choice only while it remains valid in the latest snapshot.
pub fn acp_selected_config_choice<'a>(
    option: &'a AcpSessionConfigOption,
    selection: Option<&AcpSessionConfigSelection>,
) -> Option<&'a AcpSessionConfigChoice> {
    let selected_value = selection
        .filter(|selection| selection.config_id == option.config_id)
        .and_then(|selection| {
            option
                .choices
                .iter()
                .find(|choice| choice.value_id == selection.value_id)
        });
    selected_value.or_else(|| {
        option
            .choices
            .iter()
            .find(|choice| choice.value_id == option.current_value_id)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPermissionOptionProjection {
    pub option_id: String,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPermissionRequestProjection {
    pub tool_call_id: String,
    pub name: String,
    pub arguments: String,
    pub summary: String,
    pub risk: String,
    pub options: Vec<AcpPermissionOptionProjection>,
}

/// Stable application-facing projection of ACP session state notifications.
///
/// Keeping protocol DTOs behind this boundary prevents the GPUI crate from
/// becoming coupled to the wire schema while still preserving every stateful
/// update needed to restore and render an ACP thread.
#[derive(Clone, Debug, PartialEq)]
pub enum AcpSessionStateUpdate {
    ConfigOptions(Vec<AcpSessionConfigOption>),
    CurrentMode(String),
    AvailableCommands(Vec<serde_json::Value>),
    Plan(serde_json::Value),
    SessionInfo {
        title: Option<String>,
        details: serde_json::Value,
    },
    Usage(serde_json::Value),
}

/// Identifies the lifecycle of one external ACP agent connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpConnectionState {
    Connecting,
    Ready,
    Stopped,
    Failed,
}
