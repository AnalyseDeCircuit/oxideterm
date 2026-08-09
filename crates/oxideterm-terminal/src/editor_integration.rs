const EDITOR_INTEGRATION_PROTOCOL_VERSION: &str = "3";
const EDITOR_CLIPBOARD_TEXT_LIMIT: usize = 64 * 1024;
const EDITOR_PROTOCOL_FIELD_LIMIT: usize = 12;
// Clipboard text is encoded as three ASCII bytes per UTF-8 byte. Keep room
// for the typed envelope while preserving a fixed scanner allocation bound.
pub(crate) const EDITOR_PROTOCOL_PAYLOAD_LIMIT: usize = EDITOR_CLIPBOARD_TEXT_LIMIT * 3 + 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEditorApplication {
    Vim,
    Neovim,
    Emacs,
}

impl TerminalEditorApplication {
    pub fn matches_process_command(self, command: &str) -> bool {
        let executable = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(command)
            .trim_start_matches('-')
            .to_ascii_lowercase();
        match self {
            Self::Vim => matches!(executable.as_str(), "vi" | "vim"),
            Self::Neovim => executable == "nvim",
            Self::Emacs => matches!(executable.as_str(), "emacs" | "emacs-nox" | "emacsclient"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEditorMode {
    Normal,
    Insert,
    Replace,
    Visual,
    Select,
    Emacs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEditorSelection {
    None,
    Character,
    Line,
    Block,
    Region,
}

impl TerminalEditorSelection {
    pub fn is_active(self) -> bool {
        self != Self::None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalEditorCapabilities {
    pub mouse: bool,
    pub clipboard: bool,
    pub edit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalEditorIntegrationEvent {
    pub application: TerminalEditorApplication,
    pub mode: TerminalEditorMode,
    pub selection: TerminalEditorSelection,
    pub capabilities: TerminalEditorCapabilities,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEditorClipboardOperation {
    Copy,
    Cut,
}

#[derive(Clone)]
pub struct TerminalEditorClipboardEvent {
    pub application: TerminalEditorApplication,
    pub operation: TerminalEditorClipboardOperation,
    pub text: zeroize::Zeroizing<String>,
}

pub const VIM_FREE_TYPE_INTEGRATION_SOURCE: &str =
    include_str!("editor_integration/oxideterm-free-type.vim");
pub const EMACS_FREE_TYPE_INTEGRATION_SOURCE: &str =
    include_str!("editor_integration/oxideterm-free-type.el");

pub(crate) enum TerminalEditorProtocolMessage {
    State(TerminalEditorIntegrationEvent),
    Clipboard(TerminalEditorClipboardEvent),
}

/// Parses the versioned OSC 7719 envelope before dispatching to one schema.
///
/// Field order is intentionally irrelevant, while duplicate and unknown
/// fields are rejected by the selected schema. Adding v4 therefore requires
/// one new version branch rather than another prefix check.
pub(crate) fn parse_editor_protocol_message(
    payload: &str,
) -> Option<TerminalEditorProtocolMessage> {
    let fields = parse_editor_protocol_fields(payload)?;
    let version = field_value(&fields, "v")?;
    let kind = field_value(&fields, "kind")?;
    match (version, kind) {
        (EDITOR_INTEGRATION_PROTOCOL_VERSION, "editor-state") => {
            parse_editor_integration_event(&fields).map(TerminalEditorProtocolMessage::State)
        }
        (EDITOR_INTEGRATION_PROTOCOL_VERSION, "editor-clipboard") => {
            parse_editor_clipboard_event(&fields).map(TerminalEditorProtocolMessage::Clipboard)
        }
        _ => None,
    }
}

fn parse_editor_protocol_fields(payload: &str) -> Option<Vec<(&str, &str)>> {
    let mut fields = Vec::new();
    for field in payload.split(';') {
        let (key, value) = field.split_once('=')?;
        if key.is_empty()
            || fields.len() >= EDITOR_PROTOCOL_FIELD_LIMIT
            || fields.iter().any(|(existing_key, _)| *existing_key == key)
        {
            return None;
        }
        fields.push((key, value));
    }
    Some(fields)
}

fn field_value<'a>(fields: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (*field_key == key).then_some(*value))
}

fn parse_editor_integration_event(
    fields: &[(&str, &str)],
) -> Option<TerminalEditorIntegrationEvent> {
    let mut application = None;
    let mut mode = None;
    let mut selection = None;
    let mut capabilities = TerminalEditorCapabilities::default();
    let mut active = None;

    for &(key, value) in fields {
        match key {
            "v" | "kind" => {}
            "app" => {
                application = match value {
                    "vim" => Some(TerminalEditorApplication::Vim),
                    "nvim" => Some(TerminalEditorApplication::Neovim),
                    "emacs" => Some(TerminalEditorApplication::Emacs),
                    _ => return None,
                }
            }
            "mode" => {
                mode = match value {
                    "normal" => Some(TerminalEditorMode::Normal),
                    "insert" => Some(TerminalEditorMode::Insert),
                    "replace" => Some(TerminalEditorMode::Replace),
                    "visual" => Some(TerminalEditorMode::Visual),
                    "select" => Some(TerminalEditorMode::Select),
                    "emacs" => Some(TerminalEditorMode::Emacs),
                    _ => return None,
                }
            }
            "selection" => {
                selection = match value {
                    "none" => Some(TerminalEditorSelection::None),
                    "char" => Some(TerminalEditorSelection::Character),
                    "line" => Some(TerminalEditorSelection::Line),
                    "block" => Some(TerminalEditorSelection::Block),
                    "region" => Some(TerminalEditorSelection::Region),
                    _ => return None,
                }
            }
            "caps" => {
                for capability in value.split(',').filter(|value| !value.is_empty()) {
                    match capability {
                        "mouse" => capabilities.mouse = true,
                        "clipboard" => capabilities.clipboard = true,
                        "edit" => capabilities.edit = true,
                        _ => return None,
                    }
                }
            }
            "active" => {
                active = match value {
                    "0" => Some(false),
                    "1" => Some(true),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }

    Some(TerminalEditorIntegrationEvent {
        application: application?,
        mode: mode?,
        selection: selection?,
        capabilities,
        active: active?,
    })
}

fn parse_editor_clipboard_event(fields: &[(&str, &str)]) -> Option<TerminalEditorClipboardEvent> {
    let mut application = None;
    let mut operation = None;
    let mut encoded_text = None;

    for &(key, value) in fields {
        match key {
            "v" | "kind" => {}
            "app" => {
                application = match value {
                    "vim" => Some(TerminalEditorApplication::Vim),
                    "nvim" => Some(TerminalEditorApplication::Neovim),
                    "emacs" => Some(TerminalEditorApplication::Emacs),
                    _ => return None,
                }
            }
            "op" => {
                operation = match value {
                    "copy" => Some(TerminalEditorClipboardOperation::Copy),
                    "cut" => Some(TerminalEditorClipboardOperation::Cut),
                    _ => return None,
                }
            }
            "data" => encoded_text = Some(value),
            _ => return None,
        }
    }

    let text = percent_decode_editor_text(encoded_text?)?;
    Some(TerminalEditorClipboardEvent {
        application: application?,
        operation: operation?,
        text: zeroize::Zeroizing::new(text),
    })
}

fn percent_decode_editor_text(value: &str) -> Option<String> {
    if value.len() > EDITOR_CLIPBOARD_TEXT_LIMIT.saturating_mul(3) {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len().min(EDITOR_CLIPBOARD_TEXT_LIMIT));
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' || index + 2 >= bytes.len() {
            return None;
        }
        let high = hex_value(bytes[index + 1])?;
        let low = hex_value(bytes[index + 2])?;
        decoded.push((high << 4) | low);
        if decoded.len() > EDITOR_CLIPBOARD_TEXT_LIMIT {
            return None;
        }
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_integration_parser_accepts_explicit_vim_state() {
        assert!(matches!(
            parse_editor_protocol_message(
                "v=3;kind=editor-state;app=nvim;mode=visual;selection=char;caps=mouse,clipboard,edit;active=1"
            ),
            Some(TerminalEditorProtocolMessage::State(event))
                if event == (TerminalEditorIntegrationEvent {
                    application: TerminalEditorApplication::Neovim,
                    mode: TerminalEditorMode::Visual,
                    selection: TerminalEditorSelection::Character,
                    capabilities: TerminalEditorCapabilities {
                        mouse: true,
                        clipboard: true,
                        edit: true,
                    },
                    active: true,
                })
        ));
    }

    #[test]
    fn editor_integration_parser_rejects_unknown_or_partial_state() {
        assert!(
            parse_editor_protocol_message(
                "v=3;kind=editor-state;app=nano;mode=normal;selection=none;caps=mouse;active=1"
            )
            .is_none()
        );
        assert!(
            parse_editor_protocol_message("v=3;kind=editor-state;app=vim;mode=normal").is_none()
        );
        assert!(
            parse_editor_protocol_message(
                "v=2;kind=editor-state;app=vim;mode=normal;selection=none;caps=mouse;active=1"
            )
            .is_none()
        );
    }

    #[test]
    fn editor_clipboard_parser_decodes_bounded_utf8_only() {
        let Some(TerminalEditorProtocolMessage::Clipboard(event)) = parse_editor_protocol_message(
            "v=3;kind=editor-clipboard;app=vim;op=copy;data=%E4%BD%A0%E5%A5%BD%0A",
        ) else {
            panic!("valid clipboard payload");
        };
        assert_eq!(event.application, TerminalEditorApplication::Vim);
        assert_eq!(event.operation, TerminalEditorClipboardOperation::Copy);
        assert_eq!(event.text.as_str(), "你好\n");
        assert!(
            parse_editor_protocol_message("v=3;kind=editor-clipboard;app=vim;op=copy;data=raw")
                .is_none()
        );
        assert!(
            parse_editor_protocol_message("v=3;kind=editor-clipboard;app=vim;op=copy;data=%FF")
                .is_none()
        );
    }

    #[test]
    fn editor_protocol_dispatch_is_order_independent_and_rejects_duplicates() {
        assert!(matches!(
            parse_editor_protocol_message(
                "app=vim;active=1;kind=editor-state;selection=none;v=3;caps=mouse;mode=normal"
            ),
            Some(TerminalEditorProtocolMessage::State(_))
        ));
        assert!(
            parse_editor_protocol_message(
                "v=3;kind=editor-state;v=4;app=vim;mode=normal;selection=none;caps=mouse;active=1"
            )
            .is_none()
        );
    }

    #[test]
    fn editor_application_matches_only_its_foreground_executable() {
        assert!(TerminalEditorApplication::Vim.matches_process_command("/usr/bin/vim"));
        assert!(TerminalEditorApplication::Neovim.matches_process_command("nvim"));
        assert!(TerminalEditorApplication::Emacs.matches_process_command("emacs-nox"));
        assert!(!TerminalEditorApplication::Vim.matches_process_command("nvim"));
        assert!(!TerminalEditorApplication::Emacs.matches_process_command("tmux"));
    }

    #[test]
    fn bundled_editor_integrations_gate_the_private_protocol() {
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("7719;v=3;kind=editor-state"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("7719;v=3;kind=editor-state"));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("OXIDETERM_PRIVATE_OSC"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("OXIDETERM_PRIVATE_OSC"));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("$TMUX"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("\"TMUX\""));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("$STY"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("\"STY\""));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("$ZELLIJ"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("\"ZELLIJ\""));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("has('nvim') ? 'nvim' : 'vim'"));
        assert!(VIM_FREE_TYPE_INTEGRATION_SOURCE.contains("set mouse=a"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("(xterm-mouse-mode 1)"));
        assert!(EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("(format \"%%%02X\" byte)"));
        assert!(!EMACS_FREE_TYPE_INTEGRATION_SOURCE.contains("url-hexify-string"));
    }
}
