use std::{convert::Infallible, net::Ipv4Addr, sync::Arc};

use agent_client_protocol::schema::{HttpHeader, McpServer, McpServerHttp};
use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpListener, sync::oneshot, task::JoinSet};
use zeroize::Zeroizing;

use crate::{
    AcpHostToolCallReceiver, AcpHostToolDefinition, AcpHostToolsError,
    protocol::{AcpHostToolsProtocol, MCP_REQUEST_BODY_LIMIT},
};

const MCP_ENDPOINT_PATH: &str = "/mcp";

/// Owns the loopback listener and its bounded authorization material for one ACP runtime.
pub struct AcpHostToolsServer {
    endpoint_url: String,
    authorization_header: Zeroizing<String>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    worker: tokio::task::JoinHandle<()>,
}

impl AcpHostToolsServer {
    /// Builds a fresh ACP MCP declaration. The protocol DTO necessarily owns one bounded
    /// plaintext header copy until the session request has been serialized by the ACP SDK.
    pub fn mcp_server(&self) -> McpServer {
        McpServer::Http(
            McpServerHttp::new("OxideTerm Host Tools", self.endpoint_url.clone()).headers(vec![
                HttpHeader::new("Authorization", self.authorization_header.as_str()),
            ]),
        )
    }

    /// Stops accepting requests and awaits every connection task before returning.
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
        // Explicit shutdown awaits cleanup; Drop is the cancellation fallback for aborted turns.
        self.worker.abort();
    }
}

pub async fn start_acp_host_tools_server(
    definitions: Vec<AcpHostToolDefinition>,
) -> Result<(AcpHostToolsServer, AcpHostToolCallReceiver), AcpHostToolsError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(AcpHostToolsError::Bind)?;
    let address = listener
        .local_addr()
        .map_err(AcpHostToolsError::LocalAddress)?;
    let authorization_header = Zeroizing::new(format!(
        "Bearer {}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    let (call_tx, call_rx) = tokio::sync::mpsc::unbounded_channel();
    let protocol = Arc::new(AcpHostToolsProtocol::new(
        definitions,
        call_tx,
        authorization_header.as_str(),
    ));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let worker = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _peer)) = accepted else {
                        break;
                    };
                    let protocol = protocol.clone();
                    connections.spawn(async move {
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
