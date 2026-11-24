//! Keyboard shortcut data and conflict detection, independent of app action registration.

pub fn format_combo(combo: &KeyCombo) -> String {
    format_combo_for(combo, KeybindingSide::current())
}

pub fn format_combo_for(combo: &KeyCombo, side: KeybindingSide) -> String {
    let mut parts = Vec::new();
    if side == KeybindingSide::Mac {
        if combo.ctrl {
            parts.push("⌃".to_string());
        }
        if combo.alt {
            parts.push("⌥".to_string());
        }
        if combo.shift {
            parts.push("⇧".to_string());
        }
        if combo.meta {
            parts.push("⌘".to_string());
        }
        parts.push(display_key(&combo.key).to_string());
        parts.join("")
    } else {
        if combo.ctrl {
            parts.push("Ctrl".to_string());
        }
        if combo.alt {
            parts.push("Alt".to_string());
        }
        if combo.shift {
            parts.push("Shift".to_string());
        }
        if combo.meta {
            parts.push("Meta".to_string());
        }
        parts.push(display_key(&combo.key).to_string());
        parts.join("+")
    }
}

fn display_key(key: &str) -> &str {
    match key {
        "arrowleft" => "←",
        "arrowright" => "→",
        "arrowup" => "↑",
        "arrowdown" => "↓",
        "escape" => "Esc",
        "tab" => "Tab",
        "enter" => "Enter",
        "backspace" => "Backspace",
        " " | "space" => "Space",
        key => key,
    }
}

use gpui::Keystroke;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionScope {
    Global,
    Terminal,
    Split,
    Palette,
    Editor,
    Sftp,
    FileManager,
    Preview,
    RemoteDesktop,
    Plugin,
    AiPanel,
}

impl ActionScope {
    pub fn local(self) -> bool {
        matches!(
            self,
            Self::Editor
                | Self::Sftp
                | Self::FileManager
                | Self::Preview
                | Self::RemoteDesktop
                | Self::Plugin
                | Self::AiPanel
        )
    }

    fn overlaps(self, other: Self) -> bool {
        self == other
            || matches!(self, Self::Global | Self::Palette | Self::Plugin)
            || matches!(other, Self::Global | Self::Palette | Self::Plugin)
            || matches!(
                (self, other),
                (Self::Terminal, Self::Split)
                    | (Self::Split, Self::Terminal)
                    | (Self::FileManager, Self::Preview)
                    | (Self::Preview, Self::FileManager)
                    | (Self::AiPanel, Self::Terminal | Self::Split | Self::Editor)
                    | (Self::Terminal | Self::Split | Self::Editor, Self::AiPanel)
            )
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Global => "settings_view.keybindings.scope_global",
            Self::Terminal => "settings_view.keybindings.scope_terminal",
            Self::Split => "settings_view.keybindings.scope_split",
            Self::Palette => "settings_view.keybindings.scope_palette",
            Self::Editor => "settings_view.keybindings.scope_editor",
            Self::Sftp => "settings_view.keybindings.scope_sftp",
            Self::FileManager => "settings_view.keybindings.scope_files",
            Self::Preview => "settings_view.keybindings.scope_preview",
            Self::RemoteDesktop => "settings_view.keybindings.scope_remote_desktop",
            Self::Plugin => "settings_view.keybindings.scope_plugins",
            Self::AiPanel => "settings_view.keybindings.scope_ai_panel",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalBehavior {
    Always,
    Never,
    WhenPanelOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeybindingSide {
    Mac,
    Other,
}

impl KeybindingSide {
    #[cfg(target_family = "wasm")]
    pub fn install_browser_side(self) {
        BROWSER_SIDE.set(self);
    }

    pub fn current() -> Self {
        #[cfg(target_family = "wasm")]
        {
            BROWSER_SIDE.get()
        }
        #[cfg(not(target_family = "wasm"))]
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }

    pub fn json_key(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Other => "other",
        }
    }
}

#[cfg(target_family = "wasm")]
thread_local! {
    static BROWSER_SIDE: std::cell::Cell<KeybindingSide> = const { std::cell::Cell::new(KeybindingSide::Other) };
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCombo {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

impl KeyCombo {
    pub fn plain(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    pub fn cmd(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: true,
        }
    }

    pub fn cmd_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: true,
            alt: false,
            meta: true,
        }
    }

    pub fn cmd_alt(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: true,
            meta: true,
        }
    }

    pub fn cmd_ctrl(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: false,
            alt: false,
            meta: true,
        }
    }

    pub fn ctrl(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    pub fn ctrl_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        }
    }

    pub fn ctrl_alt(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: false,
            alt: true,
            meta: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionDefinition {
    pub id: Cow<'static, str>,
    pub label: Option<String>,
    pub scope: ActionScope,
    pub terminal_behavior: TerminalBehavior,
    pub mac: KeyCombo,
    pub other: KeyCombo,
}

impl ActionDefinition {
    pub fn default_combo(&self, side: KeybindingSide) -> &KeyCombo {
        match side {
            KeybindingSide::Mac => &self.mac,
            KeybindingSide::Other => &self.other,
        }
    }

    pub fn label_key(&self) -> String {
        format!("settings_view.keybindings.actions.{}", self.id)
    }
}

pub fn effective_combo(
    definition: &ActionDefinition,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Option<KeyCombo> {
    override_binding(&definition.id, overrides, side)
        .unwrap_or_else(|| Some(definition.default_combo(side).clone()))
}

pub fn effective_combos(
    definition: &ActionDefinition,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Vec<KeyCombo> {
    let mut combos: Vec<_> = effective_combo(definition, overrides, side)
        .into_iter()
        .collect();
    if override_binding(&definition.id, overrides, side).is_none() {
        if definition.id == "terminal.scrollPageUp" {
            combos.push(KeyCombo::cmd("arrowup"));
        }
        if definition.id == "terminal.scrollPageDown" {
            combos.push(KeyCombo::cmd("arrowdown"));
        }
        if definition.scope == ActionScope::RemoteDesktop {
            for (ctrl, meta) in [(true, false), (false, true), (true, true)] {
                for shift in [false, true] {
                    let mut alias = definition.other.clone();
                    alias.ctrl = ctrl;
                    alias.meta = meta;
                    alias.shift = shift;
                    if !combos.contains(&alias) {
                        combos.push(alias);
                    }
                }
            }
        }
        if definition.id == "preview.zoomIn" {
            combos.push(KeyCombo::plain("+"));
        }
        if side == KeybindingSide::Mac
            && definition.scope == ActionScope::Plugin
            && definition.other.ctrl
        {
            combos.push(definition.other.clone());
        }
        if side == KeybindingSide::Mac
            && definition.scope == ActionScope::FileManager
            && definition.mac.meta
        {
            combos.push(definition.other.clone());
        }
        if definition.id == "sftp.delete" {
            combos.push(KeyCombo::plain("backspace"));
        }
        if side == KeybindingSide::Mac
            && ((definition.scope == ActionScope::Editor && definition.id != "editor.find")
                || matches!(definition.id.as_ref(), "sftp.selectAll" | "sftp.editPath"))
        {
            let mut control = definition.mac.clone();
            control.meta = false;
            control.ctrl = true;
            combos.push(control);
        }
        if definition.id == "editor.redo" {
            combos.push(KeyCombo::ctrl("y"));
            if side == KeybindingSide::Mac {
                combos.push(KeyCombo::cmd("y"));
            }
        }
    }
    combos
}

pub fn override_binding(
    action_id: &str,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Option<Option<KeyCombo>> {
    let side_value = overrides.get(action_id)?.get(side.json_key())?;
    if side_value.is_null() {
        return Some(None);
    }
    serde_json::from_value::<KeyCombo>(side_value.clone())
        .ok()
        .map(normalize_combo)
        .map(Some)
}

pub fn conflicts_in_definitions<'a>(
    action_id: &str,
    combo: &KeyCombo,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
    definitions: &'a [ActionDefinition],
) -> Vec<&'a ActionDefinition> {
    let Some(action) = definitions
        .iter()
        .find(|definition| definition.id == action_id)
    else {
        return Vec::new();
    };
    definitions
        .iter()
        .filter(|definition| definition.id != action_id && action.scope.overlaps(definition.scope))
        .filter(|definition| effective_combos(definition, overrides, side).contains(combo))
        .collect()
}

pub fn combo_from_keystroke(keystroke: &Keystroke) -> Option<KeyCombo> {
    let key = normalize_key_from_keystroke(keystroke)?;
    if matches!(
        key.as_str(),
        "shift" | "control" | "ctrl" | "alt" | "meta" | "cmd" | "super"
    ) {
        return None;
    }

    Some(normalize_combo(KeyCombo {
        key,
        ctrl: keystroke.modifiers.control,
        shift: keystroke.modifiers.shift,
        alt: keystroke.modifiers.alt,
        meta: keystroke.modifiers.platform,
    }))
}

fn normalize_key_from_keystroke(keystroke: &Keystroke) -> Option<String> {
    if let Some(key_char) = keystroke.key_char.as_deref()
        && !key_char.trim().is_empty()
        && key_char.chars().count() == 1
        && !keystroke.modifiers.control
        && !keystroke.modifiers.platform
    {
        return Some(key_char.to_lowercase());
    }

    let key = keystroke.key.as_str();
    if key.is_empty() {
        return None;
    }
    Some(match key {
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        "esc" => "escape".to_string(),
        key => key.to_lowercase(),
    })
}

pub fn normalize_combo(mut combo: KeyCombo) -> KeyCombo {
    combo.key = match combo.key.as_str() {
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        "esc" => "escape".to_string(),
        "space" => "space".to_string(),
        key => key.to_lowercase(),
    };

    if printable_symbol_encodes_shift(&combo.key) {
        combo.shift = false;
    }
    if (combo.ctrl || combo.meta) && layout_symbol_may_include_alt(&combo.key) {
        combo.alt = false;
    }
    combo
}

pub fn printable_symbol_encodes_shift(key: &str) -> bool {
    matches!(
        key,
        "~" | "!"
            | "@"
            | "#"
            | "$"
            | "%"
            | "^"
            | "&"
            | "*"
            | "("
            | ")"
            | "_"
            | "+"
            | "{"
            | "}"
            | "|"
            | ":"
            | "\""
            | "<"
            | ">"
            | "?"
    )
}

fn layout_symbol_may_include_alt(key: &str) -> bool {
    matches!(key, "[" | "]" | "{" | "}" | "\\" | "|" | "@" | "#" | "~")
}
