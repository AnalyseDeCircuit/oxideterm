use rand10::{rand_core::UnwrapErr, rngs::SysRng};
use russh::{
    Channel, ChannelId,
    keys::{Algorithm, HashAlg, PrivateKey},
    server::{self, Msg, Session},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

pub(super) struct TestSshServer {
    pub(super) port: u16,
    pub(super) host_key_fingerprint: String,
    pub(super) direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
    pub(super) reject_remote_cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DirectTcpipOpen {
    pub(super) originator_address: String,
    pub(super) originator_port: u32,
}

pub(super) async fn start_forwarding_ssh_server() -> TestSshServer {
    let host_key = PrivateKey::random(&mut UnwrapErr(SysRng), Algorithm::Ed25519).unwrap();
    let host_key_fingerprint = host_key
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: std::time::Duration::ZERO,
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        keys: vec![host_key],
        ..Default::default()
    });
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let forwards = Arc::new(Mutex::new(HashMap::new()));
    let direct_tcpip_opens = Arc::new(Mutex::new(Vec::new()));
    let server_direct_tcpip_opens = direct_tcpip_opens.clone();
    let reject_remote_cancel = Arc::new(AtomicBool::new(false));
    let server_reject_remote_cancel = reject_remote_cancel.clone();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let handler = ForwardingServer {
                forwards: forwards.clone(),
                direct_tcpip_opens: server_direct_tcpip_opens.clone(),
                reject_remote_cancel: server_reject_remote_cancel.clone(),
            };
            let config = config.clone();
            tokio::spawn(async move {
                let _ = server::run_stream(config, stream, handler).await;
            });
        }
    });

    TestSshServer {
        port,
        host_key_fingerprint,
        direct_tcpip_opens,
        reject_remote_cancel,
    }
}

#[derive(Clone)]
struct ForwardingServer {
    forwards: Arc<Mutex<HashMap<(String, u32), tokio::task::JoinHandle<()>>>>,
    direct_tcpip_opens: Arc<Mutex<Vec<DirectTcpipOpen>>>,
    reject_remote_cancel: Arc<AtomicBool>,
}

impl server::Handler for ForwardingServer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _user: &str,
        _password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.direct_tcpip_opens.lock().await.push(DirectTcpipOpen {
            originator_address: originator_address.to_string(),
            originator_port,
        });
        let target = format!("{host_to_connect}:{port_to_connect}");
        tokio::spawn(async move {
            let Ok(mut target) = TcpStream::connect(target).await else {
                return;
            };
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
        reply.accept().await;
        Ok(())
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let listener = TcpListener::bind((address, *port as u16)).await?;
        *port = listener.local_addr()?.port() as u32;
        let key = (address.to_string(), *port);
        let handle = session.handle();
        let connected_address = address.to_string();
        let connected_port = *port;
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut inbound, origin)) = listener.accept().await else {
                    break;
                };
                let Ok(channel) = handle
                    .channel_open_forwarded_tcpip(
                        connected_address.clone(),
                        connected_port,
                        origin.ip().to_string(),
                        origin.port() as u32,
                    )
                    .await
                else {
                    break;
                };
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut stream).await;
                });
            }
        });
        self.forwards.lock().await.insert(key, task);
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if self.reject_remote_cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if let Some(task) = self
            .forwards
            .lock()
            .await
            .remove(&(address.to_string(), port))
        {
            task.abort();
            // Release the test listener before acknowledging cancellation so
            // the client can prove that stopping the forward frees the port.
            let _ = task.await;
        }
        Ok(true)
    }
}
