use oxideterm_ssh::SshConfig;
use russh::{keys, server};
use std::{sync::Arc, time::Duration};

type FileCalls = Arc<std::sync::Mutex<Vec<(u32, String)>>>;
pub(super) enum SftpPeerEvent {
    ResolveHome(tokio::sync::oneshot::Sender<String>),
    Disconnected,
}

type SftpPeerEvents = tokio::sync::mpsc::UnboundedSender<SftpPeerEvent>;

struct Peer {
    interactive: bool,
    generation: u32,
    files: FileCalls,
    sftp_events: Option<SftpPeerEvents>,
    channels: std::collections::HashMap<russh::ChannelId, russh::Channel<server::Msg>>,
    tunnels: tokio::task::JoinSet<()>,
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        if self.interactive {
            return Ok(server::Auth::Reject {
                proceed_with_methods: Some(
                    [russh::MethodKind::KeyboardInteractive].as_slice().into(),
                ),
                partial_success: false,
            });
        }
        Ok(
            if !self.interactive && user == "audit-user" && password == "fixture-auth-secret" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
    }
    async fn channel_open_session(
        &mut self,
        channel: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<server::Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        assert_eq!(host, "127.0.0.1");
        let mut target = tokio::net::TcpStream::connect((
            std::net::Ipv4Addr::LOCALHOST,
            u16::try_from(port).unwrap(),
        ))
        .await?;
        reply.accept().await;
        // A fixture tunnel must end when its parent SSH peer is dropped.
        self.tunnels.spawn(async move {
            let _ = tokio::io::copy_bidirectional(&mut channel.into_stream(), &mut target).await;
        });
        Ok(())
    }
    async fn pty_request(
        &mut self,
        channel: russh::ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }
    async fn shell_request(
        &mut self,
        channel: russh::ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }
    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        command: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if command == b"close-fixture-transport" {
            return Err(russh::Error::Disconnect);
        }
        session.channel_failure(channel)
    }
    async fn subsystem_request(
        &mut self,
        channel: russh::ChannelId,
        name: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            return session.channel_failure(channel);
        }
        session.channel_success(channel)?;
        let stream = self.channels.remove(&channel).unwrap().into_stream();
        russh_sftp::server::run(
            stream,
            FilePeer {
                generation: self.generation,
                calls: self.files.clone(),
                events: self.sftp_events.clone(),
            },
        )
        .await;
        Ok(())
    }
    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<server::Auth, Self::Error> {
        if !self.interactive {
            return Ok(server::Auth::reject());
        }
        let password_fallback = user == "password-fallback-user";
        Ok(match response {
            None => server::Auth::Partial {
                name: "sensitive-challenge-name".into(),
                instructions: "sensitive-challenge-instructions".into(),
                prompts: vec![(
                    if password_fallback {
                        "Password:"
                    } else {
                        "sensitive-challenge-prompt"
                    }
                    .into(),
                    false,
                )]
                .into(),
            },
            Some(mut responses) => {
                let expected = if password_fallback {
                    b"fixture-auth-secret".as_slice()
                } else {
                    b"fixture-otp-secret".as_slice()
                };
                if responses.next().as_deref() == Some(expected) {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                }
            }
        })
    }
}

struct FilePeer {
    generation: u32,
    calls: FileCalls,
    events: Option<SftpPeerEvents>,
}
impl russh_sftp::server::Handler for FilePeer {
    type Error = russh_sftp::protocol::StatusCode;
    fn unimplemented(&self) -> Self::Error {
        russh_sftp::protocol::StatusCode::OpUnsupported
    }
    async fn realpath(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Name, Self::Error> {
        let path = if path != "." {
            path
        } else if let Some(events) = &self.events {
            let (reply, response) = tokio::sync::oneshot::channel();
            let _ = events.send(SftpPeerEvent::ResolveHome(reply));
            response
                .await
                .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?
        } else {
            "/fixture".into()
        };
        Ok(russh_sftp::protocol::Name {
            id,
            files: vec![russh_sftp::protocol::File::dummy(path)],
        })
    }
    async fn stat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        use russh_sftp::protocol::{Attrs, FileAttributes};
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(u64::from(self.generation)),
                permissions: Some(if path == "/fixture" {
                    0o040755
                } else {
                    0o100644
                }),
                ..Default::default()
            },
        })
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        _: russh_sftp::protocol::OpenFlags,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
        if path != "/fixture/sample" {
            return Err(russh_sftp::protocol::StatusCode::NoSuchFile);
        }
        Ok(russh_sftp::protocol::Handle { id, handle: path })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, Self::Error> {
        if handle != "/fixture/sample" || offset >= u64::from(self.generation) {
            return Err(russh_sftp::protocol::StatusCode::Eof);
        }
        let size = u64::from(len).min(u64::from(self.generation) - offset);
        Ok(russh_sftp::protocol::Data {
            id,
            data: vec![self.generation as u8; size as usize].into(),
        })
    }

    async fn close(
        &mut self,
        id: u32,
        _: String,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: russh_sftp::protocol::StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        self.calls.lock().unwrap().push((self.generation, path));
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: russh_sftp::protocol::StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}

pub(super) async fn start_peer(
    interactive: bool,
) -> (SshConfig, String, tokio::task::JoinHandle<()>, FileCalls) {
    start_peer_with_sftp_events(interactive, None).await
}

pub(super) async fn start_peer_with_sftp_events(
    interactive: bool,
    sftp_events: Option<SftpPeerEvents>,
) -> (SshConfig, String, tokio::task::JoinHandle<()>, FileCalls) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let key = keys::PrivateKey::random(&mut rand10::rng(), keys::Algorithm::Ed25519).unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(keys::HashAlg::Sha256)
        .to_string();
    let calls: FileCalls = Default::default();
    let files = calls.clone();
    let server = tokio::spawn(async move {
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::ZERO,
            ..Default::default()
        });
        let mut connections = tokio::task::JoinSet::new();
        let mut generation = 0;
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            generation += 1;
            let config = config.clone();
            let peer = Peer {
                interactive,
                generation,
                files: files.clone(),
                sftp_events: sftp_events.clone(),
                channels: Default::default(),
                tunnels: tokio::task::JoinSet::new(),
            };
            let events = sftp_events.clone();
            connections.spawn(async move {
                if let Ok(connection) = server::run_stream(config, socket, peer).await {
                    let _ = connection.await;
                }
                if let Some(events) = events {
                    let _ = events.send(SftpPeerEvent::Disconnected);
                }
            });
        }
    });
    let mut config = SshConfig::password("127.0.0.1", port, "audit-user", "fixture-auth-secret");
    config.trust_host_key = Some(false);
    config.expected_host_key_fingerprint = Some(fingerprint.clone());
    (config, fingerprint, server, calls)
}
