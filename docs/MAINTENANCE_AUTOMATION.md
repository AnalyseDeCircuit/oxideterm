# Maintenance Automation

OxideTerm maintenance automation reduces repetitive issue handling without transferring product,
security, merge, release, or user-relationship decisions to an unattended agent.

## Current status

The current implementation is **shadow-only**:

- issue events and manual dispatches produce a derived routing report;
- the report is visible in the workflow summary and as a short-lived artifact;
- raw issue bodies, logs, screenshots, and terminal content stay in an ephemeral runner directory;
- no model provider is connected;
- no labels, comments, branches, pull requests, closures, or other repository writes occur.

Set the repository variable `OXIDETERM_AUTOMATION_MODE` to `disabled` to stop shadow analysis.
Leaving it unset keeps the safe shadow default.

## Authority boundaries

| Area | Shadow automation | Future active automation | Maintainer only |
| --- | --- | --- | --- |
| Issue intake | Derive category, platform, and risk codes | Apply bounded labels and update one managed comment | Product and disputed triage decisions |
| Investigation | Identify whether an agent candidate is plausible | Read source and generate a tested patch | Unproven reproduction claims |
| Repository changes | None | Open a draft pull request through an isolated publisher | Merge and branch-policy changes |
| User communication | None | Report routing or an existing draft pull request | Sensitive, disputed, or unreproduced replies |
| Lifecycle | None | Mark a merged fix as waiting for release | Close manually reopened issues and publish releases |

The existing issue quality gate keeps sole ownership of its deterministic format closures. A
maintainer reopening an issue records `quality-check-exempt`; maintenance automation must never
remove that decision or close the issue later.

## Routing policy

`scripts/automation/maintenance_policy.cjs` emits one of four routes:

- `candidate_for_agent`: a bounded bug with useful reproduction evidence;
- `needs_human`: platform validation, compatibility, product, secret, authentication, update,
  release, destructive-data, cloud-sync, or plugin-permission judgment is required;
- `blocked_by_quality_gate`: the existing deterministic issue gate still owns a correction;
- `observe_only`: available evidence does not justify an implementation route.

Windows-only reports remain human-validated until a later workflow has a proven Windows
reproduction environment. A route is not a claim that the issue was reproduced or fixed.

## Repository-write isolation

The future implementation runner must not receive GitHub credentials. It may inspect a checkout,
edit an isolated worktree, run tests, and export a patch. A separate clean publisher will:

1. download the patch without the model-provider secret;
2. reject protected and human-review-only paths;
3. scan output for configured secret values and credential material;
4. apply the patch to a fresh checkout;
5. create a draft pull request using a short-lived GitHub App installation token.

Automation control files under `.github/workflows/`, `.github/actions/`, and
`scripts/automation/` are protected from agent-authored patches. Release, update, secret-store,
cloud-sync, and plugin capability boundaries require a maintainer.

## GitHub App

The private OxideTerm Maintainer GitHub App is configured through:

- Actions variable `OXIDETERM_AUTOMATION_CLIENT_ID`;
- Actions secret `OXIDETERM_AUTOMATION_PRIVATE_KEY`.

The `Verify maintenance app` manual job requests a read-only installation token and confirms that
the App can see only the intended repository. Shadow triage does not request an App token.

## Workflows

### Maintenance Automation

`.github/workflows/maintenance-automation.yml` runs on issue creation, edits, reopen events, and
label changes. Manual dispatch supports:

- an `issue_number` to replay shadow analysis;
- `verify_app` to validate the App installation without writing repository state.

Every issue has its own concurrency group. Runs are serialized rather than cancelled so an edit
cannot interrupt cleanup of a previous raw issue context.

### Native Platform Checks

`.github/workflows/platform-checks.yml` checks the native application and GPUI platform crate on
Windows and macOS whenever application code or workspace dependencies change. Linux continues to
use the full workspace CI job.

## Shadow evaluation

Keep shadow mode for at least one week or 20 representative issue events, whichever is longer.
Before enabling any repository writes, review:

- false candidate rate;
- sensitive or platform-specific reports incorrectly marked as candidates;
- raw content absence in uploaded reports;
- duplicate workflow behavior after edits and label changes;
- Windows and macOS check reliability;
- Actions duration and daily run volume.

Active mode must not be enabled until a separate change adds tested write application, daily
limits, isolated patch publication, and an explicit model-provider boundary.
