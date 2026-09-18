use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{ConnectOptions, Error, FtpSession, Security};

trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Socket for T {}

struct Server {
    address: SocketAddr,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
    trust: Arc<rustls::ClientConfig>,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start() -> Self {
        Self::with_legacy_listing(false).await
    }

    async fn with_legacy_listing(legacy: bool) -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate = certified.cert.der().clone();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.clone()], key.into())
            .unwrap();
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certificate).unwrap();
        let trust = Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let files = Arc::new(Mutex::new(HashMap::from([(
            "/note.txt".into(),
            b"original remote content".to_vec(),
        )])));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shared_files = files.clone();
        let shared_requests = requests.clone();
        let directories = Arc::new(Mutex::new(HashSet::from(["/notes".to_string()])));
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (socket, _) = connection.unwrap();
                        clients.spawn(serve(socket, tls.clone(), shared_files.clone(), shared_requests.clone(), directories.clone(), legacy));
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
        });
        Self {
            address,
            files,
            requests,
            trust,
            task,
        }
    }

    fn options(&self, security: Security) -> ConnectOptions {
        ConnectOptions {
            host: "localhost".into(),
            port: self.address.port(),
            username: "test".into(),
            password: Zeroizing::new("test-password".into()),
            security,
            timeout: Duration::from_secs(3),
            proxy: None,
        }
    }
}

async fn serve(
    socket: TcpStream,
    tls: tokio_rustls::TlsAcceptor,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
    directories: Arc<Mutex<HashSet<String>>>,
    legacy: bool,
) -> std::io::Result<()> {
    let mut socket: BufReader<Box<dyn Socket>> = BufReader::new(Box::new(socket));
    socket.write_all(b"220 test server\r\n").await?;
    let mut passive = None;
    let mut secured_data = false;
    let mut rename_from = None;
    loop {
        let mut line = String::new();
        if socket.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let (command, argument) = line
            .trim_end_matches(['\r', '\n'])
            .split_once(' ')
            .unwrap_or((line.trim(), ""));
        // Record only command names, never credentials or arbitrary command text.
        requests.lock().unwrap().push(command.to_string());
        match command {
            "AUTH" => {
                socket.write_all(b"234 begin TLS\r\n").await?;
                socket = BufReader::new(Box::new(tls.accept(socket.into_inner()).await?));
            }
            "USER" => socket.write_all(b"331 password required\r\n").await?,
            "PASS" => socket.write_all(b"230 authenticated\r\n").await?,
            "PBSZ" | "TYPE" => socket.write_all(b"200 OK\r\n").await?,
            "PROT" => {
                secured_data = argument == "P";
                socket.write_all(b"200 OK\r\n").await?;
            }
            "FEAT" if legacy => socket.write_all(b"502 unsupported\r\n").await?,
            "EPSV" if legacy => socket.write_all(b"502 unsupported\r\n").await?,
            "FEAT" => {
                socket
                    .write_all(b"211-Features\r\n MLST type*;size*;modify*;\r\n211 End\r\n")
                    .await?
            }
            "PWD" => socket.write_all(b"257 \"/\"\r\n").await?,
            "EPSV" | "PASV" => {
                let data = TcpListener::bind("127.0.0.1:0").await?;
                let port = data.local_addr()?.port();
                let reply = if command == "PASV" {
                    format!(
                        "227 Entering Passive Mode (127,0,0,2,{},{})\r\n",
                        port / 256,
                        port % 256
                    )
                } else {
                    format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                };
                socket.write_all(reply.as_bytes()).await?;
                passive = Some(data);
            }
            "SIZE" => {
                let size = files.lock().unwrap().get(argument).map(Vec::len);
                match size {
                    Some(size) => {
                        socket
                            .write_all(format!("213 {size}\r\n").as_bytes())
                            .await?
                    }
                    None => socket.write_all(b"550 absent\r\n").await?,
                }
            }
            "MLSD" | "LIST" | "RETR" | "STOR" => {
                socket.write_all(b"150 opening data\r\n").await?;
                let (data, _) = passive.take().unwrap().accept().await?;
                let mut data: Box<dyn Socket> = if secured_data {
                    Box::new(tls.accept(data).await?)
                } else {
                    Box::new(data)
                };
                match command {
                    "MLSD" | "LIST" => {
                        let prefix = if argument == "." || argument == "/" {
                            "/".to_string()
                        } else {
                            format!("{}/", argument.trim_end_matches('/'))
                        };
                        let mut rows = Vec::new();
                        for (path, bytes) in files.lock().unwrap().iter() {
                            if let Some(name) = path
                                .strip_prefix(&prefix)
                                .filter(|name| !name.contains('/'))
                            {
                                rows.push(if command == "MLSD" {
                                    format!("type=file;size={}; {name}\r\n", bytes.len())
                                } else {
                                    format!(
                                        "-rw-r--r-- 1 user group {} Jan 01 2026 {name}\r\n",
                                        bytes.len()
                                    )
                                });
                            }
                        }
                        for path in directories.lock().unwrap().iter() {
                            if let Some(name) = path
                                .strip_prefix(&prefix)
                                .filter(|name| !name.contains('/'))
                            {
                                rows.push(if command == "MLSD" {
                                    format!("type=dir; {name}\r\n")
                                } else {
                                    format!("drwxr-xr-x 1 user group 0 Jan 01 2026 {name}\r\n")
                                });
                            }
                        }
                        rows.sort();
                        data.write_all(rows.concat().as_bytes()).await?;
                    }
                    "STOR" => {
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes).await?;
                        files.lock().unwrap().insert(argument.to_owned(), bytes);
                    }
                    _ if argument == "/stall" => {
                        let mut one = [0];
                        let _ = data.read(&mut one).await;
                        return Ok(());
                    }
                    _ => {
                        let bytes = files
                            .lock()
                            .unwrap()
                            .get(argument)
                            .cloned()
                            .unwrap_or_default();
                        data.write_all(&bytes).await?;
                    }
                }
                // Upload clients may close the data socket immediately after
                // their TLS close_notify; the control reply completes the upload.
                if command == "STOR" {
                    let _ = data.shutdown().await;
                } else {
                    data.shutdown().await?;
                }
                drop(data);
                socket.write_all(b"226 transfer complete\r\n").await?;
            }
            "RNFR" => {
                rename_from = Some(argument.to_owned());
                socket.write_all(b"350 send destination\r\n").await?;
            }
            "MKD" => {
                directories.lock().unwrap().insert(argument.to_string());
                socket.write_all(b"257 directory created\r\n").await?;
            }
            "DELE" | "RMD" => {
                if command == "DELE" {
                    files.lock().unwrap().remove(argument);
                } else {
                    directories.lock().unwrap().remove(argument);
                }
                socket.write_all(b"250 deleted\r\n").await?;
            }
            "RNTO" => {
                let value = files
                    .lock()
                    .unwrap()
                    .remove(&rename_from.take().unwrap())
                    .unwrap();
                files.lock().unwrap().insert(argument.to_owned(), value);
                socket.write_all(b"250 renamed\r\n").await?;
            }
            "QUIT" => {
                socket.write_all(b"221 goodbye\r\n").await?;
                return Ok(());
            }
            _ => socket.write_all(b"502 unsupported\r\n").await?,
        }
    }
}

#[tokio::test]
async fn ftp_and_ftps_browse_transfer_and_replace_after_completion() {
    for security in [Security::Plain, Security::ExplicitTls] {
        let server = Server::start().await;
        let cancel = CancellationToken::new();
        let options = server.options(security);
        let mut session =
            FtpSession::connect_with_tls(&options, &cancel, Some(server.trust.clone()))
                .await
                .unwrap();
        let mut entries = session.list("/", &cancel).await.unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["note.txt", "notes"]
        );
        assert_eq!(
            session.read("/note.txt", 1024, &cancel).await.unwrap(),
            b"original remote content"
        );
        session
            .write("/note.txt", b"new text", &cancel)
            .await
            .unwrap();
        assert_eq!(server.files.lock().unwrap()["/note.txt"], b"new text");
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("note.txt");
        tokio::fs::write(&local, b"old local data").await.unwrap();
        session
            .download("/note.txt", &local, true, &cancel, |_| async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(&local).await.unwrap(), b"new text");
        tokio::fs::write(&local, b"keep existing").await.unwrap();
        assert!(
            matches!(session.download("/note.txt",&local,false,&cancel,|_|async {Ok(())}).await,Err(Error::Io(error)) if error.kind()==std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(tokio::fs::read(&local).await.unwrap(), b"keep existing");
        tokio::fs::write(&local, b"new text").await.unwrap();
        session
            .upload(&local, "/uploaded.txt", &cancel, |_| async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(server.files.lock().unwrap()["/uploaded.txt"], b"new text");
        session.disconnect().await;
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.contains(&"AUTH".into()),
            security == Security::ExplicitTls
        );
        assert_eq!(
            requests.contains(&"PROT".into()),
            security == Security::ExplicitTls
        );
    }
}

#[tokio::test]
async fn legacy_pasv_listing_and_directory_transfer() {
    let server = Server::with_legacy_listing(true).await;
    let cancel = CancellationToken::new();
    let mut session = FtpSession::connect(&server.options(Security::Plain), &cancel)
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let local = directory.path().join("source");
    tokio::fs::create_dir_all(local.join("nested"))
        .await
        .unwrap();
    tokio::fs::write(local.join("nested/hello.txt"), b"directory content")
        .await
        .unwrap();
    session
        .upload_directory(&local, "/uploaded", &cancel, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(
        server.files.lock().unwrap()["/uploaded/nested/hello.txt"],
        b"directory content"
    );
    let destination = directory.path().join("download");
    session
        .download_directory("/uploaded", &destination, &cancel, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(destination.join("nested/hello.txt"))
            .await
            .unwrap(),
        b"directory content"
    );
    session
        .delete_recursive("/uploaded", &cancel)
        .await
        .unwrap();
    assert!(
        !server
            .files
            .lock()
            .unwrap()
            .contains_key("/uploaded/nested/hello.txt")
    );
    assert!(session.stat("/uploaded", &cancel).await.unwrap().is_none());
    let commands = server.requests.lock().unwrap();
    assert!(commands.contains(&"PASV".to_string()) && commands.contains(&"LIST".to_string()));
}

#[tokio::test]
async fn cancelled_download_preserves_destination_and_discards_connection() {
    let server = Server::start().await;
    let cancel = CancellationToken::new();
    let mut session = FtpSession::connect(&server.options(Security::Plain), &cancel)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("keep.txt");
    tokio::fs::write(&destination, b"keep this").await.unwrap();
    let trigger = cancel.clone();
    let requests = server.requests.clone();
    let cancelling = tokio::spawn(async move {
        while !requests.lock().unwrap().iter().any(|c| c == "RETR") {
            tokio::task::yield_now().await;
        }
        trigger.cancel();
    });
    assert!(matches!(
        session
            .download("/stall", &destination, true, &cancel, |_| async { Ok(()) })
            .await,
        Err(Error::Cancelled)
    ));
    cancelling.await.unwrap();
    assert!(!session.is_reusable());
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"keep this");
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>(),
        ["keep.txt"]
    );
}

#[tokio::test]
async fn untrusted_ftps_is_rejected_before_credentials() {
    let server = Server::start().await;
    let error = FtpSession::connect(
        &server.options(Security::ExplicitTls),
        &CancellationToken::new(),
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(error, Error::Tls));
    assert!(
        !server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "USER" || c == "PASS")
    );
}

#[tokio::test]
async fn socks_proxy_carries_control_and_data_with_remote_dns() {
    use oxideterm_network_proxy::tcp::{
        UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol,
    };
    let server = Server::start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_address = listener.local_addr().unwrap();
    let targets = Arc::new(Mutex::new(Vec::new()));
    let recorded = targets.clone();
    let proxy = tokio::spawn(async move {
        let mut tunnels = JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept() => {
                    let (mut client, _)=accepted.unwrap();
                    let recorded=recorded.clone();
                    tunnels.spawn(async move {
                        let mut header=[0;2]; client.read_exact(&mut header).await.unwrap();
                        assert_eq!(header[0],5);
                        let mut methods=vec![0;header[1] as usize]; client.read_exact(&mut methods).await.unwrap();
                        client.write_all(&[5,0]).await.unwrap();
                        let mut request=[0;4]; client.read_exact(&mut request).await.unwrap();
                        assert_eq!(request,[5,1,0,3],"both connections must use remote DNS");
                        let size=client.read_u8().await.unwrap();
                        let mut hostname=vec![0;size as usize]; client.read_exact(&mut hostname).await.unwrap();
                        let port=client.read_u16().await.unwrap();
                        recorded.lock().unwrap().push((String::from_utf8(hostname).unwrap(),port));
                        let mut destination=TcpStream::connect(("127.0.0.1",port)).await.unwrap();
                        client.write_all(&[5,0,0,1,127,0,0,1,0,0]).await.unwrap();
                        let _=tokio::io::copy_bidirectional(&mut client,&mut destination).await;
                    });
                }
                _=tunnels.join_next(), if !tunnels.is_empty() => {}
            }
        }
    });
    let mut options = server.options(Security::Plain);
    options.host = "ftp.invalid".into();
    options.proxy = Some(UpstreamProxyConfig {
        protocol: UpstreamProxyProtocol::Socks5,
        host: proxy_address.ip().to_string(),
        port: proxy_address.port(),
        auth: UpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    });
    let cancel = CancellationToken::new();
    let mut session = FtpSession::connect(&options, &cancel).await.unwrap();
    assert_eq!(
        session.read("/note.txt", 1024, &cancel).await.unwrap(),
        b"original remote content"
    );
    session.disconnect().await;
    proxy.abort();
    let _ = proxy.await;
    let targets = targets.lock().unwrap();
    assert_eq!(targets[0], ("ftp.invalid".into(), server.address.port()));
    assert_eq!(targets[1].0, "ftp.invalid");
    assert_ne!(targets[1].1, server.address.port());
}
