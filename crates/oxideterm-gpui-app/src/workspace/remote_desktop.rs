use super::*;
use oxideterm_gpui_remote_desktop::{
    DesktopWindowAction, PendingRemoteDesktopSshTunnel, RemoteDesktopCertificateAcceptance,
    RemoteDesktopCertificateChallengeState, RemoteDesktopPanelView, RemoteDesktopSessionEvent,
    desktop_connection_audit, preview_remote_desktop_profile, remote_desktop_audit_context,
    remote_desktop_tab_visible, resize::remote_desktop_scale_factor_percent,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopFileTransferFailureKind,
    RemoteDesktopFrameDeliverySlot, RemoteDesktopProtocol, RemoteDesktopProviderManifest,
    RemoteDesktopSecret, builtin_preview_provider_registry,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use zeroize::Zeroizing;

pub(in crate::workspace) use oxideterm_gpui_remote_desktop::{
    RemoteDesktopPublicClipboardSnapshot, RemoteDesktopSessionEntity, RemoteDesktopSshTunnelLease,
    RemoteDesktopWorkspaceEntity,
};

mod certificate;
mod interaction;
mod session;
mod view;

pub(in crate::workspace) fn remote_desktop_keyboard_capture(
    root: gpui::Stateful<gpui::Div>,
    session: Entity<RemoteDesktopSessionEntity>,
    overrides: serde_json::Map<String, serde_json::Value>,
) -> gpui::Stateful<gpui::Div> {
    oxideterm_gpui_remote_desktop::remote_desktop_keyboard_capture(
        root,
        session,
        overrides,
        crate::keybindings::keystroke_matches_action,
    )
}

#[cfg(test)]
mod tests {
    use oxideterm_gpui_remote_desktop::desktop_shortcut_matches;

    fn remote_desktop_paste_shortcut(
        keystroke: &gpui::Keystroke,
        overrides: &serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        remote_desktop_clipboard_shortcut(keystroke, "remoteDesktop.paste", overrides)
    }

    fn remote_desktop_copy_shortcut(
        keystroke: &gpui::Keystroke,
        overrides: &serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        remote_desktop_clipboard_shortcut(keystroke, "remoteDesktop.copy", overrides)
    }

    fn remote_desktop_clipboard_shortcut(
        keystroke: &gpui::Keystroke,
        action: &str,
        overrides: &serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        desktop_shortcut_matches(
            keystroke,
            action,
            overrides,
            crate::keybindings::keystroke_matches_action,
        )
    }
    #[test]
    fn remote_desktop_clipboard_shortcuts_accept_physical_key_codes() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.control = true;

        assert!(remote_desktop_paste_shortcut(
            &gpui::Keystroke {
                modifiers,
                key: "KeyV".to_string(),
                key_char: Some("v".to_string()),
            },
            &serde_json::Map::new()
        ));
        assert!(remote_desktop_paste_shortcut(
            &gpui::Keystroke {
                modifiers,
                key: "keyv".to_string(),
                key_char: Some("v".to_string()),
            },
            &serde_json::Map::new()
        ));
        assert!(remote_desktop_copy_shortcut(
            &gpui::Keystroke {
                modifiers,
                key: "KeyC".to_string(),
                key_char: Some("c".to_string()),
            },
            &serde_json::Map::new()
        ));
    }

    #[test]
    fn remote_desktop_clipboard_keybindings_rebind_and_unbind() {
        use crate::keybindings::{
            KeyCombo, KeybindingSide, action_definition, set_definition_override,
            set_unbound_override,
        };
        let mut overrides = serde_json::Map::new();
        let custom = gpui::Keystroke::parse("ctrl-alt-k").unwrap();
        let combo = KeyCombo {
            key: "k".into(),
            ctrl: true,
            alt: true,
            shift: false,
            meta: false,
        };
        set_definition_override(
            &mut overrides,
            action_definition("remoteDesktop.paste").unwrap(),
            KeybindingSide::current(),
            combo,
        );
        assert!(remote_desktop_paste_shortcut(&custom, &overrides));
        assert!(!remote_desktop_paste_shortcut(
            &gpui::Keystroke::parse("ctrl-v").unwrap(),
            &overrides
        ));
        set_unbound_override(
            &mut overrides,
            "remoteDesktop.paste",
            KeybindingSide::current(),
        );
        assert!(!remote_desktop_paste_shortcut(&custom, &overrides));
    }
}
