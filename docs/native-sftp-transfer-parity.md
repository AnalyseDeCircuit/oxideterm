# Native SFTP Transfer Parity Map

This map is the required source-of-truth checklist before changing native SFTP transfer behavior. Tauri command/store behavior remains the specification.

## Sources Read

- `tauri版本代码/src-tauri/src/commands/sftp.rs`
- `tauri版本代码/src-tauri/src/commands/node_sftp.rs`
- `tauri版本代码/src-tauri/src/sftp/transfer.rs`
- `tauri版本代码/src/components/sftp/SFTPView.tsx`
- `tauri版本代码/src/lib/api.ts`
- `crates/oxideterm-sftp/src/transfer_manager.rs`
- `crates/oxideterm-sftp/src/session.rs`
- `crates/oxideterm-gpui-app/src/workspace/sftp/actions/transfers.rs`
- `crates/oxideterm-gpui-app/src/workspace/sftp/runtime.rs`

## Command And Owner Mapping

| Tauri entry | Tauri owner/state | Native mapping | Current parity |
| --- | --- | --- | --- |
| `sftp_pause_transfer(transferId)` | `TransferManager.controls[transferId].pause()` | `SftpTransferManager::pause` via queue UI | Mostly aligned for running-process pause; no standalone command/API surface. |
| `sftp_resume_transfer(transferId)` | `TransferManager.controls[transferId].resume()` | `SftpTransferManager::resume` via queue UI | Mostly aligned for running-process resume; no standalone command/API surface. |
| `sftp_cancel_transfer(transferId)` | `TransferManager.controls[transferId].cancel()` | `SftpTransferManager::cancel` via queue UI | Present; still needs runtime behavior checks for directory/tar. |
| `sftp_transfer_stats()` | active/queued/completed stats from manager | `SftpTransferManager::transfer_stats` | Core manager API present; GPUI does not yet expose a command-equivalent consumer surface. |
| `sftp_update_settings(...)` | updates max concurrent, speed limit, directory parallelism | settings store pushes into `SftpTransferManager::apply_settings` | Functionally present, but no independent command-equivalent surface. |
| `node_sftp_start_directory_transfer(...)` | registers retained `BackgroundTransferSnapshot`, spawns background task | GPUI starts background directory/tar in SFTP tab action flow | Capability present, but API granularity is UI-coupled. |
| `node_sftp_list_background_transfers(nodeId?)` | retained in-memory snapshots, finished kept 5 minutes | `SftpTransferManager::list_background_transfers` hydrated into SFTP view | Present. |
| `node_sftp_resume_transfer(nodeId, transferId)` | loads `ProgressStore`, reacquires transfer SFTP, resumes by stored strategy | GPUI incomplete-row resume now honors stored file/directory/tar strategy | Aligned for strategy selection; still needs remote runtime verification. |
| directory background state | `Pending -> Active -> Completed/Cancelled/Error`, with `strategy`, `itemCount`, `backendSpeed` | Native has same snapshot fields in `oxideterm-sftp` | Present, but resume semantics and runtime verification incomplete. |

## Confirmed Gaps To Patch Before Claiming One-To-One

- Do not convert stale `Active` progress records unless Tauri adds that behavior; both stores currently index only `Paused | Failed`.
- Directory recursive and tar resume must honor the persisted strategy. Tauri restarts the selected directory strategy rather than doing per-file persisted resume.
- Tar resume must not re-run auto strategy selection; the saved `DirectoryTar` or `DirectoryRecursive` value is the source of truth.
- GPUI should expose transfer stats/control as a reusable manager API, not only as queue button handlers.
- `conflict_action = ask` has a native pending-transfer owner and decision replay path; remaining work is Tauri visual parity plus multi-file/folder runtime verification.
