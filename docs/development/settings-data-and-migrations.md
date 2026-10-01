# Settings, Data, And Migrations

Persisted data is an interface with users, backups, cloud sync, CLI tooling, and older released builds. Treat a format change as a product change, not as a private refactor.

## Data Boundaries

| Data | Owning layer | Rules |
| --- | --- | --- |
| Settings and safe profile metadata | `oxideterm-settings` and `oxideterm-settings-model` | Validate, normalize, and write atomically through the owning store |
| Connection records and topology | `oxideterm-connections`, `oxideterm-topology` | Keep stable identifiers and safe connection metadata separate from live transport state |
| Credentials and managed keys | Secret store and portable runtime | Do not place raw values in ordinary settings JSON |
| Cloud-sync payloads and backups | `oxideterm-cloud-sync`, portable/export paths | Preview and conflict handling are part of the contract |
| Runtime-only state | Workspace entities and registries | Do not persist it merely to simplify a view restore |

The active data directory is shown in Settings. Its exact location may differ for normal, custom, and portable runtime modes, so application code should use the settings/path APIs rather than assembling an operating-system path by hand.

## Changing A Persisted Field

1. Identify whether the field exists in a released settings file, export, cloud-sync snapshot, CLI JSON response, or plugin-facing payload.
2. Define the new invariant and the behavior for missing, malformed, or older values.
3. Update the owning model, normalization or migration path, writer, UI, and all affected import/export paths together.
4. Preserve secrets as references or encrypted-store values; do not add a plaintext compatibility field.
5. Add focused coverage for the migration or normalization boundary, then validate the relevant CLI or app surface.

Unreleased development fields are not compatibility contracts. Adjust or remove them directly instead of accumulating migration branches. Released fields and documented external formats require an explicit migration or compatibility decision.

## Cloud Sync, Backups, And `.oxide`

Cloud sync and backup recovery can apply data created on another machine or another product version. Before changing these paths, identify:

- what is included by default and what needs an explicit opt-in;
- whether a conflict is previewed before applying;
- which strategy is used for skip, rename, replace, or merge;
- whether an operation can overwrite local data and therefore needs a backup or confirmation;
- whether managed keys or portable secrets cross an encrypted boundary.

Do not use cloud-sync timestamps or a visible terminal as a substitute for a durable record identity. Review the corresponding [cloud sync and backup guide](../user-guide/en/cloud-sync-and-backups.md) and [portable bundle guide](../user-guide/en/portable-oxide.md) when the change is user-visible.

## Migration Validation

Use temporary settings paths and synthetic fixture data. Test at least the current format, an older valid shape when supported, and malformed input that must fail safely. Then run the relevant settings or cloud-sync checks and inspect the persisted result without exposing secret material.

Avoid destructive local experiments against a real profile. A migration test must prove its result through the owning store, not by mutating a user's settings file in place.

## Include Business Data In Cloud Sync

The v3 coordinator uses domain stores as the owners of effective local values. [ConfigurationView](../../crates/oxideterm-cloud-sync/src/sync_v3/mapping.rs) converts portable records into fields; [SyncReplica](../../crates/oxideterm-cloud-sync/src/sync_v3/replica.rs) tracks concurrent changes. See [cloud-sync-v3.md](cloud-sync-v3.md) for the implementation record.

For a new field on an existing record, first inspect how that record is already mapped. Add a resource kind only when the data has its own stable identity and lifecycle.

1. Define a stable resource ID and the fields that should merge together. Display metadata and connection configuration already have separate groups; application settings merge by leaf. Keep local credential references, usage timestamps, and runtime identities out of portable values.
2. Extend collection and reconstruction in `ConfigurationView`. Its `originals` retain local-only reconstruction fields; only `values` enter synchronization. A reconstructed record must pass through its domain store's validation and write API.
3. Define selection in `SyncScope`, `ConfigurationView::selected`, and the applicable item filter. Include real dependencies, such as a referenced credential or command category, within the user's selected scope. Excluding a resource must not manufacture a remote deletion.
4. Keep sensitive material behind the existing sensitive-sync selection and secret-provider paths. Local terminal environment variables have their own opt-in. Extend the corresponding credential mapping when needed; a raw value must not become ordinary settings metadata or a conflict label.
5. Add reconstruction/application through the shared [synchronization coordinator](../../crates/oxideterm-cloud-sync/src/operation/synchronize.rs), and add the owning store's checkpoint and recovery behavior to [RecoveryJournal](../../crates/oxideterm-cloud-sync/src/sync_v3/recovery.rs) if this introduces another write boundary.
6. Update the existing scope controls, preview labels, and all locale catalogs for any new visible category. Desktop, CLI, and MCP should continue to invoke the same coordinator rather than maintaining separate merge implementations.

### Preserve Prepare, Apply, And Publish

`prepare_sync` collects local data and discovers remote publications. `PreparedSync::apply` re-reads live stores after network work so intervening local edits remain concurrent with downloaded changes. It calculates desired values using the effective local baseline, retaining unresolved local conflicts. Applying a preview must not blindly overwrite stores from the earlier preparation snapshot.

Before the first cross-store mutation, the coordinator writes an encrypted recovery journal. New credential slots are recorded before creation; old slots are cleaned up after commit. The journal covers the domain data and local replica so an interrupted apply can be recovered before those stores accept further changes.

`AppliedSync::publish` publishes immutable encrypted snapshots. [ReplicaStore](../../crates/oxideterm-cloud-sync/src/sync_v3/storage.rs) retains pending publication bytes and sequence identity for retries. Adding a business field normally uses this existing publication path; it does not require a new per-backend write protocol.

### Verify A New Mapping

Extend the existing [mapping and replica tests](../../crates/oxideterm-cloud-sync/src/sync_v3/tests.rs) and the coordinator tests alongside `operation/synchronize.rs`. Verify explicit expected values for:

- independent changes on two replicas and competing changes to the same field;
- retained local values for unresolved conflicts and the chosen value after resolution;
- edits made while remote preparation is in progress;
- excluded categories and local-only fields remaining unchanged;
- an interrupted domain write recovering its prior or committed state;
- credential inclusion and clearing, when the new mapping handles secrets.

Run the owning domain tests and `cargo test -p oxideterm-cloud-sync`; check the app and CLI when their integration changes. Archive import/export remains a separate contract: `.oxide` archive strategies such as skip or rename must not be applied as alternative algorithms for v3 cloud synchronization.
