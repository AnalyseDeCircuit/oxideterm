# Native Port Forwarding Parity Map

This map pins native forwarding work to the Tauri command/event/store contract. Do not add forwarding behavior outside this map without first updating the Tauri-to-native row.

## Sources Read

- `tauri版本代码/src-tauri/src/commands/forwarding.rs`
- `tauri版本代码/src-tauri/src/commands/node_forwarding.rs`
- `tauri版本代码/src/lib/api.ts`
- `tauri版本代码/src/lib/runtimeEventHub.ts`
- `tauri版本代码/src/hooks/useForwardEvents.ts`
- `tauri版本代码/src/components/forwards/ForwardsView.tsx`
- `crates/oxideterm-forwarding/src/manager.rs`
- `crates/oxideterm-forwarding/src/registry.rs`
- `crates/oxideterm-gpui-app/src/workspace/forwards/actions.rs`

## Command And State Mapping

| Tauri entry/event | Tauri owner/state | Native mapping | Current parity |
| --- | --- | --- | --- |
| `node_create_forward(local/remote/dynamic)` | node-resolved session manager, persists node-owned rule | `ForwardingRegistry`/manager via GPUI actions | Core capability present; continue to verify dynamic SOCKS5 does not gain extra behavior. |
| `node_stop_forward` | stops listener but retains rule as stopped for restart/edit | native manager `stop_forward` | Present. |
| `node_delete_forward` | deletes runtime rule and persisted saved rule | native delete action/store sync | Present. |
| `node_restart_forward` | restarts stopped rule; failed restart keeps stopped rule | native restart action/manager | Core present; failure retention needs runtime check. |
| `node_update_forward` | manager `update_forward` path | native currently biased toward stopped-rule update path | Gap: edit/update semantics must match Tauri manager exactly. |
| `node_stop_all_forwards` / `stop_all_forwards` | group stop for node/session | GPUI forwards toolbar calls manager `stop_all` | Entry present; runtime behavior still needs end-to-end check. |
| `pause_port_forwards` | suspend all and save rules for reconnect | GPUI forwards toolbar calls `suspend_all_and_save_rules` | Entry present; reconnect automation still needs parity verification. |
| `restore_port_forwards` | restore saved forwards after reconnect | GPUI forwards toolbar reacquires node connection and calls `restore_saved_forwards` | Entry present; reconnect automation still needs parity verification. |
| `list_saved_forwards` / `list_all_saved_forwards` | saved-forward persistence | native store has equivalent load APIs | Gap: saved-forwards management page/import/export/auto-start UI missing. |
| `export_saved_forwards_snapshot` / `apply_saved_forwards_snapshot` | plugin sync snapshot with tombstones | native registry has storage pieces | Gap: no GPUI/user-facing sync command surface. |
| `set_forward_auto_start` | persisted rule auto-start flag | native store supports persisted rules | Gap: no complete UI entry. |
| `forward-event:statusChanged` | event hub updates visible status | native `ForwardEvent` channel | Present but must verify payload/status names one-to-one. |
| `forward-event:statsUpdated` | traffic counters update UI | native manager stats/event path | Needs UI verification. |
| `forward-event:sessionSuspended` | session suspension toast/state | native `ForwardEvent` channel and toast text | Present but needs reconnect flow verification. |

## Type And Status Contract

- Forward types stay exactly `local`, `remote`, `dynamic`.
- Dynamic means Tauri SOCKS behavior only; do not add UDP, auth, or extra SOCKS modes unless Tauri has them.
- Status flow remains `Starting`, `Active`, `Stopped`, `Error/Failed`, `Suspended`.
- `stop` retains rules; only `delete` removes persisted/runtime ownership.
- `update` must follow Tauri `update_forward`; do not invent a separate user-visible edit model.
- Health-check failures must preserve Tauri troubleshooting text and skip-check semantics.
