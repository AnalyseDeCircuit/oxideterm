use oxideterm_ssh::SshConfig;
use russh::{Channel, ChannelId, Pty, server};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;

struct Peer {
    ready: crossbeam_channel::Sender<(server::Handle, ChannelId)>,
    input: crossbeam_channel::Sender<(Vec<u8>, Instant)>,
    forwards: std::collections::HashSet<ChannelId>,
    auth_delay: Duration,
    #[cfg(target_os = "macos")]
    sessions: std::collections::HashMap<ChannelId, Channel<server::Msg>>,
    #[cfg(target_os = "macos")]
    subsystems: std::collections::HashSet<ChannelId>,
    #[cfg(target_os = "macos")]
    subsystem_tasks: tokio::task::JoinSet<()>,
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        if !self.auth_delay.is_zero() {
            tokio::time::sleep(self.auth_delay).await;
        }
        Ok(if user == "parser-test" && password == "fixture" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        #[cfg(target_os = "macos")]
        self.sessions.insert(channel.id(), channel);
        #[cfg(not(target_os = "macos"))]
        drop(channel);
        reply.accept().await;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        #[cfg(target_os = "macos")]
        self.sessions.remove(&channel);
        session.channel_success(channel)?;
        self.ready.send((session.handle(), channel)).unwrap();
        Ok(())
    }
    #[cfg(target_os = "macos")]
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            return session.channel_failure(channel);
        }
        // Exercise the installed OpenSSH subsystem, including its real file I/O.
        let mut child = tokio::process::Command::new("/usr/libexec/sftp-server")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("macOS OpenSSH SFTP server");
        let mut input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let stream = self.sessions.remove(&channel).unwrap().into_stream();
        let (mut reader, mut writer) = tokio::io::split(stream);
        self.subsystems.insert(channel);
        session.channel_success(channel)?;
        self.subsystem_tasks.spawn(async move {
            tokio::select! {
                _ = tokio::io::copy(&mut reader, &mut input) => {}
                _ = tokio::io::copy(&mut output, &mut writer) => {}
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
        });
        Ok(())
    }
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<server::Msg>,
        _: &str,
        _: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.forwards.insert(channel.id());
        reply.accept().await;
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        #[cfg(target_os = "macos")]
        if self.subsystems.contains(&channel) {
            return Ok(());
        }
        if self.forwards.contains(&channel) {
            session.data(channel, data.to_vec())?;
        } else {
            let _ = self.input.send((data.to_vec(), Instant::now()));
        }
        Ok(())
    }
}

pub struct SshPeer {
    pub runtime: Arc<Runtime>,
    pub config: Option<SshConfig>,
    pub registry: oxideterm_ssh::SshConnectionRegistry,
    pub ready: crossbeam_channel::Receiver<(server::Handle, ChannelId)>,
    pub input: crossbeam_channel::Receiver<(Vec<u8>, Instant)>,
    task: tokio::task::JoinHandle<()>,
}
impl SshPeer {
    pub fn new() -> Self {
        Self::with_auth_delay(Duration::ZERO)
    }
    pub fn with_auth_delay(auth_delay: Duration) -> Self {
        let runtime = Arc::new(Runtime::new().unwrap());
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(("127.0.0.1", 0)))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::ZERO,
            ..Default::default()
        });
        let (ready_tx, ready_rx) = crossbeam_channel::unbounded();
        let (input_tx, input) = crossbeam_channel::unbounded();
        let server = runtime.spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            // Startup-cancellation tests can close before key exchange finishes.
            if let Ok(session) = server::run_stream(
                config,
                socket,
                Peer {
                    ready: ready_tx,
                    input: input_tx,
                    forwards: Default::default(),
                    auth_delay,
                    #[cfg(target_os = "macos")]
                    sessions: Default::default(),
                    #[cfg(target_os = "macos")]
                    subsystems: Default::default(),
                    #[cfg(target_os = "macos")]
                    subsystem_tasks: tokio::task::JoinSet::new(),
                },
            )
            .await
            {
                let _ = session.await;
            }
        });
        let mut config = SshConfig::password("127.0.0.1", port, "parser-test", "fixture");
        config.strict_host_key_checking = false;
        config.trust_host_key = Some(false);
        config.expected_host_key_fingerprint = Some(fingerprint);
        let registry = oxideterm_ssh::SshConnectionRegistry::new(Default::default());
        registry.set_task_runtime(runtime.handle().clone());
        Self {
            runtime,
            config: Some(config),
            registry,
            ready: ready_rx,
            input,
            task: server,
        }
    }
    pub fn stop(&self) {
        self.task.abort();
    }
}
impl Drop for SshPeer {
    fn drop(&mut self) {
        self.stop();
    }
}
