//! Browsing the catalog only reads published summaries; selecting a runnable release belongs to the host.
use crate::{NativePluginRegistryEntry, NativePluginRegistryIndex};

pub const OFFICIAL_NATIVE_PLUGIN_REGISTRY_V2_URL: &str = "https://raw.githubusercontent.com/AnalyseDeCircuit/oxideterm-plugins/main/registry/v2/index.json";
pub const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;

/// Read display metadata without selecting a package or requiring permission to run it.
pub fn read_summaries(bytes: &[u8]) -> Option<Vec<NativePluginRegistryEntry>> {
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return None;
    }
    let index: NativePluginRegistryIndex = serde_json::from_slice(bytes).ok()?;
    let mut ids = std::collections::HashSet::new();
    (index.version == 2
        && index.plugins.iter().all(|p| {
            !p.id.trim().is_empty()
                && !p.name.trim().is_empty()
                && !p.version.trim().is_empty()
                && ids.insert(&p.id)
        }))
    .then_some(index.plugins)
}

pub fn sort_entries(
    entries: &mut [NativePluginRegistryEntry],
    date: impl Fn(&NativePluginRegistryEntry) -> Option<&str>,
) {
    entries.sort_by_cached_key(|entry| {
        let date = date(entry)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|date| date.timestamp_millis());
        // Missing publication metadata follows dated entries; editing descriptions
        // must never stand in for publishing a version or listing a new plugin.
        (
            std::cmp::Reverse(date),
            entry.name.to_lowercase(),
            entry.id.clone(),
        )
    });
}

pub fn entry_matches(entry: &NativePluginRegistryEntry, query: &str) -> bool {
    query.is_empty()
        || entry.id.to_lowercase().contains(query)
        || entry.name.to_lowercase().contains(query)
        || entry
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains(query))
        || entry
            .author
            .as_deref()
            .is_some_and(|author| author.to_lowercase().contains(query))
        || entry
            .tags
            .as_ref()
            .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(query)))
}

pub fn filter_tags<'a>(tags: impl Iterator<Item = &'a str>) -> Vec<String> {
    tags.map(normalized_tag)
        .filter(|tag| !tag.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn tag_matches(tags: &[String], selected: Option<&str>) -> bool {
    selected.is_none_or(|selected| {
        tags.iter()
            .any(|tag| normalized_tag(tag) == normalized_tag(selected))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_catalog_browses_without_packages_and_rejects_ambiguous_ids() {
        let summary = serde_json::json!({"id":"org.example.highlight","name":"Highlight","version":"1.4.0","tags":["language"]});
        let source = |rows| serde_json::json!({"version":2,"plugins": rows}).to_string();
        let entries = read_summaries(source(vec![&summary]).as_bytes()).unwrap();
        assert_eq!(entries[0].id, "org.example.highlight");
        assert_eq!(entries[0].version, "1.4.0");
        assert!(read_summaries(source(vec![&summary, &summary]).as_bytes()).is_none());
        assert!(read_summaries(br#"{"version":1,"plugins":[]}"#).is_none());
    }
}

fn normalized_tag(tag: &str) -> String {
    match tag.trim().to_lowercase().as_str() {
        // Published catalogs remain usable until their cached tags refresh.
        "remote-desktop" => "remote-connections".into(),
        tag => tag.to_string(),
    }
}
