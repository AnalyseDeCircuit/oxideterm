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

The official client uses the v2 summary catalog and loads checksum-bound histories
for installed plugins and the current marketplace page. A history must match its
summary identity, latest version, effective host range, size, and SHA-256. Preserve
the frozen v1 endpoint, exact historical snapshot, referenced assets, and v1 cache
reader. All subsequent plugins, releases and corrections belong only to v2;
existing clients must upgrade the host to receive them. New clients fetch only v2;
on failure retain the last valid cache rather than replace it with frozen v1.
Publish v2 before shipping a host that requires it. Save verified raw histories before replacing
the compact root cache; startup and manual installation must resolve corrections
for the exact installed version. Cancel obsolete page requests and discard results
from another page or catalog snapshot. Catalog maintenance and generation belong
to the marketplace's per-plugin source records and release automation.

For language extraction, verify the grammar loader, Tree-sitter ABI, and actual
old-client behavior. A host range cannot replace those checks. Do not assign an
upper bound to old plugins just because a new plugin or app version was released.

For ACP agent extraction, keep the ACP client, permission decisions and process
ownership in the host. Published `acp` manifests remain supported and normalize
into the common helper plan. New helper manifests declare feature `acp`, protocol
`acp` and version 1. Agents speak ACP stdio directly; do not route their traffic
through the ordinary plugin supervisor. Check `registry.acp_agents`, `workspace/acp_plugins.rs`, and
`workspace/acp_workspace.rs` for startup gating, launch resolution and cleanup.
Hosts through 2.2.1 cannot load this runtime. Preserve existing agent identities
and user options when migrating bundled adapters to plugin bindings.

Published Mosh manifests use `terminal-transport`; new helper manifests declare
feature `terminal-transport`, protocol `oxideterm-mosh` and version 1. Keep SSH
bootstrap, terminal rendering and local prediction in the host; UDP, SSP and
encryption belong to the engine plugin. Hosts through 2.2.1 cannot load it.
Check `oxideterm-mosh` and its workspace-owned `MoshPluginSessions` for launch
gating and process retirement before package replacement. Both pipe directions
must remain independent under output backpressure. RDP, VNC and Mosh share the
`remote-connections` catalog category.

## Coordinate publication

Published remote desktop manifests use `remote-desktop`; new helper manifests
declare feature `remote-desktop`, protocol `oxideterm-remote-desktop` and version 1.
Both normalize into the common helper plan, outside the ordinary plugin supervisor.
Keep the native viewer, input, credentials and SSH tunnel ownership in the host.
Require a supported `contributes.remoteDesktop.protocolVersion` and resolve only
trusted, enabled, compatible installed executables. Stop and reap helper processes
before updating or removing their files; disable automatic reconnect until a
compatible provider is available. Keep the existing binary stdio format unchanged
when extracting helpers. Hosts through 2.2.1 cannot load this runtime.

Generic `helper` manifests require hosts from 2.2.2. Preserve published manifest
kinds, capability approval boundaries and protocol payloads when normalizing them.
Runtime-plan unification does not transfer child ownership to the plugin supervisor.
FIDO declares feature `ssh-authentication`, protocol `oxideterm-security-key` and
version 1; its authentication task owns cancellation and reaping.

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
