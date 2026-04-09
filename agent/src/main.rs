// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm Agent — lightweight remote helper for IDE mode.
//!
//! Runs on the remote host, communicates via stdin/stdout JSON-RPC.
//! Provides POSIX-native file operations, inotify file watching,
//! and structured search/git integration.
//!
//! ## Protocol
//!
//! Line-delimited JSON over stdin/stdout.
//! - Requests: `{"id": 1, "method": "fs/readFile", "params": {...}}`
//! - Responses: `{"id": 1, "result": {...}}` or `{"id": 1, "error": {...}}`
//! - Notifications: `{"method": "watch/event", "params": {...}}`
//!
//! ## Design Principles
//!
//! - Zero async runtime (no tokio) — uses std::thread + blocking I/O
//! - Minimal dependencies (serde + inotify only)
//! - Single static binary, musl-linked
//! - Self-cleans on parent connection close (stdin EOF)

mod fs_ops;
mod protocol;
mod symbols;
mod watcher;

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use protocol::*;
use watcher::Watcher;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SYMBOL_CACHE_CAPACITY: usize = 16;

fn compatibility_version() -> u32 {
    include_str!("../COMPATIBILITY_VERSION")
        .trim()
        .parse()
        .expect("agent compatibility version must be a valid u32")
}

fn supported_capabilities() -> Vec<String> {
    #[cfg(not(target_arch = "loongarch64"))]
    {
        return vec!["zstd".to_string()];
    }

    #[cfg(target_arch = "loongarch64")]
    {
        Vec::new()
    }
}

struct SymbolCache {
    entries: HashMap<String, Arc<Vec<SymbolInfo>>>,
    access_order: VecDeque<String>,
    capacity: usize,
}

impl SymbolCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    fn get(&mut self, path: &str) -> Option<Arc<Vec<SymbolInfo>>> {
        let symbols = self.entries.get(path).cloned();
        if symbols.is_some() {
            self.touch(path);
        }
        symbols
    }

    fn insert(&mut self, path: String, symbols: Vec<SymbolInfo>) -> Arc<Vec<SymbolInfo>> {
        let symbols = Arc::new(symbols);
        self.entries.insert(path.clone(), symbols.clone());
        self.touch(&path);
        self.evict_if_needed();
        symbols
    }

    fn touch(&mut self, path: &str) {
        if let Some(index) = self.access_order.iter().position(|entry| entry == path) {
            self.access_order.remove(index);
        }
        self.access_order.push_back(path.to_string());
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.capacity {
            if let Some(oldest) = self.access_order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

fn main() {
    // Handle --version flag for deploy version check
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!(
            "oxideterm-agent {} compat {}",
            VERSION,
            compatibility_version()
        );
        return;
    }

    // Stderr for agent logging (doesn't interfere with JSON-RPC on stdout)
    eprintln!(
        "[oxideterm-agent] v{} starting (pid: {})",
        VERSION,
        std::process::id()
    );

    let watcher = Watcher::new();
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // Symbol index cache: root_path → symbol index (bounded LRU-style cache)
    let symbol_cache: Arc<Mutex<SymbolCache>> =
        Arc::new(Mutex::new(SymbolCache::new(SYMBOL_CACHE_CAPACITY)));

    // Channel for serialized JSON responses/notifications → stdout writer thread
    let (out_tx, out_rx) = mpsc::channel::<String>();

    // Stdout writer thread — ensures atomic line writes
    let writer_handle = std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        for line in out_rx {
            if writeln!(out, "{}", line).is_err() {
                break;
            }
            let _ = out.flush();
        }
    });

    // We need a non-blocking approach to read stdin AND forward watch events.
    // Solution: stdin reader thread → main dispatcher, watch events polled via try_recv
    let (req_tx, req_rx) = mpsc::channel::<Option<Request>>();

    // Stdin reader thread
    let stdin_handle = {
        let req_tx = req_tx.clone();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            let reader = stdin.lock();
            for line_result in reader.lines() {
                match line_result {
                    Ok(line) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Request>(&line) {
                            Ok(req) => {
                                if req_tx.send(Some(req)).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("[agent] Invalid JSON-RPC: {}: {}", e, line);
                            }
                        }
                    }
                    Err(_) => {
                        // stdin closed (SSH connection dropped)
                        let _ = req_tx.send(None);
                        break;
                    }
                }
            }
            // stdin EOF — signal shutdown
            let _ = req_tx.send(None);
        })
    };

    // Main dispatch loop
    loop {
        // Try to drain watch events (non-blocking)
        loop {
            match watcher.rx.try_recv() {
                Ok(event) => {
                    let notification = Notification {
                        method: "watch/event".to_string(),
                        params: serde_json::to_value(&event).unwrap_or_default(),
                    };
                    if let Ok(json) = serde_json::to_string(&notification) {
                        let _ = out_tx.send(json);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        // Wait for next request (with timeout to check watch events periodically)
        match req_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Some(request)) => {
                let response = dispatch(&request, &watcher, &shutdown_flag, &symbol_cache);
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = out_tx.send(json);
                }
                // Check if shutdown was requested
                if shutdown_flag.load(Ordering::Relaxed) {
                    eprintln!("[agent] shutdown flag set, exiting main loop");
                    break;
                }
            }
            Ok(None) => {
                // Shutdown signal (stdin EOF)
                eprintln!("[agent] stdin closed, shutting down");
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No request — loop back to check watch events
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("[agent] request channel disconnected, shutting down");
                break;
            }
        }
    }

    // Cleanup
    watcher.stop_all();
    drop(out_tx);
    let _ = writer_handle.join();
    let _ = stdin_handle.join();
    eprintln!("[agent] shutdown complete");
}

/// Route a JSON-RPC request to the appropriate handler.
fn dispatch(
    req: &Request,
    watcher: &Watcher,
    shutdown_flag: &Arc<AtomicBool>,
    symbol_cache: &Arc<Mutex<SymbolCache>>,
) -> Response {
    match req.method.as_str() {
        // ─── fs/* ────────────────────────────────────────────────────
        "fs/readFile" => match serde_json::from_value::<ReadFileParams>(req.params.clone()) {
            Ok(params) => match fs_ops::read_file(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/writeFile" => match serde_json::from_value::<WriteFileParams>(req.params.clone()) {
            Ok(params) => match fs_ops::write_file(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/stat" => match serde_json::from_value::<StatParams>(req.params.clone()) {
            Ok(params) => match fs_ops::stat(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/listDir" => match serde_json::from_value::<ListDirParams>(req.params.clone()) {
            Ok(params) => match fs_ops::list_dir(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/listTree" => match serde_json::from_value::<ListTreeParams>(req.params.clone()) {
            Ok(params) => match fs_ops::list_tree(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/mkdir" => match serde_json::from_value::<MkdirParams>(req.params.clone()) {
            Ok(params) => match fs_ops::mkdir(params) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/remove" => match serde_json::from_value::<RemoveParams>(req.params.clone()) {
            Ok(params) => match fs_ops::remove(params) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/rename" => match serde_json::from_value::<RenameParams>(req.params.clone()) {
            Ok(params) => match fs_ops::rename(params) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "fs/chmod" => match serde_json::from_value::<ChmodParams>(req.params.clone()) {
            Ok(params) => match fs_ops::chmod(params) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        // ─── search/* ────────────────────────────────────────────────
        "search/grep" => match serde_json::from_value::<GrepParams>(req.params.clone()) {
            Ok(params) => match fs_ops::grep(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        // ─── git/* ──────────────────────────────────────────────────
        "git/status" => match serde_json::from_value::<GitStatusParams>(req.params.clone()) {
            Ok(params) => match fs_ops::git_status(params) {
                Ok(result) => Response::ok(req.id, serde_json::to_value(result).unwrap()),
                Err((code, msg)) => Response::err(req.id, code, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        // ─── watch/* ────────────────────────────────────────────────
        "watch/start" => match serde_json::from_value::<WatchStartParams>(req.params.clone()) {
            Ok(params) => match watcher.start(params.path, params.ignore) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err(msg) => Response::err(req.id, ERR_INTERNAL, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "watch/stop" => match serde_json::from_value::<WatchStopParams>(req.params.clone()) {
            Ok(params) => match watcher.stop(&params.path) {
                Ok(()) => Response::ok(req.id, serde_json::json!({})),
                Err(msg) => Response::err(req.id, ERR_INTERNAL, msg),
            },
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        // ─── symbols/* ────────────────────────────────────────────
        "symbols/index" => match serde_json::from_value::<SymbolIndexParams>(req.params.clone()) {
            Ok(params) => {
                let root = fs_ops::resolve_path(&params.path);
                let syms = symbols::index_directory(&root, params.max_files);
                let file_count = syms.len() as u32;
                // Cache the index for subsequent complete/definitions calls
                if let Ok(mut cache) = symbol_cache.lock() {
                    cache.insert(params.path.clone(), syms.clone());
                }
                let result = SymbolIndexResult {
                    symbols: syms,
                    file_count,
                };
                Response::ok(req.id, serde_json::to_value(result).unwrap())
            }
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "symbols/complete" => {
            match serde_json::from_value::<SymbolCompleteParams>(req.params.clone()) {
                Ok(params) => {
                    let cached = symbol_cache
                        .lock()
                        .ok()
                        .and_then(|mut cache| cache.get(&params.path));
                    if let Some(syms) = cached {
                        let completions =
                            symbols::complete(syms.as_ref(), &params.prefix, params.limit);
                        Response::ok(req.id, serde_json::to_value(completions).unwrap())
                    } else {
                        let root = fs_ops::resolve_path(&params.path);
                        let syms = symbols::index_directory(&root, 500);
                        let completions = symbols::complete(&syms, &params.prefix, params.limit);
                        if let Ok(mut cache) = symbol_cache.lock() {
                            cache.insert(params.path.clone(), syms);
                        }
                        Response::ok(req.id, serde_json::to_value(completions).unwrap())
                    }
                }
                Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
            }
        }

        "symbols/definitions" => {
            match serde_json::from_value::<SymbolDefinitionsParams>(req.params.clone()) {
                Ok(params) => {
                    let cached = symbol_cache
                        .lock()
                        .ok()
                        .and_then(|mut cache| cache.get(&params.path));
                    if let Some(syms) = cached {
                        let defs = symbols::find_definitions(syms.as_ref(), &params.name);
                        Response::ok(req.id, serde_json::to_value(defs).unwrap())
                    } else {
                        let root = fs_ops::resolve_path(&params.path);
                        let syms = symbols::index_directory(&root, 500);
                        let defs = symbols::find_definitions(&syms, &params.name);
                        if let Ok(mut cache) = symbol_cache.lock() {
                            cache.insert(params.path.clone(), syms);
                        }
                        Response::ok(req.id, serde_json::to_value(defs).unwrap())
                    }
                }
                Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
            }
        }

        // ─── sys/* ──────────────────────────────────────────────────
        "sys/info" => {
            let info = SysInfoResult {
                version: VERSION.to_string(),
                compatibility_version: compatibility_version(),
                arch: std::env::consts::ARCH.to_string(),
                os: std::env::consts::OS.to_string(),
                pid: std::process::id(),
                capabilities: supported_capabilities(),
            };
            Response::ok(req.id, serde_json::to_value(info).unwrap())
        }

        "sys/ping" => Response::ok(req.id, serde_json::json!({"pong": true})),

        "sys/shutdown" => {
            eprintln!("[agent] shutdown requested via sys/shutdown");
            shutdown_flag.store(true, Ordering::Relaxed);
            Response::ok(req.id, serde_json::json!({"ok": true}))
        }

        _ => Response::err(
            req.id,
            ERR_METHOD_NOT_FOUND,
            format!("Unknown method: {}", req.method),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(name: &str) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Function,
            path: format!("/tmp/{name}.rs"),
            line: 1,
            column: 1,
            container: None,
        }
    }

    #[test]
    fn symbol_cache_evicts_oldest_entry_when_capacity_is_exceeded() {
        let mut cache = SymbolCache::new(2);
        cache.insert("/a".to_string(), vec![symbol("a")]);
        cache.insert("/b".to_string(), vec![symbol("b")]);
        cache.insert("/c".to_string(), vec![symbol("c")]);

        assert!(cache.get("/a").is_none());
        assert!(cache.get("/b").is_some());
        assert!(cache.get("/c").is_some());
    }

    #[test]
    fn symbol_cache_touches_entries_on_read() {
        let mut cache = SymbolCache::new(2);
        cache.insert("/a".to_string(), vec![symbol("a")]);
        cache.insert("/b".to_string(), vec![symbol("b")]);
        assert!(cache.get("/a").is_some());

        cache.insert("/c".to_string(), vec![symbol("c")]);

        assert!(cache.get("/a").is_some());
        assert!(cache.get("/b").is_none());
        assert!(cache.get("/c").is_some());
    }
}
