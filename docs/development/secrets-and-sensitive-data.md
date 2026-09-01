# Secrets And Sensitive Data

Treat passwords, passphrases, private-key bytes, MFA responses, SSH-agent data, API keys, proxy credentials, cookies, authorization headers, cloud-sync credentials, and secret-bearing terminal content as sensitive data.

## Ownership Model

Every secret needs three answers before it is moved across code:

1. Which object owns the value now?
2. Which supported boundary is allowed to consume it?
3. When is every temporary owned copy cleared?

| Boundary | Required behavior |
| --- | --- |
| UI field | Keep the draft only while the field or prompt owns it; clear it on success, cancellation, replacement, or teardown |
| Async handoff | Move the smallest owned value into the task and keep its lifetime tied to a runtime owner |
| Protocol/authentication | Use zeroizing owned containers for temporary password, token, ticket, and keyboard-interactive data |
| Persistence | Store credentials only through the designated encrypted or operating-system-protected secret store; persist references and safe metadata elsewhere |
| Logging and diagnostics | Log structural status, never the value, derived command text, raw header, or raw terminal text that may contain it |
| CLI and processes | Prefer stdin or an approved environment boundary; never place secrets in shell arguments, task names, or process diagnostics |

## Implementation Rules

- Prefer `Zeroizing<T>`, `Zeroize`, or `ZeroizeOnDrop` for owned temporary values.
- Do not clone a secret for convenience. Every unavoidable copy needs the same bounded lifetime and zeroization treatment.
- Do not derive `Debug` for a type that owns secrets. Implement a redacted formatter when debugging structure is necessary.
- Keep safe display metadata separate from secret-bearing runtime types.
- Do not construct error messages, notifications, telemetry, AI context, or support bundles from a secret-bearing object unless its redaction boundary has been reviewed.

Keyboard-interactive authentication is sensitive even when the prompt text looks harmless. A one-time code, challenge response, or password-to-MFA fallback must use the same ownership and redaction rules as a stored password.

## Persistence And Export

Ordinary settings, connection labels, group names, and notes must not become alternate credential storage. A saved connection can retain a safe credential reference, while the secret itself remains in the secret store.

Portable `.oxide` export and cloud sync have explicit credential inclusion choices. Do not silently expand an export, backup, or background sync to include managed keys, passphrases, or portable secrets. See the [user-facing portable bundle contract](../user-guide/en/portable-oxide.md) before changing those paths.

## Review Checklist

Search the changed code for terms such as `password`, `passphrase`, `private_key`, `token`, `secret`, `credential`, `authorization`, and keyboard-interactive answers. Trace each value through UI input, async capture, protocol use, persistence, logs, errors, diagnostics, and drop.

Verify all of the following:

- representative secret text is absent from `Debug`, errors, notifications, and serialized safe metadata;
- cancellation and timeout drop temporary authentication answers;
- support reports contain status and hints, not credentials;
- CLI examples and subprocess invocation keep secrets out of arguments and shell history;
- any added task has a bounded owner and does not retain a secret after the operation completes.

Use a focused redaction or lifecycle test when the behavior has a stable local contract. Never paste real credentials into a test, issue, screen recording, commit, or AI prompt.
