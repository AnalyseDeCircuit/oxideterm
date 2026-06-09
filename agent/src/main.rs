// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
    time::UNIX_EPOCH,
};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const COMPATIBILITY_VERSION: u32 = 2;
const ERR_METHOD_NOT_FOUND: i32 = -32601;
const ERR_INVALID_PARAMS: i32 = -32602;
const ERR_INTERNAL: i32 = -32603;
const ERR_IO: i32 = -1;
const ERR_NOT_FOUND: i32 = -2;
const ERR_PERMISSION: i32 = -3;
const ERR_ALREADY_EXISTS: i32 = -4;
const DEFAULT_SYMBOL_MAX_FILES: u32 = 500;
const DEFAULT_SYMBOL_COMPLETE_LIMIT: u32 = 20;
const SYMBOL_MAX_FILE_BYTES: u64 = 500_000;

static SYMBOL_CACHE: LazyLock<Mutex<HashMap<String, Vec<SymbolInfo>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Serialize)]
struct SysInfoResult {
    version: String,
    compatibility_version: u32,
    arch: String,
    os: String,
    pid: u32,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReadFileResult {
    content: String,
    hash: String,
    size: u64,
    mtime: u64,
    encoding: String,
}

#[derive(Debug, Serialize)]
struct WriteFileResult {
    hash: String,
    size: u64,
    mtime: u64,
    atomic: bool,
}

#[derive(Debug, Serialize)]
struct StatResult {
    exists: bool,
    file_type: Option<String>,
    size: Option<u64>,
    mtime: Option<u64>,
    permissions: Option<String>,
}

#[derive(Debug, Serialize)]
struct FileEntry {
    name: String,
    path: String,
    file_type: String,
    is_symlink: bool,
    symlink_target: Option<String>,
    target_file_type: Option<String>,
    size: u64,
    mtime: Option<u64>,
    permissions: Option<String>,
    children: Option<Vec<FileEntry>>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ListTreeResult {
    entries: Vec<FileEntry>,
    truncated: bool,
    total_scanned: u32,
}

#[derive(Debug, Serialize)]
struct GrepMatch {
    path: String,
    line: u32,
    column: u32,
    text: String,
}

#[derive(Debug, Serialize)]
struct GitStatusResult {
    branch: String,
    files: Vec<GitFileEntry>,
}

#[derive(Debug, Serialize)]
struct GitFileEntry {
    path: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct SymbolIndexResult {
    symbols: Vec<SymbolInfo>,
    file_count: u32,
}

#[derive(Clone, Debug, Serialize)]
struct SymbolInfo {
    name: String,
    kind: String,
    path: String,
    line: u32,
    column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SymbolIndexParams {
    path: String,
    max_files: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SymbolCompleteParams {
    path: String,
    prefix: String,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SymbolDefinitionsParams {
    path: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct PathParams {
    path: String,
}

#[derive(Debug, Deserialize)]
struct ReadFileParams {
    path: String,
}

#[derive(Debug, Deserialize)]
struct WriteFileParams {
    path: String,
    content: String,
    #[serde(default = "plain_encoding")]
    encoding: String,
    expect_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListTreeParams {
    path: String,
    max_depth: Option<u32>,
    max_entries: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GrepParams {
    pattern: String,
    path: String,
    #[serde(default)]
    case_sensitive: bool,
    max_results: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MkdirParams {
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Deserialize)]
struct RemoveParams {
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Deserialize)]
struct RenameParams {
    old_path: String,
    new_path: String,
}

#[derive(Debug, Deserialize)]
struct ChmodParams {
    path: String,
    mode: String,
}

fn main() {
    if env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!(
            "oxideterm-agent {} compat {}",
            env!("CARGO_PKG_VERSION"),
            COMPATIBILITY_VERSION
        );
        return;
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in BufReader::new(stdin.lock()).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("[oxideterm-agent] stdin read failed: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle_request(request),
            Err(error) => Response {
                id: 0,
                result: None,
                error: Some(rpc_error(
                    ERR_INVALID_PARAMS,
                    format!("Invalid request JSON: {error}"),
                )),
            },
        };
        if let Ok(serialized) = serde_json::to_string(&response) {
            let _ = writeln!(stdout, "{serialized}");
            let _ = stdout.flush();
        }
    }
}

fn handle_request(request: Request) -> Response {
    let result = dispatch(&request.method, request.params);
    match result {
        Ok(result) => Response {
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            id: request.id,
            result: None,
            error: Some(error),
        },
    }
}

fn dispatch(method: &str, params: Value) -> Result<Value, RpcError> {
    match method {
        "sys/ping" => Ok(json!({ "ok": true })),
        "sys/info" => to_value(sys_info()),
        "sys/shutdown" => Ok(json!({})),
        "fs/readFile" => to_value(read_file(from_params(params)?)?),
        "fs/writeFile" => to_value(write_file(from_params(params)?)?),
        "fs/stat" => to_value(stat_path(&from_params::<PathParams>(params)?.path)),
        "fs/listDir" => to_value(list_dir(&from_params::<PathParams>(params)?.path)?),
        "fs/listTree" => to_value(list_tree(from_params(params)?)?),
        "fs/mkdir" => {
            mkdir(from_params(params)?)?;
            Ok(json!({}))
        }
        "fs/remove" => {
            remove(from_params(params)?)?;
            Ok(json!({}))
        }
        "fs/rename" => {
            rename(from_params(params)?)?;
            Ok(json!({}))
        }
        "fs/chmod" => {
            chmod(from_params(params)?)?;
            Ok(json!({}))
        }
        "search/grep" => to_value(grep(from_params(params)?)?),
        "git/status" => to_value(git_status(&from_params::<PathParams>(params)?.path)?),
        "watch/start" | "watch/stop" => Ok(json!({})),
        "symbols/index" => to_value(symbol_index(from_params(params)?)),
        "symbols/complete" => to_value(symbol_complete(from_params(params)?)),
        "symbols/definitions" => to_value(symbol_definitions(from_params(params)?)),
        _ => Err(rpc_error(
            ERR_METHOD_NOT_FOUND,
            format!("Unknown method: {method}"),
        )),
    }
}

fn sys_info() -> SysInfoResult {
    SysInfoResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        compatibility_version: COMPATIBILITY_VERSION,
        arch: env::consts::ARCH.to_string(),
        os: env::consts::OS.to_string(),
        pid: std::process::id(),
        capabilities: Vec::new(),
    }
}

fn read_file(params: ReadFileParams) -> Result<ReadFileResult, RpcError> {
    let path = normalize_path(&params.path);
    let bytes = fs::read(&path).map_err(|error| map_io_error(error, &path))?;
    if looks_binary(&bytes) {
        return Err(rpc_error(ERR_INVALID_PARAMS, "File is not a text file"));
    }
    let metadata = fs::metadata(&path).map_err(|error| map_io_error(error, &path))?;
    let hash = hash_bytes(&bytes);
    let (content, encoding) = encode_content(&bytes)?;
    Ok(ReadFileResult {
        content,
        hash,
        size: metadata.len(),
        mtime: mtime_secs(&metadata),
        encoding,
    })
}

fn write_file(params: WriteFileParams) -> Result<WriteFileResult, RpcError> {
    let path = normalize_path(&params.path);
    if let Some(expected) = params.expect_hash.as_deref()
        && path.exists()
    {
        let current = fs::read(&path).map_err(|error| map_io_error(error, &path))?;
        let current_hash = hash_bytes(&current);
        if current_hash != expected {
            return Err(rpc_error(
                ERR_ALREADY_EXISTS,
                "CONFLICT: File modified externally",
            ));
        }
    }

    let bytes = decode_content(&params.content, &params.encoding)?;
    let atomic = atomic_write(&path, &bytes)?;
    let metadata = fs::metadata(&path).map_err(|error| map_io_error(error, &path))?;
    Ok(WriteFileResult {
        hash: hash_bytes(&bytes),
        size: metadata.len(),
        mtime: mtime_secs(&metadata),
        atomic,
    })
}

fn stat_path(path: &str) -> StatResult {
    let path = normalize_path(path);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => StatResult {
            exists: true,
            file_type: Some(file_type(&metadata).to_string()),
            size: Some(metadata.len()),
            mtime: Some(mtime_secs(&metadata)),
            permissions: permissions(&metadata),
        },
        Err(_) => StatResult {
            exists: false,
            file_type: None,
            size: None,
            mtime: None,
            permissions: None,
        },
    }
}

fn list_dir(path: &str) -> Result<Vec<FileEntry>, RpcError> {
    let path = normalize_path(path);
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(&path).map_err(|error| map_io_error(error, &path))?;
    for entry in read_dir {
        let entry = entry.map_err(|error| map_io_error(error, &path))?;
        entries.push(file_entry(&entry.path(), None)?);
    }
    entries.sort_by(|a, b| {
        (a.file_type != "directory", a.name.to_lowercase())
            .cmp(&(b.file_type != "directory", b.name.to_lowercase()))
    });
    Ok(entries)
}

fn list_tree(params: ListTreeParams) -> Result<ListTreeResult, RpcError> {
    let root = normalize_path(&params.path);
    let max_depth = params.max_depth.unwrap_or(8) as usize;
    let max_entries = params.max_entries.unwrap_or(5000);
    let mut entries = Vec::new();
    let mut total_scanned = 0_u32;
    let mut truncated = false;

    for entry in WalkDir::new(&root).min_depth(1).max_depth(max_depth) {
        let entry = entry.map_err(|error| rpc_error(ERR_IO, error.to_string()))?;
        total_scanned = total_scanned.saturating_add(1);
        if total_scanned > max_entries {
            truncated = true;
            break;
        }
        entries.push(file_entry(entry.path(), None)?);
    }

    Ok(ListTreeResult {
        entries,
        truncated,
        total_scanned,
    })
}

fn mkdir(params: MkdirParams) -> Result<(), RpcError> {
    let path = normalize_path(&params.path);
    if params.recursive {
        fs::create_dir_all(&path)
    } else {
        fs::create_dir(&path)
    }
    .map_err(|error| map_io_error(error, &path))
}

fn remove(params: RemoveParams) -> Result<(), RpcError> {
    let path = normalize_path(&params.path);
    let metadata = fs::symlink_metadata(&path).map_err(|error| map_io_error(error, &path))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        if params.recursive {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_dir(&path)
        }
    } else {
        fs::remove_file(&path)
    }
    .map_err(|error| map_io_error(error, &path))
}

fn rename(params: RenameParams) -> Result<(), RpcError> {
    let old_path = normalize_path(&params.old_path);
    let new_path = normalize_path(&params.new_path);
    fs::rename(&old_path, &new_path).map_err(|error| map_io_error(error, &old_path))
}

fn chmod(params: ChmodParams) -> Result<(), RpcError> {
    let path = normalize_path(&params.path);
    let mode = u32::from_str_radix(params.mode.trim_start_matches("0o"), 8)
        .map_err(|_| rpc_error(ERR_INVALID_PARAMS, "Invalid chmod mode"))?;
    set_permissions(&path, mode)
}

fn grep(params: GrepParams) -> Result<Vec<GrepMatch>, RpcError> {
    let root = normalize_path(&params.path);
    let regex = RegexBuilder::new(&params.pattern)
        .case_insensitive(!params.case_sensitive)
        .build()
        .map_err(|error| rpc_error(ERR_INVALID_PARAMS, error.to_string()))?;
    let max_results = params.max_results.unwrap_or(1000) as usize;
    let mut matches = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry
            .file_name()
            .to_str()
            .map(|name| !ignored_name(name))
            .unwrap_or(true)
    }) {
        let entry = entry.map_err(|error| rpc_error(ERR_IO, error.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let bytes = match fs::read(entry.path()) {
            Ok(bytes) if !looks_binary(&bytes) => bytes,
            _ => continue,
        };
        let text = String::from_utf8_lossy(&bytes);
        for (line_index, line) in text.lines().enumerate() {
            if let Some(found) = regex.find(line) {
                matches.push(GrepMatch {
                    path: entry.path().display().to_string(),
                    line: (line_index + 1) as u32,
                    column: (found.start() + 1) as u32,
                    text: line.to_string(),
                });
                if matches.len() >= max_results {
                    return Ok(matches);
                }
            }
        }
    }
    Ok(matches)
}

fn git_status(path: &str) -> Result<GitStatusResult, RpcError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-b")
        .output()
        .map_err(|error| rpc_error(ERR_IO, error.to_string()))?;
    if !output.status.success() {
        return Err(rpc_error(
            ERR_IO,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branch = String::new();
    let mut files = Vec::new();
    for line in stdout.lines() {
        if let Some(raw) = line.strip_prefix("## ") {
            branch = raw.split("...").next().unwrap_or(raw).to_string();
            continue;
        }
        if line.len() >= 4 {
            files.push(GitFileEntry {
                status: line[..2].trim().to_string(),
                path: line[3..].to_string(),
            });
        }
    }
    Ok(GitStatusResult { branch, files })
}

fn symbol_index(params: SymbolIndexParams) -> SymbolIndexResult {
    let max_files = params.max_files.unwrap_or(DEFAULT_SYMBOL_MAX_FILES);
    let root = normalize_path(&params.path);
    let index = index_symbols_in_directory(&root, max_files);
    if let Ok(mut cache) = SYMBOL_CACHE.lock() {
        cache.insert(params.path, index.symbols.clone());
    }
    SymbolIndexResult {
        file_count: index.file_count,
        symbols: index.symbols,
    }
}

fn symbol_complete(params: SymbolCompleteParams) -> Vec<SymbolInfo> {
    let symbols = cached_or_indexed_symbols(&params.path);
    let prefix = params.prefix.to_lowercase();
    symbols
        .into_iter()
        .filter(|symbol| symbol.name.to_lowercase().starts_with(&prefix))
        .take(params.limit.unwrap_or(DEFAULT_SYMBOL_COMPLETE_LIMIT) as usize)
        .collect()
}

fn symbol_definitions(params: SymbolDefinitionsParams) -> Vec<SymbolInfo> {
    cached_or_indexed_symbols(&params.path)
        .into_iter()
        .filter(|symbol| symbol.name == params.name)
        .collect()
}

fn cached_or_indexed_symbols(path: &str) -> Vec<SymbolInfo> {
    if let Some(symbols) = SYMBOL_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).cloned())
    {
        return symbols;
    }

    let root = normalize_path(path);
    let symbols = index_symbols_in_directory(&root, DEFAULT_SYMBOL_MAX_FILES).symbols;
    if let Ok(mut cache) = SYMBOL_CACHE.lock() {
        cache.insert(path.to_string(), symbols.clone());
    }
    symbols
}

struct SymbolDirectoryIndex {
    symbols: Vec<SymbolInfo>,
    file_count: u32,
}

fn index_symbols_in_directory(root: &Path, max_files: u32) -> SymbolDirectoryIndex {
    let mut symbols = Vec::new();
    let mut scanned_files = 0u32;
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry
            .file_name()
            .to_str()
            .map(|name| !ignored_symbol_name(name))
            .unwrap_or(true)
    }) {
        if scanned_files >= max_files {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > SYMBOL_MAX_FILE_BYTES {
            continue;
        }
        scanned_files += 1;
        symbols.extend(extract_symbols_from_file(entry.path()));
    }
    SymbolDirectoryIndex {
        symbols,
        file_count: scanned_files,
    }
}

fn extract_symbols_from_file(path: &Path) -> Vec<SymbolInfo> {
    let Some(patterns) = symbol_patterns_for_path(path) else {
        return Vec::new();
    };
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    let mut in_block_comment = false;
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if in_block_comment {
            in_block_comment = !trimmed.contains("*/");
            continue;
        }
        if trimmed.starts_with("/*") {
            in_block_comment = !trimmed.contains("*/");
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        for pattern in &patterns {
            if let Some(captures) = pattern.regex.captures(line)
                && let Some(name_match) = captures.name("name")
            {
                let name = name_match.as_str();
                if !reserved_symbol_name(name) {
                    symbols.push(SymbolInfo {
                        name: name.to_string(),
                        kind: pattern.kind.to_string(),
                        path: path.display().to_string(),
                        line: (line_index + 1) as u32,
                        column: (name_match.start() + 1) as u32,
                        container: None,
                    });
                    break;
                }
            }
        }
    }
    symbols
}

struct CompiledSymbolPattern {
    kind: &'static str,
    regex: Regex,
}

fn symbol_patterns_for_path(path: &Path) -> Option<Vec<CompiledSymbolPattern>> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let specs: &[(&str, &str)] = match extension.as_str() {
        "rs" => &[
            ("function", r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("struct", r"\b(?:pub(?:\([^)]*\))?\s+)?struct\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("enum", r"\b(?:pub(?:\([^)]*\))?\s+)?enum\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("trait", r"\b(?:pub(?:\([^)]*\))?\s+)?trait\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("typeAlias", r"\b(?:pub(?:\([^)]*\))?\s+)?type\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("constant", r"\b(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
        ],
        "py" | "pyw" => &[
            ("function", r"\b(?:async\s+)?def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("class", r"\bclass\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
        ],
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" => &[
            ("function", r"\b(?:export\s+)?(?:async\s+)?function\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
            ("class", r"\b(?:export\s+)?class\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
            ("interface", r"\b(?:export\s+)?interface\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
            ("typeAlias", r"\b(?:export\s+)?type\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
            ("enum", r"\b(?:export\s+)?enum\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
            ("constant", r"\b(?:export\s+)?(?:const|let|var)\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)"),
        ],
        "go" => &[
            ("function", r"\bfunc\s+(?:\([^)]*\)\s*)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("struct", r"\btype\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s+struct\b"),
            ("interface", r"\btype\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s+interface\b"),
        ],
        "java" | "kt" | "kts" | "cs" | "swift" => &[
            ("class", r"\b(?:public\s+|private\s+|internal\s+|open\s+|final\s+|abstract\s+|data\s+|sealed\s+|partial\s+|static\s+)*class\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("struct", r"\b(?:public\s+|private\s+|internal\s+|readonly\s+)*struct\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("interface", r"\b(?:public\s+|private\s+|internal\s+)*(?:interface|protocol)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("enum", r"\b(?:public\s+|private\s+|internal\s+)*enum\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("function", r"\b(?:public\s+|private\s+|internal\s+|static\s+|override\s+|func\s+)*func\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
        ],
        "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "hxx" => &[
            ("class", r"\bclass\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("struct", r"\b(?:typedef\s+)?struct\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("enum", r"\b(?:typedef\s+)?enum(?:\s+class)?\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("typeAlias", r"\b(?:typedef|using)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("module", r"\bnamespace\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
        ],
        "rb" => &[
            ("function", r"\bdef\s+(?P<name>[A-Za-z_][A-Za-z0-9_!?=]*)"),
            ("class", r"\bclass\s+(?P<name>[A-Za-z_][A-Za-z0-9_:]*)"),
            ("module", r"\bmodule\s+(?P<name>[A-Za-z_][A-Za-z0-9_:]*)"),
        ],
        "php" => &[
            ("function", r"\bfunction\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("class", r"\bclass\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
        ],
        "sh" | "bash" | "zsh" => &[
            ("function", r"\bfunction\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"),
            ("function", r"^\s*(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)"),
        ],
        _ => return None,
    };

    let mut compiled = Vec::with_capacity(specs.len());
    for (kind, pattern) in specs {
        if let Ok(regex) = Regex::new(pattern) {
            compiled.push(CompiledSymbolPattern { kind, regex });
        }
    }
    Some(compiled)
}

fn file_entry(path: &Path, children: Option<Vec<FileEntry>>) -> Result<FileEntry, RpcError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| map_io_error(error, path))?;
    let symlink_target = if metadata.file_type().is_symlink() {
        fs::read_link(path)
            .ok()
            .map(|target| target.display().to_string())
    } else {
        None
    };
    let target_file_type = if metadata.file_type().is_symlink() {
        fs::metadata(path)
            .ok()
            .map(|target| file_type(&target).to_string())
    } else {
        None
    };
    Ok(FileEntry {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        path: path.display().to_string(),
        file_type: file_type(&metadata).to_string(),
        is_symlink: metadata.file_type().is_symlink(),
        symlink_target,
        target_file_type,
        size: metadata.len(),
        mtime: Some(mtime_secs(&metadata)),
        permissions: permissions(&metadata),
        children,
        truncated: false,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<bool, RpcError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| map_io_error(error, parent))?;
    let swap = parent.join(format!(
        ".{}.oxswp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    match write_then_rename(&swap, path, bytes) {
        Ok(()) => Ok(true),
        Err(_) => {
            fs::write(path, bytes).map_err(|error| map_io_error(error, path))?;
            Ok(false)
        }
    }
}

fn write_then_rename(swap: &Path, path: &Path, bytes: &[u8]) -> Result<(), RpcError> {
    {
        let mut file = File::create(swap).map_err(|error| map_io_error(error, swap))?;
        file.write_all(bytes)
            .map_err(|error| map_io_error(error, swap))?;
        file.sync_all().map_err(|error| map_io_error(error, swap))?;
    }
    fs::rename(swap, path).map_err(|error| map_io_error(error, path))
}

fn encode_content(bytes: &[u8]) -> Result<(String, String), RpcError> {
    String::from_utf8(bytes.to_vec())
        .map(|content| (content, plain_encoding()))
        .map_err(|_| rpc_error(ERR_INVALID_PARAMS, "File is not valid UTF-8"))
}

fn decode_content(content: &str, encoding: &str) -> Result<Vec<u8>, RpcError> {
    match encoding {
        "plain" => Ok(content.as_bytes().to_vec()),
        other => Err(rpc_error(
            ERR_INVALID_PARAMS,
            format!("Unsupported encoding: {other}"),
        )),
    }
}

fn from_params<T>(params: Value) -> Result<T, RpcError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(params).map_err(|error| rpc_error(ERR_INVALID_PARAMS, error.to_string()))
}

fn to_value<T: Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|error| rpc_error(ERR_INTERNAL, error.to_string()))
}

fn normalize_path(path: &str) -> PathBuf {
    let path = if path.is_empty() { "." } else { path };
    PathBuf::from(path)
}

fn file_type(metadata: &fs::Metadata) -> &'static str {
    if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "unknown"
    }
}

fn mtime_secs(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(4096).any(|byte| *byte == 0)
}

fn ignored_name(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target" | ".next" | "dist")
}

fn ignored_symbol_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | "node_modules"
            | "__pycache__"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | "vendor"
            | ".venv"
            | "venv"
    )
}

fn reserved_symbol_name(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "return"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "new"
            | "this"
            | "self"
    )
}

fn rpc_error(code: i32, message: impl Into<String>) -> RpcError {
    RpcError {
        code,
        message: message.into(),
    }
}

fn map_io_error(error: io::Error, path: impl AsRef<Path>) -> RpcError {
    let path = path.as_ref().display();
    let code = match error.kind() {
        io::ErrorKind::NotFound => ERR_NOT_FOUND,
        io::ErrorKind::PermissionDenied => ERR_PERMISSION,
        io::ErrorKind::AlreadyExists => ERR_ALREADY_EXISTS,
        _ => ERR_IO,
    };
    rpc_error(code, format!("{path}: {error}"))
}

fn plain_encoding() -> String {
    "plain".to_string()
}

#[cfg(unix)]
fn permissions(metadata: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    Some(format!("{:o}", metadata.permissions().mode() & 0o777))
}

#[cfg(not(unix))]
fn permissions(_metadata: &fs::Metadata) -> Option<String> {
    None
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: u32) -> Result<(), RpcError> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions).map_err(|error| map_io_error(error, path))
}

#[cfg(not(unix))]
fn set_permissions(_path: &Path, _mode: u32) -> Result<(), RpcError> {
    Err(rpc_error(
        ERR_METHOD_NOT_FOUND,
        "chmod is not supported on this platform",
    ))
}

#[allow(dead_code)]
fn _regex_is_send_sync(_: &Regex) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_index_extracts_rust_symbols_and_ignores_build_dirs() {
        let root = test_root("symbols-rust");
        let src = root.join("src");
        let ignored = root.join("target");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&ignored).unwrap();
        fs::write(
            src.join("lib.rs"),
            "pub struct Worker {}\npub async fn run_job() {}\nconst LIMIT: usize = 3;\n",
        )
        .unwrap();
        fs::write(ignored.join("noise.rs"), "pub fn ignored_symbol() {}\n").unwrap();

        let result = symbol_index(SymbolIndexParams {
            path: root.display().to_string(),
            max_files: Some(20),
        });
        let names = symbol_names(&result.symbols);

        assert_eq!(result.file_count, 1);
        assert!(names.contains(&"Worker".to_string()));
        assert!(names.contains(&"run_job".to_string()));
        assert!(names.contains(&"LIMIT".to_string()));
        assert!(!names.contains(&"ignored_symbol".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn symbol_completion_and_definitions_reuse_indexed_root() {
        let root = test_root("symbols-complete");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.py"),
            "class RemoteRunner:\n    pass\n\ndef render_result():\n    return None\n",
        )
        .unwrap();

        let _ = symbol_index(SymbolIndexParams {
            path: root.display().to_string(),
            max_files: Some(20),
        });
        let completions = symbol_complete(SymbolCompleteParams {
            path: root.display().to_string(),
            prefix: "remote".to_string(),
            limit: Some(5),
        });
        let definitions = symbol_definitions(SymbolDefinitionsParams {
            path: root.display().to_string(),
            name: "render_result".to_string(),
        });

        assert_eq!(symbol_names(&completions), vec!["RemoteRunner".to_string()]);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].line, 4);

        let _ = fs::remove_dir_all(root);
    }

    fn symbol_names(symbols: &[SymbolInfo]) -> Vec<String> {
        symbols.iter().map(|symbol| symbol.name.clone()).collect()
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "oxideterm-agent-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }
}
