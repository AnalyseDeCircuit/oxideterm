#![cfg(unix)]

use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;

fn spawn_status_server(
    socket_path: &Path,
    status: serde_json::Value,
) -> std::thread::JoinHandle<()> {
    let listener = UnixListener::bind(socket_path).unwrap();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let reader_stream = stream.try_clone().unwrap();
        let mut reader = BufReader::new(reader_stream);
        let mut line = String::new();

        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }

            let request: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            let id = request.get("id").and_then(|value| value.as_u64()).unwrap();
            let method = request
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap();

            let response = match method {
                "status" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": status,
                }),
                _ => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {method}"),
                    }
                }),
            };

            stream
                .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                .unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        }
    })
}

fn run_doctor(socket_path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxt"))
        .env_remove("OXIDETERM_SOCK")
        .env_remove("OXIDETERM_PIPE")
        .args([
            "--socket",
            socket_path.to_str().unwrap(),
            "--json",
            "doctor",
        ])
        .output()
        .unwrap()
}

fn find_item<'a>(report: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap()
}

#[test]
fn doctor_reports_running_gui() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("doctor-running.sock");
    let server = spawn_status_server(
        &socket_path,
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "pid": 4242,
            "cli_api": { "version": 1, "min_supported": 1 },
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }),
    );

    let output = run_doctor(&socket_path);
    server.join().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["endpoint"]["source"], "CLI flag (--socket)");
    assert_eq!(find_item(&report, "endpoint_presence")["status"], "ok");
    assert_eq!(find_item(&report, "gui_connectivity")["status"], "ok");
    assert_eq!(find_item(&report, "cli_api_compatibility")["status"], "ok");
}

#[test]
fn doctor_reports_missing_gui_without_hiding_local_diagnostics() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("doctor-missing.sock");

    let output = run_doctor(&socket_path);

    assert_eq!(output.status.code(), Some(1));

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(find_item(&report, "endpoint_resolution")["status"], "warn");
    assert_eq!(find_item(&report, "endpoint_presence")["status"], "fail");
    assert_eq!(find_item(&report, "endpoint_ownership")["status"], "warn");
    assert_eq!(find_item(&report, "gui_connectivity")["status"], "fail");
    assert_eq!(
        find_item(&report, "cli_api_compatibility")["status"],
        "warn"
    );
    assert!(find_item(&report, "gui_connectivity")["detail"]
        .as_str()
        .unwrap()
        .contains("socket not found"));
}
