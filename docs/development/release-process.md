# Release Process

This page is for maintainers preparing native releases. Contributors should not create release tags or dispatch publishing workflows unless explicitly asked.

## Prepare

1. Obtain explicit approval that the GUI build has been run successfully and is ready to publish. That approval is the release gate.
2. Start from an up-to-date `main` with a clean working tree. Fetch tags before deciding that a version is available.
3. Select one supported channel and its exact version/tag form:

   | Channel | Version | Tag | Changelog |
   | --- | --- | --- | --- |
   | Stable | `X.Y.Z` | `vX.Y.Z` | `stable-changelog.md` |
   | Beta | `X.Y.Z-beta.N` | `vX.Y.Z-beta.N` | `beta-changelog.md` |
   | GPUI preview | `X.Y.Z-gpui-preview.N` | `gpui-vX.Y.Z-gpui-preview.N` | `gpui-preview-changelog.md` |

4. Establish the changelog range with the repository helper rather than commit subjects alone:

   ```sh
   python3 .agents/skills/oxideterm-release/scripts/release_context.py \
     --repo . --channel <channel> --version <version>
   ```

5. Review the meaningful changes since that range and update the selected channel changelog under [`.github/release-notes/`](../../.github/release-notes/). Each entry begins with `## <version>` and contains matching `### English` and `### 中文` blocks.
6. Validate the intended version without writing files:

   ```sh
   python3 scripts/release/bump_version.py <version> --dry-run
   ```

7. Apply the version update with `scripts/release/bump_version.py <version>`. It updates the workspace version, localized README badges, and lockfile; do not hand-edit those outputs.
8. Compose the release body into a temporary file with `.github/scripts/compose_release_notes.py`, then verify channel, language-block order, version section, and stable download links.
9. Review the complete release diff and stage only the intended release files. Commit and push the release preparation before creating an annotated tag on that verified commit.

## Publish

The `Native Package` workflow is the publishing authority. Dispatch it from `main` with the existing release tag and enable release upload. The workflow validates that the tag belongs to `main`, that its version matches the tagged workspace manifest, builds the native platform matrix, composes notes from the selected channel template and changelog, uploads artifacts, and verifies the final asset set.

The tag does not publish by itself. After pushing the annotated tag, dispatch the workflow explicitly:

```sh
gh workflow run native-package.yml \
  --repo AnalyseDeCircuit/oxideterm \
  --ref main \
  -f release_tag=<tag> \
  -f upload_release=true
```

Do not manually create a GitHub Release. The workflow checks out the tagged commit and publishes the release only after its validation steps pass.

## Failure Boundaries

- If a tag exists unexpectedly, compare its annotated object and peeled commit with the intended release commit, then stop.
- If a packaging dispatch was canceled before a GitHub Release exists and the tag still points at the intended commit, leave the tag unchanged and dispatch a new `Native Package` run with the same tag.
- Moving an existing tag requires explicit authorization and a force-with-lease update against the recorded annotated tag object. Never delete the remote tag as a shortcut.
- If a GitHub Release already exists or updater assets may have reached users, keep the tag immutable and publish the next patch version.
- If a completed packaging run produced artifacts but uploaded them incompletely, use the repair workflow with its original run identifier. It can republish existing artifacts; it cannot build a platform that never produced one.

## Verify And Recover

- Confirm the published tag, release notes, checksums, signatures when configured, and all expected platform assets.
- Use [`.github/workflows/repair-release-assets.yml`](../../.github/workflows/repair-release-assets.yml) only for artifacts produced by a completed Native Package run.
- Keep stable, beta, and GPUI preview channel notes separate. Do not publish a preview as a stable release.
- If a release must be rebuilt, inspect the existing tag and release assets before making any destructive GitHub change.

For a release that has already reached users, prefer a new patch release to retagging. The recovery path exists only for explicitly authorized, unpublished same-version repairs.

The detailed packaging implementation lives in `scripts/release/` and [`.github/workflows/native-package.yml`](../../.github/workflows/native-package.yml). Treat those files as the executable source of truth when this guide and workflow behavior differ.
