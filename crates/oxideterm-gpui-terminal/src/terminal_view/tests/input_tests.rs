use super::*;
use gpui::Keystroke;
use oxideterm_terminal::TermMode;

#[test]
#[ignore = "requires ConPTY staged beside the test executable; see local-development.md"]
fn bundled_conpty_delivers_distinct_ctrl_j_and_enter_events() {
    use crate::TerminalPane;
    use oxideterm_terminal::{LocalPtyConfig, ShellInfo};
    use std::{
        fs,
        time::{Duration, Instant},
    };

    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("input.ps1");
    let ready = directory.path().join("ready");
    let started = directory.path().join("ready.started");
    let error = directory.path().join("ready.error");
    let result = directory.path().join("result.json");
    fs::write(
        &script,
        r#"
param($Ready, $Result)
$ErrorActionPreference = 'Stop'
try {
    [IO.File]::WriteAllText("$Ready.started", 'started')
    [Console]::TreatControlCAsInput = $true
    [IO.File]::WriteAllText($Ready, 'ready')
    $events = @(for ($i = 0; $i -lt 7; $i++) {
        $key = [Console]::ReadKey($true)
        @{ key = [int]$key.Key; character = [int]$key.KeyChar; modifiers = $key.Modifiers.ToString() }
    })
    # Publish only a complete result; the parent may stop the PTY once it sees this file.
    [IO.File]::WriteAllText("$Result.tmp", (ConvertTo-Json -InputObject $events -Compress))
    [IO.File]::Move("$Result.tmp", $Result)
} catch {
    [IO.File]::WriteAllText("$Ready.error.tmp", ($_ | Out-String))
    [IO.File]::Move("$Ready.error.tmp", "$Ready.error")
    exit 1
}
"#,
    )
    .unwrap();
    let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let config = LocalPtyConfig {
        shell: Some(
            ShellInfo::new("input-probe", "Input probe", powershell).with_args(vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                script.to_string_lossy().into_owned(),
                ready.to_string_lossy().into_owned(),
                result.to_string_lossy().into_owned(),
            ]),
        ),
        ..Default::default()
    };
    let terminal =
        TerminalPane::local_shared_session(config, &TerminalUiPreferences::default()).unwrap();
    let fail = |stage: &str| -> ! {
        let mut session = terminal.lock();
        session.read_pending();
        let lifecycle = session.lifecycle();
        let mode = session.mode();
        // This isolated fixture runs fixed input with profiles disabled, so its
        // terminal output can explain startup failures without logging user sessions.
        let output = session
            .snapshot()
            .lines
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");
        let script_error = fs::read_to_string(&error).unwrap_or_default();
        session.shutdown();
        panic!(
            "{stage}; script_started={}; ready={}; lifecycle={lifecycle:?}; mode={mode:?}\nPowerShell error: {script_error}\nTerminal output:\n{output}",
            started.exists(),
            ready.exists()
        );
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let running = {
            let mut session = terminal.lock();
            session.read_pending();
            session.lifecycle().is_running()
        };
        if error.exists() || !running {
            fail("console reader exited before becoming ready");
        }
        if ready.exists() && terminal.lock().mode().contains(TermMode::WIN32_INPUT) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !ready.exists() {
        fail("console reader did not become ready within 15 seconds");
    }
    let mode = terminal.lock().mode();
    if !mode.contains(TermMode::WIN32_INPUT) {
        fail("bundled ConPTY did not negotiate mode 9001");
    }
    for key in ["ctrl-j", "ctrl-enter", "shift-enter", "enter", "ctrl-i"] {
        for event in [KittyKeyEventType::Press, KittyKeyEventType::Release] {
            let sequence =
                oxideterm_key_escape_sequence(&Keystroke::parse(key).unwrap(), &mode, false, event)
                    .unwrap();
            terminal
                .lock()
                .write_protocol_bytes(sequence.as_bytes())
                .unwrap();
        }
    }
    terminal
        .lock()
        .write_protocol_bytes("中文".as_bytes())
        .unwrap();
    while Instant::now() < deadline && !result.exists() {
        let running = {
            let mut session = terminal.lock();
            session.read_pending();
            session.lifecycle().is_running()
        };
        if error.exists() || (!running && !result.exists()) {
            fail("console reader exited before publishing key events");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !result.exists() {
        fail("console reader did not publish key events before the deadline");
    }
    terminal.lock().shutdown();
    let events: serde_json::Value =
        serde_json::from_slice(&fs::read(result).expect("console reader result")).unwrap();
    assert_eq!(
        events,
        serde_json::json!([
            {"key": 74, "character": 10, "modifiers": "Control"},
            {"key": 13, "character": 10, "modifiers": "Control"},
            {"key": 13, "character": 13, "modifiers": "Shift"},
        {"key": 13, "character": 13, "modifiers": "0"},
            {"key": 73, "character": 9, "modifiers": "Control"},
        {"key": 0, "character": 20013, "modifiers": "0"},
        {"key": 0, "character": 25991, "modifiers": "0"},
        ])
    );
}
