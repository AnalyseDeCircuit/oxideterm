use std::fs::File;
use std::path::PathBuf;

use crate::SecretString;
use crate::store::{ImportedManagedSshKey, ManagedSshKeyOrigin, prepare_managed_ssh_key};

use super::*;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const SOURCE: ConnectionImportSource = ConnectionImportSource::XTerminal;

pub(super) enum Credential {
    Password(SecretString),
    Key {
        path: String,
        selected_file: Option<PathBuf>,
        passphrase: Option<SecretString>,
    },
}

impl Credential {
    pub(super) fn into_saved_auth(
        self,
        name: &str,
        managed_keys: &mut Vec<ImportedManagedSshKey>,
    ) -> anyhow::Result<SavedAuth> {
        Ok(match self {
            Self::Password(password) => SavedAuth::Password {
                empty_password: false,
                keychain_id: None,
                plaintext_password: Some(password),
            },
            Self::Key {
                path,
                selected_file: None,
                passphrase,
            } => SavedAuth::Key {
                key_path: path,
                has_passphrase: passphrase.is_some(),
                passphrase_keychain_id: None,
                plaintext_passphrase: passphrase,
            },
            Self::Key {
                selected_file: Some(file),
                passphrase,
                ..
            } => {
                // Read key material only from files explicitly chosen in the picker.
                let key = read_selected_file(&file)?;
                let staged = prepare_managed_ssh_key(
                    SecretString::from(key),
                    name.to_owned(),
                    passphrase.as_ref(),
                    ManagedSshKeyOrigin::ImportedFile,
                    true,
                )?;
                let key_id = if let Some(previous) = managed_keys
                    .iter()
                    .find(|entry| entry.secret == staged.secret)
                {
                    previous.key.id.clone()
                } else {
                    let id = staged.key.id.clone();
                    managed_keys.push(staged);
                    id
                };
                SavedAuth::ManagedKey {
                    key_id,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: passphrase,
                }
            }
        })
    }
}

struct Record {
    draft: ImportedConnectionDraft,
    secret: Option<SecretString>,
}

pub(super) fn collect(
    paths: &[String],
    selected_ids: Option<&HashSet<String>>,
    drafts: &mut Vec<ImportedConnectionDraft>,
    errors: &mut Vec<ConnectionImportErrorInfo>,
    credentials: &mut HashMap<String, Credential>,
) {
    let mut key_files = Vec::new();
    let mut records = Vec::new();
    for path in paths {
        let text = match read_selected_file(Path::new(path)) {
            Ok(text) => text,
            Err(error) => {
                errors.push(ConnectionImportErrorInfo {
                    source_path: path.clone(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        if text.lines().next().is_some_and(|line| {
            line.starts_with("-----BEGIN ") && line.ends_with("PRIVATE KEY-----")
        }) {
            key_files.push(PathBuf::from(path));
            continue;
        }

        for (line_number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            match parse_record(path, line_number + 1, line, selected_ids) {
                Some(record) => records.push(record),
                None => errors.push(ConnectionImportErrorInfo {
                    source_path: path.clone(),
                    // Never include source text or parsed field values in errors.
                    message: format!(
                        "Invalid or unsupported XTerminal SSH row {}",
                        line_number + 1
                    ),
                }),
            }
        }
    }

    let single_key_connection = records
        .iter()
        .filter(|record| record.draft.auth_type == ImportedConnectionAuthType::Key)
        .count()
        == 1;
    for Record { mut draft, secret } in records {
        let selected = selected_ids.is_some_and(|ids| ids.contains(&draft.id));
        match draft.auth_type {
            ImportedConnectionAuthType::Password => {
                if let Some(password) = secret {
                    credentials.insert(draft.id.clone(), Credential::Password(password));
                }
            }
            ImportedConnectionAuthType::Key => {
                let selected_file = match draft.key_path.as_deref() {
                    Some(path) => {
                        // Exported paths can use the source operating system's separators.
                        let basename = path.rsplit(['/', '\\']).next().unwrap_or(path);
                        let mut matches = key_files
                            .iter()
                            .filter(|file| file.file_name().is_some_and(|name| name == basename));
                        matches.next().filter(|_| matches.next().is_none())
                    }
                    None if single_key_connection && key_files.len() == 1 => key_files.first(),
                    None => None,
                };
                if let Some(file) = selected_file {
                    draft.key_path = Some(file.to_string_lossy().into_owned());
                }
                if let Some(path) = draft.key_path.as_ref() {
                    if selected_file.is_none() {
                        draft.warnings.push("xterminal_key_path".into());
                    }
                    if selected {
                        credentials.insert(
                            draft.id.clone(),
                            Credential::Key {
                                path: path.clone(),
                                selected_file: selected_file.cloned(),
                                passphrase: secret,
                            },
                        );
                    }
                } else {
                    draft.importable = false;
                    draft.warnings.push("xterminal_key_missing".into());
                }
            }
            _ => unreachable!("XTerminal uses only password and private key drafts"),
        }
        drafts.push(draft);
    }
}

fn read_selected_file(path: &Path) -> Result<Zeroizing<String>, ConnectionImportError> {
    let read_error = || ConnectionImportError::Read {
        path: path.display().to_string(),
        message: "Unreadable or too large".to_owned(),
    };
    let metadata = path.metadata().map_err(|_| read_error())?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(read_error());
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(metadata.len() as usize + 1));
    File::open(path)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| read_error())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(read_error());
    }
    let content = std::str::from_utf8(&bytes).map_err(|_| ConnectionImportError::Parse {
        path: path.display().to_string(),
        message: "XTerminal files must be UTF-8".to_owned(),
    })?;
    Ok(Zeroizing::new(
        content.trim_start_matches('\u{feff}').to_owned(),
    ))
}

fn parse_record(
    path: &str,
    line_number: usize,
    line: &str,
    selected_ids: Option<&HashSet<String>>,
) -> Option<Record> {
    let mut fields = fields(line)?;
    let value = |key| fields.get(key).map(|value| value.as_str());
    let protocol = value("protocol").or_else(|| value("type"));
    if protocol.is_some_and(|value| !value.eq_ignore_ascii_case("ssh")) {
        return None;
    }
    let auth = value("authType").or_else(|| value("auth"));
    // Some other XTerminal protocols share the same host/user fields. Accept only SSH rows.
    let auth_type = match auth {
        Some("password" | "askEveryTime") => ImportedConnectionAuthType::Password,
        Some("privateKey" | "identity") => ImportedConnectionAuthType::Key,
        None if protocol.is_some() => ImportedConnectionAuthType::Password,
        _ => return None,
    };
    let host = value("host")?.trim();
    if host.is_empty() || host.chars().any(char::is_whitespace) || host.contains(['/', '@']) {
        return None;
    }
    let username = value("user").or_else(|| value("username"))?.trim();
    if username.is_empty() {
        return None;
    }
    let port = value("port").map_or(Some(22), |port| port.parse().ok())?;
    if port == 0 {
        return None;
    }
    let name = value("title")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(host);
    let key_path = (auth_type == ImportedConnectionAuthType::Key)
        .then(|| {
            value("privateKey").filter(|path| !path.is_empty() && !path.starts_with("-----BEGIN "))
        })
        .flatten()
        .map(str::to_owned);
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source: SOURCE,
        source_path: path.to_owned(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.to_owned(),
        auth_type,
        key_path,
        cert_path: None,
        proxy_chain: Vec::new(),
        tags: vec![SOURCE.tag().to_owned()],
        warnings: Vec::new(),
        unsupported_fields: Vec::new(),
        duplicate: false,
        importable: true,
    };
    draft.id = format!("{}:{line_number}:{port}", draft_id(&draft));
    let secret = if selected_ids.is_some_and(|ids| ids.contains(&draft.id)) {
        match auth {
            Some("askEveryTime") => None,
            _ if auth_type == ImportedConnectionAuthType::Key => fields.remove("passphrase"),
            _ => fields.remove("password").or_else(|| fields.remove("pass")),
        }
        .filter(|secret| !secret.is_empty())
        .map(SecretString::from)
    } else {
        None
    };
    Some(Record { draft, secret })
}

fn fields(mut line: &str) -> Option<HashMap<&str, Zeroizing<String>>> {
    let mut fields = HashMap::new();
    while !line.is_empty() {
        let (key, rest) = line.split_once('=')?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        let (value, remainder) = if let Some(quoted) = rest.strip_prefix('"') {
            let mut chars = quoted.char_indices().peekable();
            let end = loop {
                let (index, ch) = chars.next()?;
                match ch {
                    '"' => break index,
                    '\\' if chars
                        .peek()
                        .is_some_and(|(_, next)| matches!(next, '"' | '\\')) =>
                    {
                        chars.next();
                    }
                    _ => {}
                }
            };
            let remainder = &quoted[end + 1..];
            if remainder
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_whitespace())
            {
                return None;
            }
            // Reserve the full value before decoding so there is no freed secret copy.
            let mut value = Zeroizing::new(String::with_capacity(end));
            let mut chars = quoted[..end].chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\\' && chars.peek().is_some_and(|next| matches!(next, '"' | '\\')) {
                    value.push(chars.next()?);
                } else {
                    value.push(ch);
                }
            }
            (value, remainder)
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (Zeroizing::new(rest[..end].to_owned()), &rest[end..])
        };
        if fields.insert(key, value).is_some() {
            return None;
        }
        line = remainder.trim_start();
    }
    Some(fields)
}

#[cfg(test)]
mod tests {
    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::{Algorithm, PrivateKey, ssh_key::LineEnding};

    use super::*;

    fn preview(paths: &[String]) -> ConnectionImportPreview {
        preview_connection_import(SOURCE, paths, &HashSet::new()).unwrap()
    }

    fn request(
        paths: Vec<String>,
        selected_draft_ids: Vec<String>,
    ) -> ConnectionImportApplyRequest {
        ConnectionImportApplyRequest {
            source: SOURCE,
            paths,
            selected_draft_ids,
            duplicate_strategy: ConnectionImportDuplicateStrategy::Skip,
            target_group: None,
        }
    }

    #[test]
    fn selected_xterminal_passwords_are_protected_and_preview_never_contains_them() {
        let directory = tempfile::tempdir().unwrap();
        let export = directory.path().join("SSH.txt");
        let catalog = directory.path().join("connections.json");
        fs::write(
            &export,
            concat!(
                "\u{feff}host=dev.example user=deploy port=2201 authType=password password=\"synthetic = \\\"secret\\\"\" title=\"Dev server\"\r\n",
                "host=dev.example user=deploy port=2202 authType=password password=synthetic-unselected title=\"Dev server\"\r\n",
                "host=router.example user=admin port=23 type=telnet password=synthetic-router\r\n",
                "host=bad.example user=ops authType=password password=\"synthetic-invalid\r\n",
            ),
        )
        .unwrap();
        let paths = vec![export.display().to_string()];
        let mut store = ConnectionStore::load(catalog.clone()).unwrap();
        let preview = preview(&paths);
        assert_eq!(preview.errors.len(), 2);
        assert_eq!(preview.drafts.len(), 2);
        assert_ne!(preview.drafts[0].id, preview.drafts[1].id);
        assert_eq!(preview.drafts[0].name, "Dev server");
        assert_eq!(preview.drafts[0].port, 2201);
        assert!(!format!("{preview:?}").contains("synthetic"));
        assert!(
            !serde_json::to_string(&preview)
                .unwrap()
                .contains("synthetic")
        );
        apply_connection_import(
            &mut store,
            request(paths, vec![preview.drafts[0].id.clone()]),
        )
        .unwrap();
        let [connection] = store.connections() else {
            panic!("only the selected row should be imported");
        };
        assert_eq!(connection.host, "dev.example");
        assert_eq!(connection.port, 2201);
        assert_eq!(
            store.get_connection_password(&connection.id).unwrap(),
            "synthetic = \"secret\""
        );
        let persisted = fs::read_to_string(catalog).unwrap();
        assert!(!persisted.contains("synthetic"));
        assert!(!persisted.contains("2202"));
    }

    #[test]
    fn xterminal_copies_explicit_key_files_but_does_not_read_exported_paths() {
        let directory = tempfile::tempdir().unwrap();
        let export = directory.path().join("SSH.txt");
        let key_file = directory.path().join("id_test");
        let catalog = directory.path().join("connections.json");
        let passphrase = "synthetic-key-passphrase";
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = key.encrypt(&mut rng, passphrase).unwrap();
        key.write_openssh_file(&key_file, LineEnding::LF).unwrap();
        let expected_key = read_selected_file(&key_file).unwrap();
        fs::write(
            &export,
            format!(
                "host=key.example user=deploy authType=privateKey privateKey=\"{}\" passphrase=\"{passphrase}\"",
                key_file.display()
            ),
        )
        .unwrap();
        let mut store = ConnectionStore::load(catalog.clone()).unwrap();
        let paths = vec![export.display().to_string()];
        let linked = preview(&paths);
        assert_eq!(linked.drafts[0].warnings, ["xterminal_key_path"]);
        apply_connection_import(
            &mut store,
            request(paths, vec![linked.drafts[0].id.clone()]),
        )
        .unwrap();
        assert!(
            matches!(store.connections()[0].auth, SavedAuth::Key { .. })
                && store.managed_ssh_keys().is_empty()
        );

        fs::write(
            &export,
            format!(
                r#"host=managed.example user=ops authType=privateKey privateKey="C:\old\id_test" passphrase="{passphrase}"
host=unresolved.example user=ops authType=identity identityId=source-only"#
            ),
        )
        .unwrap();
        let mut paths = vec![export.display().to_string()];
        let missing = preview(&paths);
        assert!(!missing.drafts[1].importable);
        assert_eq!(missing.drafts[1].warnings, ["xterminal_key_missing"]);
        paths.push(key_file.display().to_string());
        let paired = preview(&paths);
        assert!(paired.drafts[0].importable);
        assert_eq!(paired.drafts[0].key_path.as_deref(), key_file.to_str());
        assert!(!paired.drafts[1].importable);
        assert!(!format!("{paired:?}").contains(passphrase));
        apply_connection_import(
            &mut store,
            request(paths, vec![paired.drafts[0].id.clone()]),
        )
        .unwrap();
        let connection = store
            .connections()
            .iter()
            .find(|row| row.host == "managed.example")
            .unwrap();
        let SavedAuth::ManagedKey { key_id, .. } = &connection.auth else {
            panic!("a selected key file should become a protected managed key");
        };
        assert_eq!(
            store.resolve_managed_ssh_key_private_key(key_id).unwrap(),
            expected_key.as_str()
        );
        assert_eq!(
            store
                .get_connection_passphrase(&connection.id)
                .unwrap()
                .as_ref()
                .map(SecretString::expose_secret),
            Some(passphrase)
        );

        fs::write(&export, "host=locked.example user=ops authType=privateKey").unwrap();
        let paths = vec![export.display().to_string(), key_file.display().to_string()];
        let locked = preview(&paths);
        apply_connection_import(
            &mut store,
            request(paths, vec![locked.drafts[0].id.clone()]),
        )
        .unwrap();
        let locked = store
            .connections()
            .iter()
            .find(|row| row.host == "locked.example")
            .unwrap();
        let SavedAuth::ManagedKey { key_id, .. } = &locked.auth else {
            panic!("an encrypted OpenSSH key can be copied without unlocking it");
        };
        fs::remove_file(key_file).unwrap();
        assert_eq!(
            store.resolve_managed_ssh_key_private_key(key_id).unwrap(),
            expected_key.as_str()
        );
        assert!(
            store
                .managed_ssh_key_metadata(key_id)
                .unwrap()
                .requires_passphrase
        );
        assert!(
            store
                .get_connection_passphrase(&locked.id)
                .unwrap()
                .is_none()
        );
        let persisted = fs::read_to_string(catalog).unwrap();
        assert!(!persisted.contains(passphrase));
        assert!(!persisted.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(!persisted.contains("source-only"));
    }
}
