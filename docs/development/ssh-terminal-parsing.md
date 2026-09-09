# SSH Terminal Background Parsing

Status: proposed; the SSH parser still runs in the UI drain. Local output scanning and local render snapshot changes do not move SSH parsing to another thread.

## Problem and boundary

`SshPtySession::drain_transport_output_with_budget` feeds output through protocol consumers, decoding, prompt/trigger detection, shell integration and VT parsing on the UI thread. It yields between 4 KiB slices under a 2 ms drain budget. The transport already batches output and retains a 1 MiB byte permit until each `SshOutputChunk` is consumed. Moving only `Processor::advance` would leave substantial work on the UI thread and split protocol state across two owners.

Use one terminal-owned parser worker. Keep the registry / NodeRouter as the owner of the physical SSH connection. Do not add another SSH connection or a daemon for this optimization.

## State ownership

| Owner | State |
| --- | --- |
| Registry / NodeRouter | Physical connection, reconnect, other SFTP/forwarding consumers and jump-host ancestry |
| `SshPtySession` | Terminal consumer lease, startup/lifecycle state, worker control sender and completion handle |
| Parser worker | Ordered output receiver, unconsumed chunk and permit, VT processor, decoding state, modem/trzsz consumers, Shell integration, prompt and trigger streams, tmux parser/controller |
| Shared terminal state | Grid and the graphics/selection metadata needed to take a coherent render snapshot |
| UI | Current immutable snapshot, menus, prompt presentation, recording destination and user interaction |

Protocol ingress belongs to the worker. Graphics events must update the shared render state in stream order; the UI must not apply a newer placement to an older grid. Preserve the existing protocol and recording privacy boundaries: modem/trzsz consume raw bytes before display transforms, private OSC is removed before recording, and raw terminal data is not duplicated into a generic UI event queue.

## Ordering and control

The worker is the sole sequencer for parser-affecting commands: resize, encoding, output processor, trigger rules, recording enablement, clear-buffer and tmux operations.

When accepting a command, capture a finite boundary in the already-published transport output queue, drain that prefix, then apply the command before accepting later output into the parser. Capturing a prefix is essential: “drain until empty” can starve commands under continuous output. Add the narrow queue-boundary API to `SshOutputReceiver`; do not infer a boundary from elapsed time or release byte permits at receipt.

- Resize changes the local grid and submits the remote PTY resize at that boundary. Bytes already in the network or remote PTY can still reflect the old size; SSH provides no byte-to-resize marker. Preserve this existing ambiguity rather than promising exact remote synchronization.
- Encoding changes finish/reset the previous decoder at the boundary according to the existing encoding contract. They must not reinterpret a pending chunk midway through its old encoding.
- Transfer input and terminal-generated replies go through the same ordered transport writer. Preserve reply-before-following-output side effects where required by the protocol.
- Ordinary input goes through the worker when modem/trzsz/tmux owns its interpretation. Close/cancellation has an independent signal that can interrupt waits even when the output queue is full.
- Preserve deferred PTY startup and post-connect input: the first layout resize and successful shell start remain prerequisites where currently required.

Only adjacent redundant wakeups may be collapsed. Do not collapse protocol replies, ordered metadata, recording chunks or failure/exit events.

## Backpressure and scheduling

Retain the current transport byte permit while a chunk is queued or partially parsed; return it only after consumption or cancellation. Do not enlarge the queue to make short `cat` runs appear faster. Include retained decoder/protocol data in the existing per-protocol limits.

Use a dedicated parser thread for CPU work; do not run long parse loops on a Tokio executor worker. Feed it via an activity signal that wakes for output, controls, cancellation and protocol timers. Avoid periodic idle polling. Keep bounded parse turns so a continuous producer cannot starve controls.

UI notifications carry coalesced activity plus ordered semantic events. Non-coalescible payloads need a byte-bounded queue. If publication is blocked, cancellation must still complete and unrelated SSH consumers must remain usable.

Render snapshots use the local terminal's deferred-snapshot contract: try while the parser is busy, preserve pending damage, then request a fair snapshot after the UI's bounded deferral interval. Capture grid, selection and terminal mode together. Tmux's multi-grid compositor remains a separate locking path until explicitly adapted.

## Cancellation and cleanup

Create and retain the worker only for the terminal session. On close, mark the session closed, signal cancellation, close its output receiver to release blocked senders, cancel transfer/timer work, discard unconsumed chunks with their permits, and wait for worker completion before releasing its terminal consumer lease. Never disconnect the shared physical node as part of parser cleanup.

A join must not block the UI. Reuse an application-owned runtime/terminal-task owner for the completion task, retaining that task until it finishes. The current optional runtime stored in `SshPtySession` must not be dropped while it still owns transport or cleanup work; runtime injection and teardown are part of this change, not follow-up work. Make every wait cancellation-aware; a deadline reports failed cleanup rather than silently detaching the worker or terminating a Rust thread.

A late startup result or a retired worker's event must be rejected using the existing session/connection identity. Worker errors become a terminal failure event, not a silent switch back to UI parsing. Temporary secret-bearing owned buffers retain the existing zeroization and redaction rules.

## Implementation sequence and acceptance

1. Separate parser-owned state inside `oxideterm-terminal`, preserving the current synchronous behavior and protocol tests. Do not create a new crate merely to relocate a large file.
2. Add the finite output-boundary and cancellation interfaces, then run the same parser state on the worker. Wire lifetime and cleanup in the same change.
3. Connect coherent snapshots and ordered UI events; measure sustained SSH output and interaction during output.

Required checks:

- trzsz and lrzsz (ZMODEM, XMODEM and YMODEM) remain ahead of display transforms, encoding, prompt scanning and graphics ingress. Exercise every handshake split, binary bytes, cancellation and the trailing shell prompt with both consumers enabled. A trzsz version, ID or port ending at a packet boundary is incomplete until the handshake line terminator arrives.
- The same transcript and control sequence produce the same final grid, modes, selection, graphics placement, recording bytes and protocol replies across packet splits.
- Resize/encoding/clear/recording commands during continuous output cross their captured boundary exactly once; controls are not starved.
- A stalled parser or UI cannot exceed the output/event budgets; close wakes blocked senders and releases permits without waiting for another network packet.
- Close during startup, active output, transfer and reconnect produces one terminal lifecycle transition and no stale events. Another registered SFTP or forwarding consumer remains operational.
- The final screen appears after a busy snapshot is deferred and output stops; continuous output does not indefinitely starve snapshots.
- Measure producer completion, parser completion and presentation separately, plus input latency during output, CPU and peak memory. Use workloads larger than queue capacity and compare identical configurations on the same machine. SSH worker implementation is not complete until these checks pass.
