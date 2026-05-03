use std::collections::HashMap;

use serde_json::Value;

const EN_PARTS: &[&str] = &[
    include_str!("../locales/en/common.json"),
    include_str!("../locales/en/menu.json"),
    include_str!("../locales/en/sidebar.json"),
    include_str!("../locales/en/ssh.json"),
    include_str!("../locales/en/terminal.json"),
];
const DE_PARTS: &[&str] = &[
    include_str!("../locales/de/common.json"),
    include_str!("../locales/de/menu.json"),
    include_str!("../locales/de/sidebar.json"),
    include_str!("../locales/de/ssh.json"),
    include_str!("../locales/de/terminal.json"),
];
const ES_ES_PARTS: &[&str] = &[
    include_str!("../locales/es-ES/common.json"),
    include_str!("../locales/es-ES/menu.json"),
    include_str!("../locales/es-ES/sidebar.json"),
    include_str!("../locales/es-ES/ssh.json"),
    include_str!("../locales/es-ES/terminal.json"),
];
const FR_FR_PARTS: &[&str] = &[
    include_str!("../locales/fr-FR/common.json"),
    include_str!("../locales/fr-FR/menu.json"),
    include_str!("../locales/fr-FR/sidebar.json"),
    include_str!("../locales/fr-FR/ssh.json"),
    include_str!("../locales/fr-FR/terminal.json"),
];
const IT_PARTS: &[&str] = &[
    include_str!("../locales/it/common.json"),
    include_str!("../locales/it/menu.json"),
    include_str!("../locales/it/sidebar.json"),
    include_str!("../locales/it/ssh.json"),
    include_str!("../locales/it/terminal.json"),
];
const JA_PARTS: &[&str] = &[
    include_str!("../locales/ja/common.json"),
    include_str!("../locales/ja/menu.json"),
    include_str!("../locales/ja/sidebar.json"),
    include_str!("../locales/ja/ssh.json"),
    include_str!("../locales/ja/terminal.json"),
];
const KO_PARTS: &[&str] = &[
    include_str!("../locales/ko/common.json"),
    include_str!("../locales/ko/menu.json"),
    include_str!("../locales/ko/sidebar.json"),
    include_str!("../locales/ko/ssh.json"),
    include_str!("../locales/ko/terminal.json"),
];
const PT_BR_PARTS: &[&str] = &[
    include_str!("../locales/pt-BR/common.json"),
    include_str!("../locales/pt-BR/menu.json"),
    include_str!("../locales/pt-BR/sidebar.json"),
    include_str!("../locales/pt-BR/ssh.json"),
    include_str!("../locales/pt-BR/terminal.json"),
];
const VI_PARTS: &[&str] = &[
    include_str!("../locales/vi/common.json"),
    include_str!("../locales/vi/menu.json"),
    include_str!("../locales/vi/sidebar.json"),
    include_str!("../locales/vi/ssh.json"),
    include_str!("../locales/vi/terminal.json"),
];
const ZH_CN_PARTS: &[&str] = &[
    include_str!("../locales/zh-CN/common.json"),
    include_str!("../locales/zh-CN/menu.json"),
    include_str!("../locales/zh-CN/sidebar.json"),
    include_str!("../locales/zh-CN/ssh.json"),
    include_str!("../locales/zh-CN/terminal.json"),
];
const ZH_TW_PARTS: &[&str] = &[
    include_str!("../locales/zh-TW/common.json"),
    include_str!("../locales/zh-TW/menu.json"),
    include_str!("../locales/zh-TW/sidebar.json"),
    include_str!("../locales/zh-TW/ssh.json"),
    include_str!("../locales/zh-TW/terminal.json"),
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
    catalogs: HashMap<Locale, LocaleCatalog>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let mut catalogs = HashMap::new();
        catalogs.insert(Locale::De, LocaleCatalog::from_json_parts(DE_PARTS));
        catalogs.insert(Locale::En, LocaleCatalog::from_json_parts(EN_PARTS));
        catalogs.insert(Locale::EsEs, LocaleCatalog::from_json_parts(ES_ES_PARTS));
        catalogs.insert(Locale::FrFr, LocaleCatalog::from_json_parts(FR_FR_PARTS));
        catalogs.insert(Locale::It, LocaleCatalog::from_json_parts(IT_PARTS));
        catalogs.insert(Locale::Ja, LocaleCatalog::from_json_parts(JA_PARTS));
        catalogs.insert(Locale::Ko, LocaleCatalog::from_json_parts(KO_PARTS));
        catalogs.insert(Locale::PtBr, LocaleCatalog::from_json_parts(PT_BR_PARTS));
        catalogs.insert(Locale::Vi, LocaleCatalog::from_json_parts(VI_PARTS));
        catalogs.insert(Locale::ZhCn, LocaleCatalog::from_json_parts(ZH_CN_PARTS));
        catalogs.insert(Locale::ZhTw, LocaleCatalog::from_json_parts(ZH_TW_PARTS));

        Self {
            locale,
            fallback_locale: Locale::En,
            catalogs,
        }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    pub fn t(&self, key: &str) -> String {
        self.catalogs
            .get(&self.locale)
            .and_then(|catalog| catalog.get(key))
            .or_else(|| {
                self.catalogs
                    .get(&self.fallback_locale)
                    .and_then(|catalog| catalog.get(key))
            })
            .unwrap_or(key)
            .to_string()
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
