// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::RemoteDesktopEndpoint;

const CERTIFICATE_STORE_VERSION: u32 = 1;
pub const REMOTE_DESKTOP_CERTIFICATE_STORE_FILE: &str = "rdp-known-certificates.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteDesktopCertificateStore {
    path: PathBuf,
    document: CertificateStoreDocument,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertificateStoreDocument {
    version: u32,
    #[serde(default)]
    certificates: BTreeMap<String, CertificatePin>,
}

impl Default for CertificateStoreDocument {
    fn default() -> Self {
        Self {
            version: CERTIFICATE_STORE_VERSION,
            certificates: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertificatePin {
    sha256_fingerprint: String,
}

impl RemoteDesktopCertificateStore {
    pub fn load(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let document = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<CertificateStoreDocument>(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                CertificateStoreDocument::default()
            }
            Err(error) => return Err(error),
        };
        if document.version != CERTIFICATE_STORE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported RDP certificate store version {}",
                    document.version
                ),
            ));
        }
        Ok(Self { path, document })
    }

    pub fn path_next_to_settings(settings_path: &Path) -> PathBuf {
        settings_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(REMOTE_DESKTOP_CERTIFICATE_STORE_FILE)
    }

    pub fn fingerprint(&self, endpoint: &RemoteDesktopEndpoint) -> Option<&str> {
        self.document
            .certificates
            .get(&certificate_endpoint_key(endpoint))
            .map(|pin| pin.sha256_fingerprint.as_str())
    }

    pub fn trust(
        &mut self,
        endpoint: &RemoteDesktopEndpoint,
        sha256_fingerprint: impl Into<String>,
    ) -> io::Result<()> {
        let key = certificate_endpoint_key(endpoint);
        let fingerprint = sha256_fingerprint.into();
        let mut next = self.document.clone();
        next.certificates.insert(
            key,
            CertificatePin {
                sha256_fingerprint: fingerprint,
            },
        );
        let bytes = serde_json::to_vec_pretty(&next)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        // Commit in-memory trust only after durable replacement succeeds.
        oxideterm_atomic_file::durable_write(&self.path, &bytes)?;
        self.document = next;
        Ok(())
    }
}

pub fn certificate_endpoint_key(endpoint: &RemoteDesktopEndpoint) -> String {
    let normalized_host = endpoint
        .host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .map(|address| address.to_string())
        .unwrap_or_else(|_| endpoint.host.trim().to_ascii_lowercase());
    if normalized_host.contains(':') {
        format!("[{normalized_host}]:{}", endpoint.port)
    } else {
        format!("{normalized_host}:{}", endpoint.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_keys_normalize_dns_and_ipv6() {
        assert_eq!(
            certificate_endpoint_key(&RemoteDesktopEndpoint::new("EXAMPLE.test", 3389)),
            "example.test:3389"
        );
        assert_eq!(
            certificate_endpoint_key(&RemoteDesktopEndpoint::new("[2001:0db8::1]", 3389)),
            "[2001:db8::1]:3389"
        );
    }

    #[test]
    fn trust_is_durable_and_reloads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(REMOTE_DESKTOP_CERTIFICATE_STORE_FILE);
        let endpoint = RemoteDesktopEndpoint::new("example.test", 3389);
        let mut store = RemoteDesktopCertificateStore::load(&path).unwrap();

        store.trust(&endpoint, "AA:BB").unwrap();

        let reloaded = RemoteDesktopCertificateStore::load(path).unwrap();
        assert_eq!(reloaded.fingerprint(&endpoint), Some("AA:BB"));
    }

    #[test]
    fn corrupt_store_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(REMOTE_DESKTOP_CERTIFICATE_STORE_FILE);
        fs::write(&path, b"{not-json").unwrap();

        assert_eq!(
            RemoteDesktopCertificateStore::load(path)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
