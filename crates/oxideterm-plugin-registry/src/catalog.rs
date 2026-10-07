use super::*;
use futures_util::{StreamExt, stream};

const HISTORY_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub(super) enum MetadataError {
    Status(u16),
    Invalid,
    Transport,
    TooLarge,
}
impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status(status) => write!(f, "Plugin catalog returned HTTP {status}"),
            Self::Invalid => f.write_str("Invalid plugin catalog URL"),
            Self::Transport => f.write_str("Cannot download plugin catalog metadata"),
            Self::TooLarge => f.write_str("Plugin catalog metadata exceeds its size limit"),
        }
    }
}

pub(super) async fn download_metadata(url: &str, limit: u64) -> Result<Vec<u8>, MetadataError> {
    validate_native_plugin_package_url(url).map_err(|_| MetadataError::Invalid)?;
    let client =
        oxideterm_network_proxy::application_http_client().map_err(|_| MetadataError::Transport)?;
    let mut response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| MetadataError::Transport)?;
    if !response.status().is_success() {
        return Err(MetadataError::Status(response.status().as_u16()));
    }
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(MetadataError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| MetadataError::Transport)?
    {
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            return Err(MetadataError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn validate_history_reference(
    history: &NativePluginRegistryHistory,
) -> Result<(), String> {
    let url =
        reqwest::Url::parse(&history.download_url).map_err(|_| "Invalid catalog history URL")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || history.size == 0
        || history.size > HISTORY_MAX_BYTES
        || history.checksum.strip_prefix("sha256:").is_none_or(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err("Invalid catalog history reference".into());
    }
    Ok(())
}

pub(super) fn validate_history_origins(
    url: &str,
    index: &NativePluginRegistryIndex,
) -> Result<(), String> {
    let origin = reqwest::Url::parse(url)
        .map_err(|_| "Invalid catalog URL")?
        .origin();
    for entry in &index.plugins {
        if let Some(history) = &entry.history {
            let destination = reqwest::Url::parse(&history.download_url)
                .map_err(|_| "Invalid catalog history URL")?;
            if destination.origin() != origin {
                return Err("Catalog histories must share the catalog origin".into());
            }
        }
    }
    Ok(())
}

fn history_path(settings: &Path, entry: &NativePluginRegistryEntry) -> Result<PathBuf, String> {
    validate_native_plugin_id(&entry.id)?;
    let history = entry
        .history
        .as_ref()
        .ok_or("Catalog history reference missing")?;
    validate_history_reference(history)?;
    let hash = history
        .checksum
        .strip_prefix("sha256:")
        .expect("validated checksum");
    Ok(settings
        .parent()
        .unwrap_or(settings)
        .join("plugin-catalog-histories")
        .join(&entry.id)
        .join(format!("{hash}.json")))
}

fn decode_history(
    summary: &NativePluginRegistryEntry,
    bytes: &[u8],
) -> Result<NativePluginRegistryEntry, String> {
    let reference = summary
        .history
        .as_ref()
        .ok_or("Catalog history reference missing")?;
    validate_history_reference(reference)?;
    if bytes.len() as u64 != reference.size
        || format!("sha256:{:x}", Sha256::digest(bytes)) != reference.checksum.to_ascii_lowercase()
    {
        return Err("Catalog history checksum or size differs".into());
    }
    let mut entry: NativePluginRegistryEntry =
        serde_json::from_slice(bytes).map_err(|_| "Invalid catalog history JSON")?;
    if entry.id != summary.id || entry.history.is_some() {
        return Err("Catalog history identity differs".into());
    }
    registry::validate_native_plugin_registry(&NativePluginRegistryIndex {
        version: 1,
        plugins: vec![entry.clone()],
    })?;
    let latest = entry
        .releases
        .iter()
        .filter_map(|release| {
            semver::Version::parse(&release.version)
                .ok()
                .map(|version| (version, release))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .ok_or("Catalog history has no releases")?;
    if latest.1.version != summary.version
        || Some(latest.1.effective_engines()) != summary.engines.as_ref()
        || entry.language != summary.language
        || entry.listed_at != summary.listed_at
        || entry.latest_release_at != summary.latest_release_at
    {
        return Err("Catalog summary and history differ".into());
    }
    entry.history = Some(reference.clone());
    Ok(entry)
}

pub(super) fn load_cached_history(
    settings: &Path,
    summary: &NativePluginRegistryEntry,
) -> Result<Option<NativePluginRegistryEntry>, String> {
    let file = history_path(settings, summary)?;
    match fs::File::open(file) {
        Ok(file) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(HISTORY_MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read catalog history cache")?;
            decode_history(summary, &bytes).map(Some)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot open catalog history cache".into()),
    }
}

pub(super) fn summary(entry: &NativePluginRegistryEntry) -> NativePluginRegistryEntry {
    let mut summary = entry.clone();
    if summary.history.is_none() {
        return summary;
    }
    if let Some(latest) = entry.releases.iter().max_by(|a, b| {
        semver::Version::parse(&a.version)
            .ok()
            .cmp(&semver::Version::parse(&b.version).ok())
    }) {
        summary.version = latest.version.clone();
        summary.engines = Some(latest.effective_engines().clone());
    }
    summary.packages.clear();
    summary.releases.clear();
    summary.download_url.clear();
    summary.checksum = None;
    summary.size = None;
    summary.min_oxideterm_version = None;
    summary
}

pub(super) fn summaries(index: &NativePluginRegistryIndex) -> NativePluginRegistryIndex {
    NativePluginRegistryIndex {
        version: index.version,
        plugins: index.plugins.iter().map(summary).collect(),
    }
}

impl NativePluginRegistry {
    pub fn registry_summary(entry: &NativePluginRegistryEntry) -> NativePluginRegistryEntry {
        summary(entry)
    }

    pub async fn fetch_registry_history(
        settings: &Path,
        summary: &NativePluginRegistryEntry,
    ) -> Result<NativePluginRegistryEntry, String> {
        if !summary.history_pending() {
            return Ok(summary.clone());
        }
        if let Ok(Some(entry)) = load_cached_history(settings, summary) {
            return Ok(entry);
        }
        let reference = summary.history.as_ref().expect("pending history reference");
        let bytes = download_metadata(&reference.download_url, reference.size)
            .await
            .map_err(|error| error.to_string())?;
        let entry = decode_history(summary, &bytes)?;
        let file = history_path(settings, summary)?;
        fs::create_dir_all(file.parent().expect("history directory"))
            .map_err(|_| "Cannot create catalog cache directory")?;
        oxideterm_atomic_file::durable_write(&file, &bytes)
            .map_err(|_| "Cannot save catalog history cache")?;
        Ok(entry)
    }

    pub async fn hydrate_registry_histories(
        settings: &Path,
        index: &mut NativePluginRegistryIndex,
        ids: &[String],
    ) -> Result<(), String> {
        let requested = index
            .plugins
            .iter()
            .filter(|entry| ids.contains(&entry.id) && entry.history_pending())
            .cloned()
            .collect::<Vec<_>>();
        for (_, result) in Self::fetch_registry_histories(settings, &requested).await {
            let entry = result?;
            if let Some(summary) = index
                .plugins
                .iter_mut()
                .find(|summary| summary.id == entry.id)
            {
                *summary = entry;
            }
        }
        Ok(())
    }

    pub async fn fetch_registry_histories(
        settings: &Path,
        entries: &[NativePluginRegistryEntry],
    ) -> Vec<(String, Result<NativePluginRegistryEntry, String>)> {
        let entries = entries.to_vec();
        let settings = settings.to_path_buf();
        stream::iter(entries)
            .map(move |entry| {
                let settings = settings.clone();
                async move {
                    (
                        entry.id.clone(),
                        Self::fetch_registry_history(&settings, &entry).await,
                    )
                }
            })
            .buffer_unordered(4)
            .collect()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> NativePluginRegistryEntry {
        serde_json::from_value(serde_json::json!({
            "id":"com.example.demo", "name":"Demo", "version":"1.0.0",
            "license":"MIT", "licenseUrl":"https://example.com/LICENSE",
            "language":{"id":"custom-lang","displayName":"Custom Language","extensions":["custom.expr"]},
            "listedAt":"2026-01-01T00:00:00Z", "latestReleaseAt":"2026-10-06T00:00:00Z",
            "engines":{"oxideterm":">=2.0.0"},
            "packages":[{"target":"any", "downloadUrl":"https://example.com/1.zip", "checksum":"a".repeat(64), "size":128}],
            "releases":[
                {"version":"1.0.0", "engines":{"oxideterm":">=2.0.0"},
                 "packages":[{"target":"any", "downloadUrl":"https://example.com/1.zip", "checksum":"a".repeat(64), "size":128}],
                 "compatibilityCorrections":[{"engines":{"oxideterm":">=2.0.0, <3.0.0"},"reason":"Removed API","recordedAt":"2026-10-06T00:00:00Z"}]},
                {"version":"2.0.0", "engines":{"oxideterm":">=3.0.0"},
                 "packages":[{"target":"any", "downloadUrl":"https://example.com/2.zip", "checksum":"b".repeat(64), "size":256}]}
            ]
        })).unwrap()
    }

    fn referenced(mut entry: NativePluginRegistryEntry) -> (NativePluginRegistryEntry, Vec<u8>) {
        let bytes = serde_json::to_vec(&entry).unwrap();
        entry.history = Some(NativePluginRegistryHistory {
            download_url: "https://example.com/history.json".into(),
            checksum: format!("sha256:{:x}", Sha256::digest(&bytes)),
            size: bytes.len() as u64,
        });
        (summary(&entry), bytes)
    }

    #[test]
    fn verified_history_selects_compatible_assets_and_rejects_mixed_snapshots() {
        let (summary, bytes) = referenced(history());
        let hydrated = decode_history(&summary, &bytes).unwrap();
        assert_eq!(summary.license.as_deref(), Some("MIT"));
        assert_eq!(hydrated.license.as_deref(), Some("MIT"));
        assert_eq!(
            hydrated.license_url.as_deref(),
            Some("https://example.com/LICENSE")
        );
        let old_host = NativePluginRegistry::select_registry_release(&hydrated).unwrap();
        assert_eq!(old_host.license.as_deref(), Some("MIT"));
        assert_eq!(
            (
                old_host.version.as_str(),
                old_host.packages[0].download_url.as_str()
            ),
            ("1.0.0", "https://example.com/1.zip")
        );
        for case in [
            "checksum",
            "size",
            "identity",
            "version",
            "engines",
            "language",
            "listedAt",
            "latestReleaseAt",
        ] {
            let mut invalid = summary.clone();
            match case {
                "checksum" => {
                    invalid.history.as_mut().unwrap().checksum =
                        format!("sha256:{}", "c".repeat(64))
                }
                "size" => invalid.history.as_mut().unwrap().size += 1,
                "identity" => invalid.id = "com.example.other".into(),
                "version" => invalid.version = "1.0.0".into(),
                "engines" => invalid.engines = None,
                "language" => invalid.language.as_mut().unwrap().extensions = vec!["wrong".into()],
                "listedAt" => invalid.listed_at = Some("2026-02-01T00:00:00Z".into()),
                "latestReleaseAt" => {
                    invalid.latest_release_at = Some("2026-10-07T00:00:00Z".into())
                }
                _ => unreachable!(),
            }
            assert!(decode_history(&invalid, &bytes).is_err(), "{case}");
        }
        let mut root = NativePluginRegistryIndex {
            version: 2,
            plugins: vec![summary],
        };
        registry::validate_native_plugin_registry(&root).unwrap();
        let mut missing_reference = root.clone();
        missing_reference.plugins[0].history = None;
        assert!(registry::validate_native_plugin_registry(&missing_reference).is_err());
        let mut missing_range = root.clone();
        missing_range.plugins[0].engines = None;
        assert!(registry::validate_native_plugin_registry(&missing_range).is_err());
        validate_history_origins("https://example.com/index.json", &root).unwrap();
        assert!(validate_history_origins("https://other.example/index.json", &root).is_err());
        root.version = 1;
        assert!(registry::validate_native_plugin_registry(&root).is_err());
    }

    #[test]
    fn compact_cache_restores_only_installed_histories_and_preserves_corrections() {
        let directory = crate::tests::unique_temp_dir("compact-catalog");
        fs::create_dir_all(&directory).unwrap();
        let settings = directory.join("settings.json");
        let mut compatible = history();
        compatible.releases[1].engines.oxideterm = Some(">=2.0.0".into());
        let (summary, bytes) = referenced(compatible);
        let file = history_path(&settings, &summary).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, &bytes).unwrap();
        let hydrated = decode_history(&summary, &bytes).unwrap();
        let mut manifest = crate::tests::minimal_manifest();
        let root = NativePluginRegistryIndex {
            version: 2,
            plugins: vec![hydrated],
        };
        NativePluginRegistry::cache_official_catalog(&settings, &root).unwrap();
        let cached: NativePluginRegistryIndex =
            serde_json::from_slice(&fs::read(catalog_cache_path(&settings)).unwrap()).unwrap();
        assert_eq!(cached.plugins[0].version, "2.0.0");
        assert!(cached.plugins[0].history_pending());
        assert_eq!(cached.plugins[0].history, summary.history);
        assert_eq!(cached.plugins[0].language, summary.language);
        assert_eq!(
            NativePluginRegistry::discover(&settings).catalog_languages(),
            &[("com.example.demo".into(), summary.language.clone().unwrap())]
        );
        let not_installed = load_catalog_cache(&settings, &[]).unwrap().unwrap();
        assert!(not_installed.plugins[0].history_pending());
        let installed = load_catalog_cache(&settings, &["com.example.demo"])
            .unwrap()
            .unwrap();
        apply_catalog_compatibility(&mut manifest, &installed);
        assert_eq!(
            manifest.engines.unwrap().oxideterm.as_deref(),
            Some(">=2.0.0, <3.0.0")
        );
        // An unrelated cache miss must not discard the root or another plugin's data.
        fs::remove_file(&file).unwrap();
        let missing = load_catalog_cache(&settings, &["com.example.demo"])
            .unwrap()
            .unwrap();
        assert_eq!(missing.plugins[0].history, summary.history);
        assert!(missing.plugins[0].history_pending());
        fs::write(&file, b"corrupt history").unwrap();
        assert!(load_catalog_cache(&settings, &["com.example.demo"]).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn offline_startup_rechecks_the_exact_installed_version_from_split_history() {
        let directory = crate::tests::unique_temp_dir("split-catalog-startup");
        let settings = directory.join("settings.json");
        let plugin_dir = native_plugins_dir(&settings).join("com.example.demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let manifest = serde_json::to_vec(&crate::tests::minimal_manifest()).unwrap();
        fs::write(plugin_dir.join("plugin.json"), &manifest).unwrap();
        NativePluginRegistry::discover(&settings)
            .set_plugin_enabled("com.example.demo", true)
            .unwrap();
        let config = fs::read(native_plugin_config_path(&settings)).unwrap();
        for (range, state) in [
            (">=999.0.0", NativePluginState::Error),
            (">=2.0.0, <3.0.0", NativePluginState::ReadyManifestOnly),
        ] {
            let mut entry = history();
            entry.releases[0].compatibility_corrections[0]
                .engines
                .oxideterm = Some(range.into());
            let (summary, bytes) = referenced(entry);
            let file = history_path(&settings, &summary).unwrap();
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, &bytes).unwrap();
            NativePluginRegistry::cache_official_catalog(
                &settings,
                &NativePluginRegistryIndex {
                    version: 2,
                    plugins: vec![summary],
                },
            )
            .unwrap();
            let offline = NativePluginRegistry::discover(&settings);
            assert_eq!(offline.plugins()[0].state, state);
            assert_eq!(
                offline.plugins()[0]
                    .manifest
                    .engines
                    .as_ref()
                    .unwrap()
                    .oxideterm
                    .as_deref(),
                Some(range)
            );
            assert_eq!(fs::read(plugin_dir.join("plugin.json")).unwrap(), manifest);
            assert_eq!(
                fs::read(native_plugin_config_path(&settings)).unwrap(),
                config
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn metadata_download_bounds_advertised_and_chunked_bodies() {
        use std::io::{Read, Write};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (response, expected) in [
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
                Ok(b"abc".to_vec()),
            ),
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd",
                Err("Plugin catalog metadata exceeds its size limit"),
            ),
            (
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\nab\r\n2\r\ncd\r\n0\r\n\r\n",
                Err("Plugin catalog metadata exceeds its size limit"),
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 1024];
                socket.read(&mut request).unwrap();
                socket.write_all(response.as_bytes()).unwrap();
            });
            let result = runtime
                .block_on(download_metadata(
                    &format!("http://{address}/index.json"),
                    3,
                ))
                .map_err(|error| error.to_string());
            assert_eq!(
                result.as_ref().map_err(String::as_str),
                expected.as_ref().map_err(|error| *error)
            );
            server.join().unwrap();
        }
    }
}
