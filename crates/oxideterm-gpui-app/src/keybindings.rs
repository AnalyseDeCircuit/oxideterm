use gpui::{KeyBinding, Keystroke, NoAction};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::LazyLock;

use crate::{
    CloseOtherTabs, CloseTab, CommandPalette, Copy, Cut, Find, FontDecrease, FontIncrease,
    FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8,
    GoToTab9, NewConnection, NewTerminal, NextTab, OpenSettings, PaletteAiSidebar,
    PaletteBroadcast, PaletteEventLog, Paste, PrevTab, Quit, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, TerminalAiPanel,
    TerminalFreeTypeMode, TerminalRecording, ToggleSidebar, ZenMode,
};

const CONTEXT: &str = "Workspace";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionScope {
    Global,
    Terminal,
    Split,
    Palette,
}

impl ActionScope {
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Global => "settings_view.keybindings.scope_global",
            Self::Terminal => "settings_view.keybindings.scope_terminal",
            Self::Split => "settings_view.keybindings.scope_split",
            Self::Palette => "settings_view.keybindings.scope_palette",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalBehavior {
    Always,
    Never,
    WhenPanelOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeybindingSide {
    Mac,
    Other,
}

impl KeybindingSide {
    pub(crate) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }

    fn json_key(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyCombo {
    pub(crate) key: String,
    #[serde(default)]
    pub(crate) ctrl: bool,
    #[serde(default)]
    pub(crate) shift: bool,
    #[serde(default)]
    pub(crate) alt: bool,
    #[serde(default)]
    pub(crate) meta: bool,
}

impl KeyCombo {
    fn plain(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    fn cmd(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: true,
        }
    }

    fn cmd_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: true,
            alt: false,
            meta: true,
        }
    }

    fn cmd_alt(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: true,
            meta: true,
        }
    }

    fn ctrl(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    fn ctrl_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        }
    }

    fn ctrl_alt(key: impl Into<String>) -> Self {
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
pub(crate) struct ActionDefinition {
    pub(crate) id: &'static str,
    pub(crate) scope: ActionScope,
    pub(crate) terminal_behavior: TerminalBehavior,
    pub(crate) mac: KeyCombo,
    pub(crate) other: KeyCombo,
}

impl ActionDefinition {
    pub(crate) fn default_combo(&self, side: KeybindingSide) -> &KeyCombo {
        match side {
            KeybindingSide::Mac => &self.mac,
            KeybindingSide::Other => &self.other,
        }
    }

    pub(crate) fn label_key(&self) -> String {
        format!("settings_view.keybindings.actions.{}", self.id)
    }
}

pub(crate) static ACTION_DEFINITIONS: LazyLock<Vec<ActionDefinition>> = LazyLock::new(|| {
    let mut actions = vec![
        def(
            "app.newTerminal",
            ActionScope::Global,
            KeyCombo::cmd("t"),
            KeyCombo::ctrl("t"),
        ),
        def(
            "app.shellLauncher",
            ActionScope::Global,
            KeyCombo::cmd_shift("t"),
            KeyCombo::ctrl_shift("t"),
        ),
        def_with_terminal_behavior(
            "app.closeTab",
            ActionScope::Global,
            TerminalBehavior::Never,
            KeyCombo::cmd("w"),
            KeyCombo::ctrl("w"),
        ),
        def(
            "app.closeOtherTabs",
            ActionScope::Global,
            KeyCombo::cmd_shift("w"),
            KeyCombo::ctrl_shift("w"),
        ),
        def(
            "app.newConnection",
            ActionScope::Global,
            KeyCombo::cmd("n"),
            KeyCombo::ctrl("n"),
        ),
        def(
            "app.settings",
            ActionScope::Global,
            KeyCombo::cmd(","),
            KeyCombo::ctrl(","),
        ),
        def(
            "app.quit",
            ActionScope::Global,
            KeyCombo::cmd("q"),
            KeyCombo::ctrl("q"),
        ),
        def(
            "app.toggleSidebar",
            ActionScope::Global,
            KeyCombo::cmd("\\"),
            KeyCombo::ctrl("\\"),
        ),
        def(
            "app.commandPalette",
            ActionScope::Global,
            KeyCombo::cmd("k"),
            KeyCombo::ctrl("k"),
        ),
        def(
            "app.zenMode",
            ActionScope::Global,
            KeyCombo::cmd_shift("z"),
            KeyCombo::ctrl_shift("z"),
        ),
        def(
            "app.nextTab",
            ActionScope::Global,
            KeyCombo::cmd("}"),
            KeyCombo::ctrl("tab"),
        ),
        def(
            "app.prevTab",
            ActionScope::Global,
            KeyCombo::cmd("{"),
            KeyCombo::ctrl_shift("tab"),
        ),
        def(
            "app.navBack",
            ActionScope::Global,
            KeyCombo::cmd("["),
            KeyCombo {
                key: "arrowleft".to_string(),
                ctrl: false,
                shift: false,
                alt: true,
                meta: false,
            },
        ),
        def(
            "app.navForward",
            ActionScope::Global,
            KeyCombo::cmd("]"),
            KeyCombo {
                key: "arrowright".to_string(),
                ctrl: false,
                shift: false,
                alt: true,
                meta: false,
            },
        ),
    ];

    for index in 1..=9 {
        actions.push(def(
            Box::leak(format!("app.goToTab{index}").into_boxed_str()),
            ActionScope::Global,
            KeyCombo::cmd(index.to_string()),
            KeyCombo::ctrl(index.to_string()),
        ));
    }

    actions.extend([
        def(
            "app.fontIncrease",
            ActionScope::Global,
            KeyCombo::cmd("="),
            KeyCombo::ctrl("="),
        ),
        def(
            "app.fontDecrease",
            ActionScope::Global,
            KeyCombo::cmd("-"),
            KeyCombo::ctrl("-"),
        ),
        def(
            "app.fontReset",
            ActionScope::Global,
            KeyCombo::cmd("0"),
            KeyCombo::ctrl("0"),
        ),
        def(
            "app.showShortcuts",
            ActionScope::Global,
            KeyCombo::cmd("/"),
            KeyCombo::ctrl("/"),
        ),
        def(
            "terminal.search",
            ActionScope::Terminal,
            KeyCombo::cmd("f"),
            KeyCombo::ctrl_shift("f"),
        ),
        def(
            "terminal.copy",
            ActionScope::Terminal,
            KeyCombo::cmd("c"),
            KeyCombo::ctrl_shift("c"),
        ),
        def(
            "terminal.cut",
            ActionScope::Terminal,
            KeyCombo::cmd("x"),
            KeyCombo::ctrl_shift("x"),
        ),
        def(
            "terminal.paste",
            ActionScope::Terminal,
            KeyCombo::cmd("v"),
            KeyCombo::ctrl_shift("v"),
        ),
        def(
            "terminal.aiPanel",
            ActionScope::Terminal,
            KeyCombo::cmd("i"),
            KeyCombo::ctrl_shift("i"),
        ),
        def(
            "terminal.recording",
            ActionScope::Terminal,
            KeyCombo::cmd_shift("r"),
            KeyCombo::ctrl_shift("r"),
        ),
        def(
            "terminal.toggleFreeTypeMode",
            ActionScope::Terminal,
            KeyCombo::cmd_shift("f"),
            KeyCombo::ctrl_alt("f"),
        ),
        def_with_terminal_behavior(
            "terminal.closePanel",
            ActionScope::Terminal,
            TerminalBehavior::WhenPanelOpen,
            KeyCombo::plain("escape"),
            KeyCombo::plain("escape"),
        ),
        def(
            "split.horizontal",
            ActionScope::Split,
            KeyCombo::cmd_shift("e"),
            KeyCombo::ctrl_shift("e"),
        ),
        def(
            "split.vertical",
            ActionScope::Split,
            KeyCombo::cmd_shift("d"),
            KeyCombo::ctrl_shift("d"),
        ),
        def(
            "split.closePane",
            ActionScope::Split,
            KeyCombo::cmd_shift("w"),
            KeyCombo::ctrl_shift("w"),
        ),
        def(
            "split.navLeft",
            ActionScope::Split,
            KeyCombo::cmd_alt("arrowleft"),
            KeyCombo::ctrl_alt("arrowleft"),
        ),
        def(
            "split.navRight",
            ActionScope::Split,
            KeyCombo::cmd_alt("arrowright"),
            KeyCombo::ctrl_alt("arrowright"),
        ),
        def(
            "palette.eventLog",
            ActionScope::Palette,
            KeyCombo::cmd("j"),
            KeyCombo::ctrl("j"),
        ),
        def(
            "palette.aiSidebar",
            ActionScope::Palette,
            KeyCombo::cmd_shift("a"),
            KeyCombo::ctrl_shift("a"),
        ),
        // Keep Ctrl+B available as the tmux prefix on Windows and Linux.
        def(
            "palette.broadcast",
            ActionScope::Palette,
            KeyCombo::cmd("b"),
            KeyCombo::ctrl_shift("b"),
        ),
    ]);

    actions
});

fn def(id: &'static str, scope: ActionScope, mac: KeyCombo, other: KeyCombo) -> ActionDefinition {
    def_with_terminal_behavior(id, scope, TerminalBehavior::Always, mac, other)
}

fn def_with_terminal_behavior(
    id: &'static str,
    scope: ActionScope,
    terminal_behavior: TerminalBehavior,
    mac: KeyCombo,
    other: KeyCombo,
) -> ActionDefinition {
    ActionDefinition {
        id,
        scope,
        terminal_behavior,
        mac: normalize_combo(mac),
        other: normalize_combo(other),
    }
}

pub(crate) fn action_definition(id: &str) -> Option<&'static ActionDefinition> {
    ACTION_DEFINITIONS.iter().find(|action| action.id == id)
}

pub(crate) fn effective_combo(
    definition: &ActionDefinition,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> KeyCombo {
    override_combo(definition.id, overrides, side)
        .unwrap_or_else(|| definition.default_combo(side).clone())
}

pub(crate) fn override_combo(
    action_id: &str,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Option<KeyCombo> {
    let side_value = overrides.get(action_id)?.get(side.json_key())?.clone();
    serde_json::from_value::<KeyCombo>(side_value)
        .ok()
        .map(normalize_combo)
}

pub(crate) fn set_override(
    overrides: &mut Map<String, Value>,
    action_id: &str,
    side: KeybindingSide,
    combo: KeyCombo,
) {
    let Some(definition) = action_definition(action_id) else {
        return;
    };
    let combo = normalize_combo(combo);
    if combo == *definition.default_combo(side) {
        reset_override(overrides, action_id, side);
        return;
    }

    let mut entry = overrides
        .get(action_id)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    if let Ok(value) = serde_json::to_value(combo) {
        entry.insert(side.json_key().to_string(), value);
        overrides.insert(action_id.to_string(), Value::Object(entry));
    }
}

pub(crate) fn reset_override(
    overrides: &mut Map<String, Value>,
    action_id: &str,
    side: KeybindingSide,
) {
    let mut remove_action = false;
    if let Some(value) = overrides.get_mut(action_id)
        && let Some(object) = value.as_object_mut()
    {
        object.remove(side.json_key());
        remove_action = object.is_empty();
    }
    if remove_action {
        overrides.remove(action_id);
    }
}

pub(crate) fn sanitize_imported_overrides(value: Value) -> Result<Map<String, Value>, String> {
    let Value::Object(input) = value else {
        return Err("root must be an object".to_string());
    };

    let mut sanitized = Map::new();
    for (action_id, value) in input {
        if action_definition(&action_id).is_none() {
            return Err(format!("unknown action id: {action_id}"));
        }
        let Value::Object(object) = value else {
            return Err(format!("invalid override for {action_id}"));
        };

        let mut entry = Map::new();
        for side in [KeybindingSide::Mac, KeybindingSide::Other] {
            if let Some(combo_value) = object.get(side.json_key()) {
                let combo = serde_json::from_value::<KeyCombo>(combo_value.clone())
                    .map_err(|_| format!("invalid shortcut for {action_id}"))?;
                let normalized = normalize_combo(combo);
                let value = serde_json::to_value(normalized)
                    .map_err(|_| format!("invalid shortcut for {action_id}"))?;
                entry.insert(side.json_key().to_string(), value);
            }
        }
        if !entry.is_empty() {
            sanitized.insert(action_id, Value::Object(entry));
        }
    }

    Ok(sanitized)
}

pub(crate) fn modified_count(overrides: &Map<String, Value>) -> usize {
    ACTION_DEFINITIONS
        .iter()
        .filter(|definition| overrides.contains_key(definition.id))
        .count()
}

pub(crate) fn conflicts_for_combo(
    action_id: &str,
    combo: &KeyCombo,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Vec<&'static ActionDefinition> {
    ACTION_DEFINITIONS
        .iter()
        .filter(|definition| definition.id != action_id)
        .filter(|definition| effective_combo(definition, overrides, side) == *combo)
        .collect()
}

pub(crate) fn keystroke_matches_action(
    keystroke: &Keystroke,
    action_id: &str,
    overrides: &Map<String, Value>,
) -> bool {
    let Some(definition) = action_definition(action_id) else {
        return false;
    };
    let Some(combo) = combo_from_keystroke(keystroke) else {
        return false;
    };
    combo == effective_combo(definition, overrides, KeybindingSide::current())
}

pub(crate) fn matched_action_for_keystroke(
    keystroke: &Keystroke,
    overrides: &Map<String, Value>,
) -> Option<(&'static ActionDefinition, KeyCombo)> {
    let combo = combo_from_keystroke(keystroke)?;
    let side = KeybindingSide::current();
    ACTION_DEFINITIONS
        .iter()
        .find(|definition| effective_combo(definition, overrides, side) == combo)
        .map(|definition| (definition, combo))
}

#[cfg(test)]
fn normalize_plugin_key_combo(keybinding: &str) -> Option<String> {
    let mut parts = keybinding
        .split('+')
        .filter_map(normalize_plugin_key_part)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    Some(parts.join("+"))
}

pub(crate) fn normalize_plugin_keystroke(keystroke: &Keystroke) -> Option<String> {
    let combo = combo_from_keystroke(keystroke)?;
    let mut parts = Vec::new();
    // Tauri's pluginHostUi collapses Cmd/Meta and Ctrl into the same "ctrl"
    // token for plugin keybindings. Preserve that public contract so existing
    // plugin descriptors such as "Cmd+Shift+R" keep working cross-platform.
    if combo.ctrl || combo.meta {
        parts.push("ctrl".to_string());
    }
    if combo.shift {
        parts.push("shift".to_string());
    }
    if combo.alt {
        parts.push("alt".to_string());
    }
    parts.push(normalize_plugin_event_key(&combo.key)?);
    parts.sort();
    Some(parts.join("+"))
}

fn normalize_plugin_key_part(part: &str) -> Option<String> {
    let normalized = part.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    Some(match normalized.as_str() {
        "cmd" | "command" | "meta" | "super" | "win" | "⌘" => "ctrl".to_string(),
        "control" | "ctrl" | "⌃" => "ctrl".to_string(),
        "option" | "alt" | "⌥" => "alt".to_string(),
        "shift" | "⇧" => "shift".to_string(),
        "escape" | "esc" => "esc".to_string(),
        "spacebar" | "space" | " " => "space".to_string(),
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        key => key.to_string(),
    })
}

fn normalize_plugin_event_key(key: &str) -> Option<String> {
    normalize_plugin_key_part(if key == " " { "space" } else { key })
}

pub(crate) fn action_allowed_by_terminal_behavior(
    definition: &ActionDefinition,
    combo: &KeyCombo,
    terminal_active: bool,
    terminal_panel_open: bool,
) -> bool {
    if !terminal_active {
        return definition.terminal_behavior != TerminalBehavior::WhenPanelOpen
            || terminal_panel_open;
    }

    let mac_safe_meta = cfg!(target_os = "macos") && combo.meta && !combo.ctrl;
    if mac_safe_meta {
        return definition.terminal_behavior != TerminalBehavior::WhenPanelOpen
            || terminal_panel_open;
    }

    match definition.terminal_behavior {
        TerminalBehavior::Always => true,
        TerminalBehavior::Never => false,
        TerminalBehavior::WhenPanelOpen => terminal_panel_open,
    }
}

pub(crate) fn format_combo(combo: &KeyCombo) -> String {
    let mut parts = Vec::new();
    if cfg!(target_os = "macos") {
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

pub(crate) fn combo_from_keystroke(keystroke: &Keystroke) -> Option<KeyCombo> {
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

fn normalize_combo(mut combo: KeyCombo) -> KeyCombo {
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

fn printable_symbol_encodes_shift(key: &str) -> bool {
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

fn combo_to_gpui(combo: &KeyCombo) -> String {
    let mut parts = Vec::new();
    if combo.meta {
        parts.push("cmd".to_string());
    }
    if combo.ctrl {
        parts.push("ctrl".to_string());
    }
    if combo.alt {
        parts.push("alt".to_string());
    }
    if combo.shift {
        parts.push("shift".to_string());
    }
    parts.push(match combo.key.as_str() {
        "arrowleft" => "left".to_string(),
        "arrowright" => "right".to_string(),
        "arrowup" => "up".to_string(),
        "arrowdown" => "down".to_string(),
        key => key.to_string(),
    });
    parts.join("-")
}

pub(crate) fn startup_key_bindings(overrides: &Map<String, Value>) -> Vec<KeyBinding> {
    let side = KeybindingSide::current();
    let mut bindings = Vec::new();
    for definition in ACTION_DEFINITIONS.iter() {
        let default = definition.default_combo(side).clone();
        let effective = effective_combo(definition, overrides, side);
        if effective != default {
            let default_keystroke = combo_to_gpui(&default);
            bindings.push(KeyBinding::new(
                &default_keystroke,
                NoAction {},
                Some(CONTEXT),
            ));
            if matches!(definition.id, "app.commandPalette" | "app.quit") {
                bindings.push(KeyBinding::new(&default_keystroke, NoAction {}, None));
            }
        }
        if definition.terminal_behavior != TerminalBehavior::Always {
            continue;
        }
        if definition.id == "split.closePane" && effective == default {
            continue;
        }
        push_action_binding(&mut bindings, definition.id, &effective);
    }
    bindings
}

pub(crate) fn runtime_rebind_key_bindings(
    action_id: &str,
    previous: &KeyCombo,
    next: &KeyCombo,
) -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    if previous != next {
        let previous_keystroke = combo_to_gpui(previous);
        bindings.push(KeyBinding::new(
            &previous_keystroke,
            NoAction {},
            Some(CONTEXT),
        ));
        if matches!(action_id, "app.commandPalette" | "app.quit") {
            bindings.push(KeyBinding::new(&previous_keystroke, NoAction {}, None));
        }
    }
    if action_id == "split.closePane"
        && action_definition(action_id)
            .is_some_and(|definition| next == definition.default_combo(KeybindingSide::current()))
    {
        return bindings;
    }
    if action_definition(action_id)
        .is_some_and(|definition| definition.terminal_behavior != TerminalBehavior::Always)
    {
        return bindings;
    }
    push_action_binding(&mut bindings, action_id, next);
    bindings
}

fn push_action_binding(bindings: &mut Vec<KeyBinding>, action_id: &str, combo: &KeyCombo) {
    let keystroke = combo_to_gpui(combo);
    macro_rules! push_binding {
        ($action:expr) => {
            bindings.push(KeyBinding::new(&keystroke, $action, Some(CONTEXT)))
        };
        ($action:expr, global) => {
            bindings.push(KeyBinding::new(&keystroke, $action, None))
        };
        ($action:expr, workspace_and_global) => {{
            bindings.push(KeyBinding::new(&keystroke, $action.clone(), Some(CONTEXT)));
            bindings.push(KeyBinding::new(&keystroke, $action, None));
        }};
    }
    match action_id {
        "app.newTerminal" => push_binding!(NewTerminal),
        "app.shellLauncher" => push_binding!(ShellLauncher),
        "app.closeTab" => push_binding!(CloseTab),
        "app.closeOtherTabs" => push_binding!(CloseOtherTabs),
        "app.newConnection" => push_binding!(NewConnection),
        "app.settings" => push_binding!(OpenSettings),
        "app.quit" => push_binding!(Quit, workspace_and_global),
        "app.toggleSidebar" => push_binding!(ToggleSidebar),
        "app.commandPalette" => push_binding!(CommandPalette, workspace_and_global),
        "app.zenMode" => push_binding!(ZenMode),
        "app.nextTab" => push_binding!(NextTab),
        "app.prevTab" => push_binding!(PrevTab),
        "app.goToTab1" => push_binding!(GoToTab1),
        "app.goToTab2" => push_binding!(GoToTab2),
        "app.goToTab3" => push_binding!(GoToTab3),
        "app.goToTab4" => push_binding!(GoToTab4),
        "app.goToTab5" => push_binding!(GoToTab5),
        "app.goToTab6" => push_binding!(GoToTab6),
        "app.goToTab7" => push_binding!(GoToTab7),
        "app.goToTab8" => push_binding!(GoToTab8),
        "app.goToTab9" => push_binding!(GoToTab9),
        "app.fontIncrease" => push_binding!(FontIncrease),
        "app.fontDecrease" => push_binding!(FontDecrease),
        "app.fontReset" => push_binding!(FontReset),
        "app.showShortcuts" => push_binding!(ShowShortcuts),
        "terminal.search" => push_binding!(Find),
        "terminal.copy" => push_binding!(Copy),
        "terminal.cut" => push_binding!(Cut),
        "terminal.paste" => push_binding!(Paste),
        "terminal.aiPanel" => push_binding!(TerminalAiPanel),
        "terminal.recording" => push_binding!(TerminalRecording),
        "terminal.toggleFreeTypeMode" => push_binding!(TerminalFreeTypeMode),
        "terminal.closePanel" => {}
        "split.horizontal" => push_binding!(SplitHorizontal),
        "split.vertical" => push_binding!(SplitVertical),
        "split.closePane" => push_binding!(crate::ClosePane),
        "split.navLeft" => push_binding!(SplitNavLeft),
        "split.navRight" => push_binding!(SplitNavRight),
        "palette.eventLog" => push_binding!(PaletteEventLog),
        "palette.aiSidebar" => push_binding!(PaletteAiSidebar),
        "palette.broadcast" => push_binding!(PaletteBroadcast),
        "app.navBack" | "app.navForward" => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Keystroke, Modifiers};

    #[test]
    fn tauri_default_registry_contains_all_orchestrated_actions() {
        assert_eq!(ACTION_DEFINITIONS.len(), 43);
        assert!(action_definition("app.commandPalette").is_some());
        assert_eq!(
            action_definition("app.quit")
                .map(|definition| definition.default_combo(KeybindingSide::Mac)),
            Some(&KeyCombo::cmd("q"))
        );
        assert!(action_definition("terminal.closePanel").is_some());
        assert!(action_definition("terminal.copy").is_some());
        assert!(action_definition("terminal.cut").is_some());
        assert!(action_definition("terminal.toggleFreeTypeMode").is_some());
        assert!(action_definition("palette.broadcast").is_some());
        assert_eq!(
            action_definition("app.closeTab").map(|definition| definition.terminal_behavior),
            Some(TerminalBehavior::Never)
        );
        assert_eq!(
            action_definition("app.shellLauncher")
                .map(|definition| definition.default_combo(KeybindingSide::Mac)),
            Some(&KeyCombo::cmd_shift("t"))
        );
        assert_eq!(
            action_definition("terminal.closePanel").map(|definition| definition.terminal_behavior),
            Some(TerminalBehavior::WhenPanelOpen)
        );
    }

    #[test]
    fn free_type_mode_shortcut_defaults_do_not_conflict() {
        let definition = action_definition("terminal.toggleFreeTypeMode")
            .expect("free type mode shortcut definition");
        let overrides = Map::new();

        for side in [KeybindingSide::Mac, KeybindingSide::Other] {
            assert!(
                conflicts_for_combo(
                    definition.id,
                    definition.default_combo(side),
                    &overrides,
                    side,
                )
                .is_empty()
            );
        }
    }

    #[test]
    fn printable_symbols_normalize_like_tauri_registry() {
        let combo = normalize_combo(KeyCombo {
            key: "}".to_string(),
            ctrl: false,
            shift: true,
            alt: true,
            meta: true,
        });
        assert_eq!(
            combo,
            KeyCombo {
                key: "}".to_string(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: true,
            }
        );
    }

    #[test]
    fn overrides_are_diff_based_per_platform_side() {
        let mut overrides = Map::new();
        set_override(
            &mut overrides,
            "app.newTerminal",
            KeybindingSide::Mac,
            KeyCombo::cmd("t"),
        );
        assert!(overrides.is_empty());

        set_override(
            &mut overrides,
            "app.newTerminal",
            KeybindingSide::Mac,
            KeyCombo::cmd("n"),
        );
        assert!(overrides.contains_key("app.newTerminal"));

        reset_override(&mut overrides, "app.newTerminal", KeybindingSide::Mac);
        assert!(overrides.is_empty());
    }

    #[test]
    fn keystroke_matching_uses_effective_override() {
        let mut overrides = Map::new();
        set_override(
            &mut overrides,
            "app.newTerminal",
            KeybindingSide::current(),
            if cfg!(target_os = "macos") {
                KeyCombo::cmd("n")
            } else {
                KeyCombo::ctrl("n")
            },
        );

        let keystroke = Keystroke {
            modifiers: Modifiers {
                control: !cfg!(target_os = "macos"),
                platform: cfg!(target_os = "macos"),
                ..Default::default()
            },
            key: "n".to_string(),
            key_char: None,
        };

        assert!(keystroke_matches_action(
            &keystroke,
            "app.newTerminal",
            &overrides
        ));
    }

    #[test]
    fn terminal_behavior_matches_tauri_gating() {
        let close_tab = action_definition("app.closeTab").unwrap();
        let close_panel = action_definition("terminal.closePanel").unwrap();
        let search = action_definition("terminal.search").unwrap();

        assert!(!action_allowed_by_terminal_behavior(
            close_tab,
            &KeyCombo::ctrl("w"),
            true,
            false,
        ));
        assert!(!action_allowed_by_terminal_behavior(
            close_panel,
            close_panel.default_combo(KeybindingSide::current()),
            true,
            false,
        ));
        assert!(action_allowed_by_terminal_behavior(
            close_panel,
            close_panel.default_combo(KeybindingSide::current()),
            true,
            true,
        ));
        assert!(action_allowed_by_terminal_behavior(
            search,
            search.default_combo(KeybindingSide::current()),
            true,
            false,
        ));
    }

    #[test]
    fn broadcast_default_does_not_claim_tmux_prefix() {
        let broadcast = action_definition("palette.broadcast").expect("broadcast action");
        let tmux_prefix = Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: "b".to_string(),
            key_char: None,
        };
        let broadcast_shortcut = Keystroke {
            modifiers: Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
            key: "b".to_string(),
            key_char: None,
        };

        assert_ne!(
            combo_from_keystroke(&tmux_prefix).as_ref(),
            Some(&broadcast.other)
        );
        assert_eq!(
            combo_from_keystroke(&broadcast_shortcut).as_ref(),
            Some(&broadcast.other)
        );
    }

    #[test]
    fn plugin_keybindings_match_tauri_normalization() {
        assert_eq!(
            normalize_plugin_key_combo("Cmd+Shift+R").as_deref(),
            Some("ctrl+r+shift")
        );
        assert_eq!(
            normalize_plugin_key_combo("K+SHIFT+CTRL").as_deref(),
            Some("ctrl+k+shift")
        );
        assert_eq!(
            normalize_plugin_key_combo("Command+Option+Escape").as_deref(),
            Some("alt+ctrl+esc")
        );

        let cmd_shift_r = Keystroke {
            modifiers: Modifiers {
                platform: true,
                shift: true,
                ..Default::default()
            },
            key: "r".to_string(),
            key_char: None,
        };
        assert_eq!(
            normalize_plugin_keystroke(&cmd_shift_r).as_deref(),
            Some("ctrl+r+shift")
        );

        let ctrl_shift_k = Keystroke {
            modifiers: Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
            key: "k".to_string(),
            key_char: None,
        };
        assert_eq!(
            normalize_plugin_keystroke(&ctrl_shift_k).as_deref(),
            Some("ctrl+k+shift")
        );
    }
}
