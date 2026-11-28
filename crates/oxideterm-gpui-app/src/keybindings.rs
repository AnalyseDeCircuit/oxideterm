use gpui::{KeyBinding, Keystroke, NoAction};
use serde_json::{Map, Value};
use std::sync::LazyLock;

use crate::{
    CloseOtherTabs, CloseTab, CommandPalette, Copy, Cut, Find, FontDecrease, FontIncrease,
    FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8,
    GoToTab9, NewConnection, NewTerminal, NextTab, OpenSettings, PaletteAiSidebar,
    PaletteBroadcast, PaletteEventLog, Paste, PrevTab, Quit, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, TerminalAiPanel,
    TerminalClearScreen, TerminalFreeTypeMode, TerminalRecording, ToggleFullscreen, ToggleSidebar,
    ZenMode,
};

use oxideterm_gpui_shell::shortcuts::{self, WORKSPACE_CONTEXT as CONTEXT, def};

// Window commands stay available in text fields, but must not fire while recording a shortcut.
#[cfg(target_os = "macos")]
const WINDOW_ACTION_CONTEXT: &str = "!KeybindingRecording";

#[cfg(test)]
use oxideterm_gpui_settings_view::keybindings::conflicts_in_definitions;
pub(crate) use oxideterm_gpui_settings_view::keybindings::{
    ActionDefinition, ActionScope, KeyCombo, KeybindingSide, TerminalBehavior,
    combo_from_keystroke, effective_combo, effective_combos, format_combo,
};
use oxideterm_gpui_settings_view::keybindings::{normalize_combo, override_binding};

pub(crate) static ACTION_DEFINITIONS: LazyLock<Vec<ActionDefinition>> = LazyLock::new(|| {
    let mut actions = shortcuts::workspace_definitions();
    actions.push(def(
        "app.quit",
        ActionScope::Global,
        KeyCombo::cmd("q"),
        KeyCombo::ctrl("q"),
    ));
    #[cfg(target_os = "macos")]
    for (id, combo) in [
        ("app.hide", KeyCombo::cmd("h")),
        ("app.hideOthers", KeyCombo::cmd_alt("h")),
        ("app.minimize", KeyCombo::cmd("m")),
        ("app.minimizeAll", KeyCombo::cmd_alt("m")),
    ] {
        actions.push(def(id, ActionScope::Global, combo.clone(), combo));
    }

    actions
});

pub(crate) fn action_definition(id: &str) -> Option<&'static ActionDefinition> {
    ACTION_DEFINITIONS.iter().find(|action| action.id == id)
}

pub(crate) fn set_unbound_override(
    overrides: &mut Map<String, Value>,
    action_id: &str,
    side: KeybindingSide,
) {
    if action_definition(action_id).is_none() && !is_plugin_action_id(action_id) {
        return;
    }
    let mut entry = overrides
        .get(action_id)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    // JSON null is an explicit tombstone that suppresses the built-in default.
    entry.insert(side.json_key().to_string(), Value::Null);
    overrides.insert(action_id.to_string(), Value::Object(entry));
}

#[cfg(test)]
pub(crate) fn set_override(
    overrides: &mut Map<String, Value>,
    action_id: &str,
    side: KeybindingSide,
    combo: KeyCombo,
) {
    let Some(definition) = action_definition(action_id) else {
        return;
    };
    set_definition_override(overrides, definition, side, combo);
}

pub(crate) fn set_definition_override(
    overrides: &mut Map<String, Value>,
    definition: &ActionDefinition,
    side: KeybindingSide,
    combo: KeyCombo,
) {
    let action_id = definition.id.as_ref();
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
        if action_definition(&action_id).is_none() && !is_plugin_action_id(&action_id) {
            return Err(format!("unknown action id: {action_id}"));
        }
        let Value::Object(object) = value else {
            return Err(format!("invalid override for {action_id}"));
        };

        let mut entry = Map::new();
        for side in [KeybindingSide::Mac, KeybindingSide::Other] {
            if let Some(combo_value) = object.get(side.json_key()) {
                if combo_value.is_null() {
                    entry.insert(side.json_key().to_string(), Value::Null);
                    continue;
                }
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

#[cfg(test)]
pub(crate) fn modified_count(overrides: &Map<String, Value>) -> usize {
    overrides.len()
}

#[cfg(test)]
pub(crate) fn conflicts_for_combo(
    action_id: &str,
    combo: &KeyCombo,
    overrides: &Map<String, Value>,
    side: KeybindingSide,
) -> Vec<&'static ActionDefinition> {
    conflicts_in_definitions(action_id, combo, overrides, side, &ACTION_DEFINITIONS)
}

pub(crate) fn keystroke_matches_action(
    keystroke: &Keystroke,
    action_id: &str,
    overrides: &Map<String, Value>,
) -> bool {
    action_definition(action_id).is_some_and(|definition| {
        shortcuts::keystroke_matches_action(
            keystroke,
            definition,
            KeybindingSide::current(),
            overrides,
        )
    })
}

pub(crate) fn matched_action_for_keystroke(
    keystroke: &Keystroke,
    overrides: &Map<String, Value>,
) -> Option<(&'static ActionDefinition, KeyCombo)> {
    shortcuts::matched_action_for_keystroke(
        &ACTION_DEFINITIONS,
        KeybindingSide::current(),
        keystroke,
        overrides,
    )
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
    shortcuts::action_allowed_by_terminal_behavior(
        KeybindingSide::current(),
        definition,
        combo,
        terminal_active,
        terminal_panel_open,
    )
}

use shortcuts::combo_to_gpui;

pub(crate) fn startup_key_bindings(overrides: &Map<String, Value>) -> Vec<KeyBinding> {
    let side = KeybindingSide::current();
    let mut bindings = Vec::new();
    for definition in ACTION_DEFINITIONS
        .iter()
        .filter(|definition| !definition.scope.local() && !terminal_leaf_action(&definition.id))
    {
        let default = definition.default_combo(side).clone();
        let effective = effective_combo(definition, overrides, side);
        if effective.as_ref() != Some(&default) {
            let default_keystroke = combo_to_gpui(&default);
            bindings.push(KeyBinding::new(
                &default_keystroke,
                NoAction {},
                Some(CONTEXT),
            ));
            if let Some(context) = window_action_context(&definition.id) {
                bindings.push(KeyBinding::new(
                    &default_keystroke,
                    NoAction {},
                    Some(context),
                ));
            } else if matches!(definition.id.as_ref(), "app.commandPalette" | "app.quit") {
                bindings.push(KeyBinding::new(&default_keystroke, NoAction {}, None));
            }
        }
        if definition.terminal_behavior != TerminalBehavior::Always {
            continue;
        }
        let Some(effective) = effective else {
            continue;
        };
        if definition.id == "split.closePane" && effective == default {
            continue;
        }
        push_action_binding(&mut bindings, &definition.id, &effective);
    }
    bindings
}

pub(crate) fn runtime_rebind_key_bindings(
    action_id: &str,
    previous: Option<&KeyCombo>,
    next: Option<&KeyCombo>,
) -> Vec<KeyBinding> {
    if is_plugin_action_id(action_id) || terminal_leaf_action(action_id) {
        return Vec::new();
    }
    if action_definition(action_id).is_some_and(|definition| definition.scope.local()) {
        return Vec::new();
    }
    let mut bindings = Vec::new();
    if previous != next {
        if let Some(previous) = previous {
            let previous_keystroke = combo_to_gpui(previous);
            bindings.push(KeyBinding::new(
                &previous_keystroke,
                NoAction {},
                Some(CONTEXT),
            ));
            if let Some(context) = window_action_context(action_id) {
                bindings.push(KeyBinding::new(
                    &previous_keystroke,
                    NoAction {},
                    Some(context),
                ));
            } else if matches!(action_id, "app.commandPalette" | "app.quit") {
                bindings.push(KeyBinding::new(&previous_keystroke, NoAction {}, None));
            }
        }
    }
    let Some(next) = next else {
        return bindings;
    };
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

fn window_action_context(_action_id: &str) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    if matches!(
        _action_id,
        "app.hide" | "app.hideOthers" | "app.minimize" | "app.minimizeAll"
    ) {
        return Some(WINDOW_ACTION_CONTEXT);
    }
    None
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
        ($action:expr, window) => {
            bindings.push(KeyBinding::new(
                &keystroke,
                $action,
                window_action_context(action_id),
            ))
        };
    }
    match action_id {
        "app.newTerminal" => push_binding!(NewTerminal),
        "app.shellLauncher" => push_binding!(ShellLauncher),
        "app.closeTab" => push_binding!(CloseTab),
        "app.closeOtherTabs" => push_binding!(CloseOtherTabs),
        "app.newConnection" => push_binding!(NewConnection),
        "app.settings" => push_binding!(OpenSettings),
        "app.quit" => push_binding!(Quit, workspace_and_global),
        #[cfg(target_os = "macos")]
        "app.hide" => push_binding!(crate::platform::Hide, window),
        #[cfg(target_os = "macos")]
        "app.hideOthers" => push_binding!(crate::platform::HideOthers, window),
        #[cfg(target_os = "macos")]
        "app.minimize" => push_binding!(crate::platform::Minimize, window),
        #[cfg(target_os = "macos")]
        "app.minimizeAll" => push_binding!(crate::platform::MinimizeAll, window),
        "app.toggleSidebar" => push_binding!(ToggleSidebar),
        "app.commandPalette" => push_binding!(CommandPalette, workspace_and_global),
        "app.zenMode" => push_binding!(ZenMode),
        "app.toggleFullscreen" => push_binding!(ToggleFullscreen),
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
        "terminal.clearScreen" => push_binding!(TerminalClearScreen),
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
    fn macos_window_shortcuts_are_registered_only_on_macos() {
        for (id, chord) in [
            ("app.hide", "cmd-h"),
            ("app.hideOthers", "cmd-alt-h"),
            ("app.minimize", "cmd-m"),
            ("app.minimizeAll", "cmd-alt-m"),
        ] {
            if cfg!(target_os = "macos") {
                let definition = action_definition(id).expect("missing macOS window action");
                assert_eq!(definition.scope, ActionScope::Global);
                assert!(keystroke_matches_action(
                    &Keystroke::parse(chord).unwrap(),
                    id,
                    &Map::new(),
                ));
            } else {
                assert!(action_definition(id).is_none(), "{id}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn window_shortcuts_follow_focus_overrides_and_recording() {
        use crate::platform::{Hide, HideOthers, Minimize, MinimizeAll};
        use gpui::{Action, KeyContext, Keymap};

        let cases: [(&str, &str, Box<dyn Action>); 4] = [
            ("app.hide", "cmd-h", Box::new(Hide)),
            ("app.hideOthers", "cmd-alt-h", Box::new(HideOthers)),
            ("app.minimize", "cmd-m", Box::new(Minimize)),
            ("app.minimizeAll", "cmd-alt-m", Box::new(MinimizeAll)),
        ];
        for (id, chord, action) in cases {
            let definition = action_definition(id).unwrap();
            let original = definition.mac.clone();
            let mut keymap = Keymap::default();
            keymap.add_bindings(startup_key_bindings(&Map::new()));
            let input = [Keystroke::parse(chord).unwrap()];
            for leaf in ["Terminal", "Editor", "Input", "Modal", "Workspace"] {
                let contexts = [
                    KeyContext::parse("Workspace").unwrap(),
                    KeyContext::parse(leaf).unwrap(),
                ];
                let (bindings, _) = keymap.bindings_for_input(&input, &contexts);
                assert_eq!(
                    bindings
                        .iter()
                        .map(|b| b.action().name())
                        .collect::<Vec<_>>(),
                    vec![action.name()],
                    "{id}: {leaf}",
                );
            }
            let recording = [
                KeyContext::parse("Workspace KeybindingRecording").unwrap(),
                KeyContext::parse("Input").unwrap(),
            ];
            assert!(
                keymap.bindings_for_input(&input, &recording).0.is_empty(),
                "{id}"
            );

            let mut custom = KeyCombo::cmd_alt("m");
            custom.shift = true;
            let custom_input = [Keystroke::parse("cmd-alt-shift-m").unwrap()];
            let contexts = [KeyContext::parse("Workspace").unwrap()];
            for (previous, next, expected) in [
                (Some(&original), Some(&custom), Some(&custom_input)),
                (Some(&custom), None, None),
                (None, Some(&original), Some(&input)),
            ] {
                keymap.add_bindings(runtime_rebind_key_bindings(id, previous, next));
                for keys in [&input, &custom_input] {
                    let bindings = keymap.bindings_for_input(keys, &contexts).0;
                    let expected_names = if expected == Some(keys) {
                        vec![action.name()]
                    } else {
                        Vec::new()
                    };
                    assert_eq!(
                        bindings
                            .iter()
                            .map(|b| b.action().name())
                            .collect::<Vec<_>>(),
                        expected_names,
                        "{id}",
                    );
                }
                // Native menus obtain their displayed shortcut from this same lookup.
                let menu_bindings = keymap
                    .bindings_for_action(action.as_ref())
                    .collect::<Vec<_>>();
                match expected {
                    Some(keys) => {
                        assert_eq!(menu_bindings.len(), 1, "{id}");
                        assert_eq!(menu_bindings[0].match_keystrokes(keys), Some(false), "{id}");
                    }
                    None => assert!(menu_bindings.is_empty(), "{id}"),
                }
            }
        }
    }

    #[gpui::test]
    fn suggestion_navigation_uses_existing_overrides_and_unbinding(cx: &mut gpui::TestAppContext) {
        use oxideterm_gpui_terminal::{TerminalKeybindings, TerminalShortcut};
        let mut overrides = Map::new();
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        cx.read(|cx| {
            let bindings = cx.global::<TerminalKeybindings>();
            assert_eq!(
                bindings.resolve(&Keystroke::parse("alt-up").unwrap()),
                Some(TerminalShortcut::SuggestPrevious)
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse("alt-down").unwrap()),
                Some(TerminalShortcut::SuggestNext)
            );
        });
        set_override(
            &mut overrides,
            "terminal.suggestPrevious",
            KeybindingSide::current(),
            KeyCombo::plain("arrowup"),
        );
        set_unbound_override(
            &mut overrides,
            "terminal.suggestNext",
            KeybindingSide::current(),
        );
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        cx.read(|cx| {
            let bindings = cx.global::<TerminalKeybindings>();
            assert_eq!(
                bindings.resolve(&Keystroke::parse("up").unwrap()),
                Some(TerminalShortcut::SuggestPrevious)
            );
            assert_eq!(bindings.resolve(&Keystroke::parse("alt-up").unwrap()), None);
            assert_eq!(
                bindings.resolve(&Keystroke::parse("alt-down").unwrap()),
                None
            );
        });
    }

    #[gpui::test]
    fn terminal_word_shortcuts_can_be_rebound_and_disabled(cx: &mut gpui::TestAppContext) {
        use oxideterm_gpui_terminal::{TerminalKeybindings, TerminalShortcut};
        let side = KeybindingSide::current();
        let default_backward = if cfg!(target_os = "macos") {
            "alt-left"
        } else {
            "alt-b"
        };
        let default_forward = if cfg!(target_os = "macos") {
            "alt-right"
        } else {
            "alt-f"
        };
        let mut overrides = Map::new();
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        cx.read(|cx| {
            let bindings = cx.global::<TerminalKeybindings>();
            assert_eq!(
                bindings.resolve(&Keystroke::parse(default_backward).unwrap()),
                Some(TerminalShortcut::WordBackward)
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse(default_forward).unwrap()),
                Some(TerminalShortcut::WordForward)
            );
        });
        set_override(
            &mut overrides,
            "terminal.wordBackward",
            side,
            KeyCombo::ctrl("h"),
        );
        set_unbound_override(&mut overrides, "terminal.wordForward", side);
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        cx.read(|cx| {
            let bindings = cx.global::<TerminalKeybindings>();
            assert_eq!(
                bindings.resolve(&Keystroke::parse("ctrl-h").unwrap()),
                Some(TerminalShortcut::WordBackward)
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse(default_backward).unwrap()),
                None
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse(default_forward).unwrap()),
                None
            );
        });
    }

    #[gpui::test]
    fn terminal_keybindings_disable_primary_and_alternate_chords_independently(
        cx: &mut gpui::TestAppContext,
    ) {
        use oxideterm_gpui_terminal::{TerminalKeybindings, TerminalShortcut};
        let mut overrides = Map::new();
        let side = KeybindingSide::current();
        set_override(
            &mut overrides,
            "terminal.scrollTop",
            side,
            KeyCombo::ctrl("u"),
        );
        set_unbound_override(&mut overrides, "terminal.copy", side);
        set_unbound_override(&mut overrides, "terminal.scrollPageUp", side);
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        cx.read(|cx| {
            let bindings = cx.global::<TerminalKeybindings>();
            assert_eq!(
                bindings.resolve(&Keystroke::parse("ctrl-u").unwrap()),
                Some(TerminalShortcut::Top)
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse("shift-home").unwrap()),
                None
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse("ctrl-insert").unwrap()),
                Some(TerminalShortcut::Copy)
            );
            let primary = action_definition("terminal.copy")
                .unwrap()
                .default_combo(side);
            assert_eq!(
                bindings.resolve(&Keystroke::parse(&combo_to_gpui(primary)).unwrap()),
                None
            );
            assert_eq!(
                bindings.resolve(&Keystroke::parse("shift-pageup").unwrap()),
                None
            );
            assert_eq!(bindings.resolve(&Keystroke::parse("cmd-up").unwrap()), None);
        });
    }

    #[test]
    fn plugin_keybindings_keep_overrides_across_registration_lifetimes() {
        let mut entry = oxideterm_plugin_registry::NativePluginRuntimeKeybindingContribution {
            plugin_id: "test.tools".into(),
            plugin_name: "Tools".into(),
            registration_id: "first-instance".into(),
            keybinding: "Ctrl+Shift+K".into(),
            normalized_keybinding: "ctrl+k+shift".into(),
            command: "open-tools".into(),
            label: "Open tools".into(),
        };
        let definition = plugin_action_definition(&entry).unwrap();
        let mut overrides = Map::new();
        let original = Keystroke::parse("ctrl-shift-k").unwrap();
        let custom = Keystroke::parse("ctrl-f10").unwrap();
        assert!(plugin_binding_matches(&entry, &original, &overrides));
        set_definition_override(
            &mut overrides,
            &definition,
            KeybindingSide::current(),
            KeyCombo::ctrl("f10"),
        );
        entry.registration_id = "second-instance".into();
        entry.label = "Reloaded tools".into();
        let mut imported = sanitize_imported_overrides(Value::Object(overrides)).unwrap();
        assert!(plugin_binding_matches(&entry, &custom, &imported));
        assert!(!plugin_binding_matches(&entry, &original, &imported));
        set_unbound_override(&mut imported, &definition.id, KeybindingSide::current());
        let mut imported = sanitize_imported_overrides(Value::Object(imported)).unwrap();
        assert!(!plugin_binding_matches(&entry, &custom, &imported));
        assert!(!plugin_binding_matches(&entry, &original, &imported));
        reset_override(&mut imported, &definition.id, KeybindingSide::current());
        assert!(plugin_binding_matches(&entry, &original, &imported));
        let mut definitions = ACTION_DEFINITIONS.to_vec();
        definitions.push(definition.clone());
        assert_eq!(
            conflicts_in_definitions(
                &definition.id,
                &KeyCombo::ctrl("n"),
                &imported,
                KeybindingSide::Other,
                &definitions
            )
            .iter()
            .map(|definition| definition.id.as_ref())
            .collect::<Vec<_>>(),
            ["app.newConnection"]
        );
    }

    #[test]
    fn file_and_preview_keybindings_release_old_chords_without_changing_other_scopes() {
        let side = KeybindingSide::current();
        let mut overrides = Map::new();
        set_override(
            &mut overrides,
            "fileManager.rename",
            side,
            KeyCombo::ctrl("r"),
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse("ctrl-r").unwrap(),
                ActionScope::FileManager,
                &overrides
            ),
            Some("fileManager.rename")
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse("f2").unwrap(),
                ActionScope::FileManager,
                &overrides
            ),
            None
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse("f2").unwrap(),
                ActionScope::Sftp,
                &overrides
            ),
            Some("sftp.rename")
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse("+").unwrap(),
                ActionScope::Preview,
                &overrides
            ),
            Some("preview.zoomIn")
        );
        set_unbound_override(&mut overrides, "preview.zoomIn", side);
        for key in ["+", "="] {
            assert_eq!(
                matched_scoped_action(
                    &Keystroke::parse(key).unwrap(),
                    ActionScope::Preview,
                    &overrides
                ),
                None
            );
        }
    }

    #[test]
    fn local_keybindings_conflict_only_in_overlapping_scopes() {
        let side = KeybindingSide::Other;
        let mut overrides = Map::new();
        set_override(&mut overrides, "editor.save", side, KeyCombo::ctrl("a"));
        let conflicts: Vec<_> =
            conflicts_for_combo("editor.save", &KeyCombo::ctrl("a"), &overrides, side)
                .iter()
                .map(|definition| definition.id.as_ref())
                .collect();
        assert_eq!(conflicts, ["editor.selectAll"]);
        assert_eq!(
            conflicts_for_combo("editor.save", &KeyCombo::ctrl("n"), &overrides, side)
                .iter()
                .map(|definition| definition.id.as_ref())
                .collect::<Vec<_>>(),
            ["app.newConnection"]
        );
        assert_eq!(
            conflicts_for_combo(
                "sftp.rename",
                &KeyCombo::plain("backspace"),
                &overrides,
                side
            )
            .iter()
            .map(|definition| definition.id.as_ref())
            .collect::<Vec<_>>(),
            ["sftp.delete"]
        );
        set_unbound_override(&mut overrides, "sftp.delete", side);
        assert!(
            conflicts_for_combo(
                "sftp.rename",
                &KeyCombo::plain("backspace"),
                &overrides,
                side
            )
            .is_empty()
        );
    }

    #[gpui::test]
    fn editor_keybindings_rebind_unbind_and_restore_on_an_open_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::Focusable;
        use oxideterm_gpui_editor::TextEditorView;
        let side = KeybindingSide::current();
        let mut overrides = Map::new();
        set_override(&mut overrides, "editor.undo", side, KeyCombo::ctrl("u"));
        cx.update(|cx| install_context_keybindings(&overrides, cx));
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = TextEditorView::new("base", &oxideterm_theme::default_tokens(), cx);
            editor.focus_handle(cx).focus(window, cx);
            editor
        });
        editor.update(cx, |editor, cx| editor.insert_text("new", cx));
        let default = combo_to_gpui(
            action_definition("editor.undo")
                .unwrap()
                .default_combo(side),
        );
        cx.simulate_keystrokes(&default);
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer().text(), "newbase")
        });
        cx.simulate_keystrokes("ctrl-u");
        editor.read_with(cx, |editor, _| assert_eq!(editor.buffer().text(), "base"));
        editor.update(cx, |editor, cx| {
            editor.reveal_line_column(1, 1, cx);
            editor.insert_text("later", cx);
        });
        set_unbound_override(&mut overrides, "editor.undo", side);
        cx.update(|_, cx| install_context_keybindings(&overrides, cx));
        cx.simulate_keystrokes("ctrl-u");
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer().text(), "laterbase")
        });
        reset_override(&mut overrides, "editor.undo", side);
        cx.update(|_, cx| install_context_keybindings(&overrides, cx));
        cx.simulate_keystrokes(&default);
        editor.read_with(cx, |editor, _| assert_eq!(editor.buffer().text(), "base"));
        set_override(&mut overrides, "editor.undo", side, KeyCombo::ctrl("{"));
        cx.update(|_, cx| install_context_keybindings(&overrides, cx));
        editor.update(cx, |editor, cx| editor.insert_text("symbol", cx));
        cx.simulate_keystrokes("ctrl-alt-shift-[->{");
        editor.read_with(cx, |editor, _| assert_eq!(editor.buffer().text(), "base"));
    }

    #[gpui::test]
    fn editor_keybindings_take_precedence_over_workspace_actions(cx: &mut gpui::TestAppContext) {
        use gpui::{
            AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
            Render, Styled, Window, div,
        };
        use oxideterm_gpui_editor::TextEditorView;
        struct Host {
            editor: Entity<TextEditorView>,
            zen: usize,
        }
        impl Render for Host {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .key_context("Workspace")
                    .on_action(cx.listener(|this, _: &ZenMode, _, _| this.zen += 1))
                    .child(self.editor.clone())
            }
        }
        cx.update(|cx| {
            let overrides = Map::new();
            install_context_keybindings(&overrides, cx);
            cx.bind_keys(startup_key_bindings(&overrides));
        });
        let (host, cx) = cx.add_window_view(|window, cx| {
            let editor = cx.new(|cx| {
                let mut editor =
                    TextEditorView::new("base", &oxideterm_theme::default_tokens(), cx);
                editor.insert_text("new", cx);
                editor.focus_handle(cx).focus(window, cx);
                editor
            });
            Host { editor, zen: 0 }
        });
        let side = KeybindingSide::current();
        let undo = combo_to_gpui(
            action_definition("editor.undo")
                .unwrap()
                .default_combo(side),
        );
        let redo = combo_to_gpui(
            action_definition("editor.redo")
                .unwrap()
                .default_combo(side),
        );
        cx.simulate_keystrokes(&undo);
        host.read_with(cx, |host, cx| {
            assert_eq!(host.editor.read(cx).buffer().text(), "base")
        });
        cx.simulate_keystrokes(&redo);
        host.read_with(cx, |host, cx| {
            assert_eq!(host.editor.read(cx).buffer().text(), "newbase");
            assert_eq!(host.zen, 0);
        });
    }

    #[test]
    fn sftp_keybindings_customization_releases_defaults_and_imports_tombstones() {
        let side = KeybindingSide::current();
        let mut overrides = Map::new();
        let locate = action_definition("sftp.revealTerminalDirectory").unwrap();
        // This action is used beside terminals, so its default must be distinct across scopes.
        for side in [KeybindingSide::Mac, KeybindingSide::Other] {
            let duplicates = ACTION_DEFINITIONS
                .iter()
                .filter(|definition| definition.id != locate.id)
                .filter(|definition| {
                    effective_combos(definition, &overrides, side)
                        .contains(locate.default_combo(side))
                })
                .map(|definition| definition.id.as_ref())
                .collect::<Vec<_>>();
            assert_eq!(duplicates, Vec::<&str>::new(), "{side:?}");
        }
        let locate_default = if cfg!(target_os = "macos") {
            "cmd-shift-g"
        } else {
            "ctrl-shift-g"
        };
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse(locate_default).unwrap(),
                ActionScope::Sftp,
                &overrides,
            ),
            Some("sftp.revealTerminalDirectory")
        );
        set_override(
            &mut overrides,
            "sftp.revealTerminalDirectory",
            side,
            KeyCombo::ctrl("f10"),
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse(locate_default).unwrap(),
                ActionScope::Sftp,
                &overrides,
            ),
            None
        );
        assert_eq!(
            matched_scoped_action(
                &Keystroke::parse("ctrl-f10").unwrap(),
                ActionScope::Sftp,
                &overrides,
            ),
            Some("sftp.revealTerminalDirectory")
        );
        assert!(keystroke_matches_action(
            &Keystroke::parse("backspace").unwrap(),
            "sftp.delete",
            &overrides
        ));
        set_override(
            &mut overrides,
            "sftp.delete",
            side,
            KeyCombo::ctrl_shift("d"),
        );
        for key in ["backspace", "delete"] {
            assert!(!keystroke_matches_action(
                &Keystroke::parse(key).unwrap(),
                "sftp.delete",
                &overrides
            ));
        }
        assert!(keystroke_matches_action(
            &Keystroke::parse("ctrl-shift-d").unwrap(),
            "sftp.delete",
            &overrides
        ));
        set_unbound_override(&mut overrides, "editor.find", side);
        let imported = sanitize_imported_overrides(Value::Object(overrides)).unwrap();
        assert!(keystroke_matches_action(
            &Keystroke::parse("ctrl-shift-d").unwrap(),
            "sftp.delete",
            &imported
        ));
        let find = action_definition("editor.find").unwrap();
        assert!(effective_combo(find, &imported, side).is_none());
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
    fn explicit_unbound_override_suppresses_default_and_round_trips_import() {
        let action_id = "terminal.clearScreen";
        let side = KeybindingSide::current();
        let definition = action_definition(action_id).unwrap();
        let default = definition.default_combo(side).clone();
        let mut overrides = Map::new();

        set_unbound_override(&mut overrides, action_id, side);

        assert_eq!(effective_combo(definition, &overrides, side), None);
        assert_eq!(modified_count(&overrides), 1);
        let serialized = Value::Object(overrides.clone());
        let sanitized = sanitize_imported_overrides(serialized).unwrap();
        assert_eq!(effective_combo(definition, &sanitized, side), None);

        let default_keystroke = Keystroke {
            modifiers: Modifiers {
                control: default.ctrl,
                shift: default.shift,
                alt: default.alt,
                platform: default.meta,
                ..Default::default()
            },
            key: default.key,
            key_char: None,
        };
        assert!(!keystroke_matches_action(
            &default_keystroke,
            action_id,
            &sanitized,
        ));

        reset_override(&mut overrides, action_id, side);
        assert_eq!(
            effective_combo(definition, &overrides, side),
            Some(definition.default_combo(side).clone())
        );
    }

    #[test]
    fn ctrl_l_remains_terminal_input_on_all_platforms() {
        let definition = action_definition("terminal.clearScreen").unwrap();
        let overrides = Map::new();

        for side in [KeybindingSide::Mac, KeybindingSide::Other] {
            assert_eq!(
                effective_combo(definition, &overrides, side),
                Some(KeyCombo::ctrl_shift("l")),
                "{side:?} must reserve Ctrl+L for the shell"
            );
        }

        let mut keystroke = Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: "l".to_string(),
            key_char: None,
        };
        assert!(matched_action_for_keystroke(&keystroke, &overrides).is_none());
        keystroke.modifiers.shift = true;
        assert_eq!(
            matched_action_for_keystroke(&keystroke, &overrides)
                .map(|(action, _)| action.id.as_ref()),
            Some("terminal.clearScreen")
        );
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
}

pub(crate) fn install_context_keybindings(overrides: &Map<String, Value>, cx: &mut gpui::App) {
    // Native bindings dispatch before text key handlers. Modal controls must
    // receive the chord themselves instead of invoking a background action.
    cx.bind_keys(ACTION_DEFINITIONS.iter().flat_map(|definition| {
        effective_combos(definition, overrides, KeybindingSide::current())
            .into_iter()
            .map(|combo| {
                KeyBinding::new(
                    &combo_to_gpui(&combo),
                    NoAction {},
                    Some(oxideterm_gpui_ui::modal::MODAL_KEY_CONTEXT),
                )
            })
    }));
    use oxideterm_gpui_editor::{EditorKeybindings, EditorShortcut};
    let mut bindings = Vec::new();
    let mut editor_context = Vec::new();
    for &(id, action) in EditorShortcut::BINDINGS {
        let definition = action_definition(id).expect("editor shortcut is registered");
        for combo in effective_combos(definition, overrides, KeybindingSide::current()) {
            let keystroke = combo_to_gpui(&combo);
            bindings.push((KeyBinding::new(&keystroke, NoAction {}, None), action));
            // Native action dispatch precedes keydown bubbling. Let the editor
            // handle these chords instead of a lower-priority workspace action.
            editor_context.push(KeyBinding::new(&keystroke, NoAction {}, Some("TextEditor")));
        }
    }
    cx.bind_keys(editor_context);
    cx.set_global(EditorKeybindings {
        bindings,
        normalize: |keystroke| {
            let combo = combo_from_keystroke(keystroke)?;
            Keystroke::parse(&combo_to_gpui(&combo)).ok()
        },
    });
    use oxideterm_gpui_terminal::{TerminalKeybindings, TerminalShortcut};
    cx.set_global(TerminalKeybindings {
        bindings: shortcuts::context_bindings(&ACTION_DEFINITIONS, TerminalShortcut::BINDINGS, overrides, KeybindingSide::current()),
        normalize: shortcuts::normalized_keystroke,
    });
}

fn is_plugin_action_id(id: &str) -> bool {
    id.strip_prefix("plugin.keybinding:")
        .and_then(|key| serde_json::from_str::<[String; 2]>(key).ok())
        .is_some_and(|parts| parts.iter().all(|part| !part.is_empty()))
}

pub(crate) fn plugin_action_definition(
    entry: &oxideterm_plugin_registry::NativePluginRuntimeKeybindingContribution,
) -> Option<ActionDefinition> {
    let mut other = KeyCombo::plain("");
    for part in entry.normalized_keybinding.split('+') {
        match part {
            "ctrl" => other.ctrl = true,
            "shift" => other.shift = true,
            "alt" => other.alt = true,
            key if other.key.is_empty() => other.key = key.to_string(),
            _ => return None,
        }
    }
    if other.key.is_empty() {
        return None;
    }
    let other = normalize_combo(other);
    let mut mac = other.clone();
    if mac.ctrl {
        mac.ctrl = false;
        mac.meta = true;
    }
    Some(ActionDefinition {
        // Runtime registration IDs may change on activation. The plugin and
        // declared chord identify the binding independently of that lifecycle.
        id: format!(
            "plugin.keybinding:{}",
            serde_json::to_string(&[&entry.plugin_id, &entry.normalized_keybinding]).ok()?
        )
        .into(),
        label: Some(format!("{}: {}", entry.plugin_name, entry.label)),
        scope: ActionScope::Plugin,
        terminal_behavior: TerminalBehavior::Never,
        mac,
        other,
    })
}

pub(crate) fn plugin_binding_matches(
    entry: &oxideterm_plugin_registry::NativePluginRuntimeKeybindingContribution,
    keystroke: &Keystroke,
    overrides: &Map<String, Value>,
) -> bool {
    let Some(definition) = plugin_action_definition(entry) else {
        return false;
    };
    if override_binding(&definition.id, overrides, KeybindingSide::current()).is_none() {
        return normalize_plugin_keystroke(keystroke).as_ref()
            == Some(&entry.normalized_keybinding);
    }
    combo_from_keystroke(keystroke).is_some_and(|combo| {
        effective_combo(&definition, overrides, KeybindingSide::current()).as_ref() == Some(&combo)
    })
}

pub(crate) fn matched_scoped_action(
    keystroke: &Keystroke,
    scope: ActionScope,
    overrides: &Map<String, Value>,
) -> Option<&'static str> {
    shortcuts::matched_scoped_action(
        &ACTION_DEFINITIONS,
        KeybindingSide::current(),
        keystroke,
        scope,
        overrides,
    )
}

fn terminal_leaf_action(id: &str) -> bool {
    id.starts_with("terminal.scroll")
        || matches!(
            id,
            "terminal.copyAlternate"
                | "terminal.pasteAlternate"
                | "terminal.wordBackward"
                | "terminal.wordForward"
                | "terminal.suggestPrevious"
                | "terminal.suggestNext"
                | "terminal.terminateTask"
                | "terminal.killTask"
        )
}
