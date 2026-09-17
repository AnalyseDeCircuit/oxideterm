# OxideTerm patches for russh-sftp 3.0.0

This directory is a vendored fork, not a plain crates.io copy. The upstream
baseline is the published 3.0.0 archive, commit
`c2776c64c27e554dda0e0304925f890833fea5b1`, recorded in that archive's
`.cargo_vcs_info.json`. The previous baseline was 2.3.0,
`dcc0c06a2aa14da96fb453f05b530c8fffba1b1a`.

Compare with that exact archive or commit before rebasing. Do not use upstream
master or replace this directory with an unpatched registry dependency.

## Scope of the 3.0.0 rebase

The fork adopts upstream's single-buffer generic packet serialization, owned
string/byte deserialization, timeout deadline calculation, error conversions,
explicit `File::close()`, path expansion extension, and omitted default file
attributes. High-level `read` and `write` wait for CLOSE and report its failure.
Failed CLOSE leaves destructor cleanup enabled. Seek completion clears its
pending state even on failure, and empty stream reads/writes send no requests.
Packet payload sizing accounts for the complete frame and both local and server
limits.

The following upstream changes are deliberately not used:

- Ordinary `AsyncRead` remains single-request. OxideTerm uses the separate
  range-aware pipeline below for bulk transfers, rather than adding upstream's
  second prefetch state machine. There is no `Config::max_concurrent_reads`;
  bulk request and byte caps belong to each downloader.
- Ordinary writes retain eight concurrent requests and negotiated packet sizing,
  rather than upstream's new 16-request / preferred 32 KiB defaults.
  There is no `Config::max_write_packet_len`. Bulk sizing stays with each
  uploader and its adaptive window.
- Upstream's `Request` removal-on-drop and unbounded sender are replaced by the
  session-owned transport below. Late responses retain their real request
  ownership instead of being discarded as unknown recipients.
- Raw READ/WRITE frame encoders remain. The WRITE path reserves byte capacity
  before allocating the frame, so upstream's borrowed WRITE serializer is not
  used. Generic control packets do use the new single-buffer serializer.
- DATA remains backed by `Bytes`, not upstream's `Vec<u8>`.

These are intentional product differences, not promises of identical upstream
stream throughput. Do not cite upstream README benchmark numbers as OxideTerm
measurements.

## Bounded bulk transfers

`File::into_pipelined_downloader_for_range` owns its remote handle and delivers
`Bytes` chunks in contiguous file-offset order. The application supplies
64-request / 16 MiB caps and an optional known end offset, including resume
ranges. Effective read lengths also respect the negotiated packet/read limits.

Short reads enqueue only the missing tail. Completed out-of-order chunks and
unrelated outstanding reads are retained. Ordinary short reads do not shrink
the congestion window; their bytes and latency count as successful completions. Zero
data or EOF before a known end offset is an error, not successful truncation.
EOF stops further reads. Error, shutdown, and drop clean up outstanding work
and close the remote handle.

`File::into_pipelined_uploader` owns explicit write offsets, request/byte caps,
and ACK tracking. ACKs may arrive out of order. Shutdown drains acknowledgements,
fsyncs when advertised, and closes the handle. Drop closes the handle without
claiming that outstanding writes completed.

The per-file `SftpWindowTuner` uses completion latency and success/error feedback
to adjust request count, in-flight bytes, and chunk size. Caller caps and server
limits remain hard ceilings. Conservative startup and faster clean startup
growth are retained.

## Session-owned transport

`client::transport` owns the outbound queue, byte permits, request registry,
reader/writer tasks, and cancellation token for one SFTP session.

- `Config::max_outbound_inflight_bytes` defaults to 16 MiB and covers queued
  plus sent-but-unacknowledged frames, not just queue depth.
- WRITE reserves capacity before copying its payload into an encoded frame.
  Poll-based writers register a capacity waker instead of accumulating frames.
- A queued cancellation prevents sending. Once sending has started, cancellation
  retains the request and byte permit until a late response or disconnect.
- Requests distinguish queued, sent, acknowledged, cancelled-before-send,
  abandoned-after-send, and disconnected-before/after-send states.
- The awaited raw request path closes the SFTP session on timeout, because the
  remote outcome is unknown and its permit must not remain stranded.
- Either transport half terminating clears outstanding requests, releases
  capacity, and wakes waiters.
- Best-effort CLOSE detaches only after admission. If capacity is full, a retry
  remains tied to the live session instead of silently losing handle cleanup.

`OwnedSftpWriter` accepts an owned `Bytes` frame. OxideTerm's russh adapter
passes it to `ChannelStreamWriter::write_bytes`, preserving the allocation
across SSH channel fragmentation. `new_owned_with_config` initializes the same
extensions and limits as the ordinary stream constructor.

The public low-level `client::run` compatibility function is still upstream's
unbounded handler. `RawSftpSession` and `SftpSession` do not use it.

## Directory listing and capacity hints

`read_dir` appends server batches linearly and preserves server order, instead
of rebuilding the accumulated vector for every batch. If listing fails after
opening a directory, it attempts CLOSE before returning the original error.

`advertised_limits`, `negotiated_packet_len`, and
`advertised_open_handle_limit` expose read-only capacity hints. Application
directory scheduling uses the advertised handle cap.

## Diagnostics and dependency policy

Bulk diagnostic snapshots contain numeric counters and small enums only:
window targets, byte counts, latency, short reads, reordering, queue state,
and ACK/capacity waits. They contain no endpoint identity, path, payload, or
credential and are only formatted by explicitly enabled local diagnostics.

The manifest retains local benchmark dependencies and workspace lints.
Its russh development dependency uses the workspace's pinned fork, not upstream's
0.63.2 development dependency; this rebase does not upgrade SSH transport.
The WASM dependency versions remain at the existing compatible requirements.

## Files carrying retained patches

- `src/client/transport.rs`: session ownership, capacity, cancellation and cleanup.
- `src/client/mod.rs`: owned writer contract and outbound byte budget.
- `src/client/rawsession.rs`: owned constructors, raw encoders, admission,
  timeout termination and retained CLOSE.
- `src/client/fs/file.rs`: bulk transfer state machines, stream-policy
  differences, adaptive windows, short-read repair and diagnostics.
- `src/client/fs/mod.rs`: bulk API exports.
- `src/client/session.rs`: owned initialization, capacity hints and directory cleanup.
- `src/protocol/data.rs`, `src/protocol/mod.rs`: Bytes-backed DATA encoding/decoding.

## Verification and later upgrades

Run the focused lifecycle, packet encoding, short-read/resume, default-attribute,
path-extension and explicit-close tests before measuring performance:

```sh
cargo +1.94.1 fmt -p russh-sftp -- --check
cargo +1.94.1 test -p russh-sftp
cargo +1.94.1 test -p oxideterm-sftp -p oxideterm-ssh
cargo +1.94.1 check -p oxideterm-gpui-app
git diff --check
```

The russh fork is now a Git dependency, not a workspace member. Run
`cargo +1.94.1 test --manifest-path /path/to/pinned-russh/Cargo.toml
channel_tx_write_bytes_preserves_owned_slices` from a checkout of the pinned
revision when verifying its owned-write contract.

For a controlled rebase, run the same local OpenSSH subsystem benchmark before
and after. It covers both ordinary stream I/O and the application's dedicated
bulk interfaces; bulk download checks offsets, content and the final boundary.

```sh
cargo +1.94.1 bench -p russh-sftp --bench local_transfer_benchmark -- --save-baseline before-upgrade
# Apply the rebase, then compare under the same host/load conditions.
cargo +1.94.1 bench -p russh-sftp --bench local_transfer_benchmark -- --baseline before-upgrade
```

Local pipe throughput does not establish SSH throughput, high-RTT behavior,
or process memory stability. Keep the cancellation, byte-capacity and short-read
regressions, and separately exercise remote resume, cancellation, disconnect,
large directories and multi-file transfers when a controlled SSH test endpoint
is available.

On later upgrades, remove a local patch only when upstream behavior and tests
cover the same contract. Review semantic changes even when a three-way merge
has no textual conflicts, and update this baseline and its deliberate differences.
