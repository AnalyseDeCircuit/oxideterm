use std::{
    convert::Infallible,
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::Arc,
};

use bytes::Bytes;
use http::{Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Empty, combinators::BoxBody};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use tokio::{sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;
use tower_service::Service;

use crate::{ClientRegistry, PublicMcpService, PublicMcpState};

const MCP_ENDPOINT_PATH: &str = "/mcp";
// The bound includes base64 expansion for the largest accepted staged artifact.
const MCP_REQUEST_BODY_LIMIT: usize = 24 * 1024 * 1024;
const MCP_CONNECTION_LIMIT: usize = 32;

pub struct PublicMcpHttpServer {
    address: SocketAddr,
    cancellation: CancellationToken,
    worker: tokio::task::JoinHandle<()>,
}

impl PublicMcpHttpServer {
    pub fn endpoint_url(&self) -> String {
        format!("http://{}{MCP_ENDPOINT_PATH}", self.address)
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    pub async fn shutdown(mut self) {
        self.cancellation.cancel();
        let _ = (&mut self.worker).await;
    }
}

impl Drop for PublicMcpHttpServer {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.worker.abort();
    }
}

/// Starts the loopback MCP endpoint on an already-owned application runtime.
pub fn start_http_server(
    runtime: &tokio::runtime::Handle,
    state: Arc<PublicMcpState>,
    preferred_port: u16,
) -> io::Result<PublicMcpHttpServer> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred_port))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let listener = {
        let _runtime_guard = runtime.enter();
        tokio::net::TcpListener::from_std(listener)?
    };

    let cancellation = CancellationToken::new();
    let service = StreamableHttpService::<PublicMcpService, NeverSessionManager>::new(
        {
            let state = state.clone();
            move || Ok(PublicMcpService::new(state.clone()))
        },
        Default::default(),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_cancellation_token(cancellation.clone())
            .with_max_request_body_bytes(MCP_REQUEST_BODY_LIMIT)
            .with_allowed_hosts([
                "localhost".to_owned(),
                "127.0.0.1".to_owned(),
                format!("localhost:{}", address.port()),
                address.to_string(),
            ])
            .with_allowed_origins([
                format!("http://localhost:{}", address.port()),
                format!("http://127.0.0.1:{}", address.port()),
            ]),
    );
    let clients = state.clients.clone();
    let approvals = state.approvals.clone();
    let artifacts = state.artifacts.clone();
    let worker_cancellation = cancellation.clone();
    let worker = runtime.spawn(async move {
        serve_loopback(
            listener,
            service,
            clients,
            approvals,
            artifacts,
            worker_cancellation,
        )
        .await;
    });

    Ok(PublicMcpHttpServer {
        address,
        cancellation,
        worker,
    })
}

async fn serve_loopback(
    listener: tokio::net::TcpListener,
    service: StreamableHttpService<PublicMcpService, NeverSessionManager>,
    clients: Arc<ClientRegistry>,
    approvals: Arc<crate::ApprovalStore>,
    artifacts: Arc<crate::ArtifactStore>,
    cancellation: CancellationToken,
) {
    let connection_slots = Arc::new(Semaphore::new(MCP_CONNECTION_LIMIT));
    let mut connections = JoinSet::new();
    let mut expiry_tick = tokio::time::interval(std::time::Duration::from_secs(30));
    expiry_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            _ = expiry_tick.tick() => {
                // Frozen actions and sensitive artifact data must expire without new traffic.
                approvals.expire();
                artifacts.expire();
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else {
                    break;
                };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Ok(connection_slot) = connection_slots.clone().try_acquire_owned() else {
                    continue;
                };
                let service = service.clone();
                let clients = clients.clone();
                connections.spawn(async move {
                    let _connection_slot = connection_slot;
                    let request_service = service_fn(move |request| {
                        handle_http_request(request, service.clone(), clients.clone())
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), request_service)
                        .await;
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
}

async fn handle_http_request(
    request: Request<Incoming>,
    mut service: StreamableHttpService<PublicMcpService, NeverSessionManager>,
    clients: Arc<ClientRegistry>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    if request.uri().path() != MCP_ENDPOINT_PATH {
        return Ok(empty_response(StatusCode::NOT_FOUND));
    }
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| clients.authenticate_bearer(value))
        .is_some();
    if !authorized {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::WWW_AUTHENTICATE, "Bearer realm=\"OxideTerm\"")
            .body(Empty::new().boxed())
            .unwrap_or_else(|_| empty_response(StatusCode::UNAUTHORIZED)));
    }
    service.call(request).await
}

fn empty_response(status: StatusCode) -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(status)
        .body(Empty::new().boxed())
        .unwrap_or_else(|_| Response::new(Empty::new().boxed()))
}
