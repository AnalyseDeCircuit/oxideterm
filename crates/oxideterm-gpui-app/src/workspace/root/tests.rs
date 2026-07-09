mod tests {
    use std::collections::BTreeSet;

    use oxideterm_gpui_settings_view::background_tab_options;
    use oxideterm_workspace::TabKind;

    use super::*;

    #[test]
    fn background_tab_options_cover_native_tab_background_keys() {
        let native_keys = [
            TabKind::LocalTerminal,
            TabKind::SshTerminal,
            TabKind::FileManager,
            TabKind::Launcher,
            TabKind::Graphics,
            TabKind::Runtime,
            TabKind::ConnectionPool,
            TabKind::ConnectionMonitor,
            TabKind::Topology,
            TabKind::NotificationCenter,
            TabKind::Sftp,
            TabKind::Ide,
            TabKind::Forwards,
            TabKind::SessionManager,
            TabKind::PluginManager,
            TabKind::Plugin {
                plugin_id: "plugin".to_string(),
                tab_id: "tab".to_string(),
            },
            TabKind::CloudSync,
            TabKind::RemoteDesktop,
            TabKind::Settings,
        ]
        .iter()
        .map(tab_background_key)
        .collect::<BTreeSet<_>>();

        let settings_keys = background_tab_options()
            .iter()
            .map(|(key, _, _)| *key)
            .collect::<BTreeSet<_>>();

        assert_eq!(settings_keys, native_keys);
    }

    #[test]
    fn ui_font_uses_first_configured_family() {
        assert_eq!(
            settings_ui_font_family("\"DengXian\", \"Microsoft YaHei\"").as_ref(),
            "DengXian"
        );
    }

    #[test]
    fn localized_dengxian_name_uses_gpui_family_name() {
        assert_eq!(settings_ui_font_family("\"等线\", sans-serif").as_ref(), "DengXian");
    }

    #[test]
    fn empty_ui_font_uses_tauri_platform_fallback() {
        #[cfg(target_os = "macos")]
        let expected = "SF Pro Text";
        #[cfg(target_os = "windows")]
        let expected = "Segoe UI";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let expected = "Roboto";

        assert_eq!(settings_ui_font_family("").as_ref(), expected);
    }

    #[test]
    fn failed_session_tree_replace_preserves_previous_snapshot() {
        let tempdir = std::env::temp_dir().join(format!(
            "oxideterm-session-tree-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&tempdir).unwrap();
        let path = tempdir.join("session_tree.json");
        let previous = PersistedNodeTreeSnapshot {
            version: 1,
            exported_at_ms: 1,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        write_session_tree_snapshot(&path, &previous).unwrap();
        let previous_bytes = fs::read(&path).unwrap();
        let replacement = PersistedNodeTreeSnapshot {
            version: 1,
            exported_at_ms: 2,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        inject_session_tree_replace_failure();

        assert!(write_session_tree_snapshot(&path, &replacement).is_err());
        assert_eq!(fs::read(path).unwrap(), previous_bytes);
        let _ = fs::remove_dir_all(tempdir);
    }

}
