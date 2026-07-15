use super::*;

// Keep empty-workspace hints aligned with the same effective bindings used by dispatch.
pub(super) fn effective_shortcut_label(
    action_id: &str,
    overrides: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let definition = crate::keybindings::action_definition(action_id)?;
    let combo = crate::keybindings::effective_combo(
        definition,
        overrides,
        crate::keybindings::KeybindingSide::current(),
    );
    Some(crate::keybindings::format_combo(&combo))
}

pub(super) fn readiness_for_connection_state(state: &ConnectionState) -> NodeReadiness {
    match state {
        ConnectionState::Active | ConnectionState::Idle => NodeReadiness::Ready,
        ConnectionState::Connecting | ConnectionState::Reconnecting => NodeReadiness::Connecting,
        ConnectionState::Error(_) | ConnectionState::LinkDown => NodeReadiness::Error,
        ConnectionState::Disconnecting | ConnectionState::Disconnected => {
            NodeReadiness::Disconnected
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_workspace_shortcut_uses_effective_override() {
        let side = crate::keybindings::KeybindingSide::current();
        let combo = crate::keybindings::KeyCombo {
            key: "p".to_string(),
            ctrl: !cfg!(target_os = "macos"),
            shift: true,
            alt: false,
            meta: cfg!(target_os = "macos"),
        };
        let expected = crate::keybindings::format_combo(&combo);
        let mut overrides = serde_json::Map::new();

        // Use the public override path so the test covers persisted settings semantics too.
        crate::keybindings::set_override(&mut overrides, "app.commandPalette", side, combo);

        assert_eq!(
            effective_shortcut_label("app.commandPalette", &overrides),
            Some(expected)
        );
    }
}
