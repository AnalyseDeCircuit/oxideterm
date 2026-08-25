---
name: oxideterm-release
description: Prepare, publish, or recover canceled OxideTerm stable, beta, or GPUI preview releases by deriving changelog content from the previous tag, selecting the requested release-note detail level, running the repository version-bump script, validating channel-specific notes and fork ownership, committing, pushing, creating the correct annotated tag, and dispatching the main-scoped native package workflow. Use when the user asks to upgrade the OxideTerm version, prepare a release, write a release changelog, commit and push a release, create a release tag, republish the same version after its release workflow was canceled, or repair release assets from a completed Native Package run.
---

# OxideTerm Release

Publish an OxideTerm release through the repository-owned automation. Treat the user's completed GUI run as the release gate and do not run Cargo tests by default.

## Required release input

Resolve these values before making release changes:

- Target version.
- Target channel: `stable`, `beta`, or `gpui-preview`.
- Explicit confirmation that the user ran the GUI application and approved publishing.
- Release-note detail level when the user explicitly requests `minimal`, `medium`, or `detailed`; otherwise use `detailed`.

Infer the channel only when the version is unambiguous. Ask before publishing if GUI approval is absent. Preparing a changelog or dry run does not require approval.

## Channel contract

| Channel | Version | Tag | Changelog | Base notes |
|---|---|---|---|---|
| Stable | `X.Y.Z` | `vX.Y.Z` | `.github/release-notes/stable-changelog.md` | `.github/release-notes/stable.md` |
| Beta | `X.Y.Z-beta.N` | `vX.Y.Z-beta.N` | `.github/release-notes/beta-changelog.md` | `.github/release-notes/beta.md` |
| GPUI preview | `X.Y.Z-gpui-preview.N` | `gpui-vX.Y.Z-gpui-preview.N` | `.github/release-notes/gpui-preview-changelog.md` | `.github/release-notes/gpui-preview.md` |

Do not invent another tag prefix. The native package workflow receives the existing tag through its `release_tag` dispatch input and selects release notes from that tag.

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

Insert `## <version>` as the newest entry in the selected changelog. The heading must exactly match the version because `.github/scripts/compose_release_notes.py` uses it to locate the entry. The heading is an extraction boundary, not necessarily part of the published release body.

Write every version entry as two complete language blocks in this order:

```markdown
## <version>

### English

<English summary and sections>

### 中文

<Chinese summary and sections>
```

Keep both blocks structurally aligned: they must describe the same outcomes, limitations, validation status, and upgrade requirements in the same order. Translate for natural release-note language rather than word for word, and keep product names, commands, file names, and protocol names unchanged when translation would reduce precision. Do not merge English and Chinese into the same bullet. Use concise user-facing past tense in English and concise completed-action wording in Chinese. Keep each language's opening summary paragraph on one physical line so the GitHub Release editor does not show an artificial break. Combine related commits into one outcome and avoid raw commit-title dumps, implementation trivia, unsupported performance claims, and claims that were not verified. Use one restrained, semantically relevant emoji on each main stable-release section heading in both blocks; do not decorate every bullet or mix multiple emoji styles within one section.

For a fork release, each language block must clearly separate changes inherited from official OxideTerm upstream from changes implemented by the fork. Use explicit localized headings equivalent to `Upstream changes` / `上游更新` and `Fork-specific changes` / `Fork 自有更新`; subsystem headings may be nested below them when the selected detail level warrants it. Classify merged or cherry-picked official work as upstream regardless of commit author, and classify behavior unique to the publishing fork as fork-specific. When a fork modifies an upstream feature, describe the inherited capability under upstream and the fork's material delta under fork-specific changes. If either category has no changes in the release range, state that explicitly instead of omitting the category. Never let a combined summary or bullet imply that the fork authored upstream work.

#### Release-note detail level

Use the same level for the English and Chinese blocks. The levels control coverage and grouping, not a mandatory word count. Never omit a breaking change, security boundary, upgrade requirement, known limitation, or material compatibility issue just to satisfy a shorter level.

| Level | Required density |
|---|---|
| `minimal` | Use only when the user explicitly requests it. Write one short summary followed by roughly 2–5 highly consolidated outcome bullets, with at most one or two headings when they materially improve scanning. Cover only the release's defining user-visible changes and required warnings. |
| `medium` | Use only when the user explicitly requests it. Write one summary, a small number of useful sections, and grouped bullets that cover the major features, fixes, and security or compatibility outcomes. Consolidate related supporting workflows instead of enumerating each one separately. |
| `detailed` | Default unless the user explicitly chooses another level. Organize the notes by meaningful subsystem or workflow and give each distinct user-visible capability, behavior change, compatibility fix, supported limitation, and verified performance or validation result enough space to be understood without reading the diff. Include supporting workflows when they materially affect how users understand or use the change, but do not add filler when a release has fewer real changes. |

Treat the levels as coverage and grouping guidance, not fixed templates. Determine the complete factual delta first, then consolidate it to the selected level. If the user says only “write the release notes” or “prepare the release,” use `detailed`.

Apply channel-specific emphasis:

- **Stable:** Summarize the complete delta since the previous stable tag. In each language block, start with one short release summary, then use only useful sections such as `#### ✨ Highlights` / `#### ✨ 重点更新`, `#### 🛠️ Fixes` / `#### 🛠️ 修复`, `#### 🔒 Security` / `#### 🔒 安全`, or `#### 🧰 Release Maintenance` / `#### 🧰 发布维护`. Emphasize user-visible behavior and compatibility. The composed GitHub Release body must begin with `### English`, place the matching `### 中文` block immediately after the complete English block, omit both a product-major heading such as `# OxideTerm 2.0` and a repeated version heading such as `## 2.0.7`, then place `## 📥 Download for your system`, installation tips, and links after both language blocks.
- **Beta:** Summarize the delta since the previous beta tag in complete English and Chinese blocks. State what is approaching stable, what changed, and which workflows need validation. Mention known limitations only when supported by the diff or issue context, and include them in both languages.
- **GPUI preview:** Summarize the delta since the previous GPUI preview tag in complete English and Chinese blocks. Focus on newly testable native UI/runtime work, parity, rough edges, and concrete testing targets. Apply the selected detail level while keeping the writing test-oriented rather than turning preview notes into stable-release marketing.

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

Read the generated file and verify that the intended section appears once, the channel is correct, both `### English` and `### 中文` appear once in that order, their claims and bullet coverage match, and stable download URLs use the target tag. For stable notes, also verify that `### English` is the first visible content, the Chinese block appears before downloads, the GitHub Release title is not repeated in the body, and the order is bilingual changelog, downloads, installation tips, then links.

### Fork release attribution check

This release skill ships inside the repository, so forks that reuse it inherit this rule. Determine the publishing repository from the remote that will receive the branch and tag. Treat it as a fork whenever that repository is not `AnalyseDeCircuit/oxideterm`, even when a separate `upstream` remote points to the official repository.

```bash
git remote -v
git remote get-url origin
```

For an official `AnalyseDeCircuit/oxideterm` release, keep the official links, updater endpoints, and About attribution. For a fork release, all three checks below are blocking:

1. **Release-note ownership and provenance.** The notes must clearly state that the build is a community fork. In both language blocks, they must separately identify changes inherited from official upstream and changes implemented by the fork, following the provenance rules in the changelog-writing section. Compare the release range with official upstream history rather than inferring ownership from author names or commit-message wording. Support, documentation, issue, download, and changelog links must resolve to resources owned by the fork, not `AnalyseDeCircuit/oxideterm/issues`, the upstream changelog, upstream release downloads, or `oxideterm.app` documentation. The stable download composer currently derives upstream asset URLs from `.github/scripts/compose_release_notes.py`; a fork must redirect that source as well as the visible base-note links.
2. **In-app updater ownership.** Every update channel exposed by the forked application must resolve to a fork-owned signed manifest and fork-owned release assets. The compiled endpoints currently live in `crates/oxideterm-update/src/channel.rs`; they must not contain `github.com/AnalyseDeCircuit/oxideterm/releases`. A fork may instead disable an update channel, but then the application must not offer or contact that channel. Never ship a fork that can update itself to an official OxideTerm build.
3. **Help & About attribution.** Keep the existing `Copyright © <year> AnalyseDeCircuit` attribution; a fork must not replace or remove it. Add an adjacent small, localized line that clearly identifies the application as a fork and names the fork maintainer or project. Render it in the Help & About legal footer and add the corresponding key to all 11 locale catalogs.

Why this rule exists: users cannot tell an official release from a fork build when the fork reuses the same name, version numbers, and release notes. Unmarked fork releases route bug reports and support traffic into the upstream repository, confuse downloads, and misattribute defects to the upstream maintainers. A short fork-attribution line and fork-owned support links prevent that confusion at no cost to the fork, and they are the standard expectation for any GPL redistribution.

```bash
gh release list --repo <fork-owner>/oxideterm --limit 3
rg -n 'AnalyseDeCircuit/oxideterm|oxideterm\.app' \
  <composed-release-notes> <base-notes> .github/scripts/compose_release_notes.py
rg -n 'github\.com/AnalyseDeCircuit/oxideterm/releases' crates/oxideterm-update/src
rg -n 'settings_view\.help\.copyright|AnalyseDeCircuit|fork' \
  crates/oxideterm-gpui-app/src/workspace/settings/pages/help.rs \
  crates/oxideterm-i18n/locales/*/settings_view.json
cargo test -p oxideterm-i18n locale_catalogs_have_the_same_complete_key_set
```

The `rg` commands are discovery checks: review each match rather than blindly requiring zero results, because the preserved copyright attribution must still name `AnalyseDeCircuit`. If a fork release fails any of the three ownership checks, treat publishing as blocked. Do not commit, push, or tag until the fork owns its release links and updater source and the About footer both preserves the upstream copyright and identifies the fork in all supported locales.

### 6. Review, commit, push, tag, and dispatch

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

Afterward, verify both refs, then dispatch `native-package.yml` from `main`. Passing the existing release tag explicitly keeps all release caches in the default-branch scope while the workflow checks out and packages the tagged commit:

```bash
git rev-parse HEAD
git rev-list -n 1 <tag>
git ls-remote --heads origin <branch>
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
gh workflow run native-package.yml \
  --repo <owner/repo> \
  --ref main \
  -f release_tag=<tag> \
  -f upload_release=true
git status --short --branch
```

Find the new `workflow_dispatch` run and verify that its `headSha` is the dispatched branch commit and its display title names the intended tag:

```bash
gh run list \
  --repo <owner/repo> \
  --workflow native-package.yml \
  --branch main \
  --event workflow_dispatch \
  --limit 10 \
  --json databaseId,status,conclusion,headSha,displayTitle,url
```

The tag does not trigger packaging by itself. Do not manually create a GitHub Release; the explicitly dispatched workflow publishes it after validating that the tag exists in the selected main-branch history and that its version matches the tagged workspace.

The first release dispatched after this migration seeds the main-scoped cache for each target. Later releases can restore compatible dependency artifacts from those keys even though their release tags and lockfile hashes differ.

## Recover a canceled release dispatch

Use this procedure only when the maintainer explicitly requests republishing the same version. The earlier main-scoped Native Package dispatch must have been canceled before it created a published GitHub Release. If `gh release view <tag>` finds a published release or updater assets may already have reached users, keep the tag immutable and publish the next patch version instead.

1. Fetch and record both the remote annotated tag object and its peeled commit before changing anything:

```bash
git fetch origin --tags
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
gh run list --repo <owner/repo> --workflow native-package.yml --branch main --event workflow_dispatch --limit 10
gh release view <tag> --repo <owner/repo>
```

Treat a missing GitHub Release as expected only when the packaging run was canceled. When the existing tag already peels to the intended release commit, leave it unchanged and dispatch `native-package.yml` again from `main` with the same `release_tag`. Do not use **Re-run jobs** after correcting release code or moving a tag: GitHub retains the canceled run's original workflow revision and inputs.

2. If the tag must move because the release commit itself changed, obtain explicit authorization to move it. Prepare and validate the corrected release normally, commit and push the branch, then re-fetch and verify that the remote tag object still equals the value recorded in step 1.

3. Recreate the annotated local tag on the verified release commit, then update the remote tag with a lease against the old annotated tag object, not the peeled commit:

```bash
git tag -fa <tag> -m "OxideTerm <version>" <release-commit>
git push --force-with-lease=refs/tags/<tag>:<old-tag-object> origin refs/tags/<tag>
```

Never delete the remote tag before recreating it; deletion creates an unprotected interval and loses the comparison guard. If the lease fails, fetch and stop to inspect who changed the tag.

4. Verify that the peeled tag resolves to the intended release commit, then dispatch a new main-scoped packaging run and confirm its display title contains the expected tag and its `headSha` matches the selected publishing branch revision:

```bash
git rev-parse <release-commit>
git rev-list -n 1 <tag>
git ls-remote --heads origin <branch>
git ls-remote --tags origin refs/tags/<tag> refs/tags/<tag>^{}
gh workflow run native-package.yml \
  --repo <owner/repo> \
  --ref main \
  -f release_tag=<tag> \
  -f upload_release=true
gh run list --repo <owner/repo> --workflow native-package.yml --branch main --event workflow_dispatch --limit 10
```

Report the new run URL and status. Do not keep monitoring it unless the user explicitly asks.

## Repair release assets from a completed build

Use this procedure when packaging failed on one or more platforms **after** a prior `Native Package` run already built the successful platforms, or when release assets uploaded incompletely and the GitHub Release exists. It reuses the finished run's artifacts without rebuilding anything.

Prerequisites:

- The target release exists (`gh release view <tag>` succeeds).
- A `Native Package` workflow run produced `OxideTerm-*` artifacts for at least the platforms you want to repair. Main-scoped dispatches are the normal source.

1. Find the completed run id for the artifacts to republish:

```bash
gh run list --repo <owner/repo> --workflow native-package.yml --limit 10
```

2. Open **Actions → Repair Release Assets → Run workflow** and enter:

- `tag`: the release tag to repair, for example `v2.0.15`.
- `run_id`: the run id from step 1.

3. The workflow validates the tag format, confirms the release exists, downloads `OxideTerm-*` artifacts from that run, re-signs every asset with the minisign key, regenerates `sha256sums.txt`, and uploads with `update_release: true` (overwrite in place).

4. Verify the repaired assets on the release page or with:

```bash
gh release view <tag> --repo <owner/repo>
```

If the release does not exist yet, do not use this workflow — dispatch `native-package.yml` from `main` with the existing `release_tag` instead. If a platform failed during the build itself (no artifact was produced), the repair workflow cannot conjure it: re-run only the failed matrix job from the `Native Package` run, or dispatch the workflow again. The `native-package.yml` release job is idempotent (`update_release: true`) and caches survive failures (`cache-on-failure: true`), so a single-platform retry resumes incrementally.

## Failure handling

- If the branch push fails, do not create or push the tag.
- If the branch push succeeds but tag creation or push fails, do not dispatch packaging and report that partial state precisely.
- If the tag push succeeds but the Native Package dispatch fails, leave the immutable tag in place, report the partial state, and retry the same explicit dispatch after resolving the workflow problem.
- If the tag already exists unexpectedly, compare its target with the intended commit and stop. Never retag without the maintainer's explicit same-version recovery authorization.
- If new commits or worktree changes appear during preparation, re-read status and regenerate the release range before publishing.
- If channel detection, previous-tag selection, or release scope is ambiguous, ask rather than guessing.
