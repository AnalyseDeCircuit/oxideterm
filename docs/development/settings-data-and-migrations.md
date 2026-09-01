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
