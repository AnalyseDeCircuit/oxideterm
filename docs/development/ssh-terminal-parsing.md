# SSH Terminal Background Parsing

Status: implemented. Production SSH terminals use a dedicated parser thread; the UI drains events and takes coherent snapshots. `SshPtyCore` remains available internally for parser tests and the synchronous benchmark comparison.

## Scope and ownership

Previously, the UI drove SSH decoding, protocol processing and VT parsing in 4 KiB slices under a 2 ms drain budget. Moving only VT parsing would have split stream state and left protocol work on the UI thread. The worker now owns the entire ingress pipeline.

| Owner | State |
| --- | --- |
| Registry / NodeRouter | Physical SSH connection, reconnect, SFTP/forwarding consumers and jump-host ancestry |
| `SshPtySession` facade | Control submission, cancellation signal and UI activity receiver |
| Worker and its `SshPtyCore` | Startup task, terminal consumer lease, ordered output receiver, retained chunks/permits, lifecycle and runtime reference |
| `SshParser` inside the core | VT processor, encoding state, modem/trzsz consumers, shell integration, prompts, triggers, graphics and tmux controller |
| Shared core lock | Grid, selection, graphics and mode needed for a coherent snapshot |
| UI | Immutable snapshot, menus, transfer prompts, recording destination and user interaction |
| Process completion-task owner | Retained join tasks and runtime references until parser threads finish |

Application terminals receive an `Arc<Runtime>` from the existing application runtime owner. Standalone callers use a process-owned runtime so closing a terminal cannot shut down another registry consumer. The worker never owns the physical connection exclusively and does not disconnect it during terminal cleanup.

## Stream and control ordering

The worker is the sole sequencer for parser-affecting controls, including resize, encoding, output transforms, trigger rules, recording, clearing and tmux operations. The facade captures the transport publication sequence when accepting a control. The worker drains that finite prefix, then applies the control before parsing later chunks. Continuous output cannot extend an already-captured boundary indefinitely.

Transport chunks retain their byte permits until fully parsed or discarded. The consumed sequence advances only after the entire chunk, including all 4 KiB slices, has been consumed. Controls accepted before startup are applied before processing startup output. Deferred PTY startup and post-connect input retain their layout and shell-start prerequisites.

Resize updates the local grid and submits the remote PTY resize at the boundary. SSH has no byte-to-resize marker: output already in flight may still reflect the old remote size. Encoding changes finish the old decoder before switching. Terminal replies, user input and transfer/tmux writes share the ordered transport writer; a full writer retains pending commands and stops further ingress until it becomes writable.

Transport batching now preserves raw bytes without holding UTF-8-looking tails. Modem and trzsz consumers see those bytes before any display transform. Only the subsequent text path retains incomplete UTF-8. This prevents a binary CRC ending in bytes such as `e4 bd` from being held while the sender waits for an acknowledgment. Private OSC content is still removed from recording, and terminal output events are generated only when requested by the recording consumer.

Manual modem startup completes asynchronously through the existing transfer prompt event. Tmux pane selection also reports completion; the UI replays the pending mouse down, moves and release in order against the selected pane. It does not optimistically send the first click to the old pane.

## Backpressure, wakeups and snapshots

- Transport output keeps its existing 1 MiB byte budget. Receiving a chunk does not release that budget.
- Controls allow 256 queued requests and 1 MiB of ordinary payload. A single oversized input is accepted when no payload is queued, preserving large-paste behavior; further input receives an explicit queue-full error.
- Ordered UI events use a 1 MiB high-water mark, including owned payload capacity. Parsing stops before another turn when full; one bounded parse turn can cross the mark. Only adjacent redundant wakeups are coalesced.
- Trzsz input pauses SSH consumption at twice `MAX_TRANSFER_CHUNK_SIZE` (currently 2 MiB), with at most a parse slice of overhang. Reading or cancelling the transfer wakes the worker. Closed transfers reject and zeroize later writer output.

CPU work runs on one dedicated OS thread per terminal. The existing activity channel wakes it for output, controls, transfer progress and cancellation. The thread enters Tokio only while idle; pending protocol deadlines use timed waits, and a retained writable-notification task wakes a blocked writer. There is no periodic idle polling.

UI drains use separate status/event/report locks and perform no parsing. Snapshot acquisition tries the core lock while deferral is allowed, then uses the fair lock after the existing bounded deferral interval. Grid, mode, selection and graphics are captured together. Tmux retains its multi-grid compositor path. A final output wake remains sufficient to render the last screen after a deferred snapshot.

## Cancellation and failure

Close has an independent atomic signal and wake path. Receiver cancellation wakes both byte-capacity and message-slot waiters, including when a partially parsed chunk still owns its permit. Dropping the receiver also closes it.

Cleanup closes the output receiver, stops transfers, clears owned queues and retires the parser writer. It releases the core lock before aborting/awaiting startup or submitting the channel close. Late startup results are discarded. The terminal lease remains held through this cleanup; a channel-close send has a five-second deadline and logs a timeout. Other registry consumers remain attached.

The UI never joins an OS thread. A retained completion task waits for the parser completion signal before scheduling the join, avoiding a second idle OS thread for each parser. Finished tasks are reaped without a UI-thread runtime shutdown wait. A parser failure produces one localized processing-failure event; it does not fall back to foreground parsing. Session replacement keeps the existing distinct session identities. Owned queued input and protocol-write buffers use the existing zeroization rules.

## Validation

The regression suites cover:

- Real loopback SSH parsing and protocol replies without UI drains; deferred startup, post-connect input, startup cancellation and replacement isolation.
- API-time control boundaries, continuous-output progress, oversized input delivery, byte/message/event backpressure and cancellation while producers or transfer readers are blocked.
- Packet-split transcripts with explicit expected text, Unicode/combining characters, alternate screen, modes, selection, Kitty placement/pixels, private-OSC recording filtering and terminal replies.
- Existing trzsz and ZMODEM/XMODEM/YMODEM protocol suites, plus worker transfer handoffs, writer wakeups and binary tails without a subsequent packet.
- A surviving real port-forward consumer after terminal close, with both injected and standalone runtimes.
- GPUI integration from real SSH through layout and headless painting, final-frame recovery, and exact tmux selection/click/release output.

On 2026-09-11, the application all-target check passed; the terminal, SSH, modem, triggers and trzsz library suites passed 424 tests; GPUI terminal passed 199 tests. The manual performance test is intentionally excluded from normal test runs and was executed separately. Locale key/file/placeholder checks passed for all 11 languages; the audit also reports pre-existing untranslated-copy warnings.

These are macOS loopback and headless GPUI checks. They do not establish Windows hardware behavior, real WAN performance, or physical display latency.

## Performance comparison

The manual release benchmark uses the same parser core in synchronous and background modes, through a real local russh server/client. It uses an 80×24 terminal, 1,000 scrollback lines, default graphics, trzsz detection and approximately 16 MiB per workload, exceeding queue capacity. Each workload has one warm-up followed by three measured runs.

The synchronous baseline uses adaptive normal/interactive/throughput budgets, a 2 ms parse deadline and the existing 1 ms boosted drain cadence when exhausted. Otherwise drains are activity-driven. Snapshot requests target 120 Hz independently of drain cadence. A fixed 120 Hz drain baseline would exaggerate the gain and is not used.

Producer completion, parser completion and final snapshot readiness are recorded separately. Producer completion means data has been submitted to the russh server API, not consumed by the client. UI callback P95 includes event draining, input probes and any snapshot taken during that callback; it is not the whole application's frame time. Input probes are timestamped when the server receives them. CPU and peak RSS cover the entire benchmark process, including its server and fixture buffers; RSS is a process high-water mark, not per-terminal allocation.

Reproduce sequentially with no other builds running:

```sh
OXIDETERM_SSH_BENCH_MODE=sync cargo +1.94.1 test --release -p oxideterm-terminal ssh_background_performance --lib -- --ignored --nocapture
OXIDETERM_SSH_BENCH_MODE=worker cargo +1.94.1 test --release -p oxideterm-terminal ssh_background_performance --lib -- --ignored --nocapture
```

Results and raw samples are recorded in [the benchmark report](benchmarks/ssh-background-2026-09-11.md).
