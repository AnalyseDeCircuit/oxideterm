use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde_json::Value;

const EN_PARTS: &[&str] = &[
    include_str!("../locales/en/common.json"),
    include_str!("../locales/en/menu.json"),
    include_str!("../locales/en/sidebar.json"),
    include_str!("../locales/en/settings.json"),
    include_str!("../locales/en/settings_view.json"),
    include_str!("../locales/en/sessionManager.json"),
    include_str!("../locales/en/modals.json"),
    include_str!("../locales/en/connections.json"),
    include_str!("../locales/en/eventLog.json"),
    include_str!("../locales/en/profiler.json"),
    include_str!("../locales/en/forwards.json"),
    include_str!("../locales/en/sftp.json"),
    include_str!("../locales/en/ssh.json"),
    include_str!("../locales/en/terminal.json"),
    include_str!("../locales/en/ide.json"),
    include_str!("../locales/en/fileManager.json"),
    include_str!("../locales/en/launcher.json"),
    include_str!("../locales/en/graphics.json"),
    include_str!("../locales/en/ai.json"),
];
const DE_PARTS: &[&str] = &[
    include_str!("../locales/de/common.json"),
    include_str!("../locales/de/menu.json"),
    include_str!("../locales/de/sidebar.json"),
    include_str!("../locales/de/settings.json"),
    include_str!("../locales/de/settings_view.json"),
    include_str!("../locales/de/sessionManager.json"),
    include_str!("../locales/de/modals.json"),
    include_str!("../locales/de/connections.json"),
    include_str!("../locales/de/eventLog.json"),
    include_str!("../locales/de/profiler.json"),
    include_str!("../locales/de/forwards.json"),
    include_str!("../locales/de/sftp.json"),
    include_str!("../locales/de/ssh.json"),
    include_str!("../locales/de/terminal.json"),
    include_str!("../locales/de/ide.json"),
    include_str!("../locales/de/fileManager.json"),
    include_str!("../locales/de/launcher.json"),
    include_str!("../locales/de/graphics.json"),
    include_str!("../locales/de/ai.json"),
];
const ES_ES_PARTS: &[&str] = &[
    include_str!("../locales/es-ES/common.json"),
    include_str!("../locales/es-ES/menu.json"),
    include_str!("../locales/es-ES/sidebar.json"),
    include_str!("../locales/es-ES/settings.json"),
    include_str!("../locales/es-ES/settings_view.json"),
    include_str!("../locales/es-ES/sessionManager.json"),
    include_str!("../locales/es-ES/modals.json"),
    include_str!("../locales/es-ES/connections.json"),
    include_str!("../locales/es-ES/eventLog.json"),
    include_str!("../locales/es-ES/profiler.json"),
    include_str!("../locales/es-ES/forwards.json"),
    include_str!("../locales/es-ES/sftp.json"),
    include_str!("../locales/es-ES/ssh.json"),
    include_str!("../locales/es-ES/terminal.json"),
    include_str!("../locales/es-ES/ide.json"),
    include_str!("../locales/es-ES/fileManager.json"),
    include_str!("../locales/es-ES/launcher.json"),
    include_str!("../locales/es-ES/graphics.json"),
    include_str!("../locales/es-ES/ai.json"),
];
const FR_FR_PARTS: &[&str] = &[
    include_str!("../locales/fr-FR/common.json"),
    include_str!("../locales/fr-FR/menu.json"),
    include_str!("../locales/fr-FR/sidebar.json"),
    include_str!("../locales/fr-FR/settings.json"),
    include_str!("../locales/fr-FR/settings_view.json"),
    include_str!("../locales/fr-FR/sessionManager.json"),
    include_str!("../locales/fr-FR/modals.json"),
    include_str!("../locales/fr-FR/connections.json"),
    include_str!("../locales/fr-FR/eventLog.json"),
    include_str!("../locales/fr-FR/profiler.json"),
    include_str!("../locales/fr-FR/forwards.json"),
    include_str!("../locales/fr-FR/sftp.json"),
    include_str!("../locales/fr-FR/ssh.json"),
    include_str!("../locales/fr-FR/terminal.json"),
    include_str!("../locales/fr-FR/ide.json"),
    include_str!("../locales/fr-FR/fileManager.json"),
    include_str!("../locales/fr-FR/launcher.json"),
    include_str!("../locales/fr-FR/graphics.json"),
    include_str!("../locales/fr-FR/ai.json"),
];
const IT_PARTS: &[&str] = &[
    include_str!("../locales/it/common.json"),
    include_str!("../locales/it/menu.json"),
    include_str!("../locales/it/sidebar.json"),
    include_str!("../locales/it/settings.json"),
    include_str!("../locales/it/settings_view.json"),
    include_str!("../locales/it/sessionManager.json"),
    include_str!("../locales/it/modals.json"),
    include_str!("../locales/it/connections.json"),
    include_str!("../locales/it/eventLog.json"),
    include_str!("../locales/it/profiler.json"),
    include_str!("../locales/it/forwards.json"),
    include_str!("../locales/it/sftp.json"),
    include_str!("../locales/it/ssh.json"),
    include_str!("../locales/it/terminal.json"),
    include_str!("../locales/it/ide.json"),
    include_str!("../locales/it/fileManager.json"),
    include_str!("../locales/it/launcher.json"),
    include_str!("../locales/it/graphics.json"),
    include_str!("../locales/it/ai.json"),
];
const JA_PARTS: &[&str] = &[
    include_str!("../locales/ja/common.json"),
    include_str!("../locales/ja/menu.json"),
    include_str!("../locales/ja/sidebar.json"),
    include_str!("../locales/ja/settings.json"),
    include_str!("../locales/ja/settings_view.json"),
    include_str!("../locales/ja/sessionManager.json"),
    include_str!("../locales/ja/modals.json"),
    include_str!("../locales/ja/connections.json"),
    include_str!("../locales/ja/eventLog.json"),
    include_str!("../locales/ja/profiler.json"),
    include_str!("../locales/ja/forwards.json"),
    include_str!("../locales/ja/sftp.json"),
    include_str!("../locales/ja/ssh.json"),
    include_str!("../locales/ja/terminal.json"),
    include_str!("../locales/ja/ide.json"),
    include_str!("../locales/ja/fileManager.json"),
    include_str!("../locales/ja/launcher.json"),
    include_str!("../locales/ja/graphics.json"),
    include_str!("../locales/ja/ai.json"),
];
const KO_PARTS: &[&str] = &[
    include_str!("../locales/ko/common.json"),
    include_str!("../locales/ko/menu.json"),
    include_str!("../locales/ko/sidebar.json"),
    include_str!("../locales/ko/settings.json"),
    include_str!("../locales/ko/settings_view.json"),
    include_str!("../locales/ko/sessionManager.json"),
    include_str!("../locales/ko/modals.json"),
    include_str!("../locales/ko/connections.json"),
    include_str!("../locales/ko/eventLog.json"),
    include_str!("../locales/ko/profiler.json"),
    include_str!("../locales/ko/forwards.json"),
    include_str!("../locales/ko/sftp.json"),
    include_str!("../locales/ko/ssh.json"),
    include_str!("../locales/ko/terminal.json"),
    include_str!("../locales/ko/ide.json"),
    include_str!("../locales/ko/fileManager.json"),
    include_str!("../locales/ko/launcher.json"),
    include_str!("../locales/ko/graphics.json"),
    include_str!("../locales/ko/ai.json"),
];
const PT_BR_PARTS: &[&str] = &[
    include_str!("../locales/pt-BR/common.json"),
    include_str!("../locales/pt-BR/menu.json"),
    include_str!("../locales/pt-BR/sidebar.json"),
    include_str!("../locales/pt-BR/settings.json"),
    include_str!("../locales/pt-BR/settings_view.json"),
    include_str!("../locales/pt-BR/sessionManager.json"),
    include_str!("../locales/pt-BR/modals.json"),
    include_str!("../locales/pt-BR/connections.json"),
    include_str!("../locales/pt-BR/eventLog.json"),
    include_str!("../locales/pt-BR/profiler.json"),
    include_str!("../locales/pt-BR/forwards.json"),
    include_str!("../locales/pt-BR/sftp.json"),
    include_str!("../locales/pt-BR/ssh.json"),
    include_str!("../locales/pt-BR/terminal.json"),
    include_str!("../locales/pt-BR/ide.json"),
    include_str!("../locales/pt-BR/fileManager.json"),
    include_str!("../locales/pt-BR/launcher.json"),
    include_str!("../locales/pt-BR/graphics.json"),
    include_str!("../locales/pt-BR/ai.json"),
];
const VI_PARTS: &[&str] = &[
    include_str!("../locales/vi/common.json"),
    include_str!("../locales/vi/menu.json"),
    include_str!("../locales/vi/sidebar.json"),
    include_str!("../locales/vi/settings.json"),
    include_str!("../locales/vi/settings_view.json"),
    include_str!("../locales/vi/sessionManager.json"),
    include_str!("../locales/vi/modals.json"),
    include_str!("../locales/vi/connections.json"),
    include_str!("../locales/vi/eventLog.json"),
    include_str!("../locales/vi/profiler.json"),
    include_str!("../locales/vi/forwards.json"),
    include_str!("../locales/vi/sftp.json"),
    include_str!("../locales/vi/ssh.json"),
    include_str!("../locales/vi/terminal.json"),
    include_str!("../locales/vi/ide.json"),
    include_str!("../locales/vi/fileManager.json"),
    include_str!("../locales/vi/launcher.json"),
    include_str!("../locales/vi/graphics.json"),
    include_str!("../locales/vi/ai.json"),
];
const ZH_CN_PARTS: &[&str] = &[
    include_str!("../locales/zh-CN/common.json"),
    include_str!("../locales/zh-CN/menu.json"),
    include_str!("../locales/zh-CN/sidebar.json"),
    include_str!("../locales/zh-CN/settings.json"),
    include_str!("../locales/zh-CN/settings_view.json"),
    include_str!("../locales/zh-CN/sessionManager.json"),
    include_str!("../locales/zh-CN/modals.json"),
    include_str!("../locales/zh-CN/connections.json"),
    include_str!("../locales/zh-CN/eventLog.json"),
    include_str!("../locales/zh-CN/profiler.json"),
    include_str!("../locales/zh-CN/forwards.json"),
    include_str!("../locales/zh-CN/sftp.json"),
    include_str!("../locales/zh-CN/ssh.json"),
    include_str!("../locales/zh-CN/terminal.json"),
    include_str!("../locales/zh-CN/ide.json"),
    include_str!("../locales/zh-CN/fileManager.json"),
    include_str!("../locales/zh-CN/launcher.json"),
    include_str!("../locales/zh-CN/graphics.json"),
    include_str!("../locales/zh-CN/ai.json"),
];
const ZH_TW_PARTS: &[&str] = &[
    include_str!("../locales/zh-TW/common.json"),
    include_str!("../locales/zh-TW/menu.json"),
    include_str!("../locales/zh-TW/sidebar.json"),
    include_str!("../locales/zh-TW/settings.json"),
    include_str!("../locales/zh-TW/settings_view.json"),
    include_str!("../locales/zh-TW/sessionManager.json"),
    include_str!("../locales/zh-TW/modals.json"),
    include_str!("../locales/zh-TW/connections.json"),
    include_str!("../locales/zh-TW/eventLog.json"),
    include_str!("../locales/zh-TW/profiler.json"),
    include_str!("../locales/zh-TW/forwards.json"),
    include_str!("../locales/zh-TW/sftp.json"),
    include_str!("../locales/zh-TW/ssh.json"),
    include_str!("../locales/zh-TW/terminal.json"),
    include_str!("../locales/zh-TW/ide.json"),
    include_str!("../locales/zh-TW/fileManager.json"),
    include_str!("../locales/zh-TW/launcher.json"),
    include_str!("../locales/zh-TW/graphics.json"),
    include_str!("../locales/zh-TW/ai.json"),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Locale {
    De,
    En,
    EsEs,
    FrFr,
    It,
    Ja,
    Ko,
    PtBr,
    Vi,
    ZhCn,
    ZhTw,
}

#[derive(Clone, Debug)]
pub struct I18n {
    locale: Locale,
    fallback_locale: Locale,
    catalogs: Arc<RwLock<HashMap<Locale, LocaleCatalog>>>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let i18n = Self {
            locale,
            fallback_locale: Locale::En,
            catalogs: Arc::new(RwLock::new(HashMap::new())),
        };
        // Preload the visible locale and English fallback so the startup UI does
        // not parse every catalog, while the first render has its two hot paths.
        i18n.ensure_catalog(Locale::En);
        i18n.ensure_catalog(locale);
        i18n
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        // Locale changes are synchronous in the settings flow; preloading here
        // avoids moving JSON parsing into the next render pass.
        self.ensure_catalog(locale);
        self.locale = locale;
    }

    pub fn t(&self, key: &str) -> String {
        self.catalog_message(self.locale, key)
            .or_else(|| self.catalog_message(self.fallback_locale, key))
            .unwrap_or_else(|| key.to_string())
    }

    fn ensure_catalog(&self, locale: Locale) {
        if self
            .catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .contains_key(&locale)
        {
            return;
        }
        let mut catalogs = self
            .catalogs
            .write()
            .expect("native locale catalog lock poisoned");
        catalogs
            .entry(locale)
            .or_insert_with(|| LocaleCatalog::from_json_parts(locale_parts(locale)));
    }

    fn catalog_message(&self, locale: Locale, key: &str) -> Option<String> {
        self.ensure_catalog(locale);
        self.catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .get(&locale)
            .and_then(|catalog| catalog.get(key).map(str::to_string))
    }

    #[cfg(test)]
    fn loaded_catalog_count(&self) -> usize {
        self.catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .len()
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::ZhCn)
    }
}

#[derive(Clone, Debug)]
struct LocaleCatalog {
    messages: HashMap<String, String>,
}

impl LocaleCatalog {
    fn from_json_parts(parts: &[&str]) -> Self {
        let mut messages = HashMap::new();
        for source in parts {
            let value: Value =
                serde_json::from_str(source).expect("invalid native locale catalog part");
            flatten_json("", &value, &mut messages);
        }
        Self { messages }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }
}

fn locale_parts(locale: Locale) -> &'static [&'static str] {
    match locale {
        Locale::De => DE_PARTS,
        Locale::En => EN_PARTS,
        Locale::EsEs => ES_ES_PARTS,
        Locale::FrFr => FR_FR_PARTS,
        Locale::It => IT_PARTS,
        Locale::Ja => JA_PARTS,
        Locale::Ko => KO_PARTS,
        Locale::PtBr => PT_BR_PARTS,
        Locale::Vi => VI_PARTS,
        Locale::ZhCn => ZH_CN_PARTS,
        Locale::ZhTw => ZH_TW_PARTS,
    }
}

fn flatten_json(prefix: &str, value: &Value, messages: &mut HashMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let key = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json(&key, child, messages);
            }
        }
        Value::String(message) => {
            let previous = messages.insert(prefix.to_string(), message.clone());
            assert!(previous.is_none(), "duplicate native locale key: {prefix}");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_active_locale() {
        let mut i18n = I18n::default();
        assert_eq!(i18n.t("menu.new_terminal"), "新建终端");

        i18n.set_locale(Locale::En);
        assert_eq!(i18n.t("menu.new_terminal"), "New Terminal");
    }

    #[test]
    fn falls_back_to_english_then_key() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("missing.key"), "missing.key");
    }

    #[test]
    fn split_catalogs_keep_expected_domains() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("ssh.form.title"), "新建连接");
        assert_eq!(i18n.t("sidebar.panels.sessions"), "活动会话");
        assert_eq!(i18n.t("terminal.local_terminal"), "本地终端");
        assert_eq!(i18n.t("terminal.trzsz.completed_title"), "传输已完成");
    }

    #[test]
    fn loads_only_active_locale_and_fallback_until_switch() {
        let mut i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.loaded_catalog_count(), 2);

        i18n.set_locale(Locale::Ja);
        assert_eq!(i18n.loaded_catalog_count(), 3);
        assert_eq!(i18n.t("menu.new_terminal"), "新しいターミナル");
    }

    #[test]
    fn english_locale_loads_single_catalog() {
        let i18n = I18n::new(Locale::En);
        assert_eq!(i18n.loaded_catalog_count(), 1);
        assert_eq!(i18n.t("menu.new_terminal"), "New Terminal");
    }

    #[test]
    #[should_panic(expected = "duplicate native locale key")]
    fn duplicate_keys_are_rejected() {
        let _ = LocaleCatalog::from_json_parts(&[
            r#"{"menu":{"copy":"Copy"}}"#,
            r#"{"menu":{"copy":"Duplicate"}}"#,
        ]);
    }

    #[test]
    fn all_eleven_locales_load_without_panic() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        for locale in locales {
            let i18n = I18n::new(locale);
            let name = i18n.t("app.name");
            assert_eq!(name, "OxideTerm");
        }
    }

    #[test]
    fn preview_stable_upgrade_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "settings_view.help.preview_stable_upgrade_title",
            "settings_view.help.preview_stable_upgrade_hint",
            "settings_view.help.download_stable",
        ];
        let english = I18n::new(Locale::En);

        // Cross-channel protection must remain understandable without falling
        // back to raw keys or English in any shipped locale.
        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                let translated = i18n.t(key);
                assert_ne!(translated, key, "{locale:?} missing {key}");
                if locale != Locale::En {
                    assert_ne!(
                        translated,
                        english.t(key),
                        "{locale:?} falls back for {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn major_version_migration_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "migration.overview_title",
            "migration.cli_page_title",
            "migration.cli_migrate",
            "migration.gpui_title",
            "migration.gpui_memory_description",
            "migration.visual_title",
            "migration.visual_disable_hint",
            "migration.features_title",
            "migration.features_sftp_description",
            "migration.internal_title",
            "migration.continue_button",
            "command_palette.cmd_show_version_migration",
        ];
        let english = I18n::new(Locale::En);

        // The migration tour is shown before ordinary settings navigation, so
        // every shipped locale must own its critical release and repair copy.
        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                let translated = i18n.t(key);
                assert_ne!(translated, key, "{locale:?} missing {key}");
                if locale != Locale::En {
                    assert_ne!(
                        translated,
                        english.t(key),
                        "{locale:?} falls back for {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn plugin_manager_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = ["plugin.manager_title", "plugin.manager_description"];

        // The plugin page is easy to visually miss in locale smoke tests because
        // the title key existed while the descriptive key fell through to raw UI.
        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn ssh_connection_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "ssh.form.checking_host_key",
            "ssh.form.test_running",
            "ssh.form.test_success",
            "ssh.form.default_key_desc",
            "ssh.form.passphrase",
            "ssh.form.passphrase_placeholder",
            "ssh.form.key_file",
            "ssh.form.certificate_note",
            "ssh.form.private_key",
            "ssh.form.certificate",
            "ssh.form.agent_desc",
            "ssh.form.two_factor_desc",
            "ssh.form.key_path_required",
            "ssh.form.certificate_paths_required",
            "ssh.form.key_path_not_ready",
            "ssh.form.certificate_not_ready",
            "ssh.form.keyboard_interactive_not_ready",
            "ssh.host_key.title_unknown",
            "ssh.host_key.title_changed",
            "ssh.host_key.title_error",
            "ssh.host_key.unknown_message",
            "ssh.host_key.changed_warning",
            "ssh.host_key.key_type_label",
            "ssh.host_key.fingerprint_label",
            "ssh.host_key.expected_fingerprint",
            "ssh.host_key.actual_fingerprint",
            "ssh.host_key.cancelled",
            "ssh.host_key.changed_requires_remove",
            "ssh.host_key.actions.cancel",
            "ssh.host_key.actions.trust_once",
            "ssh.host_key.actions.trust_save",
            "ssh.kbi.title",
            "ssh.kbi.continue",
            "ssh.kbi.cancelled",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn sidebar_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "sidebar.active_sessions",
            "sidebar.no_sessions",
            "sidebar.add_new_connection",
            "sidebar.actions.expand",
            "sidebar.actions.collapse",
            "sidebar.actions.new_local_terminal",
            "sidebar.actions.open_pylance_guide",
            "sidebar.panels.sessions",
            "sidebar.panels.saved",
            "sidebar.panels.saved_connections",
            "sidebar.panels.saved_title",
            "sidebar.panels.sftp",
            "sidebar.panels.sftp_sessions",
            "sidebar.panels.files",
            "sidebar.panels.forwards",
            "sidebar.panels.forwards_title",
            "sidebar.panels.connection_pool",
            "sidebar.panels.connection_monitor",
            "sidebar.panels.connection_matrix",
            "sidebar.panels.ai",
            "sidebar.panels.ai_assistant",
            "sidebar.panels.settings",
            "sidebar.panels.no_saved_connections",
            "sidebar.panels.no_active_sessions",
            "sidebar.panels.no_connected_sessions",
            "sidebar.panels.import_tooltip",
            "sidebar.panels.export_tooltip",
            "sidebar.panels.system_health",
            "sidebar.panels.session_manager",
            "sidebar.panels.open_session_manager",
            "sidebar.panels.plugins",
            "sidebar.panels.activity",
            "sidebar.panels.event_log",
            "sidebar.panels.notifications",
            "sidebar.tooltips.settings",
            "sidebar.tooltips.ai_hint",
            "sidebar.tooltips.switch_tree",
            "sidebar.tooltips.switch_focus",
            "sidebar.tooltips.new_connection",
            "sidebar.tooltips.collapse",
            "sessions.tree.no_sessions",
            "sessions.tree.click_to_add",
            "sessions.tree.actions.connect",
            "sessions.tree.actions.new_terminal",
            "sessions.tree.actions.sftp",
            "sessions.tree.actions.port_forwarding",
            "sessions.tree.actions.disconnect",
            "sessions.tree.actions.reconnect",
            "sessions.tree.actions.drill_in",
            "sessions.actions.connect",
            "sessions.actions.new_terminal",
            "sessions.actions.sftp_explorer",
            "sessions.actions.port_forwarding",
            "sessions.actions.disconnect",
            "sessions.actions.reconnect",
            "sessions.actions.drill_in",
            "common.confirm.disconnect_node",
            "sessions.breadcrumb.back_to_root",
            "sessions.breadcrumb.all_servers",
            "sessions.focused_list.all_servers",
            "sessions.focused_list.child",
            "sessions.focused_list.children",
            "sessions.focused_list.no_child_nodes",
            "sessions.focused_list.no_servers",
            "sessions.focused_list.add_by_drilling",
            "sessions.focused_list.click_to_add",
            "sessions.focused_list.terminal",
            "sessions.focused_list.close_terminal",
            "ssh.drill_down.title",
            "ssh.drill_down.description",
            "ssh.drill_down.target_host",
            "ssh.drill_down.port",
            "ssh.drill_down.username",
            "ssh.drill_down.auth_method",
            "ssh.drill_down.connect",
            "ssh.drill_down.connecting",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn event_log_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "event_log.title",
            "event_log.dnd.on",
            "event_log.empty",
            "event_log.clear",
            "event_log.severity.info",
            "event_log.severity.warn",
            "event_log.severity.error",
            "event_log.category.connection",
            "event_log.category.reconnect",
            "event_log.category.node",
            "event_log.events.connected",
            "event_log.events.disconnected",
            "event_log.events.link_down",
            "event_log.events.reconnecting",
            "event_log.events.affected_children",
            "event_log.events.node_state_ready",
            "event_log.events.node_state_connecting",
            "event_log.events.node_state_error",
            "event_log.events.node_state_disconnected",
            "event_log.events.node_state_unknown",
            "event_log.events.already_connecting",
            "event_log.events.already_connected",
            "event_log.events.reconnect_phase",
            "event_log.phase.queued",
            "event_log.phase.snapshot",
            "event_log.phase.grace-period",
            "event_log.phase.ssh-connect",
            "event_log.phase.await-terminal",
            "event_log.phase.restore-forwards",
            "event_log.phase.resume-transfers",
            "event_log.phase.restore-ide",
            "event_log.phase.verify",
            "event_log.phase.done",
            "event_log.phase.failed",
            "event_log.phase.cancelled",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn connection_monitor_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "layout.connection_monitor.title",
            "connections.monitor.title",
            "connections.monitor.loading",
            "connections.monitor.active",
            "connections.monitor.idle",
            "connections.monitor.reconnecting",
            "connections.monitor.link_down",
            "connections.monitor.terminals",
            "connections.monitor.sftp",
            "connections.monitor.forwards",
            "connections.monitor.summary",
            "connections.monitor.live",
            "profiler.panel.no_connection",
            "profiler.panel.disabled",
            "profiler.panel.enable",
            "profiler.panel.sampling",
            "profiler.panel.no_data",
            "profiler.panel.cpu",
            "profiler.panel.memory",
            "profiler.panel.network",
            "profiler.panel.load_avg",
            "profiler.panel.rtt",
            "profiler.panel.source",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn forwarding_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "copy_address",
            "copied",
            "forwards.quick.title",
            "forwards.quick.jupyter",
            "forwards.quick.tensorboard",
            "forwards.quick.vscode",
            "forwards.table.title",
            "forwards.table.type",
            "forwards.table.local_address",
            "forwards.table.remote_address",
            "forwards.table.status",
            "forwards.table.actions",
            "forwards.table.no_forwards",
            "forwards.type.local",
            "forwards.type.remote",
            "forwards.type.dynamic",
            "forwards.status.active",
            "forwards.status.stopped",
            "forwards.status.failed",
            "forwards.status.starting",
            "forwards.status.error",
            "forwards.status.suspended",
            "forwards.status.suspended_hint",
            "forwards.actions.new_forward",
            "forwards.actions.stop",
            "forwards.actions.restart",
            "forwards.actions.edit",
            "forwards.actions.delete",
            "forwards.actions.will_recover",
            "forwards.actions.confirm_delete_title",
            "forwards.actions.confirm_delete_desc",
            "forwards.form.new_title",
            "forwards.form.edit_title",
            "forwards.form.cancel",
            "forwards.form.type",
            "forwards.form.create_forward",
            "forwards.form.creating",
            "forwards.form.save_changes",
            "forwards.form.saving",
            "forwards.form.type_local",
            "forwards.form.type_remote",
            "forwards.form.type_dynamic",
            "forwards.form.local_client",
            "forwards.form.remote_server",
            "forwards.form.bind_address",
            "forwards.form.target_address",
            "forwards.form.socks5_mode",
            "forwards.form.host_placeholder",
            "forwards.form.port_placeholder",
            "forwards.form.skip_check",
            "forwards.form.checking_port",
            "forwards.form.port_required",
            "forwards.form.port_invalid",
            "forwards.form.desc",
            "forwards.toast.jupyter_created",
            "forwards.toast.jupyter_desc",
            "forwards.toast.tensorboard_created",
            "forwards.toast.tensorboard_desc",
            "forwards.toast.vscode_created",
            "forwards.toast.vscode_desc",
            "forwards.toast.create_failed",
            "forwards.toast.forward_updated",
            "forwards.toast.update_failed",
            "forwards.toast.suspended_title",
            "forwards.toast.suspended_desc",
            "forwards.toast.error_title",
            "forwards.toast.session_suspended_title",
            "forwards.toast.session_suspended_desc",
            "forwards.messages.node_not_ready",
            "forwards.messages.connection_not_ready",
            "forwards.messages.created",
            "forwards.messages.updated",
            "forwards.messages.stopped",
            "forwards.messages.restarted",
            "forwards.messages.deleted",
            "forwards.detection.detected",
            "forwards.detection.port",
            "forwards.detection.forward",
            "forwards.detection.auto",
            "forwards.detection.forwarded",
            "forwards.detection.forwardError",
            "forwards.detection.remotePorts",
            "forwards.detection.bindAddr",
            "forwards.detection.process",
            "forwards.detection.action",
            "forwards.detection.scanning",
            "forwards.detection.noPorts",
            "forwards.detection.alreadyForwarded",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn ide_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "ide.loading_project",
            "ide.open_failed",
            "ide.retry",
            "ide.disconnected_overlay",
            "ide.no_open_files",
            "ide.click_to_open",
            "ide.loading_file",
            "ide.save_failed",
            "ide.unsaved_changes",
            "ide.unsaved_changes_desc",
            "ide.save",
            "ide.discard",
            "ide.cancel",
            "ide.agent_status_sftp",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn sftp_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "sftp.file_list.local",
            "sftp.file_list.remote",
            "sftp.file_list.filter_placeholder",
            "sftp.file_list.path_placeholder",
            "sftp.file_list.col_name",
            "sftp.file_list.col_size",
            "sftp.file_list.col_modified",
            "sftp.toolbar.show_drives",
            "sftp.toolbar.go_up",
            "sftp.toolbar.home",
            "sftp.toolbar.refresh",
            "sftp.toolbar.upload_count",
            "sftp.toolbar.download_count",
            "sftp.context.preview",
            "sftp.context.rename",
            "sftp.context.delete",
            "sftp.context.new_folder",
            "sftp.dialogs.select_drive",
            "sftp.dialogs.rename",
            "sftp.dialogs.new_folder",
            "sftp.dialogs.delete",
            "sftp.conflict.title",
            "sftp.conflict.overwrite",
            "sftp.diff.title",
            "sftp.preview.description",
            "sftp.queue.title",
            "sftp.queue.clear_done",
            "sftp.queue.incomplete_title",
            "sftp.queue.status_waiting",
            "sftp.queue.status_paused",
            "sftp.queue.status_completed",
            "sftp.queue.status_cancelled",
            "sftp.queue.status_error",
            "sftp.queue.empty",
            "sftp.errors.connection_lost",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn file_manager_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "fileManager.title",
            "fileManager.pathPlaceholder",
            "fileManager.showDrives",
            "fileManager.newFolder",
            "fileManager.newFile",
            "fileManager.openTerminalHere",
            "fileManager.compress",
            "fileManager.extract",
            "fileManager.favorites",
            "fileManager.addBookmark",
            "fileManager.removeBookmark",
            "fileManager.properties",
            "fileManager.confirmDelete",
            "fileManager.preview",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn terminal_trzsz_strings_exist_in_every_locale() {
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];
        let keys = [
            "terminal.trzsz.select_upload_directory_title",
            "terminal.trzsz.select_upload_directory_description",
            "terminal.trzsz.select_upload_files_title",
            "terminal.trzsz.select_upload_files_description",
            "terminal.trzsz.select_download_directory_title",
            "terminal.trzsz.select_download_directory_description",
            "terminal.trzsz.cancelled_title",
            "terminal.trzsz.cancelled_description",
            "terminal.trzsz.completed_title",
            "terminal.trzsz.completed_description",
            "terminal.trzsz.failed_title",
            "terminal.trzsz.failed_description",
            "terminal.trzsz.version_mismatch_title",
            "terminal.trzsz.path_invalid_title",
            "terminal.trzsz.symlink_not_supported_title",
            "terminal.trzsz.conflict_detected_title",
            "terminal.trzsz.directory_not_allowed_title",
            "terminal.trzsz.max_file_count_title",
            "terminal.trzsz.max_total_bytes_title",
            "terminal.trzsz.disabled_title",
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for key in keys {
                assert_ne!(i18n.t(key), key, "{locale:?} missing {key}");
            }
        }
    }

    #[test]
    fn language_names_are_autonyms_in_every_locale() {
        let expected = [
            ("language.english", "English"),
            ("language.simplified_chinese", "简体中文"),
            ("language.traditional_chinese", "繁體中文"),
            ("language.german", "Deutsch"),
            ("language.spanish", "Español"),
            ("language.french", "Français"),
            ("language.italian", "Italiano"),
            ("language.japanese", "日本語"),
            ("language.korean", "한국어"),
            ("language.portuguese_brazil", "Português (Brasil)"),
            ("language.vietnamese", "Tiếng Việt"),
        ];
        let locales = [
            Locale::De,
            Locale::En,
            Locale::EsEs,
            Locale::FrFr,
            Locale::It,
            Locale::Ja,
            Locale::Ko,
            Locale::PtBr,
            Locale::Vi,
            Locale::ZhCn,
            Locale::ZhTw,
        ];

        for locale in locales {
            let i18n = I18n::new(locale);
            for (key, value) in expected {
                assert_eq!(i18n.t(key), value);
            }
        }
    }
}
