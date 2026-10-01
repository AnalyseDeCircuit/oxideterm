# Cloud Sync v3 implementation

## Accepted design

- Keep the `.oxide` extension. Version 2 containers authenticate the header and
  encrypt metadata together with the payload. Released version 1 archives remain
  readable. Archive restores produce local changes, never restore replica identity.
- Use Automerge for causal merging while domain stores continue to own application
  data. Persist the replica and the last values actually applied to domain stores.
- Merge independent changes automatically. Retain concurrent candidates and keep
  the current local value until the user resolves a conflict. Excluding a resource
  from sync does not delete it remotely.
- Encrypt all remote data. Credentials remain opt-in and use encrypted objects;
  the CRDT carries credential version references rather than plaintext secrets.
- Publish immutable full snapshots under `sync-v3/<device>/<sequence>-<digest>.oxide`.
  Persist upload bytes and reserve sequence numbers before network writes. Discover
  and merge each device's latest snapshot instead of overwriting a shared pointer.
- Preserve all eight backends. HTTP JSON gains pagination and deletion contracts;
  an older server must report that an upgrade is required rather than overwrite v2.
- Prepare encrypted recovery data before coordinated domain writes. Commit domain
  data, effective baseline and replica together through a recoverable operation.
  Recover interrupted writes before accepting further changes to affected stores.
- Upgrade old cloud snapshots into the new namespace without deleting them. New
  clients do not continue writing the old protocol. Password changes initialize a
  new namespace and switch only after successful verification.
- GUI, CLI and MCP share the coordinator. UI work must first read existing project
  implementations and reuse their controls, focus/IME handling and overlay lifecycle.

## Delivery order

1. Authenticated container, CRDT model, encrypted local replica and conflict model.
2. Immutable publication, provider discovery, coordinated apply and crash recovery.
3. Legacy upgrade, all resource mappings, GUI/CLI/MCP integration and translations.

## Verification

Extend existing format, service and backend-request tests at their real boundaries.
Check independent edits, concurrent candidates, selected-resource semantics, retries
and interrupted application using explicit expected data. Use temporary stores and
existing request fixtures; no separate benchmark or multi-device test deployment.

## Progress

- Project UI source-reading requirement recorded in `AGENTS.md`.
- Added the version 2 authenticated container for archives, sync snapshots and
  local replicas. Purpose, header, salt and nonce are authenticated. Archive
  metadata is encrypted with its payload; the released version 1 reader remains.
- Added Automerge replicas with field groups, causal candidates, explicit conflict
  resolution, selection-aware local change capture and record deletion conflicts.
  CRDT operations contain content references; field bytes have zeroizing ownership.
- Added encrypted local replica persistence through the existing native/portable
  secret provider, an operation-scoped process lock, and durable pending uploads.
  Retries reuse the exact reserved sequence, path and ciphertext.
- Added encrypted full snapshot encoding and validation of digest, writer and
  sequence.
- Mapped saved SSH and local terminals, serial, Telnet, Mosh, standalone SFTP/FTP,
  remote desktops, forwarding rules, Quick Commands/collections, TOTP metadata,
  application settings and plugin settings into causal field groups. Display
  metadata is independent of connection configuration; settings merge by leaf.
  Device credential references and usage timestamps stay local. Environment
  variables retain their separate opt-in, including when another device sends them.
- Added resolved-result application through the owning stores, without legacy
  display-name matching or timestamp conflict arbitration. Profile credentials use
  one register per target for both replacement and explicit clearing. Their scope
  follows the selected owner; TOTP and collection dependencies are included.
- Added discovery, publication and deletion for WebDAV, S3, Dropbox, OneDrive,
  Google Drive, Gist, Git and HTTP JSON. Flat providers encode the namespace into
  their object names. Publication writes one immutable object and verifies its
  bytes before acknowledgement, including when the write response was lost.
- Added a shared prepare/apply/publish coordinator. It re-reads live domain values
  after network preparation so edits made during download remain concurrent.
  Observed writer versions skip repeat downloads; unchanged documents skip uploads.
  The local encrypted record binds its sync password with a keyed verifier; changing
  the password requires a fresh namespace before pending bytes can be published.
  Own snapshots retain the newest five and everything acknowledged within 24 hours.
  Cleanup never targets another writer or removes causal history from a snapshot.
- Added an encrypted durable recovery journal covering connection data, forwarding,
  settings, Quick Commands, plugins and the local replica. New credential slots are
  recorded before creation; old slots are removed after commit. Desktop startup
  and the new CLI entry scan every target for unfinished journals before use.
- Added `cloud-sync sync --yes [--json]` as a v3 entry point. The CLI holds the
  desktop instance locks throughout recovery and synchronization. Its default
  dry run prepares a remote merge summary without uploading or applying domain data.
  The common CLI write guard blocks other configuration writes while recovery is
  pending; read-only and portable-unlock operations remain available.

### Entry points and secret integration

The desktop remote actions and MCP preview/apply tools now use the shared causal
coordinator. Existing local-file import/export still uses the archive workflow.
The desktop reuses the existing preview cards, list rows and action buttons.
Conflict candidates are selected explicitly; credential values are hidden, and
unresolved conflicts retain local values. The strings are present in all 11 locales.

CLI `sync`, `push`, `pull` and remote `apply --strategy merge` use the coordinator.
A dry run prepares a real remote preview without publishing. Unsupported legacy
replace/rename/skip strategies return an error. `resolve --strategy local-wins`
chooses the current local candidates; individual remote choices use the desktop.
The separate CLI preview/diff commands inspect cached state.

Legacy structured cloud objects and version 1 archives are converted into a causal
snapshot. Old remote objects remain untouched. Conversion retains excluded sections
in encrypted history while applying only the selected resources. The conversion
itself does not write domain files or protected secrets.

Managed SSH keys and passphrases, primary/hop passwords, proxy credentials,
privilege credentials, AI keys and plugin keys now have owner adapters. New managed
and privilege slots are journaled before creation; old slots are retained until
commit. AI/plugin rollback restores both the values and the protected account
inventory, including explicit deletion markers. Keys are never returned to MCP.

AI providers are discovered through settings. Previously stored plugin keys become
discoverable when their plugin reads, checks, writes or deletes them; native secret
storage does not expose an inventory of arbitrary historical plugin account names.

The first Gist sync creates its private destination only after confirmation and
persists the ID before uploading configuration. Retries and the saved Gist ID reuse
the same replica and pending publication. State/history updates are shared by the
desktop and CLI; MCP uses the desktop owner. The synchronized baseline is captured
at local application, so edits made during upload remain dirty.

Startup and CLI recover pending journals before constructing consumers. Recovery
during a running desktop operation uses the live forwarding registry and refreshes
runtime consumers. MCP previews hold bounded, expiring plans; applying them checks
the current settings and local configuration. Undo handles are limited to scopes
whose checkpoint can restore the affected data.

No live cloud account was modified during verification. Provider checks use request
fixtures; desktop integration has been compiled without launching another GUI.

### HTTP JSON object contract

The existing authenticated object endpoints are reused:

- `GET /v1/namespaces/{namespace}/objects?prefix=sync-v3/&cursor={cursor}` returns
  `{"objects":[{"path":"sync-v3/<writer>/<sequence>-<digest>.oxide"}],"nextCursor":null}`.
  Omit `cursor` on the first request. A nonempty `nextCursor` requests another page.
  Return an empty `objects` array for a new namespace; a 404, 405 or 501 on this
  endpoint means the server needs the new listing API.
- `GET /v1/namespaces/{namespace}/objects/{path}` returns exact object bytes, or 404
  when absent. `PUT` at that path stores the bytes. The client verifies them with
  a subsequent `GET` before acknowledging publication.
- `DELETE /v1/namespaces/{namespace}/objects/{path}` removes a superseded object;
  successful status codes and 404 are idempotent success.

Listings must include every matching object across pages. Cursors are opaque and
must advance. The client rejects malformed publication identities and repeated
cursors instead of treating a partial listing as complete.

The official `oxideterm.cloud-sync-server` also needs these endpoints. Its earlier
HTTP JSON implementation only provided individual object reads and writes, so it
returned 404 for v3 discovery. The companion server change adds bounded prefix
pagination and atomic object/ETag deletion to its existing redb tables, under the
existing read/write token permissions. Object-only namespaces appear in the admin
panel and participate in its namespace lifecycle. Existing storage and encryption
keys are retained; deploy a server build containing this change before using v3.
The client maps the upgrade-required error to localized copy in all 11 catalogs.

### Archive and password-change completion

- Embedded SSH certificate/private-key pairs migrate into managed authentication.
  The private key uses the existing protected store and staged recovery; the public
  certificate stays with its managed-key metadata. SSH authenticates with that
  certificate directly from memory. Import matches both the fingerprint and the
  certificate before reusing an existing key, and preserves a certificate key's
  passphrase. An archive containing only half of an embedded pair reports an
  incomplete archive before application.
- Manual archive export, import, preview, CLI and plugin entry points use the new
  container. File selection exposes metadata only after successful decryption.
  Archives contain application data, never replica identity or publication history.
- Changing an established cloud password uses the existing settings field and
  confirmation dialog. The coordinator writes to a fresh namespace, verifies the
  uploaded bytes, then activates the new settings. A new protected password
  reference makes activation a single configuration write; failed upload or
  activation keeps the previous destination usable. Old snapshots and passwords
  remain available. Other devices join the new namespace with its new password.
- CLI offers `cloud-sync change-password --yes` with the new password on stdin.
  Existing secret set/import commands remain setup operations; established spaces
  use password change. Local-file mode keeps its own archive password. Selecting
  another destination clears cached remote status without overwriting the
  previous destination's password.

The remaining release verification is exercising the preview, conflict-choice,
password-change and recovery screens in the existing desktop session, including
native AI/plugin keychain access. No additional GUI was launched for these edits.

### Checks performed

- Official-server discovery reproduced a 404 before the companion fix. The router
  regression now passes with actual encrypted redb storage, prefix pagination,
  namespace/read-write authorization, idempotent cleanup and soft-delete refusal.
  All 33 server tests, clippy with warnings denied, formatting and diff checks
  passed. The client error-mapping test and 11-catalog locale audit also passed.
  The remote deployment has not been updated or exercised by these checks.
- The container test covers prefix/content tampering, wrong keys, purpose mismatch,
  truncated input and trailing bytes.
- Replica scenarios cover independent edits, duplicate delivery, preserved conflict
  candidates, stale resolution, deselection and concurrent delete/edit.
- Local storage and publication are exercised through real encrypted files: reopen,
  directory relocation, process locking, preserved pending bytes, identity validation
  and missing-key refusal.
- Passed four focused replica scenarios, the authenticated-container scenario and
  the existing legacy-archive checksum scenario. Both affected crates passed
  `cargo check --all-targets --locked`; `cargo fmt --check` and `git diff --check`
  passed. No performance benchmark or GUI run was performed for these backend changes.
- The cloud-sync library suite passed all 91 scenarios after the initial integration.
  Added request-level checks for paginated discovery, unsupported HTTP servers,
  WebDAV directory traversal and upload verification after a failed write response.
  The coordinator scenario verifies independent remote/live-local edits against
  persisted domain values. Recovery tests reopen encrypted journals after dropping
  the operation, checking exact rollback bytes and retained committed values.
- Desktop and CLI builds passed `cargo check -p oxideterm-cli -p oxideterm-gpui-app
  --locked`; the CLI's separate build also checks the forwarding implementation
  with runtime features disabled.
- After the final changes, all six v3 model/mapping/storage/recovery tests passed,
  including credential selection and password-change refusal. The live-owner apply
  scenario also passed with an incoming environment variable excluded by local
  scope. Final desktop/CLI checks, formatting and `git diff --check` passed.
- After switching the entry points and adding the owner adapters, the cloud-sync
  library suite passed all 94 tests. The 12 existing credential-sync tests and the
  focused SSH primary/hop restore and managed-key staging tests passed. The legacy
  archive fixture checks explicit password, privilege and forward values without
  modifying source stores. The coordinator scenario also verifies that edits made
  during upload remain dirty. Gist bootstrap reopening preserves the writer and
  reserved publication bytes when its ID is saved into configuration.
- `cargo check -p oxideterm-cli -p oxideterm-gpui-app --locked` passed. The locale
  audit passed for all 11 catalogs with no missing keys or placeholder mismatches.
  Native AI/plugin keychain recovery still needs validation in the running app;
  automated checks did not write to the user's keychain or cloud accounts.
- Archive regression checks passed for encrypted metadata, ciphertext tampering
  and a separately constructed version 1 wire fixture. Certificate migration passed
  with an encrypted private key, retained passphrase and an existing ordinary key
  with the same fingerprint. The existing archive suite passed all 29 scenarios.
- The cloud-sync suite passed all 95 scenarios. A local HTTP fixture exercises
  password rotation with exact and corrupted readback, checks the new password
  reference and decoded configuration, and verifies that the original writer,
  configuration and password are retained. A failed rotation retains its target
  and pending ciphertext for a retry with the same new password. Desktop and CLI
  checks include all targets.
