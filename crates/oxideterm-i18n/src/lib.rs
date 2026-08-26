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
    include_str!("../locales/en/mosh.json"),
    include_str!("../locales/en/ide.json"),
    include_str!("../locales/en/fileManager.json"),
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
    include_str!("../locales/de/mosh.json"),
    include_str!("../locales/de/ide.json"),
    include_str!("../locales/de/fileManager.json"),
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
    include_str!("../locales/es-ES/mosh.json"),
    include_str!("../locales/es-ES/ide.json"),
    include_str!("../locales/es-ES/fileManager.json"),
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
    include_str!("../locales/fr-FR/mosh.json"),
    include_str!("../locales/fr-FR/ide.json"),
    include_str!("../locales/fr-FR/fileManager.json"),
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
    include_str!("../locales/it/mosh.json"),
    include_str!("../locales/it/ide.json"),
    include_str!("../locales/it/fileManager.json"),
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
    include_str!("../locales/ja/mosh.json"),
    include_str!("../locales/ja/ide.json"),
    include_str!("../locales/ja/fileManager.json"),
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
    include_str!("../locales/ko/mosh.json"),
    include_str!("../locales/ko/ide.json"),
    include_str!("../locales/ko/fileManager.json"),
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
    include_str!("../locales/pt-BR/mosh.json"),
    include_str!("../locales/pt-BR/ide.json"),
    include_str!("../locales/pt-BR/fileManager.json"),
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
    include_str!("../locales/vi/mosh.json"),
    include_str!("../locales/vi/ide.json"),
    include_str!("../locales/vi/fileManager.json"),
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
    include_str!("../locales/zh-CN/mosh.json"),
    include_str!("../locales/zh-CN/ide.json"),
    include_str!("../locales/zh-CN/fileManager.json"),
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
    include_str!("../locales/zh-TW/mosh.json"),
    include_str!("../locales/zh-TW/ide.json"),
    include_str!("../locales/zh-TW/fileManager.json"),
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
    #[should_panic(expected = "duplicate native locale key")]
    fn duplicate_keys_are_rejected() {
        let _ = LocaleCatalog::from_json_parts(&[
            r#"{"menu":{"copy":"Copy"}}"#,
            r#"{"menu":{"copy":"Duplicate"}}"#,
        ]);
    }

    #[test]
    fn locale_catalogs_have_the_same_complete_key_set() {
        use std::collections::BTreeSet;

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
        let english_keys: BTreeSet<_> = LocaleCatalog::from_json_parts(EN_PARTS)
            .messages
            .into_keys()
            .collect();

        for locale in locales {
            let localized_keys: BTreeSet<_> = LocaleCatalog::from_json_parts(locale_parts(locale))
                .messages
                .into_keys()
                .collect();
            let missing: Vec<_> = english_keys.difference(&localized_keys).collect();
            let unexpected: Vec<_> = localized_keys.difference(&english_keys).collect();

            assert!(
                missing.is_empty() && unexpected.is_empty(),
                "{locale:?} catalog differs from English; missing: {missing:?}, unexpected: {unexpected:?}"
            );
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
