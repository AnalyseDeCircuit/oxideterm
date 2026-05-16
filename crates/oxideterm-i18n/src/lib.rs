use std::collections::HashMap;

use serde_json::Value;

const EN_US: &str = include_str!("../locales/en-US/native.json");
const ZH_CN: &str = include_str!("../locales/zh-CN/native.json");

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
        catalogs.insert(Locale::EnUs, LocaleCatalog::from_json(EN_US));
        catalogs.insert(Locale::ZhCn, LocaleCatalog::from_json(ZH_CN));

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
    fn from_json(source: &str) -> Self {
        let value: Value = serde_json::from_str(source).expect("invalid native locale catalog");
        let mut messages = HashMap::new();
        flatten_json("", &value, &mut messages);
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
            messages.insert(prefix.to_string(), message.clone());
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
}
