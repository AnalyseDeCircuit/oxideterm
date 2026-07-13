use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll, Waker},
};

use bytes::Bytes;
use dashmap::DashMap;
use tokio::{
    io::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
    select,
    sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore},
};
use tokio_util::sync::CancellationToken;

use super::{error::Error, process_handler, runtime, Handler, OwnedSftpWriter};
use crate::{error::Error as ProtocolError, protocol::Packet};

pub(crate) type RequestKey = Option<u32>;
pub(crate) type SftpResult<T> = Result<T, Error>;
pub(crate) type SharedRequests = DashMap<RequestKey, RequestEntry>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RequestPhase {
    /// The frame is locally queued. Cancellation in this phase guarantees that
    /// the request is skipped before reaching SSH and has no remote side effect.
    Queued = 0,
    /// The frame has started entering SSH. Cancellation from this phase cannot
    /// retract the SFTP operation, so its remote side effect is possible.
    Sent = 1,
    /// An SFTP response arrived. This confirms protocol completion, not durable
    /// storage; callers still need the negotiated fsync operation for durability.
    Acknowledged = 2,
    /// The local caller cancelled while the frame was still queued.
    CancelledBeforeSend = 3,
    /// The local caller stopped waiting after sending began.
    AbandonedAfterSend = 4,
    /// The live session disconnected before SSH sending began.
    DisconnectedBeforeSend = 5,
    /// The live session disconnected after SSH sending began; remote outcome is unknown.
    DisconnectedAfterSend = 6,
}

pub(crate) struct RequestLifecycle {
    phase: AtomicU8,
}

impl RequestLifecycle {
    fn queued() -> Self {
        Self {
            phase: AtomicU8::new(RequestPhase::Queued as u8),
        }
    }

    pub(crate) fn phase(&self) -> RequestPhase {
        match self.phase.load(Ordering::Acquire) {
            0 => RequestPhase::Queued,
            1 => RequestPhase::Sent,
            2 => RequestPhase::Acknowledged,
            3 => RequestPhase::CancelledBeforeSend,
            4 => RequestPhase::AbandonedAfterSend,
            5 => RequestPhase::DisconnectedBeforeSend,
            6 => RequestPhase::DisconnectedAfterSend,
            _ => unreachable!("request phase is always written from RequestPhase"),
        }
    }

    fn transition(&self, from: RequestPhase, to: RequestPhase) -> bool {
        self.phase
            .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn mark_sent(&self) -> bool {
        self.transition(RequestPhase::Queued, RequestPhase::Sent)
    }

    pub(crate) fn mark_acknowledged(&self) {
        loop {
            let current = self.phase();
            match current {
                RequestPhase::Sent
                | RequestPhase::AbandonedAfterSend
                | RequestPhase::DisconnectedAfterSend => {
                    if self.transition(current, RequestPhase::Acknowledged) {
                        return;
                    }
                }
                RequestPhase::Acknowledged => return,
                RequestPhase::Queued => {
                    debug_assert!(false, "a queued request cannot receive a response");
                    return;
                }
                _ => return,
            }
        }
    }

    fn mark_disconnected(&self) {
        loop {
            let current = self.phase();
            let disconnected = match current {
                RequestPhase::Queued => RequestPhase::DisconnectedBeforeSend,
                RequestPhase::Sent | RequestPhase::AbandonedAfterSend => {
                    RequestPhase::DisconnectedAfterSend
                }
                _ => return,
            };
            if self.transition(current, disconnected) {
                return;
            }
        }
    }
}

struct InflightPermit {
    permit: Option<OwnedSemaphorePermit>,
    capacity_waiters: Arc<CapacityWaiters>,
}

impl Drop for InflightPermit {
    fn drop(&mut self) {
        // Release before waking poll-based writers so their immediate retry can
        // observe the newly available session budget.
        drop(self.permit.take());
        self.capacity_waiters.wake_all();
    }
}

pub(crate) struct RequestEntry {
    pub(crate) response: oneshot::Sender<SftpResult<Packet>>,
    pub(crate) lifecycle: Arc<RequestLifecycle>,
    _inflight_permit: Arc<InflightPermit>,
}

pub(crate) struct PendingRequest {
    request_key: RequestKey,
    response: oneshot::Receiver<SftpResult<Packet>>,
    lifecycle: Arc<RequestLifecycle>,
    requests: Weak<SharedRequests>,
    completed: bool,
    cancel_on_drop: bool,
}

impl PendingRequest {
    fn new(
        request_key: RequestKey,
        response: oneshot::Receiver<SftpResult<Packet>>,
        lifecycle: Arc<RequestLifecycle>,
        requests: &Arc<SharedRequests>,
    ) -> Self {
        Self {
            request_key,
            response,
            lifecycle,
            requests: Arc::downgrade(requests),
            completed: false,
            cancel_on_drop: true,
        }
    }

    /// Leaves a best-effort request owned by the live session after the local
    /// caller stops waiting for its response.
    pub(crate) fn detach(mut self) {
        self.cancel_on_drop = false;
    }

    #[cfg(test)]
    pub(crate) fn lifecycle(&self) -> Arc<RequestLifecycle> {
        self.lifecycle.clone()
    }

    #[cfg(test)]
    pub(crate) fn from_test_receiver(response: oneshot::Receiver<SftpResult<Packet>>) -> Self {
        Self {
            request_key: Some(u32::MAX),
            response,
            lifecycle: Arc::new(RequestLifecycle::queued()),
            requests: Weak::new(),
            completed: false,
            cancel_on_drop: false,
        }
    }
}

impl Future for PendingRequest {
    type Output = Result<SftpResult<Packet>, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = Pin::new(&mut self.response).poll(cx);
        if result.is_ready() {
            self.completed = true;
        }
        result
    }
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        if self.completed || !self.cancel_on_drop {
            return;
        }

        match self.lifecycle.phase() {
            RequestPhase::Queued => {
                if self
                    .lifecycle
                    .transition(RequestPhase::Queued, RequestPhase::CancelledBeforeSend)
                {
                    if let Some(requests) = self.requests.upgrade() {
                        requests.remove_if(&self.request_key, |_, entry| {
                            Arc::ptr_eq(&entry.lifecycle, &self.lifecycle)
                        });
                    }
                }
            }
            RequestPhase::Sent => {
                // SFTP cannot retract a request already handed to SSH. Keep the
                // waiter and byte permit until a late reply or disconnect clears it.
                let _ = self
                    .lifecycle
                    .transition(RequestPhase::Sent, RequestPhase::AbandonedAfterSend);
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct CapacityWaiters {
    waiters: Mutex<Vec<Waker>>,
}

impl CapacityWaiters {
    fn register(&self, waker: &Waker) {
        let mut waiters = self
            .waiters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if waiters.iter().any(|registered| registered.will_wake(waker)) {
            return;
        }
        waiters.push(waker.clone());
    }

    fn wake_all(&self) {
        let waiters = {
            let mut registered = self
                .waiters
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            std::mem::take(&mut *registered)
        };
        for waker in waiters {
            waker.wake();
        }
    }
}

struct OutboundFrame {
    request_key: RequestKey,
    bytes: Bytes,
    lifecycle: Arc<RequestLifecycle>,
    // A cancelled queued request releases its registration immediately, but
    // the frame keeps this shared permit until the writer skips and drops it.
    _inflight_permit: Arc<InflightPermit>,
}

#[derive(Debug)]
pub(crate) enum TryQueueError {
    Full { required_bytes: usize },
    Sftp(Error),
}

pub(crate) struct OutboundReservation {
    frame_len: usize,
    permit: OwnedSemaphorePermit,
}

impl From<Error> for TryQueueError {
    fn from(error: Error) -> Self {
        Self::Sftp(error)
    }
}

#[derive(Clone)]
pub(crate) struct SessionTransport {
    sender: mpsc::Sender<OutboundFrame>,
    capacity: Arc<Semaphore>,
    capacity_waiters: Arc<CapacityWaiters>,
    cancellation: CancellationToken,
    requests: Arc<SharedRequests>,
    max_inflight_bytes: usize,
}

impl SessionTransport {
    fn new(
        sender: mpsc::Sender<OutboundFrame>,
        capacity: Arc<Semaphore>,
        capacity_waiters: Arc<CapacityWaiters>,
        cancellation: CancellationToken,
        requests: Arc<SharedRequests>,
        max_inflight_bytes: usize,
    ) -> Self {
        Self {
            sender,
            capacity,
            capacity_waiters,
            cancellation,
            requests,
            max_inflight_bytes,
        }
    }

    fn frame_permits(&self, frame_len: usize) -> SftpResult<u32> {
        if frame_len > self.max_inflight_bytes {
            return Err(Error::Limited(format!(
                "outbound frame length {frame_len} exceeds session byte budget {}",
                self.max_inflight_bytes
            )));
        }
        u32::try_from(frame_len)
            .map_err(|_| Error::Limited("outbound frame length exceeds u32".to_owned()))
    }

    async fn acquire(&self, frame_len: usize) -> SftpResult<OwnedSemaphorePermit> {
        let permits = self.frame_permits(frame_len)?;
        select! {
            biased;
            _ = self.cancellation.cancelled() => {
                Err(Error::UnexpectedBehavior("session closed".into()))
            }
            permit = self.capacity.clone().acquire_many_owned(permits) => {
                permit.map_err(|_| Error::UnexpectedBehavior("session closed".into()))
            }
        }
    }

    fn try_acquire(&self, frame_len: usize) -> Result<OwnedSemaphorePermit, TryQueueError> {
        let permits = self.frame_permits(frame_len)?;
        self.capacity
            .clone()
            .try_acquire_many_owned(permits)
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::NoPermits => TryQueueError::Full {
                    required_bytes: frame_len,
                },
                tokio::sync::TryAcquireError::Closed => {
                    TryQueueError::Sftp(Error::UnexpectedBehavior("session closed".into()))
                }
            })
    }

    pub(crate) async fn reserve(&self, frame_len: usize) -> SftpResult<OutboundReservation> {
        Ok(OutboundReservation {
            frame_len,
            permit: self.acquire(frame_len).await?,
        })
    }

    pub(crate) fn try_reserve(
        &self,
        frame_len: usize,
    ) -> Result<OutboundReservation, TryQueueError> {
        Ok(OutboundReservation {
            frame_len,
            permit: self.try_acquire(frame_len)?,
        })
    }

    fn register_request(
        &self,
        request_key: RequestKey,
        permit: OwnedSemaphorePermit,
    ) -> SftpResult<(PendingRequest, Arc<RequestLifecycle>, Arc<InflightPermit>)> {
        let lifecycle = Arc::new(RequestLifecycle::queued());
        let (response, receiver) = oneshot::channel();
        let inflight_permit = Arc::new(InflightPermit {
            permit: Some(permit),
            capacity_waiters: self.capacity_waiters.clone(),
        });
        let entry = RequestEntry {
            response,
            lifecycle: lifecycle.clone(),
            _inflight_permit: inflight_permit.clone(),
        };
        match self.requests.entry(request_key) {
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                vacant.insert(entry);
            }
            dashmap::mapref::entry::Entry::Occupied(_) => {
                return Err(Error::UnexpectedBehavior(format!(
                    "duplicate SFTP request id {request_key:?}"
                )));
            }
        }
        Ok((
            PendingRequest::new(request_key, receiver, lifecycle.clone(), &self.requests),
            lifecycle,
            inflight_permit,
        ))
    }

    fn remove_queued_request(&self, request_key: RequestKey, lifecycle: &Arc<RequestLifecycle>) {
        let _ = lifecycle.transition(RequestPhase::Queued, RequestPhase::CancelledBeforeSend);
        self.requests.remove_if(&request_key, |_, entry| {
            Arc::ptr_eq(&entry.lifecycle, lifecycle)
        });
    }

    pub(crate) async fn queue(
        &self,
        request_key: RequestKey,
        bytes: Bytes,
    ) -> SftpResult<PendingRequest> {
        let reservation = self.reserve(bytes.len()).await?;
        self.queue_reserved(request_key, bytes, reservation).await
    }

    pub(crate) async fn queue_reserved(
        &self,
        request_key: RequestKey,
        bytes: Bytes,
        reservation: OutboundReservation,
    ) -> SftpResult<PendingRequest> {
        if bytes.len() != reservation.frame_len {
            return Err(Error::UnexpectedBehavior(
                "encoded frame length changed after byte reservation".to_owned(),
            ));
        }
        let (pending, lifecycle, inflight_permit) =
            self.register_request(request_key, reservation.permit)?;
        let frame = OutboundFrame {
            request_key,
            bytes,
            lifecycle: lifecycle.clone(),
            _inflight_permit: inflight_permit,
        };
        let send_result = select! {
            biased;
            _ = self.cancellation.cancelled() => Err(()),
            result = self.sender.send(frame) => result.map_err(|_| ()),
        };
        if send_result.is_err() {
            self.remove_queued_request(request_key, &lifecycle);
            return Err(Error::UnexpectedBehavior("session closed".into()));
        }
        Ok(pending)
    }

    pub(crate) fn try_queue(
        &self,
        request_key: RequestKey,
        bytes: Bytes,
    ) -> Result<PendingRequest, TryQueueError> {
        if self.cancellation.is_cancelled() {
            return Err(Error::UnexpectedBehavior("session closed".into()).into());
        }
        let reservation = self.try_reserve(bytes.len())?;
        self.try_queue_reserved(request_key, bytes, reservation)
    }

    pub(crate) fn try_queue_reserved(
        &self,
        request_key: RequestKey,
        bytes: Bytes,
        reservation: OutboundReservation,
    ) -> Result<PendingRequest, TryQueueError> {
        if bytes.len() != reservation.frame_len {
            return Err(Error::UnexpectedBehavior(
                "encoded frame length changed after byte reservation".to_owned(),
            )
            .into());
        }
        let (pending, lifecycle, inflight_permit) =
            self.register_request(request_key, reservation.permit)?;
        let frame = OutboundFrame {
            request_key,
            bytes,
            lifecycle: lifecycle.clone(),
            _inflight_permit: inflight_permit,
        };
        if self.sender.try_send(frame).is_err() {
            self.remove_queued_request(request_key, &lifecycle);
            return Err(Error::UnexpectedBehavior("session closed".into()).into());
        }
        Ok(pending)
    }

    pub(crate) fn register_capacity_waker(&self, required_bytes: usize, cx: &Context<'_>) {
        self.capacity_waiters.register(cx.waker());
        if self.cancellation.is_cancelled() || self.capacity.available_permits() >= required_bytes {
            self.capacity_waiters.wake_all();
        }
    }

    pub(crate) fn close(&self) {
        self.terminate();
    }

    fn terminate(&self) {
        // Invalidating the live session wakes admission waiters and removes all
        // response ownership before either transport task can outlive it.
        self.cancellation.cancel();
        self.capacity.close();
        for entry in self.requests.iter() {
            entry.lifecycle.mark_disconnected();
        }
        self.requests.clear();
        self.capacity_waiters.wake_all();
    }
}

struct AsyncWriteOwned<W>(W);

impl<W> OwnedSftpWriter for AsyncWriteOwned<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn write_owned(&mut self, data: Bytes) -> io::Result<()> {
        self.0.write_all(data.as_ref()).await
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        self.0.shutdown().await
    }
}

pub(crate) fn run_session<S, H>(
    stream: S,
    handler: H,
    requests: Arc<SharedRequests>,
    max_inflight_bytes: usize,
) -> SessionTransport
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    H: Handler + Send + 'static,
{
    let (reader, writer) = io::split(stream);
    run_owned_session(
        reader,
        AsyncWriteOwned(writer),
        handler,
        requests,
        max_inflight_bytes,
    )
}

pub(crate) fn run_owned_session<R, W, H>(
    mut reader: R,
    mut writer: W,
    mut handler: H,
    requests: Arc<SharedRequests>,
    max_inflight_bytes: usize,
) -> SessionTransport
where
    R: AsyncRead + Unpin + Send + 'static,
    W: OwnedSftpWriter,
    H: Handler + Send + 'static,
{
    let queue_capacity = max_inflight_bytes.max(1);
    // This queue, byte semaphore, request registry, and cancellation token are
    // one ownership unit tied to the lifetime of a single live SFTP session.
    let (sender, mut receiver) = mpsc::channel(queue_capacity);
    let capacity = Arc::new(Semaphore::new(queue_capacity));
    let capacity_waiters = Arc::new(CapacityWaiters::default());
    let cancellation = CancellationToken::new();
    let transport = SessionTransport::new(
        sender,
        capacity,
        capacity_waiters,
        cancellation,
        requests,
        queue_capacity,
    );

    let read_transport = transport.clone();
    runtime::spawn(async move {
        loop {
            select! {
                biased;
                _ = read_transport.cancellation.cancelled() => break,
                result = process_handler(&mut reader, &mut handler) => {
                    match result {
                        Ok(()) => {}
                        Err(ProtocolError::BadMessage(error)) => warn!("{error}"),
                        Err(error) => {
                            if !matches!(error, ProtocolError::UnexpectedEof) {
                                warn!("{error}");
                            }
                            break;
                        }
                    }
                }
            }
        }
        read_transport.terminate();
        debug!("read half of sftp stream ended");
    });

    let write_transport = transport.clone();
    runtime::spawn(async move {
        loop {
            let frame = select! {
                biased;
                _ = write_transport.cancellation.cancelled() => break,
                frame = receiver.recv() => match frame {
                    Some(frame) => frame,
                    None => break,
                },
            };

            let write_result = select! {
                biased;
                _ = write_transport.cancellation.cancelled() => break,
                result = async {
                    // Mark Sent in the same poll which first enters the owned
                    // writer. If cancellation wins before this branch is polled,
                    // the request remains queued and has no remote side effect.
                    if !frame.lifecycle.mark_sent() {
                        return Ok(());
                    }
                    writer.write_owned(frame.bytes).await
                } => result,
            };
            if let Err(error) = write_result {
                warn!("SFTP transport write failed: {error}");
                break;
            }
        }

        receiver.close();
        while let Ok(frame) = receiver.try_recv() {
            write_transport.remove_queued_request(frame.request_key, &frame.lifecycle);
        }
        write_transport.terminate();
        let _ = writer.shutdown().await;
        debug!("write half of sftp stream ended");
    });

    transport
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Status, StatusCode};
    use tokio::{io::DuplexStream, sync::Notify, time::Duration};

    fn test_transport(
        max_inflight_bytes: usize,
    ) -> (
        SessionTransport,
        mpsc::Receiver<OutboundFrame>,
        Arc<SharedRequests>,
    ) {
        let (sender, receiver) = mpsc::channel(max_inflight_bytes.max(1));
        let requests = Arc::new(SharedRequests::new());
        let transport = SessionTransport::new(
            sender,
            Arc::new(Semaphore::new(max_inflight_bytes.max(1))),
            Arc::new(CapacityWaiters::default()),
            CancellationToken::new(),
            requests.clone(),
            max_inflight_bytes.max(1),
        );
        (transport, receiver, requests)
    }

    fn ok_status(id: u32) -> Packet {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        }
        .into()
    }

    #[tokio::test]
    async fn byte_budget_is_held_until_acknowledgement() {
        let (transport, mut receiver, requests) = test_transport(8);
        let first = transport
            .queue(Some(1), Bytes::from_static(b"12345678"))
            .await
            .expect("first frame fits budget");

        let blocked = tokio::time::timeout(
            Duration::from_millis(10),
            transport.queue(Some(2), Bytes::from_static(b"x")),
        )
        .await;
        assert!(
            blocked.is_err(),
            "second frame must wait for acknowledgement"
        );

        let frame = receiver.recv().await.expect("writer receives first frame");
        assert!(frame.lifecycle.mark_sent());
        drop(frame);
        let (_, entry) = requests.remove(&Some(1)).expect("first request registered");
        let RequestEntry {
            response,
            lifecycle,
            _inflight_permit: inflight_permit,
        } = entry;
        lifecycle.mark_acknowledged();
        response
            .send(Ok(ok_status(1)))
            .expect("first caller still waits");
        drop(inflight_permit);
        assert!(first.await.expect("response sender remains live").is_ok());

        let second = tokio::time::timeout(
            Duration::from_millis(100),
            transport.queue(Some(2), Bytes::from_static(b"x")),
        )
        .await
        .expect("acknowledgement releases byte budget")
        .expect("second frame queues");
        drop(second);
    }

    #[tokio::test]
    async fn queued_cancellation_prevents_transport_send() {
        let (transport, mut receiver, requests) = test_transport(16);
        let pending = transport
            .queue(Some(7), Bytes::from_static(b"queued"))
            .await
            .expect("frame queues");
        let lifecycle = pending.lifecycle();

        drop(pending);

        assert_eq!(lifecycle.phase(), RequestPhase::CancelledBeforeSend);
        assert!(requests.is_empty());
        let frame = receiver
            .recv()
            .await
            .expect("cancelled frame remains observable");
        assert!(!frame.lifecycle.mark_sent());
    }

    #[tokio::test]
    async fn sent_cancellation_keeps_late_response_registration() {
        let (transport, mut receiver, requests) = test_transport(16);
        let pending = transport
            .queue(Some(9), Bytes::from_static(b"sent"))
            .await
            .expect("frame queues");
        let lifecycle = pending.lifecycle();
        let frame = receiver.recv().await.expect("writer receives frame");
        assert!(frame.lifecycle.mark_sent());

        drop(pending);

        assert_eq!(lifecycle.phase(), RequestPhase::AbandonedAfterSend);
        assert!(requests.contains_key(&Some(9)));
        let (_, entry) = requests
            .remove(&Some(9))
            .expect("late response is recognized");
        entry.lifecycle.mark_acknowledged();
        let _ = entry.response.send(Ok(ok_status(9)));
        assert_eq!(lifecycle.phase(), RequestPhase::Acknowledged);
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_without_registration() {
        let (transport, _receiver, requests) = test_transport(4);

        let error = match transport.queue(Some(3), Bytes::from_static(b"12345")).await {
            Ok(_) => panic!("oversized frame must fail"),
            Err(error) => error,
        };

        assert!(matches!(error, Error::Limited(_)));
        assert!(requests.is_empty());
    }

    struct NoopHandler;

    impl Handler for NoopHandler {
        type Error = Error;
    }

    struct GateWriter {
        sent: mpsc::Sender<Bytes>,
        gate: Arc<Notify>,
    }

    impl OwnedSftpWriter for GateWriter {
        async fn write_owned(&mut self, data: Bytes) -> io::Result<()> {
            let sent = self.sent.clone();
            let gate = self.gate.clone();
            sent.send(data)
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "test receiver closed"))?;
            gate.notified().await;
            Ok(())
        }

        async fn shutdown(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn disconnect_marks_sent_and_queued_requests_differently() {
        let (client_reader, _server_writer): (DuplexStream, DuplexStream) = tokio::io::duplex(64);
        let (sent, mut sent_receiver) = mpsc::channel(2);
        let requests = Arc::new(SharedRequests::new());
        let transport = run_owned_session(
            client_reader,
            GateWriter {
                sent,
                gate: Arc::new(Notify::new()),
            },
            NoopHandler,
            requests,
            16,
        );
        let first_frame = Bytes::from_static(b"first");
        let first = transport
            .queue(Some(1), first_frame.clone())
            .await
            .expect("first frame queues");
        let second = transport
            .queue(Some(2), Bytes::from_static(b"second"))
            .await
            .expect("second frame queues");
        let first_lifecycle = first.lifecycle();
        let second_lifecycle = second.lifecycle();

        let sent_frame = sent_receiver
            .recv()
            .await
            .expect("first frame starts sending");
        assert_eq!(sent_frame.as_ref(), b"first");
        assert_eq!(sent_frame.as_ptr(), first_frame.as_ptr());
        transport.close();
        tokio::task::yield_now().await;

        assert_eq!(first_lifecycle.phase(), RequestPhase::DisconnectedAfterSend);
        assert_eq!(
            second_lifecycle.phase(),
            RequestPhase::DisconnectedBeforeSend
        );
        assert!(sent_receiver.try_recv().is_err());
    }
}
