# OxideTerm patches for russh-sftp 2.1.2

This vendored copy is based on `russh-sftp` 2.1.2.

## Raw queued sequential downloads

OxideTerm downloads large files through SFTP. The upstream `File` type keeps the
standard `AsyncRead` implementation strictly sequential: one read request is
issued and awaited before the next request starts. That behavior is compatible
with stream semantics, but it makes high-latency downloads throughput-bound by
round-trip time.

This fork adds `RawSftpSession::read_nowait_raw`, mirroring the existing
`write_nowait_raw`, and a dedicated `File::into_pipelined_downloader_for_range`
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
- `SSH_FXP_DATA` payloads are decoded into `bytes::Bytes` instead of `Vec<u8>`.
  This lets bulk downloads forward the packet body without copying it into a
  separate vector first.
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

This fork keeps ordinary `AsyncWrite` semantics unchanged and adds an
upload-owned writer that:

- writes explicit SFTP offsets from a caller-provided start offset, including
  resumed uploads;
- limits both request count and in-flight bytes;
- accepts write acknowledgements in any response order;
- drains all acknowledgements, fsyncs when supported, and closes the handle on
  `shutdown`;
- closes the handle on drop without pretending already queued writes completed.

OxideTerm currently uses this path for normal and resumed uploads with a
64-request / 8 MiB in-flight cap. The effective write size still honors the
server `limits@openssh.com` write limit or the configured packet cap.

## Raw WRITE packet encoding

The upstream `write_nowait` path accepts owned `Vec<u8>` data and routes it
through the generic `Packet::Write` serializer. For bulk upload chunks this
costs extra copies: the caller first copies the buffer slice into a `Vec`, the
serializer copies that data into a payload buffer, and the packet wrapper then
copies the payload into the final outgoing frame.

This fork adds `RawSftpSession::write_nowait_raw(handle, offset, data)`, which
builds the SFTP v3 `SSH_FXP_WRITE` frame directly into the final `BytesMut`.
The raw path still uses the same request IDs, server packet/write limits,
request map, acknowledgement handling, and outgoing channel as the generic
path. A unit test verifies that raw WRITE packet bytes match the generic packet
encoding exactly.

Both `File::into_pipelined_uploader` and ordinary `AsyncWrite` now use the raw
encoder for write requests. This changes allocation behavior only; write order,
offset handling, acknowledgement draining, fsync, and close semantics are
unchanged.

## Raw READ packet encoding

The pipelined downloader schedules many small `SSH_FXP_READ` requests. Routing
each request through the generic `Packet::Read` serializer clones the remote
handle and allocates an intermediate packet payload for every queued read.

This fork adds `RawSftpSession::read_nowait_raw(handle, offset, len)`, which
builds the SFTP v3 `SSH_FXP_READ` frame directly into the final `BytesMut` from
a borrowed handle. The raw path keeps the same request IDs, server read/packet
limits, request map, response handling, and timeout behavior as the generic
path. A unit test verifies that raw READ packet bytes match the generic packet
encoding exactly.

## Bytes-backed DATA packets

The upstream DATA packet model stores `Data.data` as `Vec<u8>`, so decoding a
download response copies the packet payload even though the underlying incoming
frame is already a `bytes::Bytes` buffer. This fork stores DATA payloads as
`Bytes` and decodes `SSH_FXP_DATA` manually with `copy_to_bytes`.

The generic serializer/deserializer path is still used for small control
packets. DATA encode/decode has focused unit coverage to verify that packet
length, request id, and payload bytes round-trip correctly.

## Adaptive bulk transfer windows

The 64-request / 8 MiB bulk values are now hard caps rather than the active
window at every moment. `PipelinedFileDownloader` and
`PipelinedFileUploader` own a small `SftpWindowTuner` that records request
send time, DATA/ACK completion latency, completed bytes, and congestion events.

The tuner uses conservative additive-increase / multiplicative-decrease rules:

- successful DATA/ACK intervals grow target request count, in-flight bytes, and
  chunk length up to the server-limited caps;
- short reads, protocol errors, status errors, and closed response channels
  shrink the target request count, in-flight bytes, and chunk length;
- server `limits@openssh.com` and the caller-provided caps remain hard upper
  bounds.

This keeps UI-level SFTP settings stable while letting each single-file
transfer adapt to RTT, ACK pace, server limits, and short-read/error feedback.

## Local-only window diagnostics

The pipelined downloader and uploader expose in-process diagnostic snapshots for
OxideTerm's local SFTP performance debugging. These snapshots contain only
numeric counters and small enums such as target request count, in-flight bytes,
chunk length, RTT summaries, shrink reason, short-read counts, queue state, and
ACK/capacity-wait counters.

The snapshot types intentionally do not derive serde serialization or broad
debug formatting. They do not include hostnames, usernames, node identifiers,
paths, filenames, server banners, raw error messages, or secret-bearing data.
OxideTerm may format them to the local stderr stream only when an explicit local
diagnostic environment variable is enabled.
