// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{self, BufReader};

use oxideterm_remote_desktop::{
    RemoteDesktopFakeBackend, RemoteDesktopHelperEvent, RemoteDesktopProtocol, read_request_line,
    run_fake_backend_stdio, write_event_line,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("oxideterm-rdp-helper: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !args.iter().any(|arg| arg == "--stdio") {
        return Err("pass --stdio to run the helper protocol boundary".to_string());
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    if !args.iter().any(|arg| arg == "--fake") {
        // Keep the real helper path protocol-shaped even before IronRDP is
        // wired in, so the app can distinguish "not implemented" from a
        // missing or crashed helper process.
        let _ = read_request_line(&mut reader).map_err(|error| error.to_string())?;
        write_event_line(
            &mut writer,
            &RemoteDesktopHelperEvent::ConnectionFailure {
                message: "Real RDP is not implemented yet.".to_string(),
            },
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let mut backend = RemoteDesktopFakeBackend::new(RemoteDesktopProtocol::Rdp);

    // The fake backend keeps the helper executable and JSON-line protocol
    // testable while the real RDP engine is still intentionally out of tree.
    run_fake_backend_stdio(&mut backend, &mut reader, &mut writer)
        .map_err(|error| error.to_string())?;
    Ok(())
}
