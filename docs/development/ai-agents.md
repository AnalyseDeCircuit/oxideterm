# OxideSens Agent Collaboration

OxideSens delegates bounded work from a parent conversation to independent child runs. Open **Settings → OxideSens → Tools** and enable **Allow subagents** before starting a task. Enablement, default model, and concurrency are application-wide settings; the switch is off by default. ACP sessions are not child-agent backends.

## Execution And Authority

`oxideterm-ai/src/agent` owns group, agent and run identities, mailboxes, shared budgets, runnable permits and resource leases. `workspace/ai_state/agents.rs` retains workspace-owned model tasks and UI records; its `AiAgentServices` GPUI global shares the child concurrency limit across workspaces. Both parent and child use the same `sidebar/ai/actions/tools/loop.rs` and the existing tool broker and approval policy.

```text
Parent conversation → group → parent run
                         ├─ child run → model / tool / waiting state
                         └─ child run → model / tool / waiting state
All child runs → application-wide runnable quota
All native runs → resource execution coordinator → existing runtime owners
```

Only parents can delegate, message or stop children. A child can report progress or ask its parent a question. It cannot spawn another child or message siblings. Completion is the model's normal final response, not a separate completion tool. Completed children may be followed up within their active group: the context is reused, but a new run ID isolates late callbacks and approvals from the new execution.

Delegation requires current runtime handles. Include each terminal, SFTP or IDE handle the child needs; a node inspection handle does not confer authority over every consumer of that node. Child discovery and execution are restricted by the same registry scope. Children receive fresh handles, not the parent's transferable authority. Global settings, credential operations, schedulers and opaque MCP targets remain parent operations because they do not expose host-enforced delegated target scopes.

When the global model preference is set to inherit, the child takes a snapshot of the parent configuration. Settings can select another configured default model; a queued run's selection remains editable until it first acquires execution capacity. Requeueing after approval or a resource wait does not unlock the model. Unavailable models or credentials fail that run without substituting another provider.

## Coordination And Budgets

The application permits one to four runnable children, defaulting to two. Approval, parent-reply and resource waits release the runnable permit. They do not release ownership of an unfinished terminal command. Waits use runtime events, not repeated model requests.

The group shares the existing tool-round budget. Delegation, user supplementation and child follow-ups do not replenish it. At exhaustion the parent stops unfinished children and can produce one tools-disabled summary. Provider usage is accumulated per request and run; missing provider counts remain unknown, including in group totals. Prompt-size estimates are not billed usage.

Messages have receipt and consumption states. Progress updates refresh the task row without waking the parent model. Questions and final results wake a waiting parent and remain available when several children finish together. Child results are evidence, not user authorization.

## Terminal Control And Cancellation

Commands targeting the same interactive terminal serialize through resource leases. A tool timeout or stable screen is not proof that the remote command ended. The lease is released only after the command-fact boundary closes and the response has finished reading its output, or through an explicit takeover/disconnect boundary. The owning run can send further interactive input without releasing its command lease.

User input takes priority and invalidates the affected lease, including broadcast destinations. Queued work from before takeover cannot resume silently. Unresolved execution exposes an explicit return-control action; stopping an agent does not claim to undo mutations or terminate an unknown remote command.

Closing the AI panel, opening child details and switching conversations do not cancel work. Stop affects the selected conversation's group; individual stop affects one child. Conversation deletion and workspace shutdown cancel their runs and revoke tool sessions, without disconnecting shared SSH nodes or unrelated SFTP and forwarding consumers.

## Follow-up Messages

The AI composer separates queued follow-ups, steering and interruption. Sending while a response is active queues the message for the next turn. The queue appears above the existing composer and supports editing, deletion, moving an item up and sending it immediately. Editing returns a message to an empty draft without replacing an existing draft. Queue entries retain the selected model and prepared context; switching conversations does not redirect delivery.

Steering uses the current native run's mailbox and leaves its model and authority unchanged. If the run can no longer accept input, the composer or queue retains the message. Interrupt-and-send cancels the current turn and submits the replacement before other queued messages; remote commands still follow the unresolved-resource rules above. Plain Stop and failures retain pending messages for explicit continuation. Normal completion dispatches one queued turn at a time, including in background conversations.

Pending messages and per-conversation drafts are workspace-owned memory, not persisted chat entries. Closing the workspace discards them and does not replay pending commands after restart. Asynchronous credential lookup, retrieval and pre-send compaction check their originating launch identity before starting a stream.

## History And Presentation

The parent reply contains a collapsed task group with compact status rows and individual or group stop actions. Child details reuse the same AI panel and retain the parent's draft and scroll owner. Replies reuse the structured Markdown and tool renderers; task context, communication and history are expandable. Child code blocks do not offer terminal insertion because the currently selected terminal may belong to a different task. Tool approvals retain their originating conversation and run; notifications identify background work without navigating away from the current page.

The existing chat database stores child summaries separately from detailed records. Children are not top-level conversations. Stored records omit runtime authority and internal reasoning; unfinished runs reopen as interrupted and their pending approvals cannot execute. Deleting a parent conversation removes its child records, and delayed writes cannot recreate them. Group completion releases provider configuration and runnable contexts.

## Focused Checks

```sh
cargo test -p oxideterm-ai --lib
cargo test -p oxideterm-gpui-app ai_state::entity_tests
cargo test -p oxideterm-gpui-app ai_runtime_context::entity::tests
cargo check -p oxideterm-gpui-app
python3 scripts/quality/audit_i18n.py
```

For interaction changes, exercise a parent with one completed child, one approval waiter and one child question. Supplement the parent, reject an approval, stop one child and switch conversations while another runs. Check narrow-panel layout, keyboard focus, return navigation and interrupted history on each supported desktop platform.

For streaming-performance changes, compare the same terminal output workload with agents disabled, with two visible streaming children and with child details hidden. Record frame timings and main-thread work; a runtime-only mailbox benchmark cannot establish terminal rendering performance. Hidden child details must not continuously run Markdown layout.
