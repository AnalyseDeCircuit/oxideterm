use std::{future::Future, sync::Arc, time::Duration};

use oxideterm_network_proxy::tcp::{UpstreamProxyConfig, dial_initial_tcp};
use suppaftp::{
    FtpError,
    tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream},
    types::{FileType, Mode},
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{Error, Result};

pub(crate) type Stream = AsyncRustlsFtpStream;
pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Security {
    Plain,
    ExplicitTls,
}

pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Zeroizing<String>,
    pub security: Security,
    pub timeout: Duration,
    pub proxy: Option<UpstreamProxyConfig>,
}

impl std::fmt::Debug for ConnectOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectOptions")
            .field("security", &self.security)
            .finish_non_exhaustive()
    }
}

pub struct FtpSession {
    pub(crate) stream: Option<Stream>,
    pub(crate) home: String,
    pub(crate) machine_listing: bool,
    pub(crate) pending_upload: Option<String>,
}

impl FtpSession {
    pub async fn connect(options: &ConnectOptions, cancel: &CancellationToken) -> Result<Self> {
        Self::connect_with_tls(options, cancel, None).await
    }

    pub(crate) async fn connect_with_tls(
        options: &ConnectOptions,
        cancel: &CancellationToken,
        tls: Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Self> {
        if options.host.trim().is_empty()
            || options.port == 0
            || options.timeout.is_zero()
            || options.host.contains(['\r', '\n', '\0'])
            || options.username.contains(['\r', '\n', '\0'])
            || options.password.contains(['\r', '\n', '\0'])
        {
            return Err(Error::InvalidInput);
        }
        let establish = async {
            let tcp = dial_initial_tcp(
                &options.host,
                options.port,
                options.timeout.as_secs().max(1),
                options.proxy.as_ref(),
            )
            .await
            .map_err(|_| Error::Protocol)?;
            let target_host = options.host.clone();
            let proxy = options.proxy.clone();
            let seconds = options.timeout.as_secs().max(1);
            let mut stream = Stream::connect_with_stream(tcp)
                .await?
                .passive_stream_builder(move |address| {
                    let host = target_host.clone();
                    let proxy = proxy.clone();
                    Box::pin(async move {
                        // PASV/EPSV supply only the port. The control socket's peer may
                        // be a proxy, and a server-supplied address must not redirect us.
                        dial_initial_tcp(&host, address.port(), seconds, proxy.as_ref())
                            .await
                            .map_err(|_| {
                                FtpError::ConnectionError(std::io::Error::other(
                                    "FTP data connection failed",
                                ))
                            })
                    })
                });
            if options.security == Security::ExplicitTls {
                let tls = match tls {
                    Some(config) => config,
                    None => system_tls()?,
                };
                stream = stream
                    .into_secure(
                        AsyncRustlsConnector::from(tokio_rustls::TlsConnector::from(tls)),
                        &options.host,
                    )
                    .await?;
            }
            stream
                .login(options.username.as_str(), options.password.as_str())
                .await?;
            stream.transfer_type(FileType::Binary).await?;
            // Probe EPSV before data commands so an unsupported command can be
            // retried safely without replaying RETR or STOR.
            match stream
                .custom_command("EPSV", &[suppaftp::Status::ExtendedPassiveMode])
                .await
            {
                Ok(_) => stream.set_mode(Mode::ExtendedPassive),
                Err(FtpError::UnexpectedResponse(r))
                    if matches!(r.status as u32, 500 | 501 | 502 | 504 | 522) =>
                {
                    stream.set_mode(Mode::Passive)
                }
                Err(error) => return Err(error.into()),
            }
            let machine_listing = match stream.feat().await {
                Ok(features) => features.keys().any(|name| {
                    name.eq_ignore_ascii_case("MLST") || name.eq_ignore_ascii_case("MLSD")
                }),
                Err(FtpError::UnexpectedResponse(r))
                    if matches!(r.status as u32, 500 | 502 | 504) =>
                {
                    false
                }
                Err(error) => return Err(error.into()),
            };
            let home = stream.pwd().await?;
            Ok(Self {
                stream: Some(stream),
                home,
                machine_listing,
                pending_upload: None,
            })
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(Error::Cancelled),
            result = tokio::time::timeout(options.timeout, establish) => result.map_err(|_| Error::Timeout)?,
        }
    }

    pub fn home(&self) -> &str {
        &self.home
    }
    pub fn is_reusable(&self) -> bool {
        self.stream.is_some()
    }

    /// A failed data exchange needs a fresh connection to remove its staging file.
    pub fn take_pending_upload(&mut self) -> Option<String> {
        self.pending_upload.take()
    }

    pub(crate) async fn operate<T, F, Fut>(
        &mut self,
        cancel: &CancellationToken,
        deadline: Option<Duration>,
        action: F,
    ) -> Result<T>
    where
        F: FnOnce(Stream) -> Fut,
        Fut: Future<Output = Result<(Stream, T)>>,
    {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let stream = self.stream.take().ok_or(Error::Disconnected)?;
        // Restore a connection only after the complete command/transfer reply.
        // Dropping this future, cancelling it, or timing out closes the socket.
        let operation = async {
            match deadline {
                Some(duration) => tokio::time::timeout(duration, action(stream))
                    .await
                    .map_err(|_| Error::Timeout)?,
                None => action(stream).await,
            }
        };
        let (stream, value) = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            result = operation => result?,
        };
        self.stream = Some(stream);
        Ok(value)
    }

    pub async fn disconnect(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), stream.quit()).await;
        }
    }

    pub async fn mkdir(&mut self, path: &str, cancel: &CancellationToken) -> Result<()> {
        validate_path(path)?;
        self.operate(cancel, Some(COMMAND_TIMEOUT), |mut stream| async move {
            stream.mkdir(path).await?;
            Ok((stream, ()))
        })
        .await
    }

    pub async fn rename(&mut self, from: &str, to: &str, cancel: &CancellationToken) -> Result<()> {
        validate_path(from)?;
        validate_path(to)?;
        self.operate(cancel, Some(COMMAND_TIMEOUT), |mut stream| async move {
            stream.rename(from, to).await?;
            Ok((stream, ()))
        })
        .await
    }

    pub async fn delete(
        &mut self,
        path: &str,
        directory: bool,
        cancel: &CancellationToken,
    ) -> Result<()> {
        validate_path(path)?;
        self.operate(cancel, Some(COMMAND_TIMEOUT), |mut stream| async move {
            if directory {
                stream.rmdir(path).await?;
            } else {
                stream.rm(path).await?;
            }
            Ok((stream, ()))
        })
        .await
    }
}

pub(crate) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains(['\r', '\n', '\0']) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}

fn system_tls() -> Result<Arc<rustls::ClientConfig>> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    if roots.is_empty() {
        return Err(Error::Tls);
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| Error::Tls)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(Arc::new(config))
}
