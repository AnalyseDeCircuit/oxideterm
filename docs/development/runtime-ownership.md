# Runtime Ownership

Remote connection lifetime is independent from any one tab or pane. This rule prevents closing a terminal from silently disconnecting SFTP, a port forward, or another terminal that shares the same SSH node.

```text
Workspace runtime
  ├─ SshConnectionRegistry / NodeRouter
  │    └─ physical SSH node connection
  │         ├─ terminal consumers
  │         ├─ SFTP consumers
  │         └─ port-forward consumers
  └─ health and reconnect jobs
```

## Owners And Consumers

| Resource | Owner | Consumer rule |
| --- | --- | --- |
| Authenticated SSH transport | `SshConnectionRegistry` and `NodeRouter` | Consumers acquire a registered identity; they do not create unmanaged duplicate transports |
| Terminal pane | The tab/pane runtime | Closing it releases only its terminal consumer |
| SFTP surface or transfer | Its SFTP runtime | It acquires a node-backed SFTP consumer and may remain usable without a terminal pane |
| Port forward | The forwarding owner | It retains listener, bridge-task, and cancellation ownership until the rule or owning node stops |
| Health check and reconnect | Node/runtime owner | UI observes emitted node state; it does not infer liveness from terminal existence |
| Jump-host child node | Child node runtime plus recorded parent dependency | The parent transport remains retained while the child needs it |

## Lifecycle Walkthroughs

### Open A Saved SSH Connection

The saved connection describes intent; it is not the running transport. The workspace resolves it into a node identity, registers a `NodeRouter` consumer, and obtains the physical connection through `SshConnectionRegistry`. Connection Monitor and tabs then observe typed runtime events rather than polling a terminal pane.

### Open Another Terminal

A terminal tab registers its own terminal consumer against the node. By default it opens another SSH channel over the existing physical transport. A connection policy that explicitly requests isolation may use a separate registry key and physical transport. Closing either terminal releases only its consumer registration.

### Open SFTP Or A Forward

SFTP registers an SFTP consumer and opens a node-backed SFTP session. A forward registers its forwarding consumer and retains the listener plus bridge tasks under its forwarding owner. Neither operation should discover a connection by searching visible terminal tabs, and neither should end when an unrelated terminal closes.

### Reconnect Or Disconnect

Health checks and reconnect jobs belong to the node/runtime owner. A reconnect replaces or reattaches the node transport while preserving valid consumer identities where the runtime contract allows it. Explicit node disconnect is different: it is the cascade boundary that stops dependent consumers, child topology, and owned background work.

### Jump Hosts

Child-node topology must use recorded parent-child identities. A child that still needs a jump transport retains the parent dependency even when no terminal pane is visible. Matching host strings is not a valid substitute because multiple saved profiles can address the same host differently.

## Required Design Checks

- Give every long-lived task an explicit owner, cancellation path, and cleanup point.
- Register the correct `ConnectionConsumer`; never resolve a node by finding the first matching terminal pane.
- Preserve unrelated consumers when a pane, SFTP request, or single channel closes.
- Route connection state through registry or router events instead of terminal-derived state.
- Treat explicit node disconnect and application shutdown as cascade boundaries; remove dependent consumers and tasks there.

## Failure And Cleanup Rules

Connection setup can fail before a consumer is fully active. Roll back any consumer registration, saved-session projection, task handle, or dialog state created by the failed path. Do not leave an invisible consumer that prevents cleanup or makes the registry appear busy.

Do not turn authentication failures, host-key rejection, user cancellation, or a closed single channel into a global reconnect loop. The runtime owner decides whether an error is retryable using node state and error class; UI surfaces report that state without creating their own retry task.

Long-lived Tokio tasks, child processes, listeners, and helper bridges must have a retained owner and bounded shutdown path. A callback, temporary modal, or one-shot command future is never a sufficient owner.

## Review Questions

Before merging a change in this area, answer these questions in the review description or code comments at the transfer point:

1. Which object owns the physical transport or background task?
2. Which consumer identities are acquired, released, or replaced?
3. What survives terminal-pane closure, and why?
4. What cancels the work on explicit node disconnect and application shutdown?
5. Which runtime event updates the UI after the state change?

The workspace owns the primary registry and router in [`workspace.rs`](../../crates/oxideterm-gpui-app/src/workspace.rs). Node creation and terminal consumer registration begin in [`tabs/create.rs`](../../crates/oxideterm-gpui-app/src/workspace/tabs/create.rs); SFTP and forwarding have their own consumer paths in `workspace/sftp.rs` and `workspace/forwards.rs`.

Read [System Invariants](../SYSTEM_INVARIANTS.md#backend-sessions) before changing session behavior. Any change in this area must use the SSH/SFTP/forwarding row of the [verification matrix](verification.md).
