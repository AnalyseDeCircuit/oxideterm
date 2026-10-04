---
name: oxideterm-plugin-compatibility
description: Preserve OxideTerm plugin compatibility when changing host plugin APIs, runtime or grammar loading, catalog version selection, or installed-plugin checks. Also use when extracting built-in language support into plugins. Ordinary app version bumps or plugin-page styling alone do not require this workflow.
---

# OxideTerm Plugin Compatibility

Read the relevant implementation before assigning a compatibility boundary:

- `crates/oxideterm-plugin-manifest/src/registry.rs`: catalog release and correction data.
- `crates/oxideterm-plugin-registry/src/registry.rs` and `compatibility.rs`: version selection, cached requirements, and startup discovery.
- `crates/oxideterm-plugin-registry/src/install.rs`: package installation checks.
- `crates/oxideterm-gpui-app/src/workspace/plugin_entity.rs`: refresh, activation gating, and task ownership.
- The specific host API, runtime, or grammar loader affected by the change.
- `crates/oxideterm-editor-syntax/src/plugin.rs` and
  `crates/oxideterm-gpui-editor/src/languages.rs` for lazy grammar loading and open-document refresh.

## Host-side contract

Select the highest release supported by both the host and platform. Offer an
update only when that compatible release is newer than the installed plugin;
explain a higher incompatible version separately. Never silently downgrade.

Recheck installed plugins on startup after both app upgrades and downgrades.
Apply the latest cached correction for the exact plugin ID and version without
rewriting package files. Retain files, settings, and enable preferences while
blocking incompatible activation. Keep the startup refresh bounded and owned
by the plugin entity; offline startup uses the last valid cache.

For language extraction, verify the grammar loader, Tree-sitter ABI, and actual
old-client behavior. A host range cannot replace those checks. Do not assign an
upper bound to old plugins just because a new plugin or app version was released.

## Coordinate publication

Plugin creation, release records, and compatibility corrections belong in the
`AnalyseDeCircuit/oxideterm-plugins` repository. Locate its actual checkout by
repository identity rather than assuming a local directory name. Read its
`.agents/skills/oxideterm-plugin-release/SKILL.md` and use its release scripts.
The canonical skill is also available at:

https://github.com/AnalyseDeCircuit/oxideterm-plugins/blob/main/.agents/skills/oxideterm-plugin-release/SKILL.md

Use a local checkout when that skill or its scripts are not published yet.
Do not invent an alternative publishing process when the required tooling is
missing. Host implementation changes alone do not authorize publishing plugins.

For changed behavior, use the existing registry and plugin-entity tests at the
boundary that owns it. Cover the affected risks: compatible-version selection,
upgrade/downgrade rejection, correction recovery, or offline startup. Do not rerun
unrelated suites merely because a plugin compatibility file changed.
