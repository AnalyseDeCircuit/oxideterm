//! Workspace actions and persisted bindings shared by native and browser windows.
pub mod bindings;
mod definitions;
use bindings::*;
pub use definitions::{def, workspace_definitions};
use gpui::Keystroke;
use serde_json::{Map, Value};

use crate::tabs::{TabLayout, strip::ShellTabRequest};
use oxideterm_workspace::TabId;
#[derive(Clone, Copy)]
pub enum TabShortcut {
    New,
    Close,
    Cycle(bool),
    Index(usize),
}
impl TabShortcut {
    pub fn request<A: Clone + Eq + std::hash::Hash, M>(
        self,
        layout: &TabLayout<A, M>,
        container: Option<TabId>,
    ) -> Option<ShellTabRequest> {
        match self {
            Self::New => Some(ShellTabRequest::NewTab),
            Self::Close => Some(ShellTabRequest::CloseTab {
                tab: layout.focused_page_id(container?),
                select_first: false,
            }),
            Self::Index(index) => layout
                .main_tabs()
                .nth(index)
                .map(|(_, tab)| ShellTabRequest::Select(tab.id)),
            Self::Cycle(forward) => {
                let tabs: Vec<_> = layout.main_tabs().map(|(_, tab)| tab.id).collect();
                if tabs.is_empty() {
                    return None;
                }
                let current = tabs
                    .iter()
                    .position(|id| Some(*id) == container)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % tabs.len()
                } else {
                    (current + tabs.len() - 1) % tabs.len()
                };
                Some(ShellTabRequest::Select(tabs[next]))
            }
        }
    }
}
pub fn context_bindings<T: std::marker::Copy>(
    definitions: &[ActionDefinition],
    actions: &[(&str, T)],
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Vec<(gpui::KeyBinding, T)> {
    let mut bindings = Vec::new();
    for &(id, action) in actions {
        if let Some(def) = definitions.iter().find(|def| def.id == id) {
            for combo in effective_combos(def, overrides, side) {
                bindings.push((gpui::KeyBinding::new(&combo_to_gpui(&combo), gpui::NoAction {}, None), action));
            }
        }
    }
    bindings
}

pub fn normalized_keystroke(key: &Keystroke) -> Option<Keystroke> {
    Keystroke::parse(&combo_to_gpui(&combo_from_keystroke(key)?)).ok()
}

pub fn combo_to_gpui(combo: &KeyCombo) -> String {
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

pub const WORKSPACE_CONTEXT: &str = "Workspace";
pub const RECORDING_CONTEXT: &str = "KeybindingRecording";
gpui::actions!(
    oxideterm,
    [
        NewTerminal,
        ShellLauncher,
        CloseTab,
        CloseOtherTabs,
        NewConnection,
        ToggleSidebar,
        CommandPalette,
        ZenMode,
        ToggleFullscreen,
        NextTab,
        PrevTab,
        GoToTab1,
        GoToTab2,
        GoToTab3,
        GoToTab4,
        GoToTab5,
        GoToTab6,
        GoToTab7,
        GoToTab8,
        GoToTab9,
        FontIncrease,
        FontDecrease,
        FontReset,
        ShowShortcuts,
        Copy,
        Cut,
        Paste,
        Find,
        FindNext,
        FindPrev,
        CloseSearch,
        OpenSettings,
        SwitchLocaleEnglish,
        SwitchLocaleChinese,
        SwitchLocaleTraditionalChinese,
        SwitchLocaleGerman,
        SwitchLocaleSpanish,
        SwitchLocaleFrench,
        SwitchLocaleItalian,
        SwitchLocaleJapanese,
        SwitchLocaleKorean,
        SwitchLocalePortugueseBrazil,
        SwitchLocaleVietnamese,
        SplitHorizontal,
        SplitVertical,
        ClosePane,
        SplitNavLeft,
        SplitNavRight,
        TerminalAiPanel,
        TerminalClearScreen,
        TerminalRecording,
        TerminalFreeTypeMode,
        PaletteEventLog,
        PaletteAiSidebar,
        PaletteBroadcast,
        PaletteDisconnectAll,
        PaletteReconnectAll,
        PaletteCancelReconnect,
        PaletteHealthCheck,
        PaletteResetPanes,
        PaletteDetachTerminal,
        PaletteCleanupDead
    ]
);

pub fn keystroke_matches_action(
    keystroke: &Keystroke,
    definition: &ActionDefinition,
    side: KeybindingSide,
    overrides: &Map<String, Value>,
) -> bool {
    let Some(combo) = combo_from_keystroke(keystroke) else {
        return false;
    };
    effective_combos(definition, overrides, side).contains(&combo)
}

pub fn matched_action_for_keystroke<'a>(
    definitions: &'a [ActionDefinition],
    side: KeybindingSide,
    keystroke: &Keystroke,
    overrides: &Map<String, Value>,
) -> Option<(&'a ActionDefinition, KeyCombo)> {
    let combo = combo_from_keystroke(keystroke)?;
    definitions
        .iter()
        .filter(|definition| !definition.scope.local())
        .find(|definition| effective_combo(definition, overrides, side).as_ref() == Some(&combo))
        .map(|definition| (definition, combo))
}

pub fn action_allowed_by_terminal_behavior(
    side: KeybindingSide,
    definition: &ActionDefinition,
    combo: &KeyCombo,
    terminal_active: bool,
    terminal_panel_open: bool,
) -> bool {
    if !terminal_active {
        return definition.terminal_behavior != TerminalBehavior::WhenPanelOpen
            || terminal_panel_open;
    }

    let mac_safe_meta = side == KeybindingSide::Mac && combo.meta && !combo.ctrl;
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

pub fn matched_scoped_action<'a>(
    definitions: &'a [ActionDefinition],
    side: KeybindingSide,
    keystroke: &Keystroke,
    scope: ActionScope,
    overrides: &Map<String, Value>,
) -> Option<&'a str> {
    let combo = combo_from_keystroke(keystroke)?;
    definitions
        .iter()
        .filter(|definition| definition.scope == scope)
        .find(|definition| effective_combos(definition, overrides, side).contains(&combo))
        .map(|definition| definition.id.as_ref())
}
