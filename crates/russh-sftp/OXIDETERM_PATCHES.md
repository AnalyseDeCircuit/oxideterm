# OxideTerm patches for russh-sftp 2.1.2

This vendored copy is based on `russh-sftp` 2.1.2.

## Raw queued sequential downloads

OxideTerm downloads large files through SFTP. The upstream `File` type keeps the
standard `AsyncRead` implementation strictly sequential: one read request is
issued and awaited before the next request starts. That behavior is compatible
with stream semantics, but it makes high-latency downloads throughput-bound by
round-trip time.

This fork adds `RawSftpSession::read_nowait`, mirroring the existing
`write_nowait`, and a dedicated `File::into_pipelined_downloader_for_range`
path for bulk sequential downloads. It intentionally does not change the normal
`AsyncRead` implementation. The downloader owns the remote file handle, keeps a
bounded number of raw read requests in flight, buffers out-of-order responses,
and emits chunks only at the next contiguous file offset.

OxideTerm uses this path for normal and resumed downloads in
`crates/oxideterm-sftp`. The effective request length still comes from the
server `limits@openssh.com` read limit or the configured packet cap, whichever
is smaller. The current bulk download window is capped at 64 requests and 8 MiB
of in-flight data.

Correctness notes:

- SFTP servers may return fewer bytes than requested. When that happens, the
  downloader discards the speculative window and restarts from the actual next
  offset so callers never skip a gap.
- EOF marks the downloader as finished so repeated `next_chunk` calls do not
  issue extra read requests.
- Dropping or shutting down the downloader discards pending speculative reads
  and closes the remote handle.
- Scheduling failures discard already queued reads before returning the error,
  so callers do not consume stale responses after the session has failed.

## Raw queued sequential uploads

OxideTerm uploads large files through a dedicated
`File::into_pipelined_uploader` path. Upstream `AsyncWrite` already queues
`write_nowait` requests, but the high-level stream interface only exposes
stream-style writes and session-wide packet/concurrency knobs. Changing those
knobs to optimize downloads also changes upload packet sizing, which can
regress throughput through SSH channel-window or server-side write backpressure.

This fork keeps ordinary `AsyncWrite` unchanged and adds an upload-owned writer
that:

- writes explicit SFTP offsets from a caller-provided start offset, including
  resumed uploads;
- limits both request count and in-flight bytes;
- accepts write acknowledgements in any response order;
- drains all acknowledgements, fsyncs when supported, and closes the handle on
  `shutdown`;
- closes the handle on drop without pretending already queued writes completed.

OxideTerm currently uses this path for normal and resumed uploads with a
64-request / 8 MiB in-flight window. The effective write size still honors the
server `limits@openssh.com` write limit or the configured packet cap.
