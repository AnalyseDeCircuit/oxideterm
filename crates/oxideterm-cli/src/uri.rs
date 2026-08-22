// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use clap::Args;
use oxideterm_ssh_launch::parse_connection_uri;
use zeroize::Zeroizing;

use crate::{
    error::{CliError, CliResult},
    ssh::{current_username, launch_request},
};

#[derive(Args)]
pub struct ConnectionUriArgs {
    #[arg(
        value_name = "URI",
        help = "Connection URI using ssh://, telnet://, mosh://, rdp://, or vnc://"
    )]
    pub uri: String,
}

impl fmt::Debug for ConnectionUriArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConnectionUriArgs([redacted URI])")
    }
}

pub fn run(args: ConnectionUriArgs) -> CliResult<i32> {
    let uri = Zeroizing::new(args.uri);
    let launch = parse_connection_uri(&uri, current_username().as_deref())
        .map_err(|error| CliError::new("invalid_connection_uri", error.to_string(), false))?;
    launch_request(&launch)?;
    println!("Opening temporary connection in OxideTerm");
    Ok(0)
}
