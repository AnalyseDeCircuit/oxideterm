use std::{
    convert::Infallible,
    net::{Ipv4Addr, TcpListener},
    sync::Arc,
};

use agent_client_protocol::schema::v1::{HttpHeader, McpServer, McpServerHttp};
use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::{
    sync::{Semaphore, oneshot},
    task::JoinSet,
};
use zeroize::Zeroizing;

use crate::{
    AcpHostToolCallReceiver, AcpHostToolDefinition, AcpHostToolsError,
    protocol::{AcpHostToolsProtocol, MCP_REQUEST_BODY_LIMIT},
};

const MCP_ENDPOINT_PATH: &str = "/mcp";
const MCP_CALL_QUEUE_CAPACITY: usize = 32;
const MCP_CONNECTION_LIMIT: usize = 32;

/// Owns one conversation-scoped loopback listener and its authorization material.
pub struct AcpHostToolsServer {
    endpoint_url: String,
    authorization_header: Zeroizing<String>,
    protocol: Arc<AcpHostToolsProtocol>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    worker: tokio::task::JoinHandle<()>,
}

impl AcpHostToolsServer {
    /// Builds the stable MCP declaration installed when the ACP session is created.
    pub fn mcp_server(&self) -> McpServer {
        McpServer::Http(
            McpServerHttp::new("OxideTerm Application Tools", self.endpoint_url.clone()).headers(
                vec![HttpHeader::new(
                    "Authorization",
                    self.authorization_header.as_str(),
                )],
            ),
        )
    }

    /// Replaces the catalog served by subsequent tools/list and tools/call requests.
    pub fn replace_definitions(&self, definitions: Vec<AcpHostToolDefinition>) {
        self.protocol.replace_definitions(definitions);
    }

    /// Stops accepting requests and awaits every connection worker.
    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let _ = (&mut self.worker).await;
    }
}

impl Drop for AcpHostToolsServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        // Explicit shutdown is preferred; abort is the cancellation fallback.
        self.worker.abort();
    }
}

/// Starts a listener synchronously so the MCP declaration can be included in the
/// same ACP session-creation request that owns it.
pub fn start_acp_host_tools_server(
    runtime: &tokio::runtime::Handle,
    definitions: Vec<AcpHostToolDefinition>,
) -> Result<(AcpHostToolsServer, AcpHostToolCallReceiver), AcpHostToolsError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(AcpHostToolsError::Bind)?;
    listener
        .set_nonblocking(true)
        .map_err(AcpHostToolsError::ConfigureListener)?;
    let address = listener
        .local_addr()
        .map_err(AcpHostToolsError::ConfigureListener)?;
    let listener = {
        let _runtime_guard = runtime.enter();
        tokio::net::TcpListener::from_std(listener).map_err(AcpHostToolsError::ConfigureListener)?
    };
    let authorization_header = Zeroizing::new(format!(
        "Bearer {}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    let (call_tx, call_rx) = tokio::sync::mpsc::channel(MCP_CALL_QUEUE_CAPACITY);
    let protocol = Arc::new(AcpHostToolsProtocol::new(
        definitions,
        call_tx,
        authorization_header.as_str(),
    ));
    let server_protocol = protocol.clone();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let worker = runtime.spawn(async move {
        let mut connections = JoinSet::new();
        let connection_slots = Arc::new(Semaphore::new(MCP_CONNECTION_LIMIT));
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, peer)) = accepted else {
                        break;
                    };
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let Ok(connection_slot) = connection_slots.clone().try_acquire_owned() else {
                        // Bound loopback work even when an ACP process floods the bridge.
                        continue;
                    };
                    let protocol = protocol.clone();
                    connections.spawn(async move {
                        let _connection_slot = connection_slot;
                        let service = service_fn(move |request| {
                            handle_http_request(request, protocol.clone())
                        });
                        let _ = http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await;
                    });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    Ok((
        AcpHostToolsServer {
            endpoint_url: format!("http://{address}{MCP_ENDPOINT_PATH}"),
            authorization_header,
            protocol: server_protocol,
            shutdown_tx: Some(shutdown_tx),
            worker,
        },
        AcpHostToolCallReceiver { inner: call_rx },
    ))
}

async fn handle_http_request(
    request: Request<Incoming>,
    protocol: Arc<AcpHostToolsProtocol>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if request.uri().path() != MCP_ENDPOINT_PATH || request.method() != Method::POST {
        return Ok(empty_response(StatusCode::NOT_FOUND));
    }
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .map(|value| value.as_bytes());
    if !protocol.authorized(authorization) {
        return Ok(empty_response(StatusCode::UNAUTHORIZED));
    }
    let body = match Limited::new(request.into_body(), MCP_REQUEST_BODY_LIMIT)
        .collect()
        .await
    {
        Ok(body) => body.to_bytes(),
        Err(_) => return Ok(empty_response(StatusCode::PAYLOAD_TOO_LARGE)),
    };
    let message = match serde_json::from_slice(&body) {
        Ok(message) => message,
        Err(_) => return Ok(empty_response(StatusCode::BAD_REQUEST)),
    };
    let protocol_response = protocol.handle_message(message).await;
    let Some(body) = protocol_response.body else {
        return Ok(empty_response(protocol_response.status));
    };
    let body = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    Ok(Response::builder()
        .status(protocol_response.status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR)))
}

fn empty_response(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}
