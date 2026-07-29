# Terminal Command Sender Implementation Plan

## Goal

Replace the legacy compact command input with one command sender that has a
hidden layout when unused, a compact layout for immediate use, and an expanded
layout for scheduled, repeatable, multi-target input. All layouts retain the
same document and running job. The sender must remain responsive, must not
create SSH connections, and must not move worker polling back into
`WorkspaceApp::render`.

## User-facing structure

The terminal surface keeps four ordered layers:

1. The terminal pane tree remains the flexible content owner.
2. An optional QuickBar shows saved quick commands in a horizontally scrollable
   row.
3. The terminal toolbar remains the sender's control header.
4. The command sender occupies one slot below the toolbar. Compact mode uses
   the former single-row command-input density; expanding replaces that row
   with the vertically resizable advanced editor in the same slot.

The sender supports hidden, compact, and expanded layouts. Hiding or compacting
it preserves its active document, settings, and running job.
Running jobs continue until they finish, the user stops them, every target
disappears, the workspace locks, or the workspace is released. Compact mode
hides line numbers and advanced status controls, uses plain Enter as its send
gesture, and retains Shift/Alt+Enter for multiline drafting. The toolbar
continues to expose running-job state and the expanded view exposes progress
and cancellation.

The old textarea command input is retired rather than stacked below the sender.
Selection insertion, file-path insertion, Git commit drafting, and quick-command
editing all target the active sender document.

## Sender documents

Each sender document owns:

- A dedicated `TextEditorView` whose inline presentation hides editor chrome
  in compact mode and whose document presentation restores line numbers and
  scrolling in expanded mode.
- Text or hexadecimal input mode.
- Line or character/byte pacing.
- A bounded interval and repeat count.
- Current, all, or explicitly selected terminal targets.
- Idle, running, completed, stopped, or failed status.
- Accepted, skipped, and total dispatch-unit counts.

Multiple documents may run concurrently only when their target sets do not
overlap. This prevents two schedules from interleaving bytes into one terminal.

## Ownership

`TerminalCommandSenderEntity` owns sender documents, panel geometry, target
selection, task handles, generations, and progress. A job uses one GPUI task and
one timer loop regardless of target count.

At start time, `WorkspaceApp` resolves the requested scope into immutable
`PaneId` plus `WeakEntity<TerminalPane>` targets. The sender never looks up a
host, node, connection, active tab, or broadcast state after the job starts.
It writes only through existing terminal pane input APIs, so the connection
registry remains the physical SSH owner.

Dropping or stopping a job drops its GPUI task and invalidates its generation.
Dropping the entity cancels every remaining timer. Weak pane references do not
extend terminal-consumer or node lifetime.

## Input semantics

Text line mode:

- Normalizes CRLF and CR to logical newlines.
- Preserves interior empty lines.
- Treats one trailing newline as the terminator of the final line instead of
  synthesizing another empty command.
- Sends each line with a terminal carriage return.
- Lets each target terminal apply its own configured character encoding.

Text character mode:

- Splits text by Unicode grapheme cluster.
- Converts logical newlines to carriage returns.
- Sends one grapheme per dispatch unit without splitting emoji or combining
  sequences.

Hexadecimal line mode:

- Parses each logical line as one raw byte block.
- Accepts whitespace and common byte separators.
- Accepts optional `0x` prefixes and contiguous even-length hex strings.
- Does not apply text encoding or append a line ending.

Hexadecimal character mode sends one parsed byte per dispatch unit.

Progress means that a payload was accepted by the local terminal input path. It
does not claim that the remote program executed or acknowledged the command.
The processed-unit counter advances with the immutable schedule, while accepted
and skipped counters show how many target deliveries succeeded or were ignored.

Scheduled sender input deliberately does not create command marks, auto-suggest
history, AI facts, or terminal recordings. A timer-driven payload does not prove
that a shell prompt is ready, and raw hexadecimal bytes cannot be represented
faithfully in text-only ledgers.

## Secret lifetime

Editor buffers are transient UI drafts and are never persisted, logged, sent to
AI, telemetry, or diagnostics. Starting a job builds an immutable execution
plan whose text and byte buffers zeroize on drop. Errors and status snapshots
contain only structural information, never command contents.

## Target selection

The sender reuses the broadcast target menu's terminal discovery and row
presentation, but keeps selection state per sender document:

- Current freezes the active pane.
- All freezes every currently open terminal pane.
- Selected freezes the document's explicitly checked panes.

Changing tabs, opening panes, or changing the ordinary command-bar broadcast
selection cannot widen a running job. A closed target becomes skipped. When all
targets disappear or reject a dispatch, the job stops without reconnecting or
replacing them.

## QuickBar

QuickBar reuses the existing quick-command store:

- Existing categories are groups.
- Existing vector order is button order.
- Existing host-pattern filtering uses the full terminal command context.
- Clicking a button uses the existing risk confirmation, toast, command mark,
  focus handoff, and broadcast execution path. Commands that need confirmation
  remain inside the Quick Commands popover instead of opening another command
  input.
- The existing popover remains the only editor and management surface.

QuickBar has a separate disabled-by-default setting. It does not add a second
store, command pin field, sorting schema, or background task.

## Delivery stages

### Stage 1: Core scheduling

- Add pure text/hex frame planning with focused tests.
- Add terminal-pane sender input APIs that gate readiness immediately before
  writing and keep scheduled input out of prompt-dependent ledgers.
- Add `TerminalCommandSenderEntity` with task cancellation and progress.

### Stage 2: Panel and targets

- Add the resizable editor panel and sender controls.
- Add current, all, and selected target scopes.
- Freeze weak pane targets at start and prevent overlapping jobs.

### Stage 3: Multiple senders and hexadecimal mode

- Add document tabs, create/remove actions, independent editors, text/hex mode,
  pacing selection, status, and progress.
- Keep jobs running when another document becomes active or the panel hides.

### Stage 4: QuickBar

- Add the optional horizontal quick-command row.
- Reuse existing command execution and management.
- Keep narrow layouts horizontally scrollable without compressing the sender or
  terminal action buttons.

## Verification

Pure tests must cover:

- CRLF, CR, LF, empty lines, and trailing empty lines.
- Unicode grapheme clusters.
- Hex separators, prefixes, contiguous input, odd nibbles, and invalid digits.
- Repeat and total-unit overflow.
- Redacted debug output and zeroizing execution buffers.

Entity and integration tests must cover:

- Immediate first dispatch and interval-controlled later dispatch.
- Stop, restart generation isolation, removal, and entity release.
- Target snapshot stability, closed-pane skips, and overlapping-job rejection.
- Hidden panels continuing active jobs.
- Text encoding and raw-byte preservation at the terminal pane boundary.
- QuickBar legacy-setting defaults and host-pattern filtering.

Final validation includes focused crate tests, formatting checks, the GPUI app
type check, and an adversarial ownership/security review.
