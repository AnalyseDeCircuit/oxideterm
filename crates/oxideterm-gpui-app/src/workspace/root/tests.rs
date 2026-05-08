mod tests {
    use super::*;

    #[test]
    fn ui_font_uses_first_configured_family() {
        assert_eq!(
            settings_ui_font_family("\"DengXian\", \"Microsoft YaHei\"").as_ref(),
            "DengXian"
        );
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
}
