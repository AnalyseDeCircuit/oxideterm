# Contributing to OxideTerm

Pull requests are welcome, including bug fixes, documentation, translations, and features. OxideTerm has one maintainer, with contributions from the community. Focused changes with clear reproduction steps and verification results are easier to review.

## Before you start

Before starting a substantial change, open an issue to discuss the problem, intended behavior, and implementation direction. Small, focused fixes may be proposed directly. Changes are evaluated based on project direction, maintainability, cross-platform behavior, and the long-term cost of supporting them, so not every proposal will be merged.

Check existing issues and pull requests to avoid duplicating work. Read the [Development Guide](development/README.md) and repository [guidelines](../AGENTS.md) before preparing a change. The development guide covers local setup, crate ownership, verification, and platform-specific requirements.

Features that do not need changes to the core application may be a better fit for a [plugin](user-guide/en/plugin-development.md). If a required plugin API is missing, open an issue to discuss the interface.

## Prepare a pull request

1. Fork the repository and create a branch from the latest `main`.
2. Keep the PR focused on one problem or feature. Leave unrelated refactoring, formatting, and dependency updates out of the diff.
3. Follow the [local development instructions](development/local-development.md). Reuse existing components, theme tokens, and runtime ownership patterns.
4. Run the relevant checks from the [verification matrix](development/verification.md), and exercise the affected user workflow.
5. Open a PR against `main`. Use a draft while the implementation is still in progress, then mark it ready for review when the changes and verification are complete.

In the PR description, explain:

- What problem the change solves, with a link to the issue if there is one.
- What changed and why this approach fits the problem.
- Which checks you ran and their results, including the operating system and any checks you could not run.
- How to reproduce and verify the behavior. Include screenshots or a short recording for visible UI changes.

For performance changes, include before-and-after measurements using the same workload and environment. Distinguish measured results from estimates.

## Keep the UI consistent across platforms

Prefer OxideTerm's existing GPUI-rendered controls over operating-system-native widgets and dialogs. Reuse the shared buttons, text fields, menus, modals, and scrollbars, together with their theme tokens and interaction handling. This keeps appearance and behavior consistent across macOS, Windows, and Linux. Read the [product UI guide](development/gpui-product-ui.md) and the complete implementation of a similar interaction before making UI changes.

Use native UI only when a required platform integration cannot reasonably be handled by the existing components, and explain that need in the PR. Do not build a separate custom control when a shared component already covers the interaction.

## Verify your changes

For Rust changes, start with:

```sh
git diff --check
cargo fmt --check
```

Before submitting a code PR, compilation checks and relevant tests must pass for the affected code on at least one supported platform. Expand the scope when the change crosses layers, and exercise the affected user workflow. Include the commands, results, and platform in the PR description; CI supplements this verification. Documentation-only changes need link and content checks, not a Rust build. The [verification matrix](development/verification.md) lists the checks for each area and explains the CI jobs.

Bug fixes should have focused regression coverage where the behavior can be tested reliably. Prefer extending an existing test. Do not remove assertions, ignore failures, or weaken behavior just to make CI pass.

User-visible text must use the existing internationalization system. Update all 11 locale catalogs and run `python scripts/quality/audit_i18n.py` when adding or changing translation keys. Keep code comments in English.

Heavy CI jobs are skipped for draft PRs and documentation-only changes. Marking a code PR ready for review starts the applicable checks. Address failures related to your changes and report platform checks you cannot perform locally.

## Respond to review

Push follow-up changes to the same PR branch so the discussion and revision history stay together. Briefly explain how you addressed each requested change and report any new verification results. If you cannot reproduce a reported problem or disagree with a suggestion, describe what you observed and provide a reproduction, code reference, or measurement that helps resolve it.

## AI-assisted contributions

Before asking an AI coding tool to make changes, ensure it reads and follows the root [AGENTS.md](../AGENTS.md) and any applicable directory-specific `AGENTS.md` files. These are required project instructions covering architecture, component reuse, testing, and sensitive data. If your tool does not load them automatically, provide them explicitly.

AI-assisted contributions are welcome, including from contributors still learning Rust or the codebase. Explain the problem you want to solve and the behavior you expect. AI tools may run the required compilation checks and tests, but those checks must actually run and pass before you submit a code PR. Read the output and report the results; an AI-generated statement that the code works does not replace verification. You can use AI tools to work through review feedback as well.

## Reports and questions

Use the [issue templates](https://github.com/AnalyseDeCircuit/oxideterm/issues/new/choose) for bug reports and feature requests. Include the application version, platform, reproduction steps, and expected behavior. Remove credentials and private connection details from logs and screenshots.

Ask usage questions in [Discussions](https://github.com/AnalyseDeCircuit/oxideterm/discussions/categories/q-a). Report security vulnerabilities through [private vulnerability reporting](https://github.com/AnalyseDeCircuit/OxideTerm/security/advisories/new), rather than a public issue or PR.

> **Note on Ideas:** The maintainer may independently implement suggestions submitted via issues. Submitting an idea or proposal does not create joint authorship, shared copyright, or intellectual property rights over any subsequent implementation.
