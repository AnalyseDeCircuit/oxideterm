---
name: oxideterm-test-audit
description: Write, revise, or audit OxideTerm tests for observable behavior, credible regressions, and independent contracts. Use when diagnosing a failed test, reviewing test quality, or consolidating redundant coverage. Ordinary test changes use a focused check, not a repository-wide audit.
---

# OxideTerm Test Audit

Keep useful regression protection at the layer that owns the behavior. Use the smallest review that resolves the requested change; judge outcomes by the contracts preserved and errors detected, not test counts or deleted lines.

Read the applicable `AGENTS.md`. Run commands from the repository root. The [verification matrix](../../../docs/development/verification.md) and [testing guide](../../../docs/development/testing-and-fixtures.md) define project-specific checks; this skill supplies the test-quality decisions.

## Choose The Scope

- **Writing or changing a test:** apply the authoring questions to that behavior and inspect its closest existing coverage.
- **Diagnosing a failure:** establish why the test failed and whether the implementation, expectation, fixture, or environment is wrong before editing.
- **Auditing existing tests:** review the requested module or behavior and record evidence for each proposed change. Expand to an entire subsystem only when that scope was requested. Use the same evidence standard for each batch.

Creating or invoking this skill does not authorize a broad cleanup, subagents, commits, pushes, or PRs. Honor the user's existing authorization and any review-only boundary. When edits are already authorized and the evidence is sufficient, proceed without another approval round.

## Authoring Questions

Before adding or changing a test, identify:

1. The observable behavior or independent contract being protected.
2. A plausible regression and the particular assertion it would fail.
3. What the nearest existing coverage misses. Extend that coverage when appropriate; another layer needs a distinct integration, protocol, or lifecycle risk.
4. Whether the test would require a production export, switch, wrapper, or injection hook that has no business caller. Prefer testing at the existing owning boundary.

Use expected values independent of the implementation. Fixtures must distinguish the correct behavior from the likely bug: include excluded candidates when testing filters, different ordering keys when testing sorting, and actual sensitive input when checking redaction. A constructor field round-trip, a nonempty result, or a mock that performs the behavior being asserted is insufficient evidence of the intended workflow.

Test helpers and fixtures can legitimately be test-only. The concern is distorting production APIs or keeping obsolete production behavior alive solely for tests.

## Diagnose A Failing Test

Read the complete test, its inputs, the production path, nearby coverage, and the relevant change history. For CI, identify the failing commit, job, test name, and assertion; a generic nonzero exit is not a root cause. Inspect dependency source or types when the claim depends on their behavior.

Classify the failure from evidence:

- **Product regression:** the intended contract still holds. Reproduce the failure and repair the owning implementation within the authorized scope.
- **Obsolete expectation:** history and the current contract show an intentional behavior change. Update the expected behavior while retaining protection for supported released formats and APIs.
- **Faulty test or fixture:** the case does not exercise its claimed condition, relies on accidental state, or fails for an unrelated reason. Repair the setup and assertions at the real boundary.
- **Environment failure:** compilation, dependencies, resources, platform, or external fixtures prevent the behavior from being tested. Report that limitation; do not reinterpret it as a passing or obsolete test.

A baseline failure is not a reason to delete a test. For a bug fix, demonstrate the intended failure before the repair and success afterward. Reuse valid CI evidence or a captured local baseline when it proves the same failure; do not repeat expensive checks solely for ceremony.

For example, the `.oxide` plugin test needs to distinguish password-free validation from decrypted import preview. The encrypted format's validation response should not reveal metadata; the password-bearing preview should still contain the explicitly expected description and connection names. Updating those assertions preserves the contract without another low-level decryption test at the plugin layer.

## Audit Before Editing

Read each candidate fully, including parameter rows. Trace its production entry point, callers, relevant dependencies, sibling implementations, and overlapping tests. Check history for the reason it exists and CI routing for which checks actually run.

Use a compact evidence record for proposed changes, in the working response unless a saved report was requested:

| Test and location | Behavior and credible failure | Decision | Remaining or repaired proof | Validation |
| --- | --- | --- | --- | --- |
| Exact test name and path | What the assertions can detect | Retain / Fix / Consolidate / Delete | Named test or boundary, with relevant history | Focused command or manual scenario |

- **Retain:** it independently protects a meaningful contract.
- **Fix:** the behavior matters, but the input, assertion, name, or expected result is wrong or too weak.
- **Consolidate:** move distinct assertions into the named primary test before removing the duplicate setup or case.
- **Delete:** name the stronger remaining proof, or demonstrate that the behavior no longer has a supported contract. Uncertainty means investigate or retain.

For production code removed with a test, also identify its non-test callers and why the code is obsolete. Search for references before removing a helper, export, feature, or fixture. Do not make unrelated simplifications part of the audit.

### Patterns Worth Investigating

- The test repeats the implementation's source text, private call shape, or exact internal representation, and fails after a behavior-preserving refactor.
- Several layers call the same helper with equivalent inputs and repeat its assertions without exercising their own integration.
- A fixture prearranges the callback order, completion notification, or persisted result that the real owner should produce.
- A denial test is rejected by parameter validation before reaching the permission check it claims to test.
- A concurrency, cancellation, or wakeup test checks only a final flag without creating the competing work or waiter.
- A success assertion checks only `Ok`, `Some`, length, or nonemptiness when content, identity, ordering, or a durable side effect is the contract.

These are investigation signals, not automatic deletion rules. Preserve independent security defaults, protocol bytes, persistence and migration formats, plugin APIs, platform behavior, packaging, internationalization, and architecture constraints. Static inspection can be useful when it guards a real contract and survives irrelevant renaming. Slow execution alone does not make a test redundant.

## OxideTerm Boundaries

| Behavior | Preferred verification boundary |
| --- | --- |
| Parsing, settings normalization, serialization, or data merging | Owning domain crate with explicit expected values and supported format samples |
| SSH node sharing, SFTP, forwarding, reconnect, or jump hosts | Runtime owner with actual consumer registration, cancellation, and cleanup; see [runtime ownership](../../../docs/development/runtime-ownership.md) |
| Mixed pages, focus, popups, or late UI results | Existing GPUI test context and feature entity; native IME, window, and GPU behavior still require host validation |
| Plugin, MCP, or CLI adapter | Real dispatch and returned data, authority, target identity, or side effect; avoid replaying the domain helper alone |
| Redaction and credential lifetime | Synthetic secret input through the relevant output or ownership boundary; see [sensitive data](../../../docs/development/secrets-and-sensitive-data.md) |
| Packaging and repository scripts | Existing script tests or the actual dry-run contract selected by the verification matrix |

For a shared-node test, closing one terminal should leave another real consumer usable. For stale-result handling, deliver an old completion after the owner has changed. Use existing fixtures and event-driven synchronization; arbitrary sleeps and extra retries do not establish these conditions.

## Validate And Finish

Finish the relevant source/test edits before starting their test run. Keep the checkout stable during that run.

1. Run the focused test or module, for example `cargo test -p <crate> <test_filter>`. Use the target and platform that actually own it; the desktop tests are in the `oxideterm-native` binary target of `oxideterm-gpui-app`. The standalone remote agent uses `--manifest-path agent/Cargo.toml`.
2. Read the result and verify that the intended tests actually ran. A successful command with zero matching tests is not proof. After a removal or move, run the named remaining coverage and any distinct affected integration check.
3. Run `cargo fmt --check` and `git diff --check` for Rust edits. Add crate checks, script tests, locale auditing, or manual workflows according to the verification matrix. Reuse results until relevant changes or new evidence invalidate them.
4. Review the final diff and status. Check that no unique behavior disappeared, no assertions were weakened to hide a product failure, and no unrelated work was changed.

Use deliberate mutation only when needed to resolve a concrete doubt about an assertion. Keep it isolated from user work, restore the exact source afterward, and do not require it for every test. Subsystem-wide audits should reconcile the removed cases against their named remaining tests before running the final affected suites.

Report the decision, the behavior still protected, checks actually run, and any material unverified boundary. Explain removed coverage when deletion occurred. Stop when the requested scope is complete; do not enlarge the audit to meet a count or reduction target.

## Source

Adapted from the test-value and evidence principles in OpenClaw's [test-audit](https://github.com/openclaw/openclaw/blob/main/.agents/skills/test-audit/SKILL.md) and [campaign guidance](https://github.com/openclaw/openclaw/blob/main/.agents/skills/test-audit/CAMPAIGN.md). Execution, ownership, and validation here follow OxideTerm's repository rules.
