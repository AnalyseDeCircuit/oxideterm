// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    env,
    fs::OpenOptions,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use clap::Args;
use oxideterm_ssh_launch::{TemporarySshLaunch, parse_user_host_target};
use zeroize::Zeroizing;

use crate::error::{CliError, CliResult};

#[derive(Debug, Args)]
#[command(
    long_about = "Open a temporary SSH terminal in the native GPUI application. Passwords are accepted only from stdin so they do not appear in shell history or process lists."
)]
#[command(
    after_help = "Examples:\n  oxideterm ssh user@example.com\n  oxideterm ssh user@example.com -p 2222\n  printf '%s' \"$SSH_PASSWORD\" | oxideterm ssh user@example.com --password-stdin"
)]
pub struct SshLaunchArgs {
    #[arg(help = "SSH target in user@host form")]
    pub target: String,
    #[arg(short = 'p', long, default_value_t = 22, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,
    #[arg(long, help = "Read the SSH password from stdin")]
    pub password_stdin: bool,
}

pub fn run(args: SshLaunchArgs) -> CliResult<i32> {
    let launch = build_launch(args)?;
    let request_path = write_launch_request(&launch)?;
    launch_native_gui(&request_path)?;
    println!("Opening temporary SSH terminal: {}", launch.title());
    Ok(0)
}

fn build_launch(args: SshLaunchArgs) -> CliResult<TemporarySshLaunch> {
    let default_username = current_username();
    let (username, host) = parse_user_host_target(&args.target, default_username.as_deref())
        .map_err(|error| {
            CliError::new(
                "invalid_ssh_target",
                format!("{error}. Use user@host, for example: oxideterm ssh alice@example.com"),
                false,
            )
        })?;
    let password = if args.password_stdin {
        Some(read_password_from_stdin()?)
    } else {
        None
    };
    Ok(TemporarySshLaunch {
        username,
        host,
        port: args.port,
        password,
    })
}

fn current_username() -> Option<String> {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn read_password_from_stdin() -> CliResult<Zeroizing<String>> {
    let mut password = Zeroizing::new(String::new());
    io::stdin()
        .read_to_string(&mut password)
        .map_err(|error| CliError::new("stdin_read_failed", error.to_string(), false))?;
    while password.ends_with(['\n', '\r']) {
        password.pop();
    }
    Ok(password)
}

fn write_launch_request(launch: &TemporarySshLaunch) -> CliResult<PathBuf> {
    let bytes = serde_json::to_vec(launch)
        .map_err(|error| CliError::new("ssh_launch_serialize_failed", error.to_string(), false))?;
    let path = unique_launch_path();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| CliError::new("ssh_launch_file_failed", error.to_string(), false))?;
    // The request may carry a password from stdin. Keep it out of argv/env and
    // create the handoff file with owner-only permissions on Unix platforms.
    file.write_all(&bytes)
        .map_err(|error| CliError::new("ssh_launch_file_failed", error.to_string(), false))?;
    Ok(path)
}

fn unique_launch_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "oxideterm-ssh-launch-{}-{stamp}.json",
        std::process::id()
    ))
}

fn launch_native_gui(request_path: &Path) -> CliResult<()> {
    if let Some(binary) = sibling_native_binary() {
        spawn_native_binary(&binary, request_path)
            .map_err(|error| CliError::new("native_gui_launch_failed", error.to_string(), false))?;
        return Ok(());
    }
    if spawn_from_path(request_path).is_ok() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        spawn_macos_bundle(request_path)?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::fs::remove_file(request_path);
        Err(CliError::new(
            "native_gui_not_found",
            "Could not find oxideterm-native next to the CLI or in PATH",
            false,
        ))
    }
}

fn sibling_native_binary() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?;
    let binary = dir.join(native_binary_name());
    binary.exists().then_some(binary)
}

fn spawn_from_path(request_path: &Path) -> io::Result<()> {
    spawn_native_binary(Path::new(native_binary_name()), request_path)
}

fn spawn_native_binary(binary: &Path, request_path: &Path) -> io::Result<()> {
    Command::new(binary)
        .arg("--ssh-launch-file")
        .arg(request_path)
        .spawn()
        .map(|_| ())
}

fn native_binary_name() -> &'static str {
    if cfg!(windows) {
        "oxideterm-native.exe"
    } else {
        "oxideterm-native"
    }
}

#[cfg(target_os = "macos")]
fn spawn_macos_bundle(request_path: &Path) -> CliResult<()> {
    Command::new("open")
        .args([
            "-b",
            "com.analysecircuit.OxideTerm",
            "--args",
            "--ssh-launch-file",
        ])
        .arg(request_path)
        .spawn()
        .map(|_| ())
        .map_err(|error| CliError::new("native_gui_launch_failed", error.to_string(), false))
}
