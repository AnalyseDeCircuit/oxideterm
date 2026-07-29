// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // Keep argument parsing inside the updater crate so the helper binary stays
    // a tiny process boundary around the staged replacement engine.
    let portable_mode = std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == "portable");
    let result = if portable_mode {
        oxideterm_update::parse_portable_update_helper_options(std::env::args_os())
            .and_then(oxideterm_update::run_portable_update_helper)
    } else {
        run_platform_update_helper()
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_platform_update_helper() -> Result<(), String> {
    oxideterm_update::parse_windows_update_helper_options(std::env::args_os())
        .and_then(oxideterm_update::run_windows_update_helper)
}

#[cfg(not(windows))]
fn run_platform_update_helper() -> Result<(), String> {
    Err("installed-app update helper mode is only available on Windows".to_string())
}
