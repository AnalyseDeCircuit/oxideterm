---
name: oxideterm-release
description: Prepare, publish, or recover canceled OxideTerm stable, beta, or GPUI preview releases by deriving changelog content from the previous tag, running the repository version-bump script, validating channel-specific release notes, committing, pushing, and creating the correct annotated tag. Use when the user asks to upgrade the OxideTerm version, prepare a release, write a release changelog, commit and push a release, create a release tag, or republish the same version after its tag-triggered workflow was canceled.
---

# OxideTerm Release

Publish an OxideTerm release through the repository-owned automation. Treat the user's completed GUI run as the release gate and do not run Cargo tests by default.

## Required release input

Resolve these values before making release changes:

- Target version.
- Target channel: `stable`, `beta`, or `gpui-preview`.
- Explicit confirmation that the user ran the GUI application and approved publishing.

Infer the channel only when the version is unambiguous. Ask before publishing if GUI approval is absent. Preparing a changelog or dry run does not require approval.

## Channel contract

| Channel | Version | Tag | Changelog | Base notes |
|---|---|---|---|---|
| Stable | `X.Y.Z` | `vX.Y.Z` | `.github/release-notes/stable-changelog.md` | `.github/release-notes/stable.md` |
| Beta | `X.Y.Z-beta.N` | `vX.Y.Z-beta.N` | `.github/release-notes/beta-changelog.md` | `.github/release-notes/beta.md` |
| GPUI preview | `X.Y.Z-gpui-preview.N` | `gpui-vX.Y.Z-gpui-preview.N` | `.github/release-notes/gpui-preview-changelog.md` | `.github/release-notes/gpui-preview.md` |

Do not invent another tag prefix. The native package workflow triggers for these tag forms and selects release notes from the version or tag.

## Workflow

### 1. Inspect repository state

Work from the OxideTerm repository root. Read `AGENTS.md`, then inspect:

```bash
git status --short --branch
git remote -v
git log -10 --oneline --decorate
```

Fetch the publishing remote and tags before deciding that a tag is available:

```bash
git fetch origin --tags
```

Confirm the branch is not behind or diverged from its upstream, and inspect the target tag locally and remotely:

```bash
git rev-list --left-right --count HEAD...@{upstream}
git tag --list <tag>
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
```

Preserve unrelated user changes. Stop if the intended release scope cannot be separated safely or the branch has diverged. For an ordinary release, stop when the target tag exists. Only move an existing tag through the explicit canceled-release recovery procedure below.

### 2. Establish the changelog range

Run the bundled helper before writing the changelog:

```bash
python3 .agents/skills/oxideterm-release/scripts/release_context.py \
  --repo . --channel <channel> --version <version>
```

Prefer the previous reachable tag from the same channel as the baseline:

- Stable compares with the previous stable tag.
- Beta compares with the previous beta tag.
- GPUI preview compares with the previous GPUI preview tag.

For the first release in a channel, use the newest reachable release tag from any of the three channels as the bootstrap baseline. Use the root commit only when the repository has no earlier reachable release tag. This keeps a first beta or preview changelog scoped to work since the actual preceding release instead of summarizing the entire repository.

The helper prints the exact range, commits, and diff summary. Also inspect actual changes, including intended uncommitted changes:

```bash
git diff <previous-tag>..HEAD
git diff
git diff --cached
```

Do not derive release notes from commit subjects alone. Read the meaningful implementation and user-facing differences. Exclude mechanical version bumps, changelog edits, formatting-only churn, and internal details that have no release impact.

### 3. Write the channel changelog

Insert `## <version>` as the newest entry in the selected changelog. The heading must exactly match the version because `.github/scripts/compose_release_notes.py` extracts it verbatim.

Write in English to match the existing release files. Use concise user-facing past tense. Combine related commits into one outcome and avoid raw commit-title dumps, implementation trivia, unsupported performance claims, and claims that were not verified.

Apply channel-specific emphasis:

- **Stable:** Summarize the complete delta since the previous stable tag. Start with one short release summary, then use only useful sections such as `### Highlights`, `### Fixes`, `### Security`, or `### Release Maintenance`. Emphasize user-visible behavior and compatibility.
- **Beta:** Summarize the delta since the previous beta tag. State what is approaching stable, what changed, and which workflows need validation. Mention known limitations only when supported by the diff or issue context.
- **GPUI preview:** Summarize the delta since the previous GPUI preview tag. Focus on newly testable native UI/runtime work, parity, rough edges, and concrete testing targets. Keep the compact summary-and-bullets style used by existing preview entries.

If there is no earlier tag for that channel, state which preceding release tag was used as the bootstrap baseline.

### 4. Run the repository version script

Always use the repository script; never hand-edit the workspace version, README badges, or lockfile:

```bash
python3 scripts/release/bump_version.py <version>
```

This validates SemVer, updates `[workspace.package]`, synchronizes every localized README badge, and refreshes `Cargo.lock` offline.

### 5. Perform lightweight release validation

Do not run `cargo test`, `cargo check`, or launch the GUI by default. The user owns GUI validation before publishing. Run broader checks only when explicitly requested or when a concrete release blocker requires them.

Validate only the release mechanics:

```bash
python3 scripts/release/bump_version.py <version> --dry-run
git diff --check
```

Compose the exact release notes into a temporary file outside the repository:

```bash
python3 .github/scripts/compose_release_notes.py \
  --version <version> \
  --tag <tag> \
  --base <base-notes> \
  --changelog <channel-changelog> \
  --output /tmp/oxideterm-release-notes-<version>.md
```

Read the generated file and verify that the intended section appears once, the channel is correct, and stable download URLs use the target tag.

### 6. Review, commit, push, and tag

Review the complete release diff and status before staging. Confirm that no secret, build artifact, unrelated file, or temporary release-notes file is included.

Stage only the reviewed release files and intended product changes, then inspect the exact commit payload:

```bash
git add -- <reviewed-files>
git diff --cached --stat
git diff --cached
```

Use the established release commit style:

```bash
git commit -m "Release OxideTerm <version>"
```

Push the branch before creating the tag. Then create an annotated tag on the verified release commit and push only that tag:

```bash
git push origin <branch>
git tag -a <tag> -m "OxideTerm <version>"
git push origin <tag>
```

Afterward, verify both refs:

```bash
git rev-parse HEAD
git rev-list -n 1 <tag>
git ls-remote --heads origin <branch>
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
git status --short --branch
```

The release tag triggers packaging; do not manually create a GitHub Release unless the user asks.

## Recover a canceled tag-triggered release

Use this procedure only when the maintainer explicitly requests republishing the same version and authorizes moving its existing tag. The earlier tag-triggered workflow must have been canceled before it created a published GitHub Release. If `gh release view <tag>` finds a published release or updater assets may already have reached users, keep the tag immutable and publish the next patch version instead.

1. Fetch and record both the remote annotated tag object and its peeled commit before changing anything:

```bash
git fetch origin --tags
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
gh run list --repo <owner/repo> --branch <tag> --limit 10
gh release view <tag> --repo <owner/repo>
```

Treat a missing GitHub Release as expected only when the packaging run was canceled. Do not use **Re-run jobs** on the canceled run: GitHub retains that run's original `head_sha`, so it rebuilds the old release commit even after the tag moves.

2. Prepare and validate the corrected release normally. Commit and push the branch before touching the tag. Re-fetch and verify that the remote tag object still equals the value recorded in step 1.

3. Recreate the annotated local tag on the verified release commit, then update the remote tag with a lease against the old annotated tag object, not the peeled commit:

```bash
git tag -fa <tag> -m "OxideTerm <version>" <release-commit>
git push --force-with-lease=refs/tags/<tag>:<old-tag-object> origin refs/tags/<tag>
```

Never delete the remote tag before recreating it; deletion creates an unprotected interval and loses the comparison guard. If the lease fails, fetch and stop to inspect who changed the tag.

4. Verify that the branch and peeled tag resolve to the release commit, then confirm a new packaging run exists with both the expected tag and new `head_sha`:

```bash
git rev-parse <release-commit>
git rev-list -n 1 <tag>
git ls-remote --heads origin <branch>
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
gh run list --repo <owner/repo> --branch <tag> --limit 10
```

Report the new run URL and status. Do not keep monitoring it unless the user explicitly asks.

## Failure handling

- If the branch push fails, do not create or push the tag.
- If the branch push succeeds but tag creation or push fails, report that partial state precisely.
- If the tag already exists unexpectedly, compare its target with the intended commit and stop. Never retag without the maintainer's explicit same-version recovery authorization.
- If new commits or worktree changes appear during preparation, re-read status and regenerate the release range before publishing.
- If channel detection, previous-tag selection, or release scope is ambiguous, ask rather than guessing.
