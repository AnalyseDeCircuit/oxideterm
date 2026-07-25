// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Debug)]
struct VncCancelledIo;

impl std::fmt::Display for VncCancelledIo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VNC connection canceled")
    }
}

impl std::error::Error for VncCancelledIo {}

pub(super) fn is_vnc_canceled_io(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.downcast_ref::<VncCancelledIo>().is_some())
}

pub(super) trait VncTransport: Read + Write + Send {
    fn shutdown_transport(&self);
    fn set_phase_timeout(&mut self, timeout: Option<Duration>);
    fn peer_certificate_der(&self) -> VncResult<Option<Vec<u8>>>;
}

#[derive(Debug)]
pub(super) struct CancellableTcpStream {
    stream: TcpStream,
    canceled: Arc<AtomicBool>,
    phase_deadline: Option<std::time::Instant>,
}

impl CancellableTcpStream {
    pub(super) fn new(stream: TcpStream, canceled: Arc<AtomicBool>) -> Self {
        Self {
            stream,
            canceled,
            phase_deadline: None,
        }
    }

    pub(super) fn set_phase_timeout(&mut self, timeout: Duration) {
        self.phase_deadline = Some(std::time::Instant::now() + timeout);
    }

    pub(super) fn clear_phase_timeout(&mut self) {
        self.phase_deadline = None;
    }

    fn ensure_active(&self) -> io::Result<()> {
        if self.canceled.load(Ordering::Acquire) {
            // `Read::read_exact` retries `Interrupted` forever, so cancellation
            // uses a typed non-retryable error that the protocol boundary maps
            // back to the structured Cancelled category.
            return Err(io::Error::new(io::ErrorKind::Other, VncCancelledIo));
        }
        if self
            .phase_deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "VNC connection phase timed out",
            ));
        }
        Ok(())
    }
}

impl Read for CancellableTcpStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            self.ensure_active()?;
            match self.stream.read(buffer) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    if self.phase_deadline.is_some() {
                        continue;
                    }
                    return Err(error);
                }
                result => return result,
            }
        }
    }
}

impl Write for CancellableTcpStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.ensure_active()?;
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.ensure_active()?;
        self.stream.flush()
    }
}

impl VncTransport for CancellableTcpStream {
    fn shutdown_transport(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    fn set_phase_timeout(&mut self, timeout: Option<Duration>) {
        match timeout {
            Some(timeout) => self.set_phase_timeout(timeout),
            None => self.clear_phase_timeout(),
        }
    }

    fn peer_certificate_der(&self) -> VncResult<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl VncTransport for native_tls::TlsStream<CancellableTcpStream> {
    fn shutdown_transport(&self) {
        self.get_ref().shutdown_transport();
    }

    fn set_phase_timeout(&mut self, timeout: Option<Duration>) {
        match timeout {
            Some(timeout) => self.get_mut().set_phase_timeout(timeout),
            None => self.get_mut().clear_phase_timeout(),
        }
    }

    fn peer_certificate_der(&self) -> VncResult<Option<Vec<u8>>> {
        self.peer_certificate()
            .map_err(|error| {
                VncError::certificate(format!("VNC TLS peer certificate read failed: {error}"))
            })?
            .map(|certificate| {
                certificate.to_der().map_err(|error| {
                    VncError::certificate(format!(
                        "VNC TLS peer certificate encoding failed: {error}"
                    ))
                })
            })
            .transpose()
    }
}
