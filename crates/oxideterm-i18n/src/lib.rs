use std::collections::HashMap;

use serde_json::Value;

const EN_US_PARTS: &[&str] = &[
    include_str!("../locales/en-US/common.json"),
    include_str!("../locales/en-US/menu.json"),
    include_str!("../locales/en-US/sidebar.json"),
    include_str!("../locales/en-US/ssh.json"),
    include_str!("../locales/en-US/terminal.json"),
];
const ZH_CN_PARTS: &[&str] = &[
    include_str!("../locales/zh-CN/common.json"),
    include_str!("../locales/zh-CN/menu.json"),
    include_str!("../locales/zh-CN/sidebar.json"),
    include_str!("../locales/zh-CN/ssh.json"),
    include_str!("../locales/zh-CN/terminal.json"),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Locale {
    EnUs,
    ZhCn,
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
        catalogs.insert(Locale::EnUs, LocaleCatalog::from_json_parts(EN_US_PARTS));
        catalogs.insert(Locale::ZhCn, LocaleCatalog::from_json_parts(ZH_CN_PARTS));

        Self {
            locale,
            fallback_locale: Locale::EnUs,
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

        i18n.set_locale(Locale::EnUs);
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
}
