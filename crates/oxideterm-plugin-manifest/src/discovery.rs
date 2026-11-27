// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{NativePluginDeclarativeUiControl, NativePluginDeclarativeUiSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePluginDiscoveryProviderDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_refresh_interval_minutes: Option<std::num::NonZeroU32>,
    pub form: NativePluginDeclarativeUiSchema,
    pub launch_kinds: Vec<DiscoveryLaunchKind>,
    #[serde(default)]
    pub terminal_entry: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_actions: Vec<DiscoverySourceAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySourceAction {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryLaunchKind {
    Terminal,
    Ssh,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryConfigUse {
    Catalog,
    Open,
    Both,
}

/// Settings contain only the revision and the public field values it was saved for.
pub fn secret_scope(config: &Value, fields: &[String]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|key| (key.clone(), config[key].clone()))
            .collect(),
    )
}

pub fn secret_key(source: &str, field: &str, binding: &Value) -> Option<String> {
    let revision = uuid::Uuid::parse_str(binding.get("discoverySecret")?.as_str()?).ok()?;
    Some(format!(
        "discovery:{}:{source}:{field}:{revision}",
        source.len()
    ))
}

impl NativePluginDiscoveryProviderDef {
    pub fn config_for(&self, source: &str, config: &Value, purpose: DiscoveryConfigUse) -> Value {
        fn collect(
            controls: &[NativePluginDeclarativeUiControl],
            section: &str,
            source: &str,
            config: &Value,
            purpose: DiscoveryConfigUse,
            out: &mut Map<String, Value>,
        ) {
            for control in controls {
                if matches!(
                    control.kind.as_str(),
                    "text" | "number" | "checkbox" | "select" | "textarea" | "password"
                ) && let Some(id) = &control.id
                    && !matches!(
                        (control.discovery_use.as_deref(), purpose),
                        (Some("catalog"), DiscoveryConfigUse::Open)
                            | (Some("open"), DiscoveryConfigUse::Catalog)
                    )
                {
                    let key = format!("{section}/{id}");
                    if control.kind == "password" {
                        let binding = &config[&key];
                        if let Some(scope) = &control.secret_scope
                            && binding.get("scope") == Some(&secret_scope(config, scope))
                            && let Some(reference) = secret_key(source, &key, binding)
                        {
                            out.insert(key, Value::String(reference));
                        }
                    } else if let Some(value) = config.get(&key).or(control.value.as_ref()) {
                        out.insert(key, value.clone());
                    }
                }
                collect(&control.children, section, source, config, purpose, out);
            }
        }
        let mut out = Map::new();
        collect(
            &self.form.controls,
            "root",
            source,
            config,
            purpose,
            &mut out,
        );
        for section in &self.form.sections {
            collect(
                &section.controls,
                &section.id,
                source,
                config,
                purpose,
                &mut out,
            );
        }
        Value::Object(out)
    }
}
