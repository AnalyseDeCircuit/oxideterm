// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use fs2::FileExt;
use oxideterm_ssh_launch::TemporarySshLaunch;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const INSTANCE_FILENAME_PREFIX: &str = "oxideterm-native-instance";
const FORWARD_RETRY_COUNT: usize = 40;
const FORWARD_RETRY_DELAY: Duration = Duration::from_millis(50);
const MAX_INSTANCE_REQUEST_BYTES: u64 = 64 * 1024;

// The application keeps this shared receiver alive while individual workspace
// windows attach and detach from the single-instance event stream.
pub(crate) type SingleInstanceReceiver = Arc<Mutex<mpsc::Receiver<SingleInstanceEvent>>>;

pub(crate) enum SingleInstanceOutcome {
    Primary {
        _guard: SingleInstanceGuard,
        receiver: SingleInstanceReceiver,
    },
    Forwarded,
}

#[derive(Debug)]
pub(crate) enum SingleInstanceEvent {
    ShowMainWindow,
    OpenTemporarySsh(TemporarySshLaunch),
}

pub(crate) struct SingleInstanceGuard {
    _lock_file: File,
    state_path: PathBuf,
}

#[derive(Clone, Debug)]
struct InstancePaths {
    lock_path: PathBuf,
    state_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InstanceState {
    port: u16,
    token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InstanceRequest {
    token: String,
    ssh_launch_file: Option<PathBuf>,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.state_path);
    }
}

impl InstancePaths {
    fn for_data_dir(data_dir: impl Into<PathBuf>, scope: &str) -> Self {
        let data_dir = data_dir.into();
        Self {
            lock_path: data_dir.join(format!("{INSTANCE_FILENAME_PREFIX}-{scope}.lock")),
            state_path: data_dir.join(format!("{INSTANCE_FILENAME_PREFIX}-{scope}.json")),
        }
    }
}

fn instance_scope_for_build(version: &str, development: bool) -> &'static str {
    // Development binaries must coexist with installed channels while each
    // installed channel retains strict single-instance behavior of its own.
    if development {
        return "development";
    }
    if version.contains("gpui-preview")
        || version.contains("native-preview")
        || version.contains("rustnative-preview")
    {
        return "gpui-preview";
    }
    if version.contains("beta") {
        return "beta";
    }
    "stable"
}

fn current_instance_scope() -> &'static str {
    instance_scope_for_build(env!("CARGO_PKG_VERSION"), cfg!(debug_assertions))
}

pub(crate) fn single_instance_runtime_paths_for_data_dir(data_dir: &Path) -> [PathBuf; 2] {
    // Startup creates these files before the pre-2.0 snapshot check. Exposing
    // their exact paths prevents current runtime state from looking like 1.x data.
    let paths = InstancePaths::for_data_dir(data_dir, current_instance_scope());
    [paths.lock_path, paths.state_path]
}

pub(crate) fn acquire_or_forward(
    ssh_launch_path: Option<PathBuf>,
) -> Result<SingleInstanceOutcome> {
    let settings_path = oxideterm_settings::default_settings_path();
    let data_dir = settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    acquire_or_forward_with_paths(
        InstancePaths::for_data_dir(data_dir, current_instance_scope()),
        ssh_launch_path,
    )
}

fn acquire_or_forward_with_paths(
    paths: InstancePaths,
    ssh_launch_path: Option<PathBuf>,
) -> Result<SingleInstanceOutcome> {
    let data_dir = paths
        .lock_path
        .parent()
        .ok_or_else(|| anyhow!("single-instance lock path has no parent"))?;
    fs::create_dir_all(data_dir).with_context(|| {
        format!(
            "failed to create single-instance directory {}",
            data_dir.display()
        )
    })?;

    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&paths.lock_path)
        .with_context(|| {
            format!(
                "failed to open single-instance lock {}",
                paths.lock_path.display()
            )
        })?;

    match lock_file.try_lock_exclusive() {
        Ok(()) => start_primary(lock_file, paths),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            forward_to_primary(&paths.state_path, ssh_launch_path).with_context(|| {
                format!(
                    "failed to forward launch request through {}",
                    paths.state_path.display()
                )
            })?;
            Ok(SingleInstanceOutcome::Forwarded)
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to acquire single-instance lock {}",
                paths.lock_path.display()
            )
        }),
    }
}

fn start_primary(lock_file: File, paths: InstancePaths) -> Result<SingleInstanceOutcome> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .context("failed to bind single-instance handoff listener")?;
    let port = listener
        .local_addr()
        .context("failed to read single-instance handoff listener address")?
        .port();
    let token = Uuid::new_v4().to_string();
    let state = InstanceState {
        port,
        token: token.clone(),
    };
    fs::write(
        &paths.state_path,
        serde_json::to_vec(&state).context("failed to encode single-instance state")?,
    )
    .with_context(|| {
        format!(
            "failed to write single-instance state {}",
            paths.state_path.display()
        )
    })?;

    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("oxideterm-single-instance".to_string())
        .spawn(move || accept_forwarded_requests(listener, token, tx))
        .context("failed to spawn single-instance handoff listener")?;

    Ok(SingleInstanceOutcome::Primary {
        _guard: SingleInstanceGuard {
            _lock_file: lock_file,
            state_path: paths.state_path,
        },
        receiver: Arc::new(Mutex::new(rx)),
    })
}

fn forward_to_primary(state_path: &Path, ssh_launch_path: Option<PathBuf>) -> Result<()> {
    let mut last_error = None;
    for _ in 0..FORWARD_RETRY_COUNT {
        match read_instance_state(state_path)
            .and_then(|state| send_instance_request(&state, ssh_launch_path.clone()))
        {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        thread::sleep(FORWARD_RETRY_DELAY);
    }

    // If forwarding fails, this process is the only owner of a one-shot CLI
    // handoff file. Remove it so a stdin password is not left behind on disk.
    if let Some(path) = ssh_launch_path {
        let _ = fs::remove_file(path);
    }

    Err(last_error.unwrap_or_else(|| anyhow!("single-instance handoff listener was unavailable")))
}

fn read_instance_state(path: &Path) -> Result<InstanceState> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).context("invalid single-instance state")
}

fn send_instance_request(state: &InstanceState, ssh_launch_path: Option<PathBuf>) -> Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", state.port))
        .context("failed to connect to existing OxideTerm instance")?;
    let request = InstanceRequest {
        token: state.token.clone(),
        ssh_launch_file: ssh_launch_path,
    };
    let bytes = serde_json::to_vec(&request).context("failed to encode launch request")?;
    stream
        .write_all(&bytes)
        .context("failed to write launch request")
}

fn accept_forwarded_requests(
    listener: TcpListener,
    token: String,
    tx: mpsc::Sender<SingleInstanceEvent>,
) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        if let Ok(events) = events_from_stream(stream, &token) {
            for event in events {
                let _ = tx.send(event);
            }
        }
    }
}

fn events_from_stream(mut stream: TcpStream, token: &str) -> Result<Vec<SingleInstanceEvent>> {
    let mut bytes = Vec::new();
    Read::by_ref(&mut stream)
        .take(MAX_INSTANCE_REQUEST_BYTES)
        .read_to_end(&mut bytes)
        .context("failed to read single-instance request")?;
    let request: InstanceRequest =
        serde_json::from_slice(&bytes).context("invalid single-instance request")?;
    if request.token != token {
        return Err(anyhow!("single-instance token mismatch"));
    }

    let mut events = vec![SingleInstanceEvent::ShowMainWindow];
    if let Some(path) = request.ssh_launch_file {
        match read_ssh_launch_file(Some(path)) {
            Ok(Some(launch)) => events.push(SingleInstanceEvent::OpenTemporarySsh(launch)),
            Ok(None) => {}
            Err(error) => eprintln!("failed to read forwarded SSH launch request: {error}"),
        }
    }
    Ok(events)
}

pub(crate) fn read_ssh_launch_file(path: Option<PathBuf>) -> Result<Option<TemporarySshLaunch>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read SSH launch file {}", path.display()))?;
    // The CLI handoff file may contain a stdin password. Delete it only after
    // the owning app instance has accepted the request.
    let _ = fs::remove_file(&path);
    serde_json::from_slice(&bytes).context("invalid SSH launch request")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_second_launch_to_primary_instance() {
        let data_dir =
            std::env::temp_dir().join(format!("oxideterm-single-instance-test-{}", Uuid::new_v4()));
        let paths = InstancePaths::for_data_dir(&data_dir, "test");

        let SingleInstanceOutcome::Primary {
            _guard: guard,
            receiver,
        } = acquire_or_forward_with_paths(paths.clone(), None).unwrap()
        else {
            panic!("first launch should become the primary instance");
        };
        let forwarded = acquire_or_forward_with_paths(paths, None).unwrap();
        assert!(matches!(forwarded, SingleInstanceOutcome::Forwarded));

        assert!(matches!(
            receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            SingleInstanceEvent::ShowMainWindow
        ));

        drop(guard);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn installed_channels_and_development_use_distinct_instance_paths() {
        let data_dir = Path::new("/tmp/oxideterm-instance-scopes");
        let development = InstancePaths::for_data_dir(data_dir, "development");
        let preview = InstancePaths::for_data_dir(data_dir, "gpui-preview");
        let stable = InstancePaths::for_data_dir(data_dir, "stable");

        assert_ne!(development.lock_path, preview.lock_path);
        assert_ne!(preview.lock_path, stable.lock_path);
        assert_ne!(development.state_path, stable.state_path);
    }

    #[test]
    fn build_versions_map_to_stable_instance_scopes() {
        assert_eq!(instance_scope_for_build("2.0.0", false), "stable");
        assert_eq!(
            instance_scope_for_build("2.0.0-gpui-preview.16", false),
            "gpui-preview"
        );
        assert_eq!(instance_scope_for_build("2.0.0-beta.1", false), "beta");
        assert_eq!(
            instance_scope_for_build("2.0.0-gpui-preview.16", true),
            "development"
        );
    }

    #[test]
    fn shared_receiver_survives_workspace_holder_drop() {
        let (tx, rx) = mpsc::channel();
        let application_receiver = Arc::new(Mutex::new(rx));
        let first_workspace_receiver = application_receiver.clone();
        let ssh_launch = TemporarySshLaunch {
            username: "test-user".to_string(),
            host: "example.test".to_string(),
            port: 22,
            password: None,
        };

        drop(first_workspace_receiver);
        tx.send(SingleInstanceEvent::ShowMainWindow).unwrap();
        tx.send(SingleInstanceEvent::OpenTemporarySsh(ssh_launch.clone()))
            .unwrap();

        let receiver = application_receiver.lock().unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            SingleInstanceEvent::ShowMainWindow
        ));
        let SingleInstanceEvent::OpenTemporarySsh(received_launch) = receiver.try_recv().unwrap()
        else {
            panic!("second event should retain the forwarded SSH launch");
        };
        assert_eq!(received_launch, ssh_launch);
    }
}
