use std::{
    env, fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use oxideterm_ssh::{
    AuthMethod, ProxyHopConfig, SshConfig, SshTransportClient, UpstreamProxyAuth,
    UpstreamProxyConfig, UpstreamProxyProtocol, upstream_proxy_from_env,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use zeroize::Zeroizing;

#[tokio::test]
#[ignore = "requires local sshd and ssh-keygen"]
async fn local_sshd_upstream_proxy_e2e_matrix() {
    let sshd = SshdFixture::start();
    let socks = Socks5ProxyFixture::start(Socks5AuthMode::None).await;
    let socks_password =
        Socks5ProxyFixture::start(Socks5AuthMode::Password("proxy-user", "proxy-secret")).await;
    let http = HttpConnectProxyFixture::start().await;

    connect_shell(target_config(&sshd).with_upstream_proxy(socks.config())).await;
    connect_shell(target_config(&sshd).with_upstream_proxy(socks_password.config_with_password()))
        .await;
    connect_shell(
        target_config(&sshd)
            .with_proxy_chain(vec![sshd.proxy_hop()])
            .with_upstream_proxy(socks.config()),
    )
    .await;
    connect_shell(target_config(&sshd).with_upstream_proxy(http.config())).await;
    connect_shell(
        target_config(&sshd)
            .with_proxy_chain(vec![sshd.proxy_hop()])
            .with_upstream_proxy(http.config()),
    )
    .await;
    let _socks_env = EnvVarGuard::set("OXIDETERM_SOCKS5_PROXY", String::new());
    let _http_env = EnvVarGuard::set(
        "OXIDETERM_HTTP_PROXY",
        format!("http://{}:{}", http.addr.ip(), http.addr.port()),
    );
    let _no_proxy_env = EnvVarGuard::set("OXIDETERM_NO_PROXY", String::new());
    connect_shell(
        target_config(&sshd).with_upstream_proxy(
            upstream_proxy_from_env()
                .expect("parse env upstream proxy")
                .expect("env upstream proxy"),
        ),
    )
    .await;

    let direct_count = socks.accept_count();
    connect_shell(target_config(&sshd)).await;
    assert_eq!(
        socks.accept_count(),
        direct_count,
        "direct connections must not touch an upstream proxy"
    );

    let no_proxy_count = socks.accept_count();
    connect_shell(
        target_config(&sshd).with_upstream_proxy(socks.config_with_no_proxy("127.0.0.1")),
    )
    .await;
    assert_eq!(
        socks.accept_count(),
        no_proxy_count,
        "matching no_proxy rules must bypass the upstream proxy"
    );
}

async fn connect_shell(config: SshConfig) {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        SshTransportClient::new(config).connect_shell(),
    )
    .await
    .expect("SSH connection timed out");
    let _handle = result.expect("SSH connection failed");
}

fn target_config(sshd: &SshdFixture) -> SshConfig {
    SshConfig {
        host: "127.0.0.1".to_string(),
        port: sshd.port,
        username: sshd.username.clone(),
        auth: AuthMethod::key(sshd.client_key.display().to_string(), None),
        timeout_secs: 10,
        strict_host_key_checking: false,
        trust_host_key: Some(false),
        expected_host_key_fingerprint: Some(sshd.host_fingerprint.clone()),
        ..SshConfig::default()
    }
}

trait SshConfigExt {
    fn with_upstream_proxy(self, proxy: UpstreamProxyConfig) -> Self;
    fn with_proxy_chain(self, proxy_chain: Vec<ProxyHopConfig>) -> Self;
}

impl SshConfigExt for SshConfig {
    fn with_upstream_proxy(mut self, proxy: UpstreamProxyConfig) -> Self {
        self.upstream_proxy = Some(proxy);
        self
    }

    fn with_proxy_chain(mut self, proxy_chain: Vec<ProxyHopConfig>) -> Self {
        self.proxy_chain = Some(proxy_chain);
        self
    }
}

struct SshdFixture {
    dir: PathBuf,
    port: u16,
    username: String,
    client_key: PathBuf,
    host_fingerprint: String,
    child: Child,
}

impl SshdFixture {
    fn start() -> Self {
        let sshd = "/usr/sbin/sshd";
        let ssh_keygen = "ssh-keygen";
        assert!(
            Command::new(sshd)
                .arg("-V")
                .stderr(Stdio::null())
                .status()
                .is_ok()
        );
        assert!(
            Command::new(ssh_keygen)
                .arg("-V")
                .stderr(Stdio::null())
                .status()
                .is_ok()
        );

        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create e2e temp dir");
        let client_key = dir.join("client_key");
        let host_key = dir.join("host_key");
        let authorized_keys = dir.join("authorized_keys");
        let config_path = dir.join("sshd_config");
        let log_path = dir.join("sshd.log");
        let pid_path = dir.join("sshd.pid");
        let port = free_port();
        let username = env::var("USER").expect("USER must be set for sshd e2e");

        run(Command::new(ssh_keygen)
            .args(["-q", "-N", "", "-t", "ed25519", "-f"])
            .arg(&client_key));
        run(Command::new(ssh_keygen)
            .args(["-q", "-N", "", "-t", "ed25519", "-f"])
            .arg(&host_key));
        fs::copy(client_key.with_extension("pub"), &authorized_keys)
            .expect("write authorized keys");
        set_private_permissions(&dir, &client_key, &host_key, &authorized_keys);

        let config = format!(
            "\
Port {port}
ListenAddress 127.0.0.1
HostKey {host_key}
PidFile {pid_path}
AuthorizedKeysFile {authorized_keys}
StrictModes no
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowTcpForwarding yes
PermitTTY yes
PermitRootLogin no
AllowUsers {username}
LogLevel VERBOSE
",
            host_key = host_key.display(),
            pid_path = pid_path.display(),
            authorized_keys = authorized_keys.display(),
        );
        fs::write(&config_path, config).expect("write sshd config");

        let child = Command::new(sshd)
            .args(["-D", "-f"])
            .arg(&config_path)
            .arg("-E")
            .arg(&log_path)
            .spawn()
            .expect("start sshd");
        wait_for_port(port);
        let host_fingerprint = served_host_key_fingerprint(ssh_keygen, port, &dir);

        Self {
            dir,
            port,
            username,
            client_key,
            host_fingerprint,
            child,
        }
    }

    fn proxy_hop(&self) -> ProxyHopConfig {
        ProxyHopConfig {
            host: "127.0.0.1".to_string(),
            port: self.port,
            username: self.username.clone(),
            auth: AuthMethod::key(self.client_key.display().to_string(), None),
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
            strict_host_key_checking: false,
            trust_host_key: Some(false),
            expected_host_key_fingerprint: Some(self.host_fingerprint.clone()),
        }
    }
}

impl Drop for SshdFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: String) -> Self {
        let previous = env::var(key).ok();
        // The ignored E2E test is run serially and temporarily controls the
        // process environment to exercise env fallback parsing.
        unsafe {
            env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // Restore process environment after the serial E2E finishes.
        unsafe {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }
}

enum Socks5AuthMode {
    None,
    Password(&'static str, &'static str),
}

struct Socks5ProxyFixture {
    addr: SocketAddr,
    accept_count: Arc<AtomicUsize>,
    task: JoinHandle<()>,
    mode: Socks5AuthMode,
}

impl Socks5ProxyFixture {
    async fn start(mode: Socks5AuthMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind socks proxy");
        let addr = listener.local_addr().expect("socks proxy addr");
        let accept_count = Arc::new(AtomicUsize::new(0));
        let accept_count_for_task = accept_count.clone();
        let mode_for_task = match mode {
            Socks5AuthMode::None => Socks5AuthMode::None,
            Socks5AuthMode::Password(username, password) => {
                Socks5AuthMode::Password(username, password)
            }
        };
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                accept_count_for_task.fetch_add(1, Ordering::SeqCst);
                let mode = match mode_for_task {
                    Socks5AuthMode::None => Socks5AuthMode::None,
                    Socks5AuthMode::Password(username, password) => {
                        Socks5AuthMode::Password(username, password)
                    }
                };
                tokio::spawn(async move {
                    handle_socks5_client(stream, mode).await;
                });
            }
        });
        Self {
            addr,
            accept_count,
            task,
            mode,
        }
    }

    fn config(&self) -> UpstreamProxyConfig {
        UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::Socks5,
            host: self.addr.ip().to_string(),
            port: self.addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        }
    }

    fn config_with_password(&self) -> UpstreamProxyConfig {
        let Socks5AuthMode::Password(username, password) = self.mode else {
            panic!("password proxy fixture required");
        };
        UpstreamProxyConfig {
            auth: UpstreamProxyAuth::Password {
                username: username.to_string(),
                password: Zeroizing::new(password.to_string()),
            },
            ..self.config()
        }
    }

    fn config_with_no_proxy(&self, no_proxy: &str) -> UpstreamProxyConfig {
        UpstreamProxyConfig {
            no_proxy: no_proxy.to_string(),
            ..self.config()
        }
    }

    fn accept_count(&self) -> usize {
        self.accept_count.load(Ordering::SeqCst)
    }
}

impl Drop for Socks5ProxyFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct HttpConnectProxyFixture {
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl HttpConnectProxyFixture {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind http proxy");
        let addr = listener.local_addr().expect("http proxy addr");
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    handle_http_connect_client(stream).await;
                });
            }
        });
        Self { addr, task }
    }

    fn config(&self) -> UpstreamProxyConfig {
        UpstreamProxyConfig {
            protocol: UpstreamProxyProtocol::HttpConnect,
            host: self.addr.ip().to_string(),
            port: self.addr.port(),
            auth: UpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        }
    }
}

impl Drop for HttpConnectProxyFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_socks5_client(mut client: TcpStream, mode: Socks5AuthMode) {
    let version = client.read_u8().await.expect("socks version");
    assert_eq!(version, 0x05);
    let method_count = client.read_u8().await.expect("socks method count") as usize;
    let mut methods = vec![0; method_count];
    client
        .read_exact(&mut methods)
        .await
        .expect("socks methods");
    let selected_method = match mode {
        Socks5AuthMode::None => 0x00,
        Socks5AuthMode::Password(_, _) => 0x02,
    };
    assert!(methods.contains(&selected_method));
    client
        .write_all(&[0x05, selected_method])
        .await
        .expect("socks method reply");

    if let Socks5AuthMode::Password(username, password) = mode {
        let auth_version = client.read_u8().await.expect("socks auth version");
        assert_eq!(auth_version, 0x01);
        let username_len = client.read_u8().await.expect("socks username len") as usize;
        let mut username_bytes = vec![0; username_len];
        client
            .read_exact(&mut username_bytes)
            .await
            .expect("socks username");
        let password_len = client.read_u8().await.expect("socks password len") as usize;
        let mut password_bytes = vec![0; password_len];
        client
            .read_exact(&mut password_bytes)
            .await
            .expect("socks password");
        assert_eq!(String::from_utf8(username_bytes).unwrap(), username);
        assert_eq!(String::from_utf8(password_bytes).unwrap(), password);
        client
            .write_all(&[0x01, 0x00])
            .await
            .expect("socks auth reply");
    }

    let mut header = [0; 4];
    client
        .read_exact(&mut header)
        .await
        .expect("socks request header");
    assert_eq!(header[0], 0x05);
    assert_eq!(header[1], 0x01);
    let target_host = match header[3] {
        0x01 => {
            let mut octets = [0; 4];
            client.read_exact(&mut octets).await.expect("socks ipv4");
            std::net::Ipv4Addr::from(octets).to_string()
        }
        0x03 => {
            let len = client.read_u8().await.expect("socks domain len") as usize;
            let mut domain = vec![0; len];
            client.read_exact(&mut domain).await.expect("socks domain");
            String::from_utf8(domain).expect("socks domain utf8")
        }
        0x04 => {
            let mut octets = [0; 16];
            client.read_exact(&mut octets).await.expect("socks ipv6");
            std::net::Ipv6Addr::from(octets).to_string()
        }
        other => panic!("unsupported socks atyp {other}"),
    };
    let target_port = client.read_u16().await.expect("socks port");
    let mut upstream = TcpStream::connect((target_host.as_str(), target_port))
        .await
        .expect("connect socks target");
    client
        .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
        .await
        .expect("socks success reply");
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
}

async fn handle_http_connect_client(mut client: TcpStream) {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        request.push(client.read_u8().await.expect("http connect request byte"));
    }
    let request = String::from_utf8(request).expect("http request utf8");
    let first_line = request.lines().next().expect("http request line");
    let authority = first_line
        .strip_prefix("CONNECT ")
        .and_then(|line| line.strip_suffix(" HTTP/1.1"))
        .expect("CONNECT request line");
    let (target_host, target_port) = authority.rsplit_once(':').expect("CONNECT authority");
    let mut upstream = TcpStream::connect((target_host, target_port.parse::<u16>().unwrap()))
        .await
        .expect("connect http target");
    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .expect("http connect response");
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    env::temp_dir().join(format!("oxideterm-ssh-e2e-{nanos}"))
}

fn free_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind temporary port");
    listener.local_addr().expect("temporary port addr").port()
}

fn wait_for_port(port: u16) {
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("sshd did not start on port {port}");
}

fn run(command: &mut Command) {
    let status = command.status().expect("run command");
    assert!(status.success(), "command failed with {status}");
}

fn host_key_fingerprint(ssh_keygen: &str, public_key: &PathBuf) -> String {
    let output = Command::new(ssh_keygen)
        .arg("-lf")
        .arg(public_key)
        .output()
        .expect("read host key fingerprint");
    assert!(output.status.success(), "ssh-keygen -lf failed");
    let stdout = String::from_utf8(output.stdout).expect("ssh-keygen output utf8");
    stdout
        .split_whitespace()
        .nth(1)
        .expect("host key fingerprint")
        .to_string()
}

fn served_host_key_fingerprint(ssh_keygen: &str, port: u16, dir: &PathBuf) -> String {
    let keyscan_output = Command::new("ssh-keyscan")
        .args(["-p", &port.to_string(), "-t", "ed25519", "127.0.0.1"])
        .output()
        .expect("scan served host key");
    assert!(keyscan_output.status.success(), "ssh-keyscan failed");
    let served_key = dir.join("served_host_key.pub");
    fs::write(&served_key, keyscan_output.stdout).expect("write served host key");
    host_key_fingerprint(ssh_keygen, &served_key)
}

#[cfg(unix)]
fn set_private_permissions(
    dir: &PathBuf,
    client_key: &PathBuf,
    host_key: &PathBuf,
    authorized_keys: &PathBuf,
) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).expect("chmod temp dir");
    for path in [client_key, host_key, authorized_keys] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("chmod ssh material");
    }
}

#[cfg(not(unix))]
fn set_private_permissions(
    _dir: &PathBuf,
    _client_key: &PathBuf,
    _host_key: &PathBuf,
    _authorized_keys: &PathBuf,
) {
}
