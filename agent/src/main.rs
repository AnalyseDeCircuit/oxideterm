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

mod protocol;
mod fs_ops;
mod symbols;
mod watcher;

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::collections::HashMap;

use protocol::*;
use watcher::Watcher;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    // Handle --version flag for deploy version check
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("oxideterm-agent {}", VERSION);
        return;
    }

    // Stderr for agent logging (doesn't interfere with JSON-RPC on stdout)
    eprintln!("[oxideterm-agent] v{} starting (pid: {})", VERSION, std::process::id());

    let watcher = Watcher::new();
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    
    // Symbol index cache: root_path → Vec<SymbolInfo>
    let symbol_cache: Arc<Mutex<HashMap<String, Vec<SymbolInfo>>>> =
        Arc::new(Mutex::new(HashMap::new()));

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
    symbol_cache: &Arc<Mutex<HashMap<String, Vec<SymbolInfo>>>>,
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
                let root = std::path::Path::new(&params.path);
                let syms = symbols::index_directory(root, params.max_files);
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

        "symbols/complete" => match serde_json::from_value::<SymbolCompleteParams>(req.params.clone()) {
            Ok(params) => {
                let cache = symbol_cache.lock().unwrap();
                if let Some(syms) = cache.get(&params.path) {
                    let completions = symbols::complete(syms, &params.prefix, params.limit);
                    Response::ok(req.id, serde_json::to_value(completions).unwrap())
                } else {
                    // Not indexed yet — index on demand
                    drop(cache);
                    let root = std::path::Path::new(&params.path);
                    let syms = symbols::index_directory(root, 500);
                    let completions = symbols::complete(&syms, &params.prefix, params.limit);
                    if let Ok(mut cache) = symbol_cache.lock() {
                        cache.insert(params.path.clone(), syms);
                    }
                    Response::ok(req.id, serde_json::to_value(completions).unwrap())
                }
            }
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        "symbols/definitions" => match serde_json::from_value::<SymbolDefinitionsParams>(req.params.clone()) {
            Ok(params) => {
                let cache = symbol_cache.lock().unwrap();
                if let Some(syms) = cache.get(&params.path) {
                    let defs = symbols::find_definitions(syms, &params.name);
                    Response::ok(req.id, serde_json::to_value(defs).unwrap())
                } else {
                    drop(cache);
                    let root = std::path::Path::new(&params.path);
                    let syms = symbols::index_directory(root, 500);
                    let defs = symbols::find_definitions(&syms, &params.name);
                    if let Ok(mut cache) = symbol_cache.lock() {
                        cache.insert(params.path.clone(), syms);
                    }
                    Response::ok(req.id, serde_json::to_value(defs).unwrap())
                }
            }
            Err(e) => Response::err(req.id, ERR_INVALID_PARAMS, e.to_string()),
        },

        // ─── sys/* ──────────────────────────────────────────────────
        "sys/info" => {
            let info = SysInfoResult {
                version: VERSION.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                os: std::env::consts::OS.to_string(),
                pid: std::process::id(),
                capabilities: vec!["zstd".to_string()],
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
