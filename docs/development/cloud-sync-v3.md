# Cloud Sync v3

The desktop, CLI, and MCP use the same prepare/apply/publish coordinator. Domain stores remain the owners of application data; Automerge records causal changes and conflict candidates. User workflows are in [Cloud Sync and Backups](../user-guide/en/cloud-sync-and-backups.md).

## Data and Merge Ownership

Persist the encrypted local replica together with the effective baseline last applied to domain stores. Capture local changes against that baseline. Independent edits merge automatically; concurrent candidates remain available and the current local value stays in use until explicit resolution. Excluding a resource from sync does not delete its remote record.

Mappings cover saved connections, local terminals, serial, Telnet, Mosh, standalone SFTP/FTP, remote desktops, forwarding rules, Quick Commands and collections, TOTP metadata, application settings, and plugin settings. Display metadata is independent of connection configuration; settings merge by leaf. Device credential references and usage timestamps stay local. Environment variables and credentials have separate opt-in scopes, including on receipt from another device.

Credentials use encrypted objects and causal version references, never plaintext values in the CRDT. Owner adapters handle managed SSH keys, passphrases, primary and hop passwords, proxy and privilege credentials, AI keys, and plugin keys. Plugin secret accounts become discoverable through their owner's read/check/write/delete operations; native storage is not an inventory of arbitrary historical plugin accounts.

For a new mapping, update capture, application, recovery, and scope handling together using [Settings, Data, and Migrations](settings-data-and-migrations.md#include-business-data-in-cloud-sync).

## Immutable Publication

Snapshots use `sync-v3/<device>/<sequence>-<digest>.oxide`. Reserve the sequence and persist the encrypted upload bytes before writing to the network. Retrying uses the same path and ciphertext, including when the original response was lost. Verify the remote bytes with a read before acknowledging publication.

Discover each writer's latest snapshot instead of overwriting a shared pointer. Validate writer, sequence, digest, and encrypted content before merging. Observed versions avoid repeated downloads; unchanged documents avoid uploads. Cleanup affects only the current writer, retaining its newest five snapshots and everything acknowledged within 24 hours. A full snapshot retains causal history even when older remote objects are removed.

WebDAV, S3, Dropbox, OneDrive, Google Drive, Gist, Git, and HTTP JSON use this contract. Flat providers encode the namespace in object names. First-time Gist creation requires confirmation and persists the private destination ID before upload; retries reuse that identity and pending publication.

## Coordinated Apply and Recovery

1. Prepare the remote merge without mutating domain stores.
2. Re-read live local values so edits made during download remain concurrent.
3. Write an encrypted recovery journal before coordinated domain mutations.
4. Apply through the owning stores and commit domain data, effective baseline, and replica.
5. Publish the reserved immutable snapshot and verify it.

Record new protected credential slots before creating them; delete obsolete slots only after commit. Rollback restores values and protected-account inventories, including explicit deletion. Startup and CLI entry points recover unfinished journals before constructing consumers. Recovery during a running desktop operation uses the live forwarding registry and refreshes runtime consumers.

The baseline is captured at local application, so edits made during upload remain dirty. MCP previews are bounded, expiring plans: apply checks current settings and configuration rather than trusting an old preview. Undo is available only where the checkpoint can restore the affected scope.

## Entry Points

CLI `cloud-sync sync`, `push`, `pull`, and remote `apply --strategy merge` use the coordinator. Without `--yes`, synchronization prepares a remote preview without applying domain data or publishing. Legacy replace/rename/skip strategies are rejected. `resolve --strategy local-wins` selects local candidates; choose individual remote candidates in the desktop. Separate preview/diff commands inspect cached state.

The CLI holds the desktop-instance locks through recovery and synchronization. Pending recovery blocks other configuration writes; read-only and portable-unlock operations remain available. MCP uses the desktop owner and does not expose secret values.

## HTTP JSON Object Contract

- `GET /v1/namespaces/{namespace}/objects?prefix=sync-v3/&cursor={cursor}` returns `{"objects":[{"path":"sync-v3/<writer>/<sequence>-<digest>.oxide"}],"nextCursor":null}`. Omit `cursor` initially and follow every nonempty `nextCursor`.
- A new namespace returns an empty `objects` array. A 404, 405, or 501 on listing means the server needs the listing API; it does not mean an empty sync space.
- `GET /v1/namespaces/{namespace}/objects/{path}` returns exact bytes or 404. `PUT` stores the object; the client reads it back before acknowledging publication.
- `DELETE` at the same path removes a superseded object. Successful responses and 404 are idempotent success.

Listings must cover all matching objects. Cursors are opaque and must advance. Malformed identities and repeated cursors are errors, not partial success. Deploy a compatible `oxideterm.cloud-sync-server` with listing and deletion support before using v3; individual-object read/write endpoints alone are insufficient.

## Archives, Upgrade, and Password Changes

The `.oxide` version 2 container authenticates its header, purpose, salt, and nonce, and encrypts metadata with the payload. Released version 1 archives remain readable. Archive restore creates local changes; it never restores replica identity or publication history. Legacy cloud objects can be converted into causal snapshots without deleting the old objects, while only selected resources are applied locally.

Embedded SSH certificate/private-key pairs are restored through managed authentication and staged secret recovery. Reuse requires both matching fingerprint and certificate; passphrases are preserved. An incomplete pair is rejected before application.

Changing an established cloud password creates a fresh namespace, publishes and verifies its snapshot, then activates the new configuration. Failed upload or activation leaves the previous destination usable. Old snapshots and credentials remain available; other devices join the new namespace with the new password. CLI `cloud-sync change-password --yes` reads the new password from stdin. Local-file archives keep their own archive password.

## Verification Boundaries

Extend existing format, coordinator, provider-request, and recovery tests for the changed contract. Important cases include independent edits, concurrent delete/edit, excluded resources, interrupted application, identical publication retries, readback corruption, and failed password activation. Use explicit expected records and encrypted temporary stores; provider fixtures must exercise pagination and authorization errors. Native keychain and live-provider behavior require their corresponding environment and must be reported separately from fixture results.
