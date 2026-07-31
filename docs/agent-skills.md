# Agent Skills in OxideSens

OxideSens supports the Agent Skills `SKILL.md` convention for both native model
providers and ACP agents. OxideTerm owns discovery and loading so the same
workflow catalog is available regardless of the selected AI backend.

## Discovery

Each skill is a directory whose name matches the `name` field in its
`SKILL.md` frontmatter. OxideTerm discovers direct child directories from these
locations, in precedence order:

1. `<workspace>/.agents/skills`
2. `<workspace>/.github/skills`
3. `<workspace>/.claude/skills`
4. `<workspace>/.opencode/skills`
5. `<OxideTerm data directory>/skills`
6. `~/.agents/skills`
7. `~/.claude/skills`
8. `~/.copilot/skills`
9. `~/.config/opencode/skills`
10. `skills` directories in enabled native plugins

Portable installations create `<dataDir>/skills` during startup. That directory
is user-owned portable data rather than an update-managed package entry, so
in-place portable updates preserve its contents.

The workspace root is captured when the OxideTerm window is created. Changing
the working directory inside a terminal does not silently replace the skill
catalog. If two locations provide the same skill name, the higher-precedence
entry wins and the other entry is reported as shadowed.

## Progressive loading

Only bounded catalog metadata is added to the initial model context. Full
instructions are loaded with `load_skill` when the model finds a matching
workflow. A user can select a skill explicitly with `/skill-name`; slash
completion lists the current catalog.

Resources referenced by a loaded skill are read with
`read_skill_resource`. Resource paths are relative to the skill directory,
must stay inside that directory after symlink resolution, and must contain
valid UTF-8 text. `SKILL.md` itself is loaded only through `load_skill`.

OxideTerm records the loaded skill identifier and content hash in conversation
metadata. Repeated loads are marked in the tool result, while the instructions
remain available to a different backend after a native-model/ACP switch. If
the file changes, resources remain unavailable until the updated skill is
loaded.

## Safety boundary

Skill metadata is treated as untrusted catalog data. Loading a skill does not
grant terminal, file, credential, plugin, forwarding, or other permissions.
Every action described by a skill still goes through the existing OxideSens
tool policy and the selected read-only, default, or approval-free safety mode.

Skill and resource reads are bounded. Discovery rejects invalid metadata,
empty instruction bodies, oversized files, invalid UTF-8, and skill symlinks
that escape their configured discovery root. Resource loading rejects path
traversal and files larger than the resource limit.

## Management

Open **Settings → OxideSens → Tools → Agent Skills** to:

- enable or disable Agent Skills globally;
- enable or disable individual discovered skills;
- inspect the active count and discovery diagnostics;
- refresh the registry after adding or removing skill directories.

ACP currently has no standard capability negotiation for host-managed skills.
OxideTerm therefore exposes the same bounded loader tools through its ACP MCP
bridge and tells the agent to use that catalog. An ACP implementation may also
have its own native skill discovery, but it cannot use that path to bypass
OxideTerm tool permissions.
