//! File system watcher using inotify (Linux) or polling fallback.
//!
//! Runs in a dedicated thread, sends `watch/event` notifications
//! through a channel that the main loop consumes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::WatchEvent;

/// Watcher handle — manages background watch threads.
pub struct Watcher {
    /// Channel to receive watch events from background threads.
    pub rx: mpsc::Receiver<WatchEvent>,
    tx: mpsc::Sender<WatchEvent>,
    /// Active watch sessions.
    watches: Arc<Mutex<HashMap<String, WatchHandle>>>,
}

struct WatchHandle {
    /// Signal the watch thread to stop.
    stop: Arc<Mutex<bool>>,
}

impl Watcher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            rx,
            tx,
            watches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start watching a directory path.
    pub fn start(&self, path: String, ignore: Vec<String>) -> Result<(), String> {
        let mut watches = self.watches.lock().map_err(|e| e.to_string())?;

        // Already watching?
        if watches.contains_key(&path) {
            return Ok(());
        }

        let stop = Arc::new(Mutex::new(false));
        let handle = WatchHandle {
            stop: Arc::clone(&stop),
        };

        let tx = self.tx.clone();
        let watch_path = path.clone();

        std::thread::spawn(move || {
            watch_thread(&watch_path, &ignore, &tx, &stop);
        });

        watches.insert(path, handle);
        Ok(())
    }

    /// Stop watching a directory path.
    pub fn stop(&self, path: &str) -> Result<(), String> {
        let mut watches = self.watches.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = watches.remove(path) {
            if let Ok(mut stop) = handle.stop.lock() {
                *stop = true;
            }
        }
        Ok(())
    }

    /// Stop all watches (shutdown).
    pub fn stop_all(&self) {
        if let Ok(mut watches) = self.watches.lock() {
            for (_, handle) in watches.drain() {
                if let Ok(mut stop) = handle.stop.lock() {
                    *stop = true;
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Linux inotify implementation
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
fn watch_thread(
    path: &str,
    ignore: &[String],
    tx: &mpsc::Sender<WatchEvent>,
    stop: &Arc<Mutex<bool>>,
) {
    use inotify::{Inotify, WatchMask};
    use std::os::unix::io::AsRawFd;
    use std::path::{Path, PathBuf};

    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[agent] Failed to init inotify: {}", e);
            return;
        }
    };

    // Set inotify fd to non-blocking so read_events won't block forever
    let fd = inotify.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    // Watch the root directory
    let mask = WatchMask::CREATE
        | WatchMask::DELETE
        | WatchMask::MODIFY
        | WatchMask::MOVED_FROM
        | WatchMask::MOVED_TO
        | WatchMask::CLOSE_WRITE;

    // Map watch descriptor → path for nested directories
    let mut wd_to_path: HashMap<inotify::WatchDescriptor, PathBuf> = HashMap::new();

    // Add watchers recursively
    add_watches_recursive(
        Path::new(path),
        ignore,
        &mut inotify,
        mask,
        &mut wd_to_path,
    );

    let mut buffer = [0; 4096];

    // Debounce: accumulate events for 100ms before sending
    let mut pending: HashMap<String, (String, Instant)> = HashMap::new();
    let debounce_duration = Duration::from_millis(100);

    loop {
        // Check stop signal
        if let Ok(s) = stop.lock() {
            if *s {
                break;
            }
        }

        // Read events with timeout
        match inotify.read_events(&mut buffer) {
            Ok(events) => {
                for event in events {
                    let dir_path = wd_to_path
                        .get(&event.wd)
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from(path));

                    let file_path = if let Some(name) = &event.name {
                        dir_path.join(name)
                    } else {
                        dir_path.clone()
                    };

                    let file_path_str = file_path.to_string_lossy().to_string();

                    // Skip ignored patterns
                    if let Some(name) = file_path.file_name() {
                        let name_str = name.to_string_lossy();
                        if ignore.iter().any(|ig| *ig == *name_str)
                            || name_str.starts_with(".oxtmp.")
                            || name_str.ends_with(".oxswp")
                        {
                            continue;
                        }
                    }

                    let kind = if event.mask.contains(inotify::EventMask::CREATE)
                        || event.mask.contains(inotify::EventMask::MOVED_TO)
                    {
                        // If a new directory was created, add a watcher for it
                        if event.mask.contains(inotify::EventMask::ISDIR) {
                            add_watches_recursive(
                                &file_path,
                                ignore,
                                &mut inotify,
                                mask,
                                &mut wd_to_path,
                            );
                        }
                        "create"
                    } else if event.mask.contains(inotify::EventMask::DELETE)
                        || event.mask.contains(inotify::EventMask::MOVED_FROM)
                    {
                        "delete"
                    } else if event.mask.contains(inotify::EventMask::MODIFY)
                        || event.mask.contains(inotify::EventMask::CLOSE_WRITE)
                    {
                        "modify"
                    } else {
                        continue;
                    };

                    // Debounce: update pending event
                    pending.insert(file_path_str, (kind.to_string(), Instant::now()));
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No events ready — check debounce queue
            }
            Err(e) => {
                eprintln!("[agent] inotify read error: {}", e);
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        }

        // Flush debounced events
        let now = Instant::now();
        let mut flushed = Vec::new();
        for (path_key, (kind, timestamp)) in &pending {
            if now.duration_since(*timestamp) >= debounce_duration {
                let _ = tx.send(WatchEvent {
                    path: path_key.clone(),
                    kind: kind.clone(),
                });
                flushed.push(path_key.clone());
            }
        }
        for key in flushed {
            pending.remove(&key);
        }

        // Small sleep to avoid busy-looping
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(target_os = "linux")]
fn add_watches_recursive(
    dir: &Path,
    ignore: &[String],
    inotify: &mut inotify::Inotify,
    mask: inotify::WatchMask,
    wd_map: &mut HashMap<inotify::WatchDescriptor, PathBuf>,
) {
    // Add watch for this directory
    match inotify.watches().add(dir, mask) {
        Ok(wd) => {
            wd_map.insert(wd, dir.to_path_buf());
        }
        Err(e) => {
            eprintln!(
                "[agent] Failed to watch {}: {}",
                dir.display(),
                e
            );
            return;
        }
    }

    // Recurse into subdirectories
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip ignored directories
            if ignore.iter().any(|ig| *ig == *name_str)
                || name_str == ".git"
                || name_str == "node_modules"
                || name_str == ".hg"
                || name_str == "__pycache__"
                || name_str == "target"
            {
                continue;
            }

            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    add_watches_recursive(&entry.path(), ignore, inotify, mask, wd_map);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Non-Linux polling fallback
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "linux"))]
fn watch_thread(
    _path: &str,
    _ignore: &[String],
    _tx: &mpsc::Sender<WatchEvent>,
    stop: &Arc<Mutex<bool>>,
) {
    // On non-Linux platforms, the watcher is a no-op.
    // File watching will rely on the polling-based approach
    // already used in the SFTP fallback mode.
    eprintln!("[agent] File watching not supported on this platform, using no-op watcher");
    loop {
        if let Ok(s) = stop.lock() {
            if *s {
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}
