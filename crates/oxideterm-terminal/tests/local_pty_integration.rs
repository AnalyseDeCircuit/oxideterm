// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#![cfg(unix)]

use std::{collections::HashMap, path::PathBuf, time::Duration};

use oxideterm_terminal::{
    GraphicsOptions, LocalPtyConfig, LocalPtySession, ShellInfo, TerminalCwdIntegrationLaunchState,
    TerminalEncoding, TerminalEvent,
};

#[test]
fn local_pty_shutdown_cleans_background_child_processes() {
    let marker_path = std::env::temp_dir().join(format!(
        "oxideterm-pty-child-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let script = r#"
marker=$1
( trap "" TERM; while :; do sleep 5; done ) &
child=$!
printf '%s\n' "$child" > "$marker"
wait
"#;
    let mut config = LocalPtyConfig::default();
    config.shell = Some(
        ShellInfo::new("test-sh", "Test sh", "/bin/sh").with_args(vec![
            "-c".to_string(),
            script.to_string(),
            "oxideterm-pty-test".to_string(),
            marker_path.display().to_string(),
        ]),
    );
    config.load_profile = false;

    let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
        80,
        24,
        config,
        GraphicsOptions::default(),
        TerminalEncoding::Utf8,
        100,
    )
    .expect("spawn local PTY");

    let child_pid = wait_for_child_pid(&marker_path);
    assert!(
        unix_process_is_running(child_pid),
        "test child should be running before PTY shutdown"
    );

    session.shutdown();

    assert_eventually(
        Duration::from_secs(3),
        || !unix_process_is_running(child_pid),
        "background child should stop after PTY shutdown",
    );
    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn local_available_shell_integrations_report_initial_cwd() {
    let expected_cwd = std::env::temp_dir();
    for shell_id in ["bash", "zsh", "fish", "pwsh"] {
        let Some(shell_path) = find_test_executable(shell_id) else {
            continue;
        };
        assert_local_shell_reports_initial_cwd(shell_id, shell_path, &expected_cwd);
    }
}

#[test]
fn integrated_zsh_loads_history_from_user_config_in_a_real_pty() {
    let Some(shell_path) = find_test_executable("zsh") else {
        return;
    };
    let user_config = tempfile::tempdir().expect("temporary user Zsh directory");
    std::fs::write(
        user_config.path().join(".zshrc"),
        "HISTFILE=\"$ZDOTDIR/.zsh_history\"\nHISTSIZE=100\nSAVEHIST=100\nsetopt share_history\n",
    )
    .expect("write user Zsh config");
    std::fs::write(
        user_config.path().join(".zsh_history"),
        "oxideterm-history-probe\n",
    )
    .expect("write user Zsh history");
    let config = LocalPtyConfig {
        shell: Some(ShellInfo::new("zsh", "Zsh", shell_path)),
        env: HashMap::from([(
            "ZDOTDIR".to_string(),
            user_config.path().display().to_string(),
        )]),
        current_directory_shell_integration: true,
        ..LocalPtyConfig::default()
    };
    let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
        80,
        24,
        config,
        GraphicsOptions::default(),
        TerminalEncoding::Utf8,
        100,
    )
    .expect("spawn integrated Zsh PTY");

    std::thread::sleep(Duration::from_secs(1));
    session.drain_output();
    session.take_events();
    session
        .write_text("print -r -- OXIDETERM_HISTORY_COUNT=${#history[@]}\n")
        .expect("query Zsh history count");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut screen = String::new();
    let parse_history_count = |output: &str| {
        output.rsplit("OXIDETERM_HISTORY_COUNT=").find_map(|value| {
            let digits = value
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            (!digits.is_empty())
                .then(|| digits.parse::<usize>().ok())
                .flatten()
        })
    };
    while std::time::Instant::now() < deadline && parse_history_count(&screen).is_none() {
        session.drain_output();
        screen = session
            .snapshot()
            .lines
            .iter()
            .map(|row| row.cells.iter().map(|cell| cell.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        std::thread::sleep(Duration::from_millis(10));
    }
    session.shutdown();

    let count = parse_history_count(&screen).expect("history count response");
    assert!(
        count > 0,
        "integrated Zsh PTY did not load configured history"
    );
}

fn assert_local_shell_reports_initial_cwd(
    shell_id: &str,
    shell_path: PathBuf,
    expected_cwd: &std::path::Path,
) {
    let config = LocalPtyConfig {
        shell: Some(ShellInfo::new(shell_id, shell_id, shell_path)),
        cwd: Some(expected_cwd.to_path_buf()),
        load_profile: false,
        current_directory_shell_integration: true,
        ..LocalPtyConfig::default()
    };
    let mut session = LocalPtySession::spawn_with_config_graphics_and_encoding(
        80,
        24,
        config,
        GraphicsOptions::default(),
        TerminalEncoding::Utf8,
        100,
    )
    .unwrap_or_else(|error| panic!("spawn integrated local {shell_id} PTY: {error}"));
    assert_eq!(
        session.shell_integration_launch_state(),
        TerminalCwdIntegrationLaunchState::Prepared
    );

    let deadline = std::time::Instant::now() + local_shell_cwd_report_timeout(shell_id);
    let mut reported_cwd = None;
    while std::time::Instant::now() < deadline && reported_cwd.is_none() {
        session.drain_output();
        reported_cwd = session
            .take_events()
            .into_iter()
            .find_map(|event| match event {
                TerminalEvent::CwdChanged { cwd, .. } => Some(cwd),
                _ => None,
            });
        std::thread::sleep(Duration::from_millis(10));
    }
    session.shutdown();

    assert_eq!(
        reported_cwd.map(PathBuf::from),
        Some(expected_cwd.canonicalize().unwrap()),
        "{shell_id} did not report its initial cwd"
    );
}

fn local_shell_cwd_report_timeout(shell_id: &str) -> Duration {
    // PowerShell's managed runtime can take longer to start on a contended CI runner.
    if shell_id == "pwsh" {
        Duration::from_secs(15)
    } else {
        Duration::from_secs(5)
    }
}

fn find_test_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn wait_for_child_pid(marker_path: &std::path::Path) -> u32 {
    let mut pid = None;
    assert_eventually(
        Duration::from_secs(3),
        || {
            pid = std::fs::read_to_string(marker_path)
                .ok()
                .and_then(|value| value.trim().parse::<u32>().ok());
            pid.is_some()
        },
        "PTY script should write background child PID",
    );
    pid.unwrap()
}

fn unix_process_is_running(pid: u32) -> bool {
    let status = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if status != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EPERM) {
        return false;
    }

    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output();
    let Ok(output) = output else {
        return true;
    };
    if !output.status.success() {
        return false;
    }

    !String::from_utf8_lossy(&output.stdout).contains('Z')
}

fn assert_eventually(timeout: Duration, mut predicate: impl FnMut() -> bool, message: &str) {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(predicate(), "{message}");
}
