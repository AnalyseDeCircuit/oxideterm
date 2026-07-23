use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
};

use chrono::Utc;
use encoding_rs::Encoding;
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    CONFIG_VERSION, ConnectionOptions, ConnectionStore, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyPolicy,
};

const DEFAULT_IMPORTED_GROUP: &str = "Imported";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionImportSource {
    #[serde(rename = "securecrt")]
    SecureCrt,
    Xshell,
    Termius,
    #[serde(rename = "mobaxterm")]
    MobaXterm,
    #[serde(rename = "windterm")]
    WindTerm,
    Electerm,
    FinalShell,
}

impl ConnectionImportSource {
    pub fn tag(self) -> &'static str {
        match self {
            Self::SecureCrt => "securecrt",
            Self::Xshell => "xshell",
            Self::Termius => "termius",
            Self::MobaXterm => "mobaxterm",
            Self::WindTerm => "windterm",
            Self::Electerm => "electerm",
            Self::FinalShell => "finalshell",
        }
    }

    pub fn default_group(self) -> &'static str {
        match self {
            Self::SecureCrt => "Imported/SecureCRT",
            Self::Xshell => "Imported/Xshell",
            Self::Termius => "Imported/Termius",
            Self::MobaXterm => "Imported/MobaXterm",
            Self::WindTerm => "Imported/WindTerm",
            Self::Electerm => "Imported/Electerm",
            Self::FinalShell => "Imported/FinalShell",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionImportDuplicateStrategy {
    Skip,
    Rename,
}

impl ConnectionImportDuplicateStrategy {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Rename => "rename",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedConnectionAuthType {
    Password,
    Key,
    Certificate,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProxyHopDraft {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: ImportedConnectionAuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedConnectionDraft {
    pub id: String,
    pub source: ConnectionImportSource,
    pub source_path: String,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: ImportedConnectionAuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    #[serde(default)]
    pub proxy_chain: Vec<ImportedProxyHopDraft>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub unsupported_fields: Vec<String>,
    #[serde(default)]
    pub duplicate: bool,
    #[serde(default)]
    pub importable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportPreview {
    pub source: ConnectionImportSource,
    pub total: usize,
    pub importable: usize,
    pub duplicates: usize,
    pub warnings: usize,
    pub errors: Vec<ConnectionImportErrorInfo>,
    pub drafts: Vec<ImportedConnectionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportErrorInfo {
    pub source_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportApplyRequest {
    pub source: ConnectionImportSource,
    pub paths: Vec<String>,
    pub selected_draft_ids: Vec<String>,
    pub duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub target_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportApplyResult {
    pub imported: usize,
    pub skipped: usize,
    pub renamed: usize,
    pub errors: Vec<ConnectionImportErrorInfo>,
}

#[derive(Debug, Error)]
pub enum ConnectionImportError {
    #[error("No import paths were provided")]
    EmptyPaths,
    #[error("Unsupported or unreadable import path: {0}")]
    InvalidPath(String),
    #[error("Failed to read {path}: {message}")]
    Read { path: String, message: String },
    #[error("Failed to parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error(transparent)]
    Store(#[from] anyhow::Error),
}

pub fn preview_connection_import(
    source: ConnectionImportSource,
    paths: &[String],
    existing_names: &HashSet<String>,
) -> Result<ConnectionImportPreview, ConnectionImportError> {
    if paths.is_empty() {
        return Err(ConnectionImportError::EmptyPaths);
    }

    let mut drafts = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        match parse_import_path(source, Path::new(path)) {
            Ok(mut parsed) => drafts.append(&mut parsed),
            Err(error) => errors.push(ConnectionImportErrorInfo {
                source_path: path.clone(),
                message: error.to_string(),
            }),
        }
    }

    for draft in &mut drafts {
        draft.duplicate = existing_names.contains(&draft.name);
        draft.importable = !draft.host.trim().is_empty() && draft.port > 0;
    }

    Ok(ConnectionImportPreview {
        source,
        total: drafts.len(),
        importable: drafts.iter().filter(|draft| draft.importable).count(),
        duplicates: drafts.iter().filter(|draft| draft.duplicate).count(),
        warnings: drafts.iter().map(|draft| draft.warnings.len()).sum(),
        errors,
        drafts,
    })
}

pub fn apply_connection_import(
    store: &mut ConnectionStore,
    request: ConnectionImportApplyRequest,
) -> Result<ConnectionImportApplyResult, ConnectionImportError> {
    let mut existing_names = store
        .connections()
        .iter()
        .map(|connection| connection.name.clone())
        .collect::<HashSet<_>>();
    let selected_ids = request
        .selected_draft_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let preview = preview_connection_import(request.source, &request.paths, &existing_names)?;
    let mut skipped = 0usize;
    let mut renamed = 0usize;
    let mut errors = preview.errors;
    let mut connections = Vec::new();

    for draft in preview.drafts {
        if !selected_ids.contains(&draft.id) {
            continue;
        }
        if !draft.importable {
            errors.push(ConnectionImportErrorInfo {
                source_path: draft.source_path.clone(),
                message: "Connection draft is not importable".to_string(),
            });
            continue;
        }

        let mut name = draft.name.clone();
        if existing_names.contains(&name) {
            match request.duplicate_strategy {
                ConnectionImportDuplicateStrategy::Skip => {
                    skipped += 1;
                    continue;
                }
                ConnectionImportDuplicateStrategy::Rename => {
                    name = unique_import_name(&name, &existing_names);
                    renamed += 1;
                }
            }
        }

        let group = normalized_import_group(
            request.target_group.as_ref(),
            draft.group.as_ref(),
            draft.source,
        );
        existing_names.insert(name.clone());
        connections.push(imported_draft_to_saved_connection(&draft, name, group));
    }

    let imported = connections.len();
    if imported > 0 {
        store.upsert_imported_connections_transaction(connections)?;
    }

    Ok(ConnectionImportApplyResult {
        imported,
        skipped,
        renamed,
        errors,
    })
}

fn parse_import_path(
    source: ConnectionImportSource,
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    match source {
        ConnectionImportSource::SecureCrt => parse_securecrt_path(path),
        ConnectionImportSource::Xshell => parse_xshell_path(path),
        ConnectionImportSource::Termius => parse_termius_path(path),
        ConnectionImportSource::MobaXterm => parse_mobaxterm_path(path),
        ConnectionImportSource::WindTerm => parse_windterm_path(path),
        ConnectionImportSource::Electerm => parse_electerm_path(path),
        ConnectionImportSource::FinalShell => parse_finalshell_path(path),
    }
}

fn parse_securecrt_path(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return parse_directory(path, |file, root| parse_securecrt_file(file, Some(root)));
    }
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
    {
        return parse_securecrt_xml_file(path);
    }
    parse_securecrt_file(path, None).map(|draft| vec![draft])
}

fn parse_xshell_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return parse_directory(path, |file, root| parse_xshell_file(file, Some(root)));
    }
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xts"))
    {
        return parse_xshell_archive(path);
    }
    parse_xshell_file(path, None).map(|draft| vec![draft])
}

fn parse_termius_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    parse_termius_file(path)
}

fn parse_mobaxterm_path(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    parse_mobaxterm_file(path)
}

fn parse_windterm_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    parse_windterm_file(path)
}

fn parse_electerm_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    parse_electerm_file(path)
}

fn parse_finalshell_path(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if !path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    let conn_root = if path.join("conn").is_dir() {
        path.join("conn")
    } else {
        path.to_path_buf()
    };
    parse_finalshell_directory(&conn_root)
}

fn parse_directory<F>(
    root: &Path,
    mut parse_file: F,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError>
where
    F: FnMut(&Path, &Path) -> Result<ImportedConnectionDraft, ConnectionImportError>,
{
    let mut drafts = Vec::new();
    visit_files(root, &mut |path| match parse_file(path, root) {
        Ok(draft) => {
            drafts.push(draft);
            Ok(())
        }
        Err(ConnectionImportError::Parse { .. }) => Ok(()),
        Err(error) => Err(error),
    })?;
    Ok(drafts)
}

fn visit_files<F>(root: &Path, visit: &mut F) -> Result<(), ConnectionImportError>
where
    F: FnMut(&Path) -> Result<(), ConnectionImportError>,
{
    for entry in fs::read_dir(root).map_err(|error| ConnectionImportError::Read {
        path: root.display().to_string(),
        message: error.to_string(),
    })? {
        let entry = entry.map_err(|error| ConnectionImportError::Read {
            path: root.display().to_string(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        if path.is_dir() {
            visit_files(&path, visit)?;
        } else if path.is_file() {
            visit(&path)?;
        }
    }
    Ok(())
}

fn read_text_file(path: &Path) -> Result<String, ConnectionImportError> {
    let bytes = fs::read(path).map_err(|error| ConnectionImportError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    Ok(decode_import_text(&bytes))
}

fn read_sensitive_text_file(path: &Path) -> Result<Zeroizing<String>, ConnectionImportError> {
    let bytes = Zeroizing::new(fs::read(path).map_err(|error| ConnectionImportError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?);
    // The decoded source may contain third-party credentials and is wiped after parsing.
    Ok(Zeroizing::new(decode_import_text(bytes.as_slice())))
}

fn decode_import_text(bytes: &[u8]) -> String {
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (decoded, _, _) = encoding.decode(&bytes[bom_len..]);
        return decoded.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_string(),
        Err(_) => {
            // Several Windows terminal tools export Chinese paths or sessions as GBK.
            let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
            decoded.into_owned()
        }
    }
}

#[derive(Default)]
struct IgnoredSensitiveField;

impl<'de> Deserialize<'de> for IgnoredSensitiveField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Consume secret-bearing values without allocating a second owned copy.
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

#[derive(Default)]
struct CollectionPresence(bool);

impl<'de> Deserialize<'de> for CollectionPresence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PresenceVisitor;

        impl<'de> serde::de::Visitor<'de> for PresenceVisitor {
            type Value = CollectionPresence;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an array or object")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut present = false;
                while sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    present = true;
                }
                Ok(CollectionPresence(present))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut present = false;
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {
                    present = true;
                }
                Ok(CollectionPresence(present))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(CollectionPresence(false))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(CollectionPresence(false))
            }
        }

        deserializer.deserialize_any(PresenceVisitor)
    }
}

fn deserialize_optional_port<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct PortVisitor;

    impl<'de> serde::de::Visitor<'de> for PortVisitor {
        type Value = Option<u16>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a valid TCP port as a number or string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(u16::try_from(value).ok().filter(|port| *port > 0))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(u16::try_from(value).ok().filter(|port| *port > 0))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.parse::<u16>().ok().filter(|port| *port > 0))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(PortVisitor)
}

#[derive(Default)]
struct SecureCrtXmlFrame {
    name: String,
    fields: BTreeMap<String, String>,
    warnings: Vec<String>,
    unsupported_fields: Vec<String>,
}

enum SecureCrtXmlCapture {
    Safe(String),
    Secret(String),
    Proxy(String),
    Ignored,
}

fn parse_securecrt_xml_file(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_sensitive_text_file(path)?;
    let mut reader = Reader::from_str(content.as_str());
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut stack = Vec::<SecureCrtXmlFrame>::new();
    let mut capture = None;
    let mut captured_value = String::new();
    let mut drafts = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| ConnectionImportError::Parse {
                path: path.display().to_string(),
                message: format!("Invalid SecureCRT XML: {error}"),
            })? {
            Event::Start(event) if event.name().as_ref() == b"key" => {
                stack.push(SecureCrtXmlFrame {
                    name: securecrt_xml_name(&reader, &event)?.unwrap_or_default(),
                    ..SecureCrtXmlFrame::default()
                });
            }
            Event::Start(event) if is_securecrt_xml_value(event.name().as_ref()) => {
                capture = securecrt_xml_name(&reader, &event)?.map(classify_securecrt_xml_field);
                captured_value.clear();
            }
            Event::Empty(event) if is_securecrt_xml_value(event.name().as_ref()) => {
                if let (Some(frame), Some(field)) =
                    (stack.last_mut(), securecrt_xml_name(&reader, &event)?)
                {
                    finish_securecrt_xml_field(frame, classify_securecrt_xml_field(field), "");
                }
            }
            Event::Text(event) => {
                if matches!(capture, Some(SecureCrtXmlCapture::Safe(_))) {
                    let value = reader.decoder().decode(event.as_ref()).map_err(|error| {
                        ConnectionImportError::Parse {
                            path: path.display().to_string(),
                            message: format!("Invalid SecureCRT XML text: {error}"),
                        }
                    })?;
                    let value = quick_xml::escape::unescape(&value).map_err(|error| {
                        ConnectionImportError::Parse {
                            path: path.display().to_string(),
                            message: format!("Invalid SecureCRT XML entity: {error}"),
                        }
                    })?;
                    captured_value.push_str(&value);
                }
            }
            Event::CData(event) => {
                if matches!(capture, Some(SecureCrtXmlCapture::Safe(_))) {
                    let value = reader.decoder().decode(event.as_ref()).map_err(|error| {
                        ConnectionImportError::Parse {
                            path: path.display().to_string(),
                            message: format!("Invalid SecureCRT XML CDATA: {error}"),
                        }
                    })?;
                    captured_value.push_str(&value);
                }
            }
            Event::End(event) if is_securecrt_xml_value(event.name().as_ref()) => {
                if let (Some(frame), Some(field)) = (stack.last_mut(), capture.take()) {
                    finish_securecrt_xml_field(frame, field, captured_value.trim());
                }
                captured_value.clear();
            }
            Event::End(event) if event.name().as_ref() == b"key" => {
                if let Some(frame) = stack.pop()
                    && let Some(draft) = securecrt_xml_frame_to_draft(path, frame, &stack)
                {
                    drafts.push(draft);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No SSH sessions found in SecureCRT XML export".to_string(),
        });
    }
    Ok(drafts)
}

fn is_securecrt_xml_value(name: &[u8]) -> bool {
    matches!(name, b"string" | b"dword")
}

fn securecrt_xml_name(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
) -> Result<Option<String>, ConnectionImportError> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| ConnectionImportError::Parse {
            path: "SecureCRT XML".to_string(),
            message: format!("Invalid XML attribute: {error}"),
        })?;
        if attribute.key.as_ref() == b"name" {
            let value = attribute
                .decode_and_unescape_value(reader.decoder())
                .map_err(|error| ConnectionImportError::Parse {
                    path: "SecureCRT XML".to_string(),
                    message: format!("Invalid XML attribute value: {error}"),
                })?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn classify_securecrt_xml_field(name: String) -> SecureCrtXmlCapture {
    let normalized = normalize_key(&name);
    if looks_like_secret_key(&normalized) {
        SecureCrtXmlCapture::Secret(name)
    } else if looks_like_proxy_key(&normalized) {
        SecureCrtXmlCapture::Proxy(name)
    } else if is_imported_connection_field(&normalized) {
        SecureCrtXmlCapture::Safe(name)
    } else {
        // Unrelated XML settings are skipped so embedded scripts never become owned strings.
        SecureCrtXmlCapture::Ignored
    }
}

fn finish_securecrt_xml_field(
    frame: &mut SecureCrtXmlFrame,
    field: SecureCrtXmlCapture,
    value: &str,
) {
    match field {
        SecureCrtXmlCapture::Safe(name) => {
            frame.fields.insert(normalize_key(&name), value.to_string());
        }
        SecureCrtXmlCapture::Secret(name) => {
            frame.warnings.push("Password was not imported".to_string());
            frame.unsupported_fields.push(name);
        }
        SecureCrtXmlCapture::Proxy(name) => {
            frame
                .warnings
                .push("Proxy/jump setting was not imported".to_string());
            frame.unsupported_fields.push(name);
        }
        SecureCrtXmlCapture::Ignored => {}
    }
}

fn securecrt_xml_frame_to_draft(
    path: &Path,
    frame: SecureCrtXmlFrame,
    ancestors: &[SecureCrtXmlFrame],
) -> Option<ImportedConnectionDraft> {
    let sessions_index = ancestors
        .iter()
        .position(|ancestor| ancestor.name == "Sessions")?;
    let protocol = pick_field(&frame.fields, &["protocolname", "protocol"])?;
    if !protocol.eq_ignore_ascii_case("ssh2") && !protocol.eq_ignore_ascii_case("ssh") {
        return None;
    }
    let host = pick_field(&frame.fields, &["hostname", "host"])?;
    if host.trim().is_empty() {
        return None;
    }
    let username = pick_field(&frame.fields, &["username", "user"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(whoami::username);
    let port = pick_field(&frame.fields, &["ssh2port", "port"])
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
        .unwrap_or(22);
    let name = if frame.name.trim().is_empty() {
        host.clone()
    } else {
        frame.name.trim().to_string()
    };
    let group_segments = ancestors[sessions_index + 1..]
        .iter()
        .map(|ancestor| ancestor.name.as_str())
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    let group = group_from_segments(group_segments.iter().copied())
        .or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string()));
    let key_path = pick_field(
        &frame.fields,
        &[
            "identityfilename",
            "identityfilenamev2",
            "keyfile",
            "keypath",
        ],
    );
    let cert_path = pick_field(&frame.fields, &["certificatefile", "certificatepath"]);
    let auth_type = if cert_path.is_some() {
        ImportedConnectionAuthType::Certificate
    } else if key_path.is_some() {
        ImportedConnectionAuthType::Key
    } else {
        ImportedConnectionAuthType::Password
    };
    let virtual_name = group_segments
        .iter()
        .copied()
        .chain(std::iter::once(name.as_str()))
        .collect::<Vec<_>>()
        .join("/");
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source: ConnectionImportSource::SecureCrt,
        source_path: format!("{}:{virtual_name}", path.display()),
        name,
        group,
        host,
        port,
        username,
        auth_type,
        key_path,
        cert_path,
        proxy_chain: Vec::new(),
        tags: vec![ConnectionImportSource::SecureCrt.tag().to_string()],
        warnings: dedupe(frame.warnings),
        unsupported_fields: dedupe(frame.unsupported_fields),
        duplicate: false,
        importable: true,
    };
    draft.id = draft_id(&draft);
    Some(draft)
}

fn parse_securecrt_file(
    path: &Path,
    root: Option<&Path>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let content = read_sensitive_text_file(path)?;
    let mut fields = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();

    for line in content.lines() {
        let Some((key, raw_value)) = parse_securecrt_setting(line) else {
            continue;
        };
        let normalized = normalize_key(&key);
        if looks_like_secret_key(&normalized) {
            warnings.push("Password was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        if looks_like_proxy_key(&normalized) {
            warnings.push("Proxy/jump setting was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        if is_imported_connection_field(&normalized) {
            fields.insert(normalized, unquote(raw_value));
        }
    }

    draft_from_fields(
        ConnectionImportSource::SecureCrt,
        path,
        root,
        fields,
        warnings,
        unsupported_fields,
    )
}

fn parse_xshell_file(
    path: &Path,
    root: Option<&Path>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let content = read_text_file(path)?;
    parse_xshell_content(&content, path, root)
}

fn parse_xshell_content(
    content: &str,
    path: &Path,
    root: Option<&Path>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let mut fields = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();

    for line in content.lines() {
        let Some((key, value)) = parse_plain_setting(line) else {
            continue;
        };
        let normalized = normalize_key(&key);
        if looks_like_secret_key(&normalized) {
            warnings.push("Password was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        if looks_like_proxy_key(&normalized) {
            warnings.push("Proxy/jump setting was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        fields.insert(normalized, value);
    }

    if let Some(protocol) = pick_field(&fields, &["protocol"]) {
        if !protocol.eq_ignore_ascii_case("ssh") {
            return Err(ConnectionImportError::Parse {
                path: path.display().to_string(),
                message: format!("Unsupported Xshell protocol: {protocol}"),
            });
        }
    }

    draft_from_fields(
        ConnectionImportSource::Xshell,
        path,
        root,
        fields,
        warnings,
        unsupported_fields,
    )
}

fn parse_xshell_archive(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let file = fs::File::open(path).map_err(|error| ConnectionImportError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| ConnectionImportError::Parse {
        path: path.display().to_string(),
        message: format!("Invalid Xshell archive: {error}"),
    })?;

    let mut drafts = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ConnectionImportError::Parse {
                path: path.display().to_string(),
                message: format!("Failed to read Xshell archive entry: {error}"),
            })?;
        if entry.is_dir() {
            continue;
        }

        let entry_path = decode_import_text(entry.name_raw()).replace('\\', "/");
        if !entry_path.to_ascii_lowercase().ends_with(".xsh") {
            continue;
        }

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ConnectionImportError::Read {
                path: format!("{}:{entry_path}", path.display()),
                message: error.to_string(),
            })?;
        let content = decode_import_text(&bytes);
        let virtual_path = Path::new(&entry_path);
        match parse_xshell_content(&content, virtual_path, Some(Path::new("Xshell/Sessions"))) {
            Ok(mut draft) => {
                draft.source_path = format!("{}:{entry_path}", path.display());
                draft.group = xshell_archive_group(&entry_path)
                    .or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string()));
                draft.id = draft_id(&draft);
                drafts.push(draft);
            }
            Err(ConnectionImportError::Parse { .. }) => {}
            Err(error) => return Err(error),
        }
    }

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No SSH sessions found in Xshell archive".to_string(),
        });
    }
    Ok(drafts)
}

fn parse_termius_file(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_text_file(path)?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|error| ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;

    let mut drafts = Vec::new();
    collect_termius_drafts(path, &value, None, &mut drafts);
    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No hosts found in Termius export".to_string(),
        });
    }
    Ok(drafts)
}

fn parse_mobaxterm_file(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_text_file(path)?;
    let sections = parse_ini_sections(&content);
    let mut drafts = Vec::new();

    for (section_name, entries) in sections {
        if !section_name.starts_with("Bookmarks") {
            continue;
        }
        let group = entries
            .get("SubRep")
            .and_then(|value| group_from_segments(value.split('\\')));
        let mut group_warnings = Vec::new();
        let mut group_unsupported_fields = Vec::new();
        if entries.contains_key("ImgNum") {
            group_warnings.push("MobaXterm icon number was not imported".to_string());
            group_unsupported_fields.push("ImgNum".to_string());
        }

        for (name, value) in entries {
            if name == "SubRep" || name == "ImgNum" {
                continue;
            }
            if let Some(mut draft) = parse_mobaxterm_entry(
                path,
                &section_name,
                &name,
                &value,
                group.clone(),
                group_warnings.clone(),
                group_unsupported_fields.clone(),
            ) {
                draft.id = draft_id(&draft);
                drafts.push(draft);
            }
        }
    }

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No SSH bookmarks found in MobaXterm export".to_string(),
        });
    }
    Ok(drafts)
}

fn parse_mobaxterm_entry(
    path: &Path,
    section_name: &str,
    name: &str,
    value: &str,
    group: Option<String>,
    warnings: Vec<String>,
    unsupported_fields: Vec<String>,
) -> Option<ImportedConnectionDraft> {
    let body = value.trim().strip_prefix('#')?;
    let (type_marker, rest) = body.split_once('%')?;
    if !type_marker.starts_with("109") {
        return None;
    }
    let fields = rest.split('%').collect::<Vec<_>>();
    if fields.len() < 3 {
        return None;
    }
    let host = fields[0].trim();
    if host.is_empty() {
        return None;
    }
    let port = fields[1].trim().parse::<u16>().unwrap_or(22);
    let username = fields
        .get(2)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("root");

    Some(ImportedConnectionDraft {
        id: String::new(),
        source: ConnectionImportSource::MobaXterm,
        source_path: format!("{}:{section_name}/{name}", path.display()),
        name: name.to_string(),
        group: group.or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
        host: host.to_string(),
        port,
        username: username.to_string(),
        auth_type: ImportedConnectionAuthType::Password,
        key_path: None,
        cert_path: None,
        proxy_chain: Vec::new(),
        tags: vec![ConnectionImportSource::MobaXterm.tag().to_string()],
        warnings: dedupe(warnings),
        unsupported_fields: dedupe(unsupported_fields),
        duplicate: false,
        importable: true,
    })
}

fn parse_windterm_file(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_text_file(path)?;
    let values: Vec<serde_json::Value> =
        serde_json::from_str(&content).map_err(|error| ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
    let mut drafts = Vec::new();

    for value in values {
        let serde_json::Value::Object(map) = value else {
            continue;
        };
        let protocol = pick_json_string(&map, &["session.protocol"]).unwrap_or_default();
        if !protocol.eq_ignore_ascii_case("ssh") {
            continue;
        }
        let target = pick_json_string(&map, &["session.target"]).unwrap_or_default();
        let (host, username) = parse_windterm_target(&target);
        if host.is_empty() {
            continue;
        }
        let name = pick_json_string(&map, &["session.label"]).unwrap_or_else(|| host.clone());
        let port = pick_json_u16(&map, &["session.port"]).unwrap_or(22);
        let group = pick_json_string(&map, &["session.group"])
            .and_then(|value| group_from_segments(value.split('>')));

        let mut unsupported_fields = Vec::new();
        collect_secret_json_keys(&map, &mut unsupported_fields);
        let warnings = if unsupported_fields.is_empty() {
            Vec::new()
        } else {
            vec!["Password was not imported".to_string()]
        };
        let mut draft = ImportedConnectionDraft {
            id: String::new(),
            source: ConnectionImportSource::WindTerm,
            source_path: path.display().to_string(),
            name,
            group: group.or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
            host,
            port,
            username,
            auth_type: ImportedConnectionAuthType::Password,
            key_path: None,
            cert_path: None,
            proxy_chain: Vec::new(),
            tags: vec![ConnectionImportSource::WindTerm.tag().to_string()],
            warnings: dedupe(warnings),
            unsupported_fields: dedupe(unsupported_fields),
            duplicate: false,
            importable: true,
        };
        draft.id = draft_id(&draft);
        drafts.push(draft);
    }

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No SSH sessions found in WindTerm export".to_string(),
        });
    }
    Ok(drafts)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElectermBookmarksFile {
    #[serde(default)]
    bookmark_groups: Vec<ElectermBookmarkGroup>,
    #[serde(default)]
    bookmarks: Vec<ElectermBookmark>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElectermBookmarkGroup {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    bookmark_ids: Vec<String>,
    #[serde(default)]
    bookmark_group_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElectermBookmark {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    auth_type: String,
    #[serde(default, deserialize_with = "deserialize_optional_port")]
    port: Option<u16>,
    #[serde(default, rename = "type")]
    session_type: String,
    #[serde(default)]
    enable_ssh: Option<bool>,
    #[serde(default)]
    use_ssh_agent: bool,
    #[serde(default)]
    password: Option<IgnoredSensitiveField>,
    #[serde(default)]
    passphrase: Option<IgnoredSensitiveField>,
    #[serde(default)]
    private_key: Option<IgnoredSensitiveField>,
    #[serde(default)]
    certificate: Option<IgnoredSensitiveField>,
    #[serde(default)]
    proxy: Option<IgnoredSensitiveField>,
    #[serde(default)]
    ssh_tunnels: CollectionPresence,
    #[serde(default)]
    connection_hoppings: CollectionPresence,
}

fn parse_electerm_file(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_sensitive_text_file(path)?;
    let trimmed = content.trim_start();
    let file = if trimmed.starts_with('[') {
        ElectermBookmarksFile {
            bookmark_groups: Vec::new(),
            bookmarks: serde_json::from_str(trimmed).map_err(|error| {
                ConnectionImportError::Parse {
                    path: path.display().to_string(),
                    message: format!("Invalid Electerm bookmarks JSON: {error}"),
                }
            })?,
        }
    } else {
        serde_json::from_str(trimmed).map_err(|error| ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: format!("Invalid Electerm bookmarks JSON: {error}"),
        })?
    };
    let groups_by_id = file
        .bookmark_groups
        .into_iter()
        .map(|group| (group.id.clone(), group))
        .collect::<HashMap<_, _>>();
    let bookmark_groups = electerm_bookmark_group_map(&groups_by_id);
    let mut drafts = file
        .bookmarks
        .into_iter()
        .filter_map(|bookmark| {
            electerm_bookmark_to_draft(path, bookmark, &groups_by_id, &bookmark_groups)
        })
        .collect::<Vec<_>>();

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No SSH bookmarks found in Electerm export".to_string(),
        });
    }
    drafts.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    Ok(drafts)
}

fn electerm_bookmark_group_map(
    groups: &HashMap<String, ElectermBookmarkGroup>,
) -> HashMap<String, String> {
    let mut bookmark_groups = HashMap::new();
    for (group_id, group) in groups {
        for bookmark_id in &group.bookmark_ids {
            bookmark_groups
                .entry(bookmark_id.clone())
                .or_insert_with(|| group_id.clone());
        }
    }
    bookmark_groups
}

fn electerm_group_segments(
    group_id: &str,
    groups: &HashMap<String, ElectermBookmarkGroup>,
) -> Option<Vec<String>> {
    let mut child_to_parent = HashMap::<&str, &str>::new();
    for (parent_id, group) in groups {
        for child_id in &group.bookmark_group_ids {
            child_to_parent
                .entry(child_id.as_str())
                .or_insert(parent_id.as_str());
        }
    }

    let mut segments = Vec::new();
    let mut visited = HashSet::new();
    let mut current = group_id;
    while visited.insert(current) {
        let group = groups.get(current)?;
        if !group.title.trim().is_empty() {
            segments.push(group.title.trim().to_string());
        }
        let Some(parent_id) = child_to_parent.get(current) else {
            break;
        };
        current = parent_id;
    }
    segments.reverse();
    (!segments.is_empty()).then_some(segments)
}

fn electerm_bookmark_to_draft(
    path: &Path,
    bookmark: ElectermBookmark,
    groups: &HashMap<String, ElectermBookmarkGroup>,
    bookmark_groups: &HashMap<String, String>,
) -> Option<ImportedConnectionDraft> {
    if !bookmark.session_type.eq_ignore_ascii_case("ssh") || bookmark.enable_ssh == Some(false) {
        return None;
    }
    let host = bookmark.host.trim().to_string();
    if host.is_empty() {
        return None;
    }
    let name = if bookmark.title.trim().is_empty() {
        host.clone()
    } else {
        bookmark.title.trim().to_string()
    };
    let username = if bookmark.username.trim().is_empty() {
        whoami::username()
    } else {
        bookmark.username.trim().to_string()
    };
    let group_segments = bookmark_groups
        .get(&bookmark.id)
        .and_then(|group_id| electerm_group_segments(group_id, groups));
    let group = group_segments
        .as_ref()
        .and_then(|segments| group_from_segments(segments.iter().map(String::as_str)))
        .or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string()));
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();

    if bookmark.password.is_some() || bookmark.passphrase.is_some() {
        warnings.push("Password was not imported".to_string());
    }
    if bookmark.password.is_some() {
        unsupported_fields.push("password".to_string());
    }
    if bookmark.passphrase.is_some() {
        unsupported_fields.push("passphrase".to_string());
    }
    if bookmark.private_key.is_some() || bookmark.certificate.is_some() {
        warnings.push("Private key material was not imported".to_string());
    }
    if bookmark.private_key.is_some() {
        unsupported_fields.push("privateKey".to_string());
    }
    if bookmark.certificate.is_some() {
        unsupported_fields.push("certificate".to_string());
    }
    if bookmark.proxy.is_some() || bookmark.connection_hoppings.0 {
        warnings.push("Proxy/jump setting was not imported".to_string());
    }
    if bookmark.proxy.is_some() {
        unsupported_fields.push("proxy".to_string());
    }
    if bookmark.connection_hoppings.0 {
        unsupported_fields.push("connectionHoppings".to_string());
    }
    if bookmark.ssh_tunnels.0 {
        warnings.push("Port forwarding settings were not imported".to_string());
        unsupported_fields.push("sshTunnels".to_string());
    }
    let auth_type = if bookmark.auth_type.eq_ignore_ascii_case("agent")
        || (bookmark.auth_type.trim().is_empty() && bookmark.use_ssh_agent)
    {
        ImportedConnectionAuthType::Agent
    } else {
        // Secret-backed Electerm auth falls back to OxideTerm's normal credential prompt.
        ImportedConnectionAuthType::Password
    };
    let source_suffix = if bookmark.id.is_empty() {
        name.clone()
    } else {
        bookmark.id
    };
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source: ConnectionImportSource::Electerm,
        source_path: format!("{}:{source_suffix}", path.display()),
        name,
        group,
        host,
        port: bookmark.port.unwrap_or(22),
        username,
        auth_type,
        key_path: None,
        cert_path: None,
        proxy_chain: Vec::new(),
        tags: vec![ConnectionImportSource::Electerm.tag().to_string()],
        warnings: dedupe(warnings),
        unsupported_fields: dedupe(unsupported_fields),
        duplicate: false,
        importable: true,
    };
    draft.id = draft_id(&draft);
    Some(draft)
}

#[derive(Deserialize)]
struct FinalShellFolder {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    delete_time: u64,
}

#[derive(Deserialize)]
struct FinalShellConnection {
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    #[serde(default, deserialize_with = "deserialize_optional_port")]
    port: Option<u16>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    conection_type: Option<i64>,
    #[serde(default)]
    delete_time: u64,
    #[serde(default)]
    password: Option<IgnoredSensitiveField>,
    #[serde(default)]
    secret_key_id: Option<String>,
    #[serde(default)]
    proxy_id: Option<String>,
    #[serde(default)]
    port_forwarding_list: CollectionPresence,
}

fn parse_finalshell_directory(
    root: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let mut files = Vec::new();
    visit_files(root, &mut |path| {
        files.push(path.to_path_buf());
        Ok(())
    })?;
    files.sort();

    let mut folders = HashMap::new();
    for path in files.iter().filter(|path| {
        path.file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("folder.json"))
    }) {
        let Ok(folder) = read_sensitive_json::<FinalShellFolder>(path) else {
            continue;
        };
        if folder.delete_time == 0 && !folder.id.trim().is_empty() {
            folders.insert(folder.id.clone(), folder);
        }
    }

    let mut drafts = Vec::new();
    for path in files.iter().filter(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_connect_config.json"))
    }) {
        let Ok(connection) = read_sensitive_json::<FinalShellConnection>(path) else {
            continue;
        };
        if let Some(draft) = finalshell_connection_to_draft(path, connection, &folders) {
            drafts.push(draft);
        }
    }

    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: root.display().to_string(),
            message: "No SSH connections found in FinalShell conn directory".to_string(),
        });
    }
    Ok(drafts)
}

fn read_sensitive_json<T>(path: &Path) -> Result<T, ConnectionImportError>
where
    T: for<'de> Deserialize<'de>,
{
    let content = read_sensitive_text_file(path)?;
    serde_json::from_str(content.as_str()).map_err(|error| ConnectionImportError::Parse {
        path: path.display().to_string(),
        message: format!("Invalid FinalShell JSON: {error}"),
    })
}

fn finalshell_group_segments(
    parent_id: &str,
    folders: &HashMap<String, FinalShellFolder>,
) -> Option<Vec<String>> {
    if parent_id.trim().is_empty() || matches!(parent_id, "root" | "0") {
        return None;
    }
    let mut segments = Vec::new();
    let mut visited = HashSet::new();
    let mut current = parent_id;
    while visited.insert(current) {
        let folder = folders.get(current)?;
        if !folder.name.trim().is_empty() {
            segments.push(folder.name.trim().to_string());
        }
        let Some(parent_id) = folder.parent_id.as_deref() else {
            break;
        };
        if parent_id.trim().is_empty() || matches!(parent_id, "root" | "0") {
            break;
        }
        current = parent_id;
    }
    segments.reverse();
    (!segments.is_empty()).then_some(segments)
}

fn finalshell_connection_to_draft(
    path: &Path,
    connection: FinalShellConnection,
    folders: &HashMap<String, FinalShellFolder>,
) -> Option<ImportedConnectionDraft> {
    if connection.delete_time != 0 || connection.conection_type != Some(100) {
        return None;
    }
    let host = connection.host.trim().to_string();
    if host.is_empty() {
        return None;
    }
    let name = if connection.name.trim().is_empty() {
        host.clone()
    } else {
        connection.name.trim().to_string()
    };
    let username = connection
        .user_name
        .as_deref()
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .map(str::to_string)
        .unwrap_or_else(whoami::username);
    let group_segments = connection
        .parent_id
        .as_deref()
        .and_then(|parent_id| finalshell_group_segments(parent_id, folders));
    let group = group_segments
        .as_ref()
        .and_then(|segments| group_from_segments(segments.iter().map(String::as_str)))
        .or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string()));
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();
    if connection.password.is_some() {
        warnings.push("Password was not imported".to_string());
        unsupported_fields.push("password".to_string());
    }
    if connection
        .secret_key_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        warnings.push("Private key reference was not imported".to_string());
        unsupported_fields.push("secret_key_id".to_string());
    }
    if connection
        .proxy_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && value != "0")
    {
        warnings.push("Proxy/jump setting was not imported".to_string());
        unsupported_fields.push("proxy_id".to_string());
    }
    if connection.port_forwarding_list.0 {
        warnings.push("Port forwarding settings were not imported".to_string());
        unsupported_fields.push("port_forwarding_list".to_string());
    }
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source: ConnectionImportSource::FinalShell,
        source_path: path.display().to_string(),
        name,
        group,
        host,
        port: connection.port.unwrap_or(22),
        username,
        auth_type: ImportedConnectionAuthType::Password,
        key_path: None,
        cert_path: None,
        proxy_chain: Vec::new(),
        tags: vec![ConnectionImportSource::FinalShell.tag().to_string()],
        warnings: dedupe(warnings),
        unsupported_fields: dedupe(unsupported_fields),
        duplicate: false,
        importable: true,
    };
    draft.id = draft_id(&draft);
    Some(draft)
}

fn draft_from_fields(
    source: ConnectionImportSource,
    path: &Path,
    root: Option<&Path>,
    fields: BTreeMap<String, String>,
    mut warnings: Vec<String>,
    unsupported_fields: Vec<String>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let host =
        pick_field(&fields, &["hostname", "host", "address", "ssh2hostname"]).ok_or_else(|| {
            ConnectionImportError::Parse {
                path: path.display().to_string(),
                message: "Missing host".to_string(),
            }
        })?;
    let username = pick_field(
        &fields,
        &["username", "user", "loginname", "account", "userid"],
    )
    .unwrap_or_else(whoami::username);
    let raw_port = pick_field(&fields, &["port", "sshport"]);
    let port = raw_port
        .as_deref()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| {
            if raw_port.is_some() {
                warnings.push("Invalid port; defaulted to 22".to_string());
            }
            22
        });
    let name =
        pick_field(&fields, &["name", "sessionname", "label"]).unwrap_or_else(|| file_stem(path));
    let key_path = pick_field(
        &fields,
        &[
            "identityfilename",
            "identityfilenamev2",
            "privatekey",
            "privatekeypath",
            "keyfile",
            "keypath",
            "publickeyfile",
        ],
    );
    let cert_path = pick_field(&fields, &["certificatefile", "certificatepath", "certpath"]);
    let auth_type = if cert_path.is_some() {
        ImportedConnectionAuthType::Certificate
    } else if key_path.is_some() {
        ImportedConnectionAuthType::Key
    } else {
        ImportedConnectionAuthType::Password
    };
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source,
        source_path: path.display().to_string(),
        name,
        group: group_from_path(path, root).or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
        host,
        port,
        username,
        auth_type,
        key_path,
        cert_path,
        proxy_chain: Vec::new(),
        tags: vec![source.tag().to_string()],
        warnings: dedupe(warnings),
        unsupported_fields: dedupe(unsupported_fields),
        duplicate: false,
        importable: true,
    };
    draft.id = draft_id(&draft);
    Ok(draft)
}

fn collect_termius_drafts(
    path: &Path,
    value: &serde_json::Value,
    group: Option<String>,
    drafts: &mut Vec<ImportedConnectionDraft>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let next_group = map
                .get("group")
                .or_else(|| map.get("folder"))
                .or_else(|| map.get("folderName"))
                .and_then(value_as_string)
                .or(group);

            if let Some(host) = pick_json_string(map, &["hostname", "host", "address"]) {
                let username = pick_json_string(map, &["username", "user", "login"])
                    .unwrap_or_else(whoami::username);
                let port = pick_json_u16(map, &["port"]).unwrap_or(22);
                let name = pick_json_string(map, &["label", "name", "title"])
                    .unwrap_or_else(|| host.clone());
                let key_path =
                    pick_json_string(map, &["identityFile", "keyPath", "privateKeyPath"]);
                let cert_path = pick_json_string(map, &["certificateFile", "certPath"]);
                let mut warnings = Vec::new();
                let mut unsupported_fields = Vec::new();
                collect_secret_json_keys(map, &mut unsupported_fields);
                if !unsupported_fields.is_empty() {
                    warnings.push("Password was not imported".to_string());
                }
                let auth_type = if cert_path.is_some() {
                    ImportedConnectionAuthType::Certificate
                } else if key_path.is_some() {
                    ImportedConnectionAuthType::Key
                } else {
                    ImportedConnectionAuthType::Password
                };
                let mut draft = ImportedConnectionDraft {
                    id: String::new(),
                    source: ConnectionImportSource::Termius,
                    source_path: path.display().to_string(),
                    name,
                    group: next_group.or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
                    host,
                    port,
                    username,
                    auth_type,
                    key_path,
                    cert_path,
                    proxy_chain: Vec::new(),
                    tags: vec![ConnectionImportSource::Termius.tag().to_string()],
                    warnings: dedupe(warnings),
                    unsupported_fields: dedupe(unsupported_fields),
                    duplicate: false,
                    importable: true,
                };
                draft.id = draft_id(&draft);
                drafts.push(draft);
                return;
            }

            for child in map.values() {
                collect_termius_drafts(path, child, next_group.clone(), drafts);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_termius_drafts(path, item, group.clone(), drafts);
            }
        }
        _ => {}
    }
}

fn imported_auth_to_saved(
    auth_type: ImportedConnectionAuthType,
    key_path: Option<&String>,
    cert_path: Option<&String>,
) -> SavedAuth {
    match auth_type {
        ImportedConnectionAuthType::Certificate => match (key_path, cert_path) {
            (Some(key_path), Some(cert_path)) => SavedAuth::Certificate {
                key_path: key_path.clone(),
                cert_path: cert_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            (Some(key_path), None) => SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            _ => SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            },
        },
        ImportedConnectionAuthType::Key => match key_path {
            Some(key_path) => SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            None => SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            },
        },
        ImportedConnectionAuthType::Agent => SavedAuth::Agent,
        ImportedConnectionAuthType::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
    }
}

fn imported_proxy_hop_to_saved(hop: &ImportedProxyHopDraft) -> SavedProxyHop {
    SavedProxyHop {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: imported_auth_to_saved(hop.auth_type, hop.key_path.as_ref(), hop.cert_path.as_ref()),
        agent_forwarding: hop.agent_forwarding,
        legacy_ssh_compatibility: false,
    }
}

fn imported_draft_to_saved_connection(
    draft: &ImportedConnectionDraft,
    name: String,
    group: Option<String>,
) -> SavedConnection {
    // Imported third-party sessions intentionally contain no secret material.
    // The normal connection prompt remains the password/passphrase boundary.
    SavedConnection {
        id: Uuid::new_v4().to_string(),
        version: CONFIG_VERSION,
        name,
        group,
        host: draft.host.clone(),
        port: draft.port,
        username: draft.username.clone(),
        auth: imported_auth_to_saved(
            draft.auth_type,
            draft.key_path.as_ref(),
            draft.cert_path.as_ref(),
        ),
        proxy_chain: draft
            .proxy_chain
            .iter()
            .map(imported_proxy_hop_to_saved)
            .collect(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        options: ConnectionOptions::default(),
        created_at: Utc::now(),
        last_used_at: None,
        updated_at: Some(Utc::now()),
        color: None,
        icon: None,
        tags: draft.tags.clone(),
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
}

fn normalized_import_group(
    request_group: Option<&String>,
    draft_group: Option<&String>,
    source: ConnectionImportSource,
) -> Option<String> {
    request_group
        .and_then(|group| {
            let trimmed = group.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .or_else(|| draft_group.cloned())
        .or_else(|| Some(source.default_group().to_string()))
}

fn unique_import_name(base_name: &str, existing_names: &HashSet<String>) -> String {
    if !existing_names.contains(base_name) {
        return base_name.to_string();
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{} ({})", base_name, index);
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn parse_securecrt_setting(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }
    let (_, rest) = trimmed.split_once(':')?;
    let (key_part, value_part) = rest.split_once('=')?;
    Some((unquote(key_part.trim()), value_part.trim()))
}

fn parse_plain_setting(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with(';')
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    Some((key.trim().to_string(), unquote(value.trim())))
}

fn parse_ini_sections(content: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current_section = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
            sections.entry(current_section.clone()).or_default();
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        sections
            .entry(current_section.clone())
            .or_default()
            .insert(key.trim().to_string(), unquote(value.trim()));
    }
    sections
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn looks_like_secret_key(key: &str) -> bool {
    key.contains("password")
        || key.contains("passphrase")
        || key.contains("credential")
        || key.contains("vault")
        || key.contains("ciphertext")
        || key.contains("secret")
}

fn looks_like_proxy_key(key: &str) -> bool {
    key.contains("proxy")
        || key.contains("firewall")
        || key.contains("jumphost")
        || key.contains("jumpserver")
        || key.contains("bastion")
        || key.contains("gateway")
        || key.contains("socks")
}

fn is_imported_connection_field(key: &str) -> bool {
    matches!(
        key,
        "protocolname"
            | "protocol"
            | "hostname"
            | "host"
            | "address"
            | "ssh2hostname"
            | "username"
            | "user"
            | "loginname"
            | "account"
            | "userid"
            | "port"
            | "sshport"
            | "ssh2port"
            | "name"
            | "sessionname"
            | "label"
            | "identityfilename"
            | "identityfilenamev2"
            | "privatekeypath"
            | "keyfile"
            | "keypath"
            | "publickeyfile"
            | "certificatefile"
            | "certificatepath"
            | "certpath"
    )
}

fn pick_field(fields: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).filter(|value| !value.trim().is_empty()))
        .cloned()
}

fn pick_json_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(value_as_string))
        .filter(|value| !value.trim().is_empty())
}

fn pick_json_u16(map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u16> {
    keys.iter().find_map(|key| match map.get(*key)? {
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|value| u16::try_from(value).ok())
        }
        serde_json::Value::String(value) => value.parse::<u16>().ok(),
        _ => None,
    })
}

fn value_as_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn collect_secret_json_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    unsupported_fields: &mut Vec<String>,
) {
    for key in map.keys() {
        if looks_like_secret_key(&normalize_key(key)) {
            unsupported_fields.push(key.clone());
        }
    }
}

fn group_from_path(path: &Path, root: Option<&Path>) -> Option<String> {
    let root = root?;
    let relative = path.strip_prefix(root).ok()?;
    let parent = relative.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(format!(
        "{}/{}",
        DEFAULT_IMPORTED_GROUP,
        parent
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    ))
}

fn group_from_segments<'a>(segments: impl Iterator<Item = &'a str>) -> Option<String> {
    let normalized = segments
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        return None;
    }
    Some(format!(
        "{}/{}",
        DEFAULT_IMPORTED_GROUP,
        normalized.join("/")
    ))
}

fn xshell_archive_group(entry_path: &str) -> Option<String> {
    let parent = Path::new(entry_path).parent()?.to_string_lossy();
    let stripped = parent
        .strip_prefix("Xshell/Sessions/")
        .or_else(|| parent.strip_prefix("Xshell/"))
        .unwrap_or(&parent);
    group_from_segments(stripped.split('/'))
}

fn parse_windterm_target(target: &str) -> (String, String) {
    let trimmed = target.trim();
    if let Some((username, host)) = trimmed.rsplit_once('@') {
        let username = username.trim();
        let host = host.trim();
        if !username.is_empty() && !host.is_empty() {
            return (host.to_string(), username.to_string());
        }
    }
    (trimmed.to_string(), "root".to_string())
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported connection".to_string())
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_string()
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn draft_id(draft: &ImportedConnectionDraft) -> String {
    let mut hasher = Sha256::new();
    hasher.update(draft.source.tag().as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.source_path.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.name.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.host.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.username.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("connection_import")
            .join(name)
    }

    fn temp_import_file(extension: &str, content: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("oxideterm-import-{id}.{extension}"));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn previews_securecrt_session_without_importing_password() {
        let path = fixture_path("securecrt/basic.ini");
        let preview = preview_connection_import(
            ConnectionImportSource::SecureCrt,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.host, "gpu.example.com");
        assert_eq!(draft.port, 2222);
        assert_eq!(draft.username, "alice");
        assert_eq!(draft.auth_type, ImportedConnectionAuthType::Key);
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Password was not imported")
        );
    }

    #[test]
    fn previews_securecrt_xml_sessions_without_retaining_secrets() {
        let path = fixture_path("securecrt/export.xml");
        let preview = preview_connection_import(
            ConnectionImportSource::SecureCrt,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.name, "Gateway");
        assert_eq!(draft.host, "gateway.example.com");
        assert_eq!(draft.port, 2202);
        assert_eq!(draft.username, "deploy");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production"));
        assert_eq!(draft.auth_type, ImportedConnectionAuthType::Key);
        assert!(
            draft
                .unsupported_fields
                .contains(&"Password V2".to_string())
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        let debug = format!("{preview:?}");
        assert!(!serialized.contains("securecrt-secret-sentinel"));
        assert!(!debug.contains("securecrt-secret-sentinel"));
    }

    #[test]
    fn previews_xshell_session_without_importing_password() {
        let path = fixture_path("xshell/model.xsh");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.name, "model");
        assert_eq!(draft.host, "10.0.0.8");
        assert_eq!(draft.username, "ubuntu");
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Password was not imported")
        );
    }

    #[test]
    fn previews_xshell_archive_sessions_with_entry_groups() {
        let path = temp_import_file("xts", "");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            archive
                .start_file("Xshell/Sessions/Production/GPU/model.xsh", options)
                .unwrap();
            archive
                .write_all(
                    b"[CONNECTION]\nProtocol=SSH\nHost=archive.example.com\nPort=2201\n\n[AUTHENTICATION]\nUserName=ops\n",
                )
                .unwrap();
            archive.finish().unwrap();
        }

        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(preview.total, 1);
        assert_eq!(draft.name, "model");
        assert_eq!(draft.host, "archive.example.com");
        assert_eq!(draft.username, "ops");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production/GPU"));
        assert!(draft.source_path.contains("Production/GPU/model.xsh"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn previews_termius_export_hosts() {
        let path = fixture_path("termius/export.json");
        let preview = preview_connection_import(
            ConnectionImportSource::Termius,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 2);
        assert!(
            preview
                .drafts
                .iter()
                .any(|draft| draft.name == "Inference A")
        );
        assert!(
            preview
                .drafts
                .iter()
                .any(|draft| draft.host == "gpu-b.example.com")
        );
        assert_eq!(preview.warnings, 1);
    }

    #[test]
    fn previews_mobaxterm_ssh_bookmarks_with_groups() {
        let path = fixture_path("mobaxterm/bookmarks.mxtsessions");
        let preview = preview_connection_import(
            ConnectionImportSource::MobaXterm,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(preview.total, 1);
        assert_eq!(draft.name, "Prod GPU");
        assert_eq!(draft.host, "prod-gpu.example.com");
        assert_eq!(draft.port, 2200);
        assert_eq!(draft.username, "deploy");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production/GPU"));
        assert_eq!(draft.unsupported_fields, vec!["ImgNum".to_string()]);
    }

    #[test]
    fn previews_windterm_ssh_sessions_with_groups() {
        let path = fixture_path("windterm/user.sessions");
        let preview = preview_connection_import(
            ConnectionImportSource::WindTerm,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(preview.total, 1);
        assert_eq!(draft.name, "Wind Prod");
        assert_eq!(draft.host, "wind.example.com");
        assert_eq!(draft.port, 2222);
        assert_eq!(draft.username, "admin");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production/Edge"));
    }

    #[test]
    fn previews_electerm_bookmarks_with_nested_groups_and_redaction() {
        let path = fixture_path("electerm/bookmarks.json");
        let preview = preview_connection_import(
            ConnectionImportSource::Electerm,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.name, "Electerm Gateway");
        assert_eq!(draft.host, "electerm.example.com");
        assert_eq!(draft.port, 2223);
        assert_eq!(draft.username, "ops");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production/Edge"));
        assert!(draft.unsupported_fields.contains(&"privateKey".to_string()));
        assert!(
            draft
                .unsupported_fields
                .contains(&"connectionHoppings".to_string())
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        let debug = format!("{preview:?}");
        assert!(!serialized.contains("electerm-private-key-sentinel"));
        assert!(!serialized.contains("electerm-passphrase-sentinel"));
        assert!(!serialized.contains("nested-secret-sentinel"));
        assert!(!debug.contains("electerm-private-key-sentinel"));
        assert!(!debug.contains("electerm-passphrase-sentinel"));
        assert!(!debug.contains("nested-secret-sentinel"));
    }

    #[test]
    fn previews_legacy_electerm_bookmark_array() {
        let path = temp_import_file(
            "json",
            r#"[{"id":"legacy","title":"Legacy","host":"legacy.example.com","username":"root","port":"2225","type":"ssh"}]"#,
        );
        let preview = preview_connection_import(
            ConnectionImportSource::Electerm,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 1);
        assert_eq!(preview.drafts[0].port, 2225);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn previews_finalshell_conn_directory_without_retaining_secrets() {
        let path = fixture_path("finalshell");
        let preview = preview_connection_import(
            ConnectionImportSource::FinalShell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.name, "FinalShell Gateway");
        assert_eq!(draft.host, "finalshell.example.com");
        assert_eq!(draft.port, 2224);
        assert_eq!(draft.username, "admin");
        assert_eq!(draft.group.as_deref(), Some("Imported/Production"));
        assert!(draft.unsupported_fields.contains(&"password".to_string()));
        assert!(
            draft
                .unsupported_fields
                .contains(&"secret_key_id".to_string())
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        let debug = format!("{preview:?}");
        assert!(!serialized.contains("finalshell-password-sentinel"));
        assert!(!debug.contains("finalshell-password-sentinel"));

        let conn_path = path.join("conn");
        let direct_preview = preview_connection_import(
            ConnectionImportSource::FinalShell,
            &[conn_path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(direct_preview.total, 1);
    }

    #[test]
    fn applies_selected_imports_with_rename_strategy() {
        let store_path = std::env::temp_dir().join(format!(
            "oxideterm-connection-import-test-{}.json",
            Uuid::new_v4()
        ));
        let mut store = ConnectionStore::load(store_path).unwrap();
        let path = fixture_path("xshell/model.xsh");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();
        let draft_id = preview.drafts[0].id.clone();

        let result = apply_connection_import(
            &mut store,
            ConnectionImportApplyRequest {
                source: ConnectionImportSource::Xshell,
                paths: vec![path.display().to_string()],
                selected_draft_ids: vec![draft_id.clone()],
                duplicate_strategy: ConnectionImportDuplicateStrategy::Rename,
                target_group: Some("Imported/Xshell".to_string()),
            },
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(store.connections()[0].name, "model");

        let result = apply_connection_import(
            &mut store,
            ConnectionImportApplyRequest {
                source: ConnectionImportSource::Xshell,
                paths: vec![path.display().to_string()],
                selected_draft_ids: vec![draft_id],
                duplicate_strategy: ConnectionImportDuplicateStrategy::Rename,
                target_group: Some("Imported/Xshell".to_string()),
            },
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.renamed, 1);
        assert!(
            store
                .connections()
                .iter()
                .any(|conn| conn.name == "model (2)")
        );
        let _ = fs::remove_file(store.path());
    }

    #[test]
    fn records_missing_host_as_preview_error() {
        let path = temp_import_file("xsh", "UserName=alice\n");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert!(preview.drafts.is_empty());
        assert_eq!(preview.errors.len(), 1);
        assert!(preview.errors[0].message.contains("Missing host"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_port_warns_and_defaults_to_ssh_port() {
        let path = temp_import_file("xsh", "Host=gpu.invalid\nUserName=alice\nPort=abc\n");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.port, 22);
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Invalid port; defaulted to 22")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_proxy_fields_warn_instead_of_silent_import() {
        let path = temp_import_file(
            "xsh",
            "Host=gpu.invalid\nUserName=alice\nProxyServer=jump.example.com\n",
        );
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert!(draft.proxy_chain.is_empty());
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Proxy/jump setting was not imported")
        );
        assert_eq!(draft.unsupported_fields, vec!["ProxyServer".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn records_malformed_termius_json_as_preview_error() {
        let path = temp_import_file("json", "{");
        let preview = preview_connection_import(
            ConnectionImportSource::Termius,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert!(preview.drafts.is_empty());
        assert_eq!(preview.errors.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn native_model_deserializes_tauri_preview_payload() {
        let json = fs::read_to_string(fixture_path("tauri_preview.json")).unwrap();
        let preview: ConnectionImportPreview = serde_json::from_str(&json).unwrap();

        assert_eq!(preview.source, ConnectionImportSource::SecureCrt);
        assert_eq!(preview.total, 1);
        assert_eq!(preview.importable, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.source_path, "/Users/example/Sessions/GPU/basic.ini");
        assert_eq!(draft.auth_type, ImportedConnectionAuthType::Key);
        assert_eq!(draft.unsupported_fields, vec!["Password V2".to_string()]);
    }
}
