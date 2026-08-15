// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::IpAddr, path::PathBuf, time::Duration};

use reqwest::{
    Url,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue},
    redirect::Policy,
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use zeroize::Zeroizing;

use crate::{
    args::{McpAction, McpBridgeArgs, McpCommand},
    error::{CliError, CliResult},
    paths,
};

const PUBLIC_MCP_ENDPOINT_FILE: &str = "public-mcp-endpoint.json";
const PUBLIC_MCP_ENDPOINT_VERSION: u32 = 1;
const PUBLIC_MCP_PATH: &str = "/mcp";
const MCP_PROTOCOL_VERSION_HEADER: &str = "MCP-Protocol-Version";
const MCP_TOKEN_ENV_NAME_MAXIMUM_BYTES: usize = 255;
const MCP_CREDENTIAL_MAXIMUM_BYTES: usize = 4 * 1024;
const MCP_MESSAGE_MAXIMUM_BYTES: usize = 24 * 1024 * 1024;
const MCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(6 * 60);

#[derive(Debug, Deserialize)]
struct EndpointState {
    version: u32,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct MessageMetadata {
    #[serde(default)]
    method: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InitializeRequest {
    params: InitializeParams,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeParams {
    protocol_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InitializeResponse {
    result: Option<InitializeResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    protocol_version: Option<String>,
}

pub fn run(command: McpCommand) -> CliResult<i32> {
    match command.action {
        McpAction::Bridge(args) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| CliError::new("mcp_runtime_failed", error.to_string(), false))?;
            runtime.block_on(run_bridge(args))?;
            Ok(0)
        }
    }
}

async fn run_bridge(args: McpBridgeArgs) -> CliResult<()> {
    let endpoint = resolve_endpoint(args.endpoint)?;
    let credential = read_credential(&args.token_env)?;
    let authorization_text = Zeroizing::new(format!("Bearer {}", credential.as_str()));
    let mut authorization = HeaderValue::from_str(&authorization_text).map_err(|_| {
        CliError::new(
            "mcp_credential_invalid",
            "The MCP credential cannot be encoded as an HTTP header",
            false,
        )
    })?;
    authorization.set_sensitive(true);
    let client = reqwest::Client::builder()
        // The bridge is intentionally loopback-only and must never inherit a proxy.
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(MCP_CONNECT_TIMEOUT)
        .timeout(MCP_REQUEST_TIMEOUT)
        .build()
        .map_err(|error| CliError::new("mcp_bridge_failed", error.to_string(), false))?;

    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut request_bytes = Zeroizing::new(Vec::new());
    let mut protocol_version = None::<String>;
    loop {
        request_bytes.clear();
        let bytes_read = (&mut stdin)
            .take(MCP_MESSAGE_MAXIMUM_BYTES.saturating_add(1) as u64)
            .read_until(b'\n', &mut request_bytes)
            .await
            .map_err(|error| CliError::new("mcp_stdio_failed", error.to_string(), false))?;
        if bytes_read == 0 {
            break;
        }
        if request_bytes.len() > MCP_MESSAGE_MAXIMUM_BYTES {
            return Err(CliError::new(
                "mcp_message_too_large",
                "An MCP stdio message exceeds the supported size limit",
                false,
            ));
        }
        while request_bytes
            .last()
            .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
        {
            request_bytes.pop();
        }
        if request_bytes.is_empty() {
            continue;
        }
        let metadata: MessageMetadata =
            serde_json::from_slice(&request_bytes).map_err(|error| {
                CliError::new(
                    "mcp_message_invalid",
                    format!("Invalid MCP JSON-RPC message: {error}"),
                    false,
                )
            })?;
        let requested_protocol = (metadata.method.as_deref() == Some("initialize"))
            .then(|| serde_json::from_slice::<InitializeRequest>(&request_bytes).ok())
            .flatten()
            .and_then(|request| request.params.protocol_version);
        let mut request = client
            .post(endpoint.clone())
            .header(AUTHORIZATION, authorization.clone())
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream");
        if let Some(version) = protocol_version.as_deref() {
            request = request.header(MCP_PROTOCOL_VERSION_HEADER, version);
        }
        // Move the sensitive JSON-RPC buffer into the HTTP body without logging it.
        let body = std::mem::take(&mut *request_bytes);
        let mut response = request.body(body).send().await.map_err(|_| {
            CliError::new(
                "mcp_endpoint_unavailable",
                "The running OxideTerm MCP endpoint could not be reached",
                false,
            )
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CliError::new(
                "mcp_unauthorized",
                "The MCP credential was rejected by OxideTerm",
                false,
            ));
        }
        if !status.is_success() {
            return Err(CliError::new(
                "mcp_http_failed",
                format!("The OxideTerm MCP endpoint returned HTTP {status}"),
                false,
            ));
        }
        let mut response_bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            CliError::new(
                "mcp_http_failed",
                "The OxideTerm MCP response could not be read",
                false,
            )
        })? {
            if response_bytes.len().saturating_add(chunk.len()) > MCP_MESSAGE_MAXIMUM_BYTES {
                return Err(CliError::new(
                    "mcp_message_too_large",
                    "An MCP response exceeds the supported size limit",
                    false,
                ));
            }
            response_bytes.extend_from_slice(&chunk);
        }
        if response_bytes.is_empty() {
            continue;
        }
        if metadata.method.as_deref() == Some("initialize") {
            protocol_version = serde_json::from_slice::<InitializeResponse>(&response_bytes)
                .ok()
                .and_then(|response| response.result)
                .and_then(|result| result.protocol_version)
                .or(requested_protocol);
        }
        stdout
            .write_all(&response_bytes)
            .await
            .map_err(|error| CliError::new("mcp_stdio_failed", error.to_string(), false))?;
        stdout
            .write_all(b"\n")
            .await
            .map_err(|error| CliError::new("mcp_stdio_failed", error.to_string(), false))?;
        stdout
            .flush()
            .await
            .map_err(|error| CliError::new("mcp_stdio_failed", error.to_string(), false))?;
    }
    Ok(())
}

fn resolve_endpoint(override_endpoint: Option<String>) -> CliResult<Url> {
    let endpoint = match override_endpoint {
        Some(endpoint) => endpoint,
        None => discover_endpoint()?,
    };
    let endpoint = Url::parse(&endpoint).map_err(|_| {
        CliError::new(
            "mcp_endpoint_invalid",
            "The OxideTerm MCP endpoint URL is invalid",
            false,
        )
    })?;
    let loopback_host = endpoint.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if endpoint.scheme() != "http"
        || !loopback_host
        || endpoint.path() != PUBLIC_MCP_PATH
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err(CliError::new(
            "mcp_endpoint_invalid",
            "The MCP bridge only accepts a plain loopback /mcp endpoint",
            false,
        ));
    }
    Ok(endpoint)
}

fn discover_endpoint() -> CliResult<String> {
    let path = endpoint_state_path();
    let bytes = std::fs::read(&path).map_err(|_| {
        CliError::new(
            "mcp_endpoint_unavailable",
            "Start OxideTerm and enable External MCP Control before launching the bridge",
            false,
        )
    })?;
    let state: EndpointState = serde_json::from_slice(&bytes).map_err(|_| {
        CliError::new(
            "mcp_endpoint_invalid",
            "The OxideTerm MCP endpoint discovery record is invalid",
            false,
        )
    })?;
    if state.version != PUBLIC_MCP_ENDPOINT_VERSION || state.port == 0 {
        return Err(CliError::new(
            "mcp_endpoint_invalid",
            "The OxideTerm MCP endpoint discovery record is unsupported",
            false,
        ));
    }
    Ok(format!("http://127.0.0.1:{}{PUBLIC_MCP_PATH}", state.port))
}

fn endpoint_state_path() -> PathBuf {
    paths::default_settings_path()
        .parent()
        .map(|directory| directory.join(PUBLIC_MCP_ENDPOINT_FILE))
        .unwrap_or_else(|| PathBuf::from(PUBLIC_MCP_ENDPOINT_FILE))
}

fn read_credential(environment_name: &str) -> CliResult<Zeroizing<String>> {
    if environment_name.is_empty()
        || environment_name.len() > MCP_TOKEN_ENV_NAME_MAXIMUM_BYTES
        || environment_name
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_')))
    {
        return Err(CliError::new(
            "mcp_credential_invalid",
            "The MCP credential environment variable name is invalid",
            false,
        ));
    }
    let credential = std::env::var(environment_name).map_err(|_| {
        CliError::new(
            "mcp_credential_missing",
            format!("Set {environment_name} to the one-time credential shown by OxideTerm"),
            false,
        )
    })?;
    if credential.is_empty() || credential.len() > MCP_CREDENTIAL_MAXIMUM_BYTES {
        return Err(CliError::new(
            "mcp_credential_invalid",
            format!("{environment_name} is empty or exceeds the supported size limit"),
            false,
        ));
    }
    Ok(Zeroizing::new(credential))
}
