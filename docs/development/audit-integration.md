# Integrating Audit And Session Recording

The audit crate records operations at their execution boundary. The desktop's Notifications & Audit page reads those records; opening or closing the page does not own collection. User instructions are in [audit and session recordings](../user-guide/en/desktop.md#audit-and-session-recordings).

## Owners And Entry Points

| Concern | Source |
| --- | --- |
| Service startup, retained client, page tasks, and shutdown ownership | [workspace/audit.rs](../../crates/oxideterm-gpui-app/src/workspace/audit.rs) |
| Context identity, operation start/result, and incomplete-operation handling | [operation.rs](../../crates/oxideterm-audit/src/operation.rs) |
| Request provenance across synchronous and async execution | [scope.rs](../../crates/oxideterm-audit/src/scope.rs) |
| Categories, sources, outcomes, evidence, and retention policy | [model.rs](../../crates/oxideterm-audit/src/model.rs) |
| Writer, reader, recording sink, and health reporting | [service.rs](../../crates/oxideterm-audit/src/service.rs) |
| Encrypted storage, recording files, and protected keys | [store.rs](../../crates/oxideterm-audit/src/store.rs) and [key.rs](../../crates/oxideterm-audit/src/key.rs) |

The application retains `AuditService` and `AuditRegistration`; feature tasks borrow or clone the client/context. Reuse that service rather than starting a writer for each page or command. The CLI also owns a service for its process lifetime.

## Add An Operation At The Execution Boundary

Follow [SFTP file operations](../../crates/oxideterm-sftp/src/session/file_ops.rs) for a concrete example. `delete_recursive` starts an operation, relates child work to its operation ID, counts completed removals, and reports `Partial` if a later failure follows successful deletions.

1. Use the actual runtime owner's `AuditContext`. It supplies the session, consumer, transport, node, endpoint, and account relevant to the work.
2. Carry the request's source and parent operation through `for_request` or `with_request`. This preserves whether the request came from a user, AI, MCP, plugin, CLI, or broadcast while retaining the identity of the resource that executes it.
3. Begin the operation before its side effect with `AuditOperation::in_request`, `in_context`, or the owner's `operation` method. Prefer an existing action identifier for the same semantic operation.
4. Record authorization where a decision actually occurs. Use a parent operation for a broker request and child operations for the concrete work when their results describe different boundaries.
5. Complete the operation with the strongest evidence available. `result` handles a completed `Result`; `changed` distinguishes a real change from `Unchanged`; `finish` accepts explicit outcome, evidence, exit code, and byte count.
6. If adding an action identifier, add its display label under `event_log.actions` in every `eventLog.json` catalog. Keep raw identifiers stable for stored records.

`Sent` with dispatch evidence establishes that input was dispatched. It does not establish that a remote command succeeded. Use a protocol response, exit code, or shell-integration event when available. Dropping an unfinished `AuditOperation` records an unknown result; cancellation code should report the boundary it actually reached rather than inventing success or confirmed remote termination.

## Preserve Context Across Tasks

`AuditContext::scope` follows a future while it is polled. A newly spawned task does not automatically inherit that scope: capture the context and scope the spawned work explicitly. `scope_optional` supports work with no current audit context. `with_sync_request` covers a synchronous dispatch only until its handler returns.

The request context supplies provenance, while the resource context supplies the target. In a broadcast or cross-host copy, retain each destination's session and transport rather than copying the source terminal's identity into every record. `scope.rs` includes a test of this distinction.

## Output Recording And Sensitive Data

Operation collection and output recording have separate policy switches, both disabled initially. Recording additionally requires audit to be enabled. Reuse `RecordingSink` and the owning terminal's output path; do not capture content from the current pane during rendering.

For the SSH path, follow [ssh_parser.rs](../../crates/oxideterm-terminal/src/session/ssh_parser.rs) and [recording_output.rs](../../crates/oxideterm-terminal/src/recording_output.rs). Protocol consumers process raw bytes before display output reaches recording. Private control payloads are filtered, and recording pressure participates in the existing wake/flush path. Preserve close, interruption, and pending-output behavior when changing that integration.

Credentials must never be passed as audit detail. Operational text such as a path or command must use the existing protected, redacted context/detail APIs; avoid formatting whole request objects. Recording captures displayed output, which can itself contain sensitive text, so keep the explicit consent, encrypted local storage, and exclusion from cloud sync. Exports cross into plaintext and use the separate confirmation and redaction path in [export.rs](../../crates/oxideterm-audit/src/export.rs).

## Focused Verification

Extend coverage at the boundary whose evidence changed:

- Check action, target, source, parent relationship, outcome, and evidence for an actual operation.
- Use a partial-success fixture when earlier side effects can survive a later failure.
- For task changes, verify that request provenance survives executor changes and targets remain distinct.
- For recording changes, preserve policy-disable, accepted-output, file-failure, and completion behavior covered by [service tests](../../crates/oxideterm-audit/src/service/tests.rs).

Use synthetic identities and temporary storage with the existing test key provider. Run `cargo test -p oxideterm-audit` plus the affected producer's focused tests. If changing the desktop controls, also check `oxideterm-gpui-app`, audit locale keys, and manually inspect filtering, export confirmation, and playback.
