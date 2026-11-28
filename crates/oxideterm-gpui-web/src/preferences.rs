use oxideterm_gpui_shell::shortcuts::bindings::{
    ActionDefinition, KeybindingSide, override_binding,
};
use oxideterm_settings::{Language, PersistedSettings, sanitize_settings_value};
use oxideterm_settings_model::browser_preferences::BrowserPreferences;

const KEY: &str = "oxideterm.display";

impl crate::host::Workspace {
    pub(super) fn download_browser_report(
        &mut self,
        w: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let size = w.viewport_size();
        // Build from counts and flags, never from connection records, settings, or terminal text.
        let report = serde_json::json!({
            "application": "OxideTerm Web",
            "version": env!("CARGO_PKG_VERSION"),
            "mode": if self.demo {"demo"} else {"gateway"},
            "gateway_connected": (!self.demo).then_some(self.ready),
            "terminal_sessions": self.sessions.len(),
            "connected_terminals": self.sessions.values().filter(|s| s.connected(self.ready)).count(),
            "bluetooth_terminals": self.sessions.values().filter(|s| s.is_ble()).count(),
            "viewport_css_pixels": [f32::from(size.width), f32::from(size.height)],
            "device_pixel_ratio": w.scale_factor(),
            "software_renderer": w.gpu_specs().map(|s| s.is_software_emulated),
        });
        if crate::browser_file::download(
            "oxideterm-web-diagnostics.json",
            report.to_string().as_bytes(),
        )
        .is_err()
        {
            self.notice("sftp.toast.download_failed", cx);
        }
    }
}

pub(super) fn context_keys(
    actions: &[ActionDefinition],
    settings: &PersistedSettings,
    side: KeybindingSide,
) -> (
    oxideterm_gpui_terminal_view::input::TerminalKeybindings,
    oxideterm_gpui_editor::EditorKeybindings,
) {
    use oxideterm_gpui_editor::{EditorKeybindings, EditorShortcut};
    use oxideterm_gpui_shell::shortcuts::{context_bindings, normalized_keystroke};
    use oxideterm_gpui_terminal_view::input::{TerminalKeybindings, TerminalShortcut};
    let overrides = &settings.keybindings.overrides;
    (
        TerminalKeybindings {
            bindings: context_bindings(actions, TerminalShortcut::BINDINGS, overrides, side),
            normalize: normalized_keystroke,
        },
        EditorKeybindings {
            bindings: context_bindings(actions, EditorShortcut::BINDINGS, overrides, side),
            normalize: normalized_keystroke,
        },
    )
}

fn capture(settings: &PersistedSettings, actions: &[ActionDefinition]) -> BrowserPreferences {
    // Persist registered actions and their chords, not opaque plugin metadata.
    let overrides = &settings.keybindings.overrides;
    let keys = actions
        .iter()
        .filter_map(|action| {
            overrides.get(action.id.as_ref())?;
            let mut entry = serde_json::Map::new();
            for side in [KeybindingSide::Mac, KeybindingSide::Other] {
                if let Some(binding) = override_binding(&action.id, overrides, side) {
                    entry.insert(side.json_key().into(), serde_json::json!(binding));
                }
            }
            (!entry.is_empty()).then(|| (action.id.to_string(), serde_json::Value::Object(entry)))
        })
        .collect();
    BrowserPreferences::capture(settings, keys)
}

pub struct DisplayStorage {
    // Demo and gateway settings share browser storage, even when different actions are visible.
    registered: Vec<ActionDefinition>,
    last: BrowserPreferences,
    failed: bool,
    pub restore_failed: bool,
}

impl DisplayStorage {
    pub fn capture(&self, settings: &PersistedSettings) -> BrowserPreferences {
        capture(settings, &self.registered)
    }

    pub fn load(actions: Vec<ActionDefinition>) -> (Self, PersistedSettings) {
        let mut defaults = PersistedSettings::default();
        if let Some(language) = web_sys::window()
            .and_then(|w| w.navigator().language())
            .and_then(|tag| Language::from_locale_tag(&tag))
        {
            defaults.general.language = language;
        }
        let stored = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(KEY).ok().flatten());
        let restored = stored.as_ref().and_then(|json| {
            let prefs: BrowserPreferences = serde_json::from_str(json).ok()?;
            let mut restored = defaults.clone();
            prefs.apply(&mut restored);
            sanitize_settings_value(restored.to_value())
                .ok()
                .map(|clean| {
                    // Desktop migrations can synthesize providers. Restore only the fields this site saved.
                    capture(&clean.settings, &actions).apply(&mut restored);
                    restored
                })
        });
        let restore_failed = stored.is_some() && restored.is_none();
        let settings = restored.unwrap_or(defaults);
        (
            Self {
                last: capture(&settings, &actions),
                registered: actions,
                failed: false,
                restore_failed,
            },
            settings,
        )
    }

    // Report an unavailable store once per failure, without breaking in-memory editing.
    pub fn save(&mut self, next: BrowserPreferences) -> bool {
        let report = !self.failed;
        self.persist(next).is_err() && report
    }

    pub fn persist(&mut self, next: BrowserPreferences) -> Result<(), ()> {
        if next == self.last && !self.failed {
            return Ok(());
        }
        let saved = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|store| {
                store
                    .set_item(KEY, &serde_json::to_string(&next).ok()?)
                    .ok()
            })
            .is_some();
        self.failed = !saved;
        if saved {
            self.last = next;
            Ok(())
        } else {
            Err(())
        }
    }
}
