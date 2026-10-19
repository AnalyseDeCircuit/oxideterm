use serde::{Deserialize, Serialize};

use crate::NativePluginDeclarativeUiSchema;

/// A connection type is namespaced by its owning plugin, not by its display title.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePluginConnectionDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub form: NativePluginDeclarativeUiSchema,
    /// Terminal sessions may launch only this package-relative executable.
    #[serde(default)]
    pub terminal_entry: Option<String>,
}
