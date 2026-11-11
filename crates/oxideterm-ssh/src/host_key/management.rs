use super::*;

static EDIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownHostEntry {
    pub host: String,
    pub port: Option<u16>,
    pub key_type: String,
    pub fingerprint: String,
    pub can_remove: bool,
}

#[derive(Clone, Debug)]
pub struct KnownHostsSnapshot {
    pub path: PathBuf,
    pub exists: bool,
    pub entries: Vec<KnownHostEntry>,
}

/// One user-approved replacement, bound to both the endpoint and the new key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostKeyReplacement {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub previous_fingerprint: String,
    pub fingerprint: String,
}

pub fn list_known_hosts() -> Result<KnownHostsSnapshot, SshTransportError> {
    read_known_hosts(default_known_hosts_path()?)
}

fn read_known_hosts(path: PathBuf) -> Result<KnownHostsSnapshot, SshTransportError> {
    let (content, exists) = match fs::read_to_string(&path) {
        Ok(content) => (content, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
        Err(error) => return Err(file_error(error)),
    };
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(first) = fields.next() else {
            continue;
        };
        let marked = first.starts_with('@');
        let hosts = if marked {
            fields.next().unwrap_or("")
        } else {
            first
        };
        let (Some(key_type), Some(key_data)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(key) = parse_public_key_base64(key_data) else {
            continue;
        };
        for host in hosts.split(',') {
            let endpoint = plain_endpoint(host);
            entries.push(KnownHostEntry {
                host: endpoint
                    .as_ref()
                    .map_or_else(|| host.to_string(), |(host, _)| host.clone()),
                port: endpoint.as_ref().map(|(_, port)| *port),
                key_type: key_type.to_string(),
                fingerprint: public_key_fingerprint(&key),
                can_remove: !marked && endpoint.is_some() && key.algorithm().as_str() == key_type,
            });
        }
    }
    entries.sort_by(|a, b| {
        (&a.host, a.port, &a.key_type, &a.fingerprint).cmp(&(
            &b.host,
            b.port,
            &b.key_type,
            &b.fingerprint,
        ))
    });
    entries.dedup();
    Ok(KnownHostsSnapshot {
        path,
        exists,
        entries,
    })
}

fn plain_endpoint(host: &str) -> Option<(String, u16)> {
    if host.is_empty() || host.contains(['|', '*', '?', '!', '@']) {
        return None;
    }
    if let Some(rest) = host.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        return Some((host.to_lowercase(), port.parse().ok()?));
    }
    Some((host.to_lowercase(), 22))
}

fn file_error(error: std::io::Error) -> SshTransportError {
    SshTransportError::HostKeyCheckFailed(error.to_string())
}

fn edit_file(
    path: &Path,
    edit: impl FnOnce(&str) -> Result<Option<String>, SshTransportError>,
) -> Result<bool, SshTransportError> {
    let _lock = EDIT_LOCK.lock().map_err(|_| {
        SshTransportError::HostKeyCheckFailed("Host trust store is unavailable".into())
    })?;
    // Keep a user-managed symlink intact; atomically replace its target instead.
    let path = if path.exists() {
        fs::canonicalize(path).map_err(file_error)?
    } else {
        path.to_path_buf()
    };
    let original = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(file_error(error)),
    };
    let Some(updated) = edit(&original)? else {
        return Ok(false);
    };
    let parent = path.parent().ok_or_else(|| {
        SshTransportError::HostKeyCheckFailed("Host trust file has no parent directory".into())
    })?;
    fs::create_dir_all(parent).map_err(file_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    if let Ok(metadata) = fs::metadata(&path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(file_error)?;
    }
    temporary
        .write_all(updated.as_bytes())
        .map_err(file_error)?;
    // An external SSH client may edit this shared file while the replacement is prepared.
    let current = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(file_error(error)),
    };
    if current != original {
        return Err(SshTransportError::HostKeyCheckFailed(
            "Host trust file changed; refresh and try again".into(),
        ));
    }
    oxideterm_atomic_file::durable_replace(temporary.path(), &path).map_err(file_error)?;
    Ok(true)
}

pub(super) fn append_key(
    path: &Path,
    host: &str,
    key_type: &str,
    key_data: &str,
) -> Result<(), SshTransportError> {
    edit_file(path, |original| {
        if original.lines().any(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            fields.len() >= 3
                && fields[0]
                    .split(',')
                    .any(|alias| KnownHostsStore::canonical_host_entry(alias) == host)
                && fields[1] == key_type
                && fields[2] == key_data
        }) {
            return Ok(None);
        }
        let separator = if original.is_empty() || original.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        Ok(Some(format!(
            "{original}{separator}{host} {key_type} {key_data}\n"
        )))
    })?;
    Ok(())
}

pub(super) fn rewrite_key(
    path: &Path,
    hosts: &[String],
    key_type: &str,
    fingerprint: &str,
    replacement: Option<&PublicKey>,
) -> Result<bool, SshTransportError> {
    edit_file(path, |original| {
        let mut changed = false;
        let mut updated = String::new();
        for line in original.split_inclusive('\n') {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 3
                || fields[0].starts_with(['#', '@'])
                || fields[1] != key_type
                || fingerprint_from_known_hosts_key_data(fields[2]) != fingerprint
            {
                updated.push_str(line);
                continue;
            }
            let (removed, kept): (Vec<_>, Vec<_>) = fields[0].split(',').partition(|alias| {
                plain_endpoint(alias).is_some()
                    && hosts.contains(&KnownHostsStore::canonical_host_entry(alias))
            });
            if removed.is_empty() {
                updated.push_str(line);
                continue;
            }
            changed = true;
            let hosts_start = line.len() - line.trim_start().len();
            let hosts_end = hosts_start + fields[0].len();
            let type_rest = line[hosts_end..].trim_start();
            let data_rest = type_rest[fields[1].len()..].trim_start();
            let data_start = line.len() - data_rest.len();
            let data_end = data_start + fields[2].len();
            if !kept.is_empty() {
                updated.push_str(&format!(
                    "{}{}{}",
                    &line[..hosts_start],
                    kept.join(","),
                    &line[hosts_end..]
                ));
                if replacement.is_some() && !line.ends_with('\n') {
                    updated.push('\n');
                }
            }
            if let Some(key) = replacement {
                updated.push_str(&format!(
                    "{}{}{}{}{}",
                    &line[..hosts_start],
                    removed.join(","),
                    &line[hosts_end..data_start],
                    key.public_key_base64(),
                    &line[data_end..]
                ));
            }
        }
        Ok(changed.then_some(updated))
    })
}

impl HostKeyReplacement {
    pub(crate) fn apply(&self, key: &PublicKey) -> Result<(), SshTransportError> {
        if public_key_fingerprint(key) != self.fingerprint || public_key_type(key) != self.key_type
        {
            return Err(SshTransportError::HostKeyChanged {
                host: self.host.clone(),
                port: self.port,
                expected_fingerprint: self.fingerprint.clone(),
                actual_fingerprint: public_key_fingerprint(key),
                key_type: public_key_type(key),
            });
        }
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            "host_trust_replace",
            Some(&self.host),
            None,
        );
        let result = (|| {
            // A completed attempt can be retried with the same runtime configuration.
            if KnownHostsStore::new()?.verify(&self.host, self.port, key)
                == HostKeyVerification::Verified
            {
                accept_host_key_for_session(&self.host, self.port, self.fingerprint.clone());
                return Ok(());
            }
            let replaced = rewrite_key(
                &default_known_hosts_path()?,
                &[KnownHostsStore::make_key(&self.host, self.port)],
                &self.key_type,
                &self.previous_fingerprint,
                Some(key),
            )?;
            if !replaced {
                return Err(SshTransportError::HostKeyCheckFailed(
                    "The saved host key changed; check the host again".into(),
                ));
            }
            HOST_KEY_CACHE.invalidate(&self.host, self.port);
            accept_host_key_for_session(&self.host, self.port, self.fingerprint.clone());
            Ok(())
        })();
        audit.result(&result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn management_preserves_other_ports_aliases_comments_and_read_only_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("known_hosts");
        let old = super::super::tests::sample_public_key();
        let new = super::super::tests::alternate_public_key();
        let blob = old.public_key_base64();
        let original = format!(
            "# keep this comment\r\n  board,alias ssh-ed25519 {blob}  board note\r\n[board]:2222 ssh-ed25519 {blob}\n@revoked revoked ssh-ed25519 {blob}\n|1|opaque|hash ssh-ed25519 {blob}\nunknown format\n"
        );
        fs::write(&path, &original).unwrap();
        let snapshot = read_known_hosts(path.clone()).unwrap();
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|entry| (entry.host.as_str(), entry.port, entry.can_remove))
                .collect::<Vec<_>>(),
            vec![
                ("alias", Some(22), true),
                ("board", Some(22), true),
                ("board", Some(2222), true),
                ("revoked", Some(22), false),
                ("|1|opaque|hash", None, false)
            ]
        );
        assert!(
            rewrite_key(
                &path,
                &["board".into()],
                "ssh-ed25519",
                &public_key_fingerprint(&old),
                Some(&new)
            )
            .unwrap()
        );
        let expected = original.replace(
            &format!("  board,alias ssh-ed25519 {blob}  board note\r\n"),
            &format!(
                "  alias ssh-ed25519 {blob}  board note\r\n  board ssh-ed25519 {}  board note\r\n",
                new.public_key_base64()
            ),
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(
            !rewrite_key(
                &path,
                &["board".into()],
                "ssh-ed25519",
                &public_key_fingerprint(&old),
                None
            )
            .unwrap()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(
            rewrite_key(
                &path,
                &["[board]:2222".into()],
                "ssh-ed25519",
                &public_key_fingerprint(&old),
                None
            )
            .unwrap()
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            expected.replace(&format!("[board]:2222 ssh-ed25519 {blob}\n"), "")
        );
    }
}
